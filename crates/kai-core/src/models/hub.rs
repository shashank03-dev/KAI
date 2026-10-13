//! The model store's open shelf: any GGUF vision model on Hugging Face,
//! turned into a catalog entry (`kai models pull <repo>`). OS-free and
//! offline: this file only reads the Hub's JSON; `kai_os::local::hub` does
//! the HTTP. See `docs/MODEL_STORE.md`.
//!
//! A pulled entry is pinned like a built-in one: URLs at the repo's commit,
//! the SHA-256 of every file from the Hub's LFS metadata, exact sizes. What
//! the Hub cannot tell (parameters, memory) is estimated and labelled so,
//! and the entry stays `untested` until `kai bench` says otherwise.

use super::ModelEntry;
use serde_json::Value;

/// Hugging Face's own address; `HF_ENDPOINT` (the Hub's standard variable)
/// points Kai at a mirror instead.
pub const DEFAULT_ENDPOINT: &str = "https://huggingface.co";

/// Quantisations tried in order when the user names none: the 4-bit ones
/// llama.cpp runs best on CPU and GPU, then larger ones.
pub const QUANT_ORDER: &[&str] = &[
    "Q4_K_M", "Q4_0", "Q4_K_S", "IQ4_XS", "IQ4_NL", "Q5_K_M", "Q6_K", "Q8_0",
];
/// Vision projectors, best first (Q8_0 is small and loses nothing measurable).
const MMPROJ_ORDER: &[&str] = &["Q8_0", "F16", "BF16", "F32"];
/// What a pulled model asks the server for, like the built-in entries.
pub const PULL_CONTEXT: u32 = 16384;

/// The Hub address in use.
pub fn endpoint() -> String {
    std::env::var("HF_ENDPOINT")
        .ok()
        .map(|e| e.trim().trim_end_matches('/').to_string())
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string())
}

/// What the user asked for: a repo and, optionally, a quantisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubRef {
    pub repo: String,
    pub quant: Option<String>,
}

/// `owner/repo`, `owner/repo:Q5_K_M`, `hf:owner/repo` or a
/// `https://huggingface.co/owner/repo` link.
pub fn parse_ref(raw: &str) -> anyhow::Result<HubRef> {
    let s = raw.trim();
    let s = s.strip_prefix("hf:").unwrap_or(s);
    let s = s
        .strip_prefix("https://huggingface.co/")
        .unwrap_or(s)
        .trim_end_matches('/');
    let (repo, quant) = match s.split_once(':') {
        Some((r, q)) => (r, Some(q.trim().to_ascii_uppercase())),
        None => (s, None),
    };
    let parts: Vec<&str> = repo.split('/').collect();
    let ok = parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        });
    anyhow::ensure!(ok, "{raw:?} is not a Hugging Face repo like owner/name");
    anyhow::ensure!(
        quant.as_deref().is_none_or(|q| !q.is_empty()),
        "empty quantisation after ':' in {raw:?}"
    );
    Ok(HubRef {
        repo: repo.to_string(),
        quant,
    })
}

/// One file of a repo, from the tree listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubFile {
    pub path: String,
    pub size: Option<u64>,
    /// The LFS object id, which is the file's SHA-256.
    pub sha256: Option<String>,
}

/// The `.gguf` files of `GET /api/models/<repo>/tree/<rev>`.
pub fn files_from_tree(tree: &Value) -> Vec<HubFile> {
    tree.as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["type"] == "file" || f["type"].is_null())
        .filter_map(|f| {
            let path = f["path"].as_str()?;
            path.to_ascii_lowercase()
                .ends_with(".gguf")
                .then(|| HubFile {
                    path: path.to_string(),
                    size: f["lfs"]["size"].as_u64().or_else(|| f["size"].as_u64()),
                    sha256: f["lfs"]["oid"].as_str().map(str::to_ascii_lowercase),
                })
        })
        .collect()
}

/// What `GET /api/models/<repo>` says about a repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubInfo {
    /// The commit every URL is pinned to.
    pub sha: String,
    pub license: Option<String>,
    pub gated: bool,
    /// The first base model, e.g. `Hcompany/Holo4-35B-A3B`.
    pub base: Option<String>,
}

