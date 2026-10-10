//! Live latency of one Kai decision call over the Vercel AI Gateway.
//!
//! Sends the real system prompt, a Set-of-Marks screenshot
//! (`docs/img/set-of-marks-gtk.jpg`, 1280x720) with a matching element list,
//! and the Decision schema, streaming on. Reports TTFT and total p50/p95, the
//! JSON validity rate (`Decision::parse_checked`), how often the expected
//! target (#2, "Page 2") was chosen, tokens and cost.
//!
//! ```text
//! AI_GATEWAY_API_KEY=... cargo run -p kai-core --example latency -- 10 \
//!     google/gemma-4-26b-a4b-it openai/gpt-5-mini openai/gpt-4.1-mini
//! KAI_EARLY_STOP=1 ...    # also measure early stop (stop reading once the JSON closes)
//! KAI_VERIFIER=1 ...      # also measure LlmVerifier (gpt-4.1-nano) and probe JEV
//! ```
//! The key is read from the environment only.

use kai_core::client::{
    DecisionClient, Images, JevVerifier, LlmVerifier, ModelClient, ModelRequest, OpenAiClient,
};
use kai_core::ports::Verifier;
use kai_core::{Decision, Element, Grounding, Rect, Role, prompt};
use std::time::{Duration, Instant};

const JPG: &[u8] = include_bytes!("../../../docs/img/set-of-marks-gtk.jpg");
const TASK: &str = "Switch to the second page of the demo (Page 2).";

fn grounding() -> Grounding {
    let e = |mark, role, name: &str, x, y, w, h| Element {
        mark,
        role,
        name: name.into(),
        bounds: Rect::new(x, y, w, h),
        enabled: true,
        secure: false,
        value: None,
    };
    Grounding::Marks(vec![
        e(1, Role::Tab, "Page 1", 330.0, 4.0, 80.0, 28.0),
        e(2, Role::Tab, "Page 2", 410.0, 4.0, 80.0, 28.0),
        e(3, Role::Tab, "Page 3", 490.0, 4.0, 80.0, 28.0),
        e(4, Role::Button, "Menu", 786.0, 4.0, 24.0, 28.0),
        e(5, Role::Button, "Minimize", 818.0, 8.0, 24.0, 20.0),
        e(6, Role::Button, "Maximize", 846.0, 8.0, 24.0, 20.0),
        e(7, Role::Button, "Close", 872.0, 8.0, 24.0, 20.0),
        e(9, Role::ComboBox, "comboboxentry", 10.0, 40.0, 232.0, 22.0),
        e(10, Role::Button, "togglebutton", 258.0, 40.0, 90.0, 22.0),
        e(35, Role::TextField, "entry", 10.0, 128.0, 232.0, 22.0),
        e(39, Role::TextField, "entry", 10.0, 158.0, 210.0, 22.0),
        e(40, Role::Button, "Clear", 220.0, 158.0, 22.0, 22.0),
        e(42, Role::ComboBox, "Left", 10.0, 188.0, 76.0, 22.0),
        e(49, Role::Checkbox, "checkbutton", 10.0, 246.0, 70.0, 14.0),
        e(50, Role::Radio, "radiobutton", 94.0, 246.0, 64.0, 14.0),
        e(63, Role::Link, "link button", 258.0, 306.0, 90.0, 20.0),
        e(70, Role::Tab, "page 1", 22.0, 394.0, 30.0, 18.0),
        e(71, Role::Tab, "page 2", 72.0, 394.0, 30.0, 18.0),
    ])
}

#[derive(Default)]
struct Stats {
    ttft: Vec<u64>,
    total: Vec<u64>,
    valid: usize,
    on_target: usize,
    errors: Vec<String>,
    cost: f64,
    completion_tokens: Vec<u32>,
}

fn pct(v: &[u64], p: f64) -> u64 {
    let mut s = v.to_vec();
    s.sort_unstable();
    if s.is_empty() {
        return 0;
    }
    s[(((s.len() - 1) as f64) * p).round() as usize]
}

/// One counted call into `s`.
async fn one(c: &OpenAiClient, req: &ModelRequest<'_>, s: &mut Stats) {
    match c.decide(req).await {
        Ok(r) => {
            s.total.push(r.total.as_millis() as u64);
            if let Some(t) = r.first_token {
                s.ttft.push(t.as_millis() as u64);
            }
            s.cost += r.cost_usd.unwrap_or(0.0);
            s.completion_tokens.extend(r.completion_tokens);
            match Decision::parse_checked(&r.raw) {
                Ok(d) => {
                    s.valid += 1;
                    if d.target == Some(2) {
                        s.on_target += 1;
                    }
                }
                Err(_) => s.errors.push(format!(
                    "invalid: {}",
                    r.raw.chars().take(120).collect::<String>()
                )),
            }
        }
        Err(e) => s.errors.push(e.to_string()),
    }
}

