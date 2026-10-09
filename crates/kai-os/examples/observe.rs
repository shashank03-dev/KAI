//! Take one real observation and save what the model would see.
//! `cargo run -p kai-os --example observe -- out.jpg`

use kai_core::Perceptor;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "observe.jpg".into());
    let p = kai_os::DesktopPerceptor::new();
    let obs = p.observe().await?;
    std::fs::write(&out, &obs.images.screenshot_jpeg)?;
    println!("{}", obs.grounding.as_text());
    println!(
        "capture {} ms · tree {} ms · encode {} ms · {} KB · hash {:016x}",
        obs.timing.capture_ms,
        obs.timing.tree_ms,
        obs.timing.encode_ms,
        obs.images.screenshot_jpeg.len() / 1024,
        obs.screen_hash
    );
    Ok(())
}