pub fn info_from_json(v: &Value) -> anyhow::Result<HubInfo> {
    let sha = v["sha"]
        .as_str()
        .filter(|s| s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| anyhow::anyhow!("the Hub gave no commit for this repo"))?;
    let card = &v["cardData"];
    let base = match &card["base_model"] {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => a.first().and_then(Value::as_str).map(str::to_string),
        _ => None,
    };
    Ok(HubInfo {
        sha: sha.to_ascii_lowercase(),
        license: card["license"].as_str().map(str::to_string),
        // `false`, or a string such as "auto"/"manual" when gated.
        gated: !matches!(v["gated"], Value::Bool(false) | Value::Null),
        base,
    })
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub repo: String,
    pub downloads: u64,
    pub likes: u64,
}

/// GGUF image-text models matching `query`, most downloaded first.
pub fn search_url(endpoint: &str, query: &str) -> String {
    let q: String = query
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
        .collect::<String>()
        .replace(' ', "+");
    format!(
        "{endpoint}/api/models?search={q}&filter=gguf&pipeline_tag=image-text-to-text\
         &sort=downloads&direction=-1&limit=20"
    )
}

pub fn hits_from_json(v: &Value) -> Vec<SearchHit> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(SearchHit {
                repo: m["id"].as_str()?.to_string(),
                downloads: m["downloads"].as_u64().unwrap_or(0),
                likes: m["likes"].as_u64().unwrap_or(0),
            })
        })
        .collect()
}

pub fn info_url(endpoint: &str, repo: &str) -> String {
    format!("{endpoint}/api/models/{repo}")
}

pub fn tree_url(endpoint: &str, repo: &str, sha: &str) -> String {
    format!("{endpoint}/api/models/{repo}/tree/{sha}?recursive=true")
}

/// Build the catalog entry for a pulled repo. Fails, saying why, when the
/// repo is gated, has no single-file GGUF of the asked quantisation, or no
/// vision projector (Kai must see the screen).
pub fn entry_from_hub(
    endpoint: &str,
    r: &HubRef,
    info: &HubInfo,
    files: &[HubFile],
) -> anyhow::Result<ModelEntry> {
    anyhow::ensure!(
        !info.gated,
        "{} is gated on Hugging Face (it needs an accepted licence and a token); \
         Kai pulls only open repos",
        r.repo
    );
    let (model, quant) = pick_model(r, files)?;
    let mmproj = pick_mmproj(files).ok_or_else(|| {
        anyhow::anyhow!(
            "{} has no vision projector (mmproj): Kai needs a model that can see the screen",
            r.repo
        )
    })?;
    let name = model_name(&r.repo);
    let total: u64 = [model, mmproj].iter().filter_map(|f| f.size).sum();
    let model_gb = model.size.unwrap_or(0) as f32 / 1e9;
    let download_gb = round2(total as f32 / 1e9);
    let (params, active) = params_from_name(&name);
    let params_b = params.unwrap_or_else(|| round1(model_gb * 8.0 / bits(&quant)));
    // Files + 1 GB runtime + a KV allowance at 16K that grows with the
    // model; an estimate, generous for the hybrid and sliding-window ones.
    let kv = (0.1 * model_gb).clamp(1.0, 3.0);
    let min_ram_gb = round1(download_gb + kv + 1.0);
    let owner = info
        .base
        .as_deref()
        .unwrap_or(&r.repo)
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string();
    let url = |f: &HubFile| format!("{endpoint}/{}/resolve/{}/{}", r.repo, info.sha, f.path);
    let file = |f: &HubFile, role: &str| super::ModelFile {
        url: url(f),
        sha256: f.sha256.clone(),
        role: role.into(),
        size_bytes: f.size,
    };
    let entry = ModelEntry {
        id: format!("{}-{}", name, quant.to_ascii_lowercase()),
        family: name.clone(),
        label: format!("{name} ({quant})"),
        maker: owner,
        license: info.license.clone().unwrap_or_else(|| "unknown".into()),
        params_b,
        active_params_b: active,
        quant: quant.clone(),
        vision: true,
        context: PULL_CONTEXT,
        download_gb,
        min_ram_gb,
        min_vram_gb: Some(min_ram_gb),
        files: vec![file(model, "model"), file(mmproj, "mmproj")],
        runtime: "llama.cpp".into(),
        served_name: name,
        notes: format!(
            "Pulled from Hugging Face ({}@{}). Not yet tried with Kai's harness; parameters \
             and memory are estimates from the file sizes.",
            r.repo,
            &info.sha[..7]
        ),
        computer_use: "untested".into(),
        agent_trained: false,
    };
    let problems = entry.problems();
    anyhow::ensure!(problems.is_empty(), "{}", problems.join("; "));
    Ok(entry)
}