fn report(label: &str, n: usize, s: &Stats) {
    println!(
        "{label}: n={n} ok={} ttft p50/p95 = {}/{} ms, total p50/p95 = {}/{} ms, \
         valid {}/{n}, target #2 {}/{n}, completion tokens p50 {}, cost ${:.5}",
        s.total.len(),
        pct(&s.ttft, 0.5),
        pct(&s.ttft, 0.95),
        pct(&s.total, 0.5),
        pct(&s.total, 0.95),
        s.valid,
        s.on_target,
        pct(
            &s.completion_tokens
                .iter()
                .map(|&t| t as u64)
                .collect::<Vec<_>>(),
            0.5
        ),
        s.cost
    );
    for e in s.errors.iter().take(3) {
        println!("   error: {e}");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let key = std::env::var("AI_GATEWAY_API_KEY")
        .map_err(|_| anyhow::anyhow!("set AI_GATEWAY_API_KEY in the environment"))?;
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(10);
    let mut models: Vec<String> = args.collect();
    if models.is_empty() {
        models = [
            "google/gemma-4-26b-a4b-it",
            "openai/gpt-5-mini",
            "openai/gpt-4.1-mini",
        ]
        .map(String::from)
        .to_vec();
    }
    let early = std::env::var_os("KAI_EARLY_STOP").is_some();
    let mut configs: Vec<(String, OpenAiClient)> = Vec::new();
    for m in &models {
        configs.push((
            format!("{m} (stream)"),
            OpenAiClient::gateway(&key).with_model(m.as_str()),
        ));
        if early {
            configs.push((
                format!("{m} (stream, early stop)"),
                OpenAiClient::gateway(&key)
                    .with_model(m.as_str())
                    .with_early_stop(true),
            ));
        }
    }
    let system = prompt::system_prompt(&[]);
    let user = prompt::user_turn(TASK, &grounding(), &[], None, false);
    let images = Images {
        screenshot_jpeg: JPG.to_vec(),
        crop_jpeg: None,
    };
    let schema = Decision::json_schema();
    let req = ModelRequest {
        system: &system,
        user_text: &user,
        images: &images,
        schema: &schema,
        max_tokens: 192,
        temperature: 0.0,
    };
    // The free tier allows 5 requests/minute per model: run round-robin and
    // pace each round so no model exceeds that. Round 0 is a warm-up
    // (connection + any field fallbacks) and is not counted.
    let per_model = if early { 2 } else { 1 };
    let round = Duration::from_millis(
        std::env::var("KAI_ROUND_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(12_500 * per_model),
    );
    let mut stats: Vec<Stats> = configs.iter().map(|_| Stats::default()).collect();
    for r in 0..=n {
        let t = Instant::now();
        for (i, (_, c)) in configs.iter().enumerate() {
            if r == 0 {
                let _ = c.decide(&req).await;
            } else {
                one(c, &req, &mut stats[i]).await;
            }
        }
        if let Some(rest) = round.checked_sub(t.elapsed()) {
            tokio::time::sleep(rest).await;
        }
    }
    for ((label, _), s) in configs.iter().zip(&stats) {
        report(label, n, s);
    }
    if std::env::var_os("KAI_VERIFIER").is_some() {
        let v = LlmVerifier::gateway(&key, None);
        let state = grounding().as_text();
        let mut ms = Vec::new();
        let mut ps = Vec::new();
        for i in 0..n {
            let expect = if i % 2 == 0 {
                "a tab named Page 2 is visible"
            } else {
                "a file dialog is open"
            };
            let t = Instant::now();
            let p = v.check(TASK, expect, &state).await;
            ms.push(t.elapsed().as_millis() as u64);
            ps.push(p);
        }
        println!(
            "LlmVerifier openai/gpt-4.1-nano: n={n} p50/p95 = {}/{} ms, answers {:?}",
            pct(&ms, 0.5),
            pct(&ms, 0.95),
            ps
        );
        let jev =
            JevVerifier::new(DecisionClient::gateway(&key).with_timeout(Duration::from_secs(8)));
        let t = Instant::now();
        let p = jev
            .check(TASK, "a tab named Page 2 is visible", &state)
            .await;
        println!(
            "JevVerifier: {:?} in {} ms (off afterwards: {})",
            p,
            t.elapsed().as_millis(),
            jev.is_off()
        );
    }
    Ok(())
}
