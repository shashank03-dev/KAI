//! API keys for a model call, read on the agent runtime's blocking pool and
//! never on the UI thread: a keychain read can wait on an OS prompt (U06 in
//! `docs/reviews/UI_UX_AUDIT_2026_10_16.md` sampled a 16.7 s macOS window stall).

use kai_core::settings::Keys;

/// `typed` keys (from a form, not saved yet) win; the keychain and the
/// environment fill the rest.
pub async fn resolve(typed: Keys) -> Keys {
    match tokio::task::spawn_blocking(kai_os::secrets::keys).await {
        Ok(stored) => typed.or(stored),
        Err(e) => {
            log::error!("keys: the keychain read stopped: {e}");
            typed
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                sources(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push((
                    p.display().to_string(),
                    std::fs::read_to_string(&p).unwrap(),
                ));
            }
        }
    }

    #[test]
    fn the_ui_thread_never_reads_the_keychain() {
        let mut files = Vec::new();
        sources(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        let calls = [
            ["secrets::", "keys("].concat(),
            ["secrets::", "get("].concat(),
        ];
        let readers: Vec<&str> = files
            .iter()
            .filter(|(path, text)| {
                !path.ends_with("keys.rs") && calls.iter().any(|c| text.contains(c))
            })
            .map(|(path, _)| path.as_str())
            .collect();
        assert!(
            readers.is_empty(),
            "keychain read outside keys.rs: {readers:?}"
        );
    }
}
