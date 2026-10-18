//! Streaming for OpenAI-compatible chat completions (server-sent events), with
//! real time-to-first-token and an early stop once the decision JSON closes.
//!
//! Wire format: `data: {chunk}\n\n` lines, `choices[0].delta.content` carries
//! text, a final chunk carries `usage` when `stream_options.include_usage` is
//! set, and `data: [DONE]` ends the stream.

use super::ClientError;
use futures::StreamExt;
use std::time::{Duration, Instant};

/// Where the first top-level JSON object in `s` closes: the byte index one past
/// its closing `}`. Leading prose or a code fence before the `{` is skipped;
/// braces inside string literals are ignored. `None` while still open.
pub fn complete_json_prefix(s: &str) -> Option<usize> {
    let start = s.find('{')?;
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for (i, &c) in s.as_bytes().iter().enumerate().skip(start) {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// What a streamed (or plain) completion produced.
#[derive(Debug, Default, Clone)]
pub struct StreamOutcome {
    pub content: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub cost_usd: Option<f64>,
    /// Time from sending the request to the first non-empty content delta.
    pub first_token: Option<Duration>,
    /// True when reading stopped as soon as the JSON object closed (usage and
    /// cost are then usually missing: they arrive in the last chunk).
    pub stopped_early: bool,
    /// Text the server put in `reasoning_content` (or `reasoning`) rather
    /// than `content`, e.g. llama.cpp's Gemma 4 thought channel.
    pub reasoning: String,
    /// Why generation stopped (`stop`, `length`), when the server said.
    pub finish_reason: Option<String>,
}

impl StreamOutcome {
    /// Fold one parsed chunk (or a whole non-streaming body) into the outcome.
    pub(crate) fn apply(&mut self, v: &serde_json::Value, started: Instant) {
        let choice = &v["choices"][0];
        let text = choice["delta"]["content"]
            .as_str()
            .or_else(|| choice["message"]["content"].as_str());
        if let Some(t) = text
            && !t.is_empty()
        {
            if self.first_token.is_none() {
                self.first_token = Some(started.elapsed());
            }
            self.content.push_str(t);
        }
        let thought = ["delta", "message"]
            .iter()
            .flat_map(|k| ["reasoning_content", "reasoning"].map(|f| &choice[k][f]))
            .find_map(serde_json::Value::as_str);
        if let Some(t) = thought {
            self.reasoning.push_str(t);
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            self.finish_reason = Some(f.to_string());
        }
        let (p, c, cost) = usage(v);
        self.prompt_tokens = p.or(self.prompt_tokens);
        self.completion_tokens = c.or(self.completion_tokens);
        self.cost_usd = cost.or(self.cost_usd);
    }

    /// Make sure the reply has decision text. With no `content`, a complete
    /// JSON object in the reasoning text is the decision (Gemma 4 can write
    /// it in its thought channel); otherwise the reply is empty, and why is
    /// logged so the cause shows in the run's log.
    pub(crate) fn require_content(&mut self) -> Result<(), ClientError> {
        if !self.content.trim().is_empty() {
            return Ok(());
        }
        if let Some(end) = complete_json_prefix(&self.reasoning) {
            log::warn!("reply came in the reasoning text, not content; using its decision");
            let start = self.reasoning.find('{').unwrap_or(0);
            self.content = self.reasoning[start..end].to_string();
            return Ok(());
        }
        log::warn!(
            "empty reply: finish_reason {}, {} characters of reasoning, {} tokens",
            self.finish_reason.as_deref().unwrap_or("unknown"),
            self.reasoning.len(),
            self.completion_tokens
                .map_or("unknown".to_string(), |n| n.to_string())
        );
        Err(ClientError::EmptyBody)
    }
}

/// Token counts and cost from a chunk or body. Cost comes from `usage.cost`
/// (number) or `provider_metadata.gateway.cost` (string) on the AI Gateway.
pub(crate) fn usage(v: &serde_json::Value) -> (Option<u32>, Option<u32>, Option<f64>) {
    let u = &v["usage"];
    let p = u["prompt_tokens"].as_u64().map(|n| n as u32);
    let c = u["completion_tokens"].as_u64().map(|n| n as u32);
    let choice = &v["choices"][0];
    let meta = if choice["message"]["provider_metadata"].is_object() {
        &choice["message"]["provider_metadata"]
    } else {
        &choice["delta"]["provider_metadata"]
    };
    let cost = number(&u["cost"]).or_else(|| number(&meta["gateway"]["cost"]));
    (p, c, cost)
}

fn number(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
}

/// A reply that is only this much whitespace is a degenerate one.
const WHITESPACE_RUN_MAX: usize = 256;

/// Read an SSE chat-completions stream to the end (or, with `early_stop`, until
/// the first JSON object in the content is complete, then drop the
/// connection). `started` is when the request was sent, for TTFT.
pub async fn read_chat_stream(
    resp: reqwest::Response,
    started: Instant,
    early_stop: bool,
) -> Result<StreamOutcome, ClientError> {
    let mut out = StreamOutcome::default();
    let mut buf: Vec<u8> = Vec::new();
    let mut body = resp.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| ClientError::Transport(e.to_string()))?;
        buf.extend_from_slice(&chunk);
        while let Some(nl) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=nl).collect();
            match handle_line(&line, &mut out, started)? {
                Line::Done => return Ok(out),
                Line::More => {}
            }
            // Gemma sometimes streams a run of whitespace until max_tokens or the
            // timeout; give up early. A decision already complete before the run
            // is kept (seen live: Gemma 4 closed its JSON, then padded with
            // whitespace, and the whole step was retried); otherwise retry.
            if out.content.len() - out.content.trim_end().len() > WHITESPACE_RUN_MAX {
                return keep_complete(out);
            }
            if early_stop && complete_json_prefix(&out.content).is_some() {
                out.stopped_early = true;
                return Ok(out);
            }
        }
    }
    // A last line without a trailing newline.
    if !buf.is_empty() {
        handle_line(&buf, &mut out, started)?;
    }
    Ok(out)
}

