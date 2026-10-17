//! Strict one-call OpenAI-compatible function transport for native GUI models.
//! Tool arguments remain raw JSON until the model adapter checks duplicate
//! keys, expected fields, coordinate bounds and the shared execution policy.

use super::{Backend, ClientError, ModelReply};
use serde_json::{Value, json};
use std::time::Duration;

pub(super) fn request_body(mut body: Value, tool: &Value) -> Value {
    body.as_object_mut().unwrap().remove("response_format");
    body.as_object_mut().unwrap().remove("stream_options");
    body["stream"] = json!(false);
    body["tools"] = json!([tool]);
    body["tool_choice"] = json!("required");
    body["parallel_tool_calls"] = json!(false);
    body
}

pub(super) fn parse_reply(
    body: &Value,
    backend: Backend,
    elapsed: Duration,
) -> Result<ModelReply, ClientError> {
    let choices = body["choices"].as_array().ok_or(ClientError::EmptyBody)?;
    if choices.len() != 1 {
        return Err(ClientError::Incomplete(
            "expected exactly one choice".into(),
        ));
    }
    let choice = &choices[0];
    if choice["finish_reason"] != "tool_calls" {
        return Err(ClientError::Incomplete(format!(
            "expected completed tool_calls, got {}",
            choice["finish_reason"]
        )));
    }
    let calls = choice["message"]["tool_calls"]
        .as_array()
        .ok_or(ClientError::EmptyBody)?;
    if calls.len() != 1 || calls[0]["type"] != "function" {
        return Err(ClientError::Incomplete(
            "expected exactly one function call".into(),
        ));
    }
    let call = &calls[0]["function"];
    if call["name"] != "computer_use" {
        return Err(ClientError::Incomplete(
            "unexpected native function name".into(),
        ));
    }
    let raw = call["arguments"].as_str().ok_or(ClientError::EmptyBody)?;
    if raw.trim().is_empty() {
        return Err(ClientError::EmptyBody);
    }
    let (prompt_tokens, completion_tokens, cost_usd) = super::stream::usage(body);
    Ok(ModelReply {
        raw: raw.to_owned(),
        prompt_tokens,
        completion_tokens,
        first_token: None,
        total: elapsed,
        backend,
        cost_usd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_complete_function_call_preserves_raw_arguments() {
        let args = r#"{"action":"left_click","coordinate":[500,400],"action":"visit_url"}"#;
        let body = json!({
            "choices":[{"finish_reason":"tool_calls","message":{"tool_calls":[
                {"type":"function","function":{"name":"computer_use","arguments":args}}
            ]}}], "usage":{"prompt_tokens":10,"completion_tokens":9}
        });
        let reply = parse_reply(&body, Backend::Local, Duration::from_millis(40)).unwrap();
        assert_eq!(
            reply.raw, args,
            "duplicate keys must reach the strict parser"
        );
        assert_eq!(reply.prompt_tokens, Some(10));
    }

    #[test]
    fn invalid_or_partial_calls_never_become_actions() {
        let good = json!({"type":"function","function":{
            "name":"computer_use","arguments":"{\"action\":\"wait\",\"time\":1}"}});
        for body in [
            json!({"choices":[{"finish_reason":"length","message":{"tool_calls":[good]}}]}),
            json!({"choices":[{"finish_reason":"stop","message":{"content":"done"}}]}),
            json!({"choices":[{"finish_reason":"tool_calls","message":{"tool_calls":[]}}]}),
            json!({"choices":[{"finish_reason":"tool_calls","message":{"tool_calls":[good,good]}}]}),
            json!({"choices":[{"finish_reason":"tool_calls","message":{"tool_calls":[{"type":"function","function":{"name":"shell","arguments":"{}"}}]}}]}),
        ] {
            assert!(parse_reply(&body, Backend::Local, Duration::ZERO).is_err());
        }
    }

    #[test]
    fn tool_body_disables_streaming_and_json_schema() {
        let body = request_body(
            json!({"stream":true,"stream_options":{},"response_format":{},"model":"holo"}),
            &json!({"type":"function"}),
        );
        assert_eq!(body["stream"], false);
        assert_eq!(body["tool_choice"], "required");
        assert_eq!(body["parallel_tool_calls"], false);
        assert!(body.get("response_format").is_none());
        assert!(body.get("stream_options").is_none());
        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
    }
}
