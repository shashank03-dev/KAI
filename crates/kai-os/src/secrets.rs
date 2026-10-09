//! API keys in the OS keychain (Windows Credential Manager, Secret Service on
//! Linux). Environment variables win, for CI and the deploy scripts.

use kai_core::settings::Keys;

const SERVICE: &str = "kai";
pub const VLLM: &str = "vllm_api_key";
pub const GEMINI: &str = "gemini_api_key";
pub const GATEWAY: &str = "ai_gateway_api_key";
pub const OPENAI: &str = "openai_api_key";

fn env_for(name: &str) -> &'static str {
    match name {
        VLLM => "VLLM_API_KEY",
        GATEWAY => "AI_GATEWAY_API_KEY",
        OPENAI => "OPENAI_API_KEY",
        _ => "GEMINI_API_KEY",
    }
}

pub fn get(name: &str) -> Option<String> {
    if let Ok(v) = std::env::var(env_for(name))
        && !v.is_empty()
    {
        return Some(v);
    }
    match keyring::Entry::new(SERVICE, name).and_then(|e| e.get_password()) {
        Ok(v) if !v.is_empty() => Some(v),
        Ok(_) | Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            log::error!("keychain read for {name} failed: {e}");
            None
        }
    }
}

pub fn set(name: &str, value: &str) -> anyhow::Result<()> {
    keyring::Entry::new(SERVICE, name)?.set_password(value.trim())?;
    Ok(())
}

pub fn delete(name: &str) -> anyhow::Result<()> {
    match keyring::Entry::new(SERVICE, name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn keys() -> Keys {
    Keys {
        vllm: get(VLLM),
        gemini: get(GEMINI),
        gateway: get(GATEWAY),
        openai: get(OPENAI),
    }
}