/// The repo name without a `-GGUF` suffix, lower case: `holo4-35b-a3b`.
fn model_name(repo: &str) -> String {
    let name = repo.rsplit('/').next().unwrap_or(repo).to_ascii_lowercase();
    let name = name
        .strip_suffix("-gguf")
        .or_else(|| name.strip_suffix("_gguf"))
        .unwrap_or(&name);
    name.to_string()
}

fn is_split(path: &str) -> bool {
    // `name-00001-of-00003.gguf`
    let lower = path.to_ascii_lowercase();
    lower.contains("-of-")
        && lower
            .split('-')
            .any(|p| p.len() == 5 && p.chars().all(|c| c.is_ascii_digit()))
}

fn is_mmproj(path: &str) -> bool {
    path.to_ascii_lowercase().contains("mmproj")
}

/// Whether `path` carries the quantisation tag `q` as a whole token
/// (`Q4_K_M` in `x.Q4_K_M.gguf`, not in `x-IQ4_K_M.gguf`).
fn has_tag(path: &str, q: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_uppercase();
    name.match_indices(q).any(|(i, _)| {
        let before = name[..i].chars().last();
        let after = name[i + q.len()..].chars().next();
        before.is_none_or(|c| !c.is_ascii_alphanumeric())
            && after.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
    })
}

fn pick_model<'f>(r: &HubRef, files: &'f [HubFile]) -> anyhow::Result<(&'f HubFile, String)> {
    let weights: Vec<&HubFile> = files.iter().filter(|f| !is_mmproj(&f.path)).collect();
    let wanted: Vec<String> = match &r.quant {
        Some(q) => vec![q.clone()],
        None => QUANT_ORDER.iter().map(|q| q.to_string()).collect(),
    };
    for q in &wanted {
        let found: Vec<&&HubFile> = weights.iter().filter(|f| has_tag(&f.path, q)).collect();
        if found.is_empty() {
            continue;
        }
        anyhow::ensure!(
            found.iter().all(|f| !is_split(&f.path)),
            "{} {q} is split into several files, which Kai does not load yet",
            r.repo
        );
        anyhow::ensure!(
            found.len() == 1,
            "{} has {} {q} files; name the one you want",
            r.repo,
            found.len()
        );
        return Ok((found[0], q.clone()));
    }
    let have: Vec<&str> = weights
        .iter()
        .map(|f| f.path.rsplit('/').next().unwrap_or(&f.path))
        .collect();
    anyhow::bail!(
        "{} has no {} GGUF; it has: {}",
        r.repo,
        r.quant.as_deref().unwrap_or("4-bit"),
        if have.is_empty() {
            "none".into()
        } else {
            have.join(", ")
        }
    )
}

fn pick_mmproj(files: &[HubFile]) -> Option<&HubFile> {
    let projectors: Vec<&HubFile> = files.iter().filter(|f| is_mmproj(&f.path)).collect();
    MMPROJ_ORDER
        .iter()
        .find_map(|q| projectors.iter().find(|f| has_tag(&f.path, q)).copied())
        .or_else(|| projectors.first().copied())
}

/// Total and active billions from names like `35b-a3b`, `0.8b`, `e4b`.
fn params_from_name(name: &str) -> (Option<f32>, Option<f32>) {
    let mut total = None;
    let mut active = None;
    for tok in name.split(['-', '_']) {
        let Some(num) = tok.strip_suffix('b') else {
            continue;
        };
        match num.strip_prefix('a').or_else(|| num.strip_prefix('e')) {
            Some(a) => active = a.parse::<f32>().ok().or(active),
            None => total = num.parse::<f32>().ok().or(total),
        }
    }
    (total, active)
}

/// Rough bits per weight of a quantisation, for the parameter estimate.
fn bits(quant: &str) -> f32 {
    match quant {
        q if q.starts_with("Q8") => 8.5,
        q if q.starts_with("Q6") => 6.6,
        q if q.starts_with("Q5") => 5.7,
        _ => 4.8,
    }
}

fn round1(x: f32) -> f32 {
    (x * 10.0).round() / 10.0
}

fn round2(x: f32) -> f32 {
    (x * 100.0).round() / 100.0
}
