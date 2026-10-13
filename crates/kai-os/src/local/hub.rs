//! The HTTP half of the model store (`kai_core::models::hub` is the rest):
//! search Hugging Face for GGUF vision models, and resolve one repo into a
//! pinned catalog entry. Read-only requests to the Hub's public API; nothing
//! is downloaded here (`LocalManager::install` does that, from the entry).

use kai_core::models::ModelEntry;
use kai_core::models::hub::{self, HubRef, SearchHit};
use std::time::Duration;

/// Longest one Hub API call may take.
const API_TIMEOUT: Duration = Duration::from_secs(30);

async fn get_json(url: &str) -> anyhow::Result<serde_json::Value> {
    let resp = super::download::download_client()
        .get(url)
        .timeout(API_TIMEOUT)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("cannot reach Hugging Face: {e}"))?;
    let status = resp.status();
    anyhow::ensure!(
        status.is_success(),
        "Hugging Face answered {status} for {url}{}",
        if status.as_u16() == 401 || status.as_u16() == 404 {
            " (no such public repo)"
        } else {
            ""
        }
    );
    Ok(resp.json().await?)
}

/// GGUF image-text models matching `query`, most downloaded first.
pub async fn search(query: &str) -> anyhow::Result<Vec<SearchHit>> {
    let v = get_json(&hub::search_url(&hub::endpoint(), query)).await?;
    Ok(hub::hits_from_json(&v))
}

/// The pinned catalog entry for `r`: the repo's current commit, its file
/// list at that commit, then [`hub::entry_from_hub`]. GGUF repos often state
/// no licence; the base model's card is asked then.
pub async fn resolve(r: &HubRef) -> anyhow::Result<ModelEntry> {
    let endpoint = hub::endpoint();
    let mut info = hub::info_from_json(&get_json(&hub::info_url(&endpoint, &r.repo)).await?)?;
    if info.license.is_none()
        && let Some(base) = info.base.as_deref().filter(|b| hub::parse_ref(b).is_ok())
        && let Ok(v) = get_json(&hub::info_url(&endpoint, base)).await
    {
        info.license = v["cardData"]["license"].as_str().map(str::to_string);
    }
    let tree = get_json(&hub::tree_url(&endpoint, &r.repo, &info.sha)).await?;
    hub::entry_from_hub(&endpoint, r, &info, &hub::files_from_tree(&tree))
}
