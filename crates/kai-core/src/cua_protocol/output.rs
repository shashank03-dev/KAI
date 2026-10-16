//! What Kai sends back: the screenshot for a `call_id`, with the safety
//! checks the user accepted, and notes on what ran.
use super::SafetyCheck;
use base64::Engine as _;
use serde_json::{Value, json};

/// The image is a `data:` URL at `original` detail (OpenAI: avoid `high`
/// and `low` for computer use). `acknowledged` must repeat the call's
/// pending checks verbatim once the user accepted them.
pub fn screenshot_output(
    call_id: &str,
    image: &[u8],
    mime: &str,
    acknowledged: &[SafetyCheck],
) -> Value {
    let url = format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(image)
    );
    let mut out = json!({
        "type": "computer_call_output",
        "call_id": call_id,
        "output": {"type": "computer_screenshot", "image_url": url, "detail": "original"},
    });
    if !acknowledged.is_empty() {
        out["acknowledged_safety_checks"] =
            serde_json::to_value(acknowledged).expect("checks serialize");
    }
    out
}

/// A user message telling the model what the harness did (which actions of
/// a batch ran, which did not and why). The screenshot output carries only
/// an image, so this travels next to it in the same `input`.
pub fn harness_note(text: &str) -> Value {
    json!({"role": "user", "content": [{"type": "input_text", "text": format!("Harness note: {text}")}]})
}
