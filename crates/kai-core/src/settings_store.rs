//! Durable settings writes. Keep the previous file when it cannot be parsed.

use super::Settings;
use anyhow::Context;
use std::io::Write;
use std::path::Path;

impl Settings {
    pub fn save(&self) -> anyhow::Result<()> {
        self.save_at(&Self::dir().join("settings.json"))
    }

    pub(super) fn save_at(&self, path: &Path) -> anyhow::Result<()> {
        let parent = path.parent().context("settings path has no parent")?;
        std::fs::create_dir_all(parent)?;
        if path.exists() {
            let current = std::fs::read_to_string(path)?;
            serde_json::from_str::<Settings>(&current).with_context(|| {
                format!(
                    "repair invalid settings at {} before saving",
                    path.display()
                )
            })?;
        }
        let data = serde_json::to_vec_pretty(self)?;
        for attempt in 0..16 {
            let temp = parent.join(format!(
                ".settings-{}-{}-{}.tmp",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos(),
                attempt
            ));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options.open(&temp);
            let mut file = match file {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            };
            let result = (|| -> anyhow::Result<()> {
                file.write_all(&data)?;
                file.sync_all()?;
                drop(file);
                std::fs::rename(&temp, path)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(&temp);
            }
            return result;
        }
        anyhow::bail!("could not reserve a temporary settings file")
    }
}