/// The reply up to its complete JSON object, or `EmptyBody` without one.
fn keep_complete(mut out: StreamOutcome) -> Result<StreamOutcome, ClientError> {
    let Some(end) = complete_json_prefix(&out.content) else {
        log::warn!(
            "reply degenerated into whitespace after {} characters, with no complete decision",
            out.content.trim_end().len()
        );
        return Err(ClientError::EmptyBody);
    };
    log::warn!("reply padded with whitespace after a complete decision; keeping the decision");
    out.content.truncate(end);
    out.stopped_early = true;
    Ok(out)
}

enum Line {
    More,
    Done,
}

fn handle_line(
    line: &[u8],
    out: &mut StreamOutcome,
    started: Instant,
) -> Result<Line, ClientError> {
    let line = String::from_utf8_lossy(line);
    let line = line.trim();
    let Some(data) = line.strip_prefix("data:") else {
        return Ok(Line::More); // comments (`: ping`), `event:` lines, blanks
    };
    let data = data.trim();
    if data == "[DONE]" {
        return Ok(Line::Done);
    }
    let v: serde_json::Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return Ok(Line::More),
    };
    if v.get("error").is_some_and(|e| !e.is_null()) {
        let code = v["error"]["code"].as_u64().unwrap_or(500) as u16;
        return Err(ClientError::from_status(code, &v["error"].to_string()));
    }
    out.apply(&v, started);
    Ok(Line::More)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A streamed reply whose content arrives in `deltas`.
    async fn stream_of(deltas: &[&str]) -> Result<StreamOutcome, ClientError> {
        let body: String = deltas
            .iter()
            .map(|d| {
                let chunk = serde_json::json!({"choices": [{"delta": {"content": d}}]});
                format!("data: {chunk}\n\n")
            })
            .chain(["data: [DONE]\n\n".to_string()])
            .collect();
        let resp = reqwest::Response::from(http::Response::new(body));
        read_chat_stream(resp, Instant::now(), false).await
    }

    #[test]
    fn a_decision_in_the_reasoning_text_is_used_when_content_is_empty() {
        let mut out = StreamOutcome::default();
        let chunk = |f: &str, t: &str| serde_json::json!({"choices": [{"delta": {f: t}}]});
        out.apply(
            &chunk("reasoning_content", "thinking {\"action\":"),
            Instant::now(),
        );
        out.apply(
            &chunk("reasoning_content", "\"done\"} tail"),
            Instant::now(),
        );
        out.require_content().unwrap();
        assert_eq!(out.content, r#"{"action":"done"}"#);
        // Content wins when there is any.
        let mut both = StreamOutcome::default();
        both.apply(&chunk("content", "{\"a\":1}"), Instant::now());
        both.apply(&chunk("reasoning", "{\"b\":2}"), Instant::now());
        both.require_content().unwrap();
        assert_eq!(both.content, r#"{"a":1}"#);
    }

    #[test]
    fn no_content_and_no_decision_is_empty() {
        let mut out = StreamOutcome::default();
        let v = serde_json::json!({"choices": [{"delta": {"reasoning_content": "hmm"}, "finish_reason": "length"}]});
        out.apply(&v, Instant::now());
        assert_eq!(out.finish_reason.as_deref(), Some("length"));
        assert!(matches!(out.require_content(), Err(ClientError::EmptyBody)));
    }

    #[tokio::test]
    async fn a_decision_padded_with_whitespace_is_kept() {
        let pad = " ".repeat(WHITESPACE_RUN_MAX + 10);
        let out = stream_of(&[r#"{"action":"done"}"#, "\n", &pad, "never read"])
            .await
            .unwrap();
        assert_eq!(out.content, r#"{"action":"done"}"#);
        assert!(out.stopped_early);
    }

    #[tokio::test]
    async fn whitespace_without_a_complete_decision_is_empty() {
        let pad = " ".repeat(WHITESPACE_RUN_MAX + 10);
        let half = stream_of(&[r#"{"action":"#, &pad]).await;
        assert!(matches!(half, Err(ClientError::EmptyBody)), "{half:?}");
        let blank = stream_of(&[&pad]).await;
        assert!(matches!(blank, Err(ClientError::EmptyBody)), "{blank:?}");
    }

    #[test]
    fn json_prefix_closes_at_the_outer_brace() {
        let s = r#"{"a":{"b":1},"c":"}{"} trailing"#;
        let end = complete_json_prefix(s).unwrap();
        assert_eq!(&s[..end], r#"{"a":{"b":1},"c":"}{"}"#);
    }

    #[test]
    fn json_prefix_skips_fences_and_waits_for_close() {
        assert_eq!(complete_json_prefix("```json\n{\"a\":\"x\\\"}\""), None);
        let s = "```json\n{\"a\":\"x\\\"}\"}\n```";
        let end = complete_json_prefix(s).unwrap();
        assert!(s[..end].ends_with("\"}"));
        assert_eq!(complete_json_prefix("no json yet"), None);
    }

    #[test]
    fn usage_and_cost_from_gateway_shapes() {
        let v = serde_json::json!({
            "usage": {"prompt_tokens": 17, "completion_tokens": 12, "cost": 0.00000921}
        });
        assert_eq!(usage(&v), (Some(17), Some(12), Some(0.00000921)));
        let v = serde_json::json!({"choices":[{"message":{"content":"x",
            "provider_metadata":{"gateway":{"cost":"0.5"}}}}]});
        assert_eq!(usage(&v).2, Some(0.5));
    }

    #[test]
    fn lines_accumulate_content_and_stop_on_done() {
        let mut out = StreamOutcome::default();
        let t = Instant::now();
        let l1 = br#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert!(matches!(handle_line(l1, &mut out, t), Ok(Line::More)));
        assert!(out.first_token.is_none());
        let l2 = br#"data: {"choices":[{"delta":{"content":"{\"a\":1}"}}]}"#;
        handle_line(l2, &mut out, t).unwrap();
        assert!(out.first_token.is_some());
        assert_eq!(out.content, r#"{"a":1}"#);
        assert!(matches!(
            handle_line(b": ping", &mut out, t),
            Ok(Line::More)
        ));
        assert!(matches!(
            handle_line(b"data: [DONE]\r\n", &mut out, t),
            Ok(Line::Done)
        ));
    }

    #[test]
    fn error_event_becomes_an_error() {
        let mut out = StreamOutcome::default();
        let l = br#"data: {"error":{"message":"Free tier users do not have access","code":403}}"#;
        let e = handle_line(l, &mut out, Instant::now()).err().unwrap();
        assert!(matches!(e, ClientError::Unavailable(_)));
    }
}
