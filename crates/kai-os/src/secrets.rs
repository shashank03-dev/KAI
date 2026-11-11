//! API keys in the OS keychain (the macOS Keychain, Windows Credential
//! Manager, Secret Service on Linux with the kernel keyring as its cache).
//! Environment variables win, for CI and the deploy scripts.

use kai_core::settings::Keys;
use std::sync::atomic::{AtomicBool, Ordering};

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

/// What the keychain holds for a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    Key(String),
    /// Nothing stored (or an empty value).
    Missing,
    /// The keychain could not be read (locked, access denied after a
    /// rebuild changed the app's signature, no keychain service): the key
    /// may well be there. The text says why, for the person.
    Unreadable(String),
}

pub fn get(name: &str) -> Option<String> {
    match lookup(name) {
        Stored::Key(v) => Some(v),
        Stored::Missing => None,
        Stored::Unreadable(why) => {
            // Every key fails the same way when the keychain is missing or
            // locked: say it once, not once per provider.
            static TOLD: AtomicBool = AtomicBool::new(false);
            if !TOLD.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "keychain read for {name} failed: {why}. Keys in environment \
                     variables (such as {}) still work",
                    env_for(name)
                );
            }
            None
        }
    }
}

/// Like [`get`], but tells "not stored" from "could not read the keychain",
/// so the app can say which instead of asking for the key again.
pub fn lookup(name: &str) -> Stored {
    if let Ok(v) = std::env::var(env_for(name))
        && !v.is_empty()
    {
        return Stored::Key(v);
    }
    stored(keyring::Entry::new(SERVICE, name).and_then(|e| e.get_password()))
}

fn stored(read: keyring::Result<String>) -> Stored {
    match read {
        Ok(v) if !v.is_empty() => Stored::Key(v),
        Ok(_) | Err(keyring::Error::NoEntry) => Stored::Missing,
        Err(e) => Stored::Unreadable(format!(
            "Kai could not read your keychain ({e}); it may be locked or Kai may need \
             permission again: unlock it, or store the key again"
        )),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_keychain_is_not_reported_as_no_key() {
        assert_eq!(stored(Ok("k".into())), Stored::Key("k".into()));
        assert_eq!(stored(Ok(String::new())), Stored::Missing);
        assert_eq!(stored(Err(keyring::Error::NoEntry)), Stored::Missing);
        let locked = keyring::Error::NoStorageAccess("locked".into());
        assert!(matches!(stored(Err(locked)), Stored::Unreadable(why) if why.contains("locked")));
    }
}
