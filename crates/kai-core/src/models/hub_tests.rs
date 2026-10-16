//! The store's pull path on real Hub JSON shapes (copied from
//! `api/models/Hcompany/Holo4-35B-A3B-GGUF`, 2026-10-13), offline.

use super::hub::*;
use super::rank::rank;
use super::{Catalog, HardwareSummary};
use serde_json::json;

const SHA: &str = "93400a4734519493de4f49910de1f6e0e24be46b";

fn holo4_tree() -> serde_json::Value {
    json!([
        {"type": "file", "path": ".gitattributes", "size": 1600},
        {"type": "file", "path": "README.md", "size": 9000},
        {"type": "file", "path": "mmproj.f16.gguf", "size": 899282976,
         "lfs": {"oid": "e1076d4179a6d82dbc7f129f043ac5cb03e4584ef5c1ed4a0c31eca5d1a187e4", "size": 899282976}},
        {"type": "file", "path": "q4_k_m.gguf", "size": 21297870912u64,
         "lfs": {"oid": "4c51212a4b5673d6a4215b9d708cd1730f1b6b4dbb5f75552df18de8341ce669", "size": 21297870912u64}}
    ])
}

fn info() -> HubInfo {
    info_from_json(&json!({
        "id": "Hcompany/Holo4-35B-A3B-GGUF", "sha": SHA, "gated": false,
        "cardData": {"license": "apache-2.0", "base_model": "Hcompany/Holo4-35B-A3B"}
    }))
    .unwrap()
}

fn file(path: &str, size: u64) -> HubFile {
    HubFile {
        path: path.into(),
        size: Some(size),
        sha256: Some("a".repeat(64)),
    }
}

fn pull(repo: &str, files: &[HubFile]) -> anyhow::Result<super::ModelEntry> {
    entry_from_hub(DEFAULT_ENDPOINT, &parse_ref(repo)?, &info(), files)
}

#[test]
fn pull_refuses_files_without_verified_size_and_hash() {
    let mut files = files_from_tree(&holo4_tree());
    files[0].sha256 = None;
    assert!(pull("Hcompany/Holo4-35B-A3B-GGUF", &files).is_err());
    let mut files = files_from_tree(&holo4_tree());
    files[1].size = None;
    assert!(pull("Hcompany/Holo4-35B-A3B-GGUF", &files).is_err());
}

#[test]
fn parses_repo_references() {
    let r = parse_ref("hf:Hcompany/Holo4-35B-A3B-GGUF:q5_k_m").unwrap();
    assert_eq!(r.repo, "Hcompany/Holo4-35B-A3B-GGUF");
    assert_eq!(r.quant.as_deref(), Some("Q5_K_M"));
    let r = parse_ref("https://huggingface.co/ggml-org/GLM-4.6V-Flash-GGUF/").unwrap();
    assert_eq!(
        (r.repo.as_str(), r.quant),
        ("ggml-org/GLM-4.6V-Flash-GGUF", None)
    );
    for bad in [
        "holo",
        "a/b/c",
        "../etc/passwd",
        "a/..",
        "a/b:",
        "a b/c",
        "",
    ] {
        assert!(parse_ref(bad).is_err(), "{bad:?} must be refused");
    }
}

#[test]
fn a_real_repo_becomes_a_pinned_entry() {
    let files = files_from_tree(&holo4_tree());
    assert_eq!(files.len(), 2, "only GGUF files: {files:?}");
    let e = entry_from_hub(
        DEFAULT_ENDPOINT,
        &parse_ref("Hcompany/Holo4-35B-A3B-GGUF").unwrap(),
        &info(),
        &files,
    )
    .unwrap();
    assert_eq!(e.id, "holo4-35b-a3b-q4_k_m");
    assert_eq!(e.served_name, "holo4-35b-a3b");
    assert_eq!((e.params_b, e.active_params_b), (35.0, Some(3.0)));
    assert_eq!(
        (e.license.as_str(), e.maker.as_str()),
        ("apache-2.0", "Hcompany")
    );
    let m = e.model_file().unwrap();
    assert_eq!(
        m.url,
        format!("https://huggingface.co/Hcompany/Holo4-35B-A3B-GGUF/resolve/{SHA}/q4_k_m.gguf")
    );
    assert_eq!(m.size_bytes, Some(21297870912));
    assert!(m.sha256.as_deref().unwrap().starts_with("4c51212a"));
    assert!(e.mmproj_file().unwrap().url.ends_with("/mmproj.f16.gguf"));
    assert_eq!(e.download_gb, 22.2);
    assert!(e.min_ram_gb > e.download_gb + 1.0, "{}", e.min_ram_gb);
    assert_eq!(
        e.computer_use, "untested",
        "never recommended before a bench"
    );
    assert!(e.problems().is_empty(), "{:?}", e.problems());
}

#[test]
fn picks_the_asked_or_the_preferred_quantisation() {
    let files = [
        file("m-IQ4_XS.gguf", 10),
        file("m-Q4_K_M.gguf", 20),
        file("m-Q8_0.gguf", 40),
        file("mmproj-m-F16.gguf", 3),
        file("mmproj-m-Q8_0.gguf", 2),
    ];
    let e = pull("o/m-GGUF", &files).unwrap();
    assert_eq!(e.quant, "Q4_K_M");
    assert!(e.mmproj_file().unwrap().url.ends_with("mmproj-m-Q8_0.gguf"));
    assert_eq!(pull("o/m-GGUF:Q8_0", &files).unwrap().quant, "Q8_0");
    // A tag matches whole: Q4_K is not Q4_K_M, and Q4_K_M is not in IQ4_K_M.
    let err = pull("o/m-GGUF:Q4_K", &files).unwrap_err().to_string();
    assert!(
        err.contains("has no Q4_K GGUF") && err.contains("m-Q4_K_M.gguf"),
        "{err}"
    );
    let only_iq = [file("m-IQ4_K_M.gguf", 1), file("mmproj.gguf", 1)];
    assert!(pull("o/m:Q4_K_M", &only_iq).is_err());
}

/// Guard: Kai must see the screen, so a text-only repo is refused.
#[test]
fn refuses_models_that_cannot_see() {
    let err = pull("o/text-only-GGUF", &[file("t-Q4_K_M.gguf", 5)])
        .unwrap_err()
        .to_string();
    assert!(err.contains("vision projector"), "{err}");
}

#[test]
fn refuses_gated_and_split_repos() {
    let gated = info_from_json(&json!({"sha": SHA, "gated": "manual"})).unwrap();
    assert!(gated.gated);
    let files = [file("m-Q4_K_M.gguf", 1), file("mmproj.gguf", 1)];
    let err = entry_from_hub(DEFAULT_ENDPOINT, &parse_ref("o/m").unwrap(), &gated, &files)
        .unwrap_err()
        .to_string();
    assert!(err.contains("gated"), "{err}");
    let split = [
        file("m-Q4_K_M-00001-of-00002.gguf", 1),
        file("m-Q4_K_M-00002-of-00002.gguf", 1),
        file("mmproj.gguf", 1),
    ];
    let err = pull("o/m", &split).unwrap_err().to_string();
    assert!(err.contains("split"), "{err}");
    assert!(info_from_json(&json!({"sha": "not-a-commit"})).is_err());
}

#[test]
fn search_reads_hits_and_cleans_the_query() {
    let url = search_url(DEFAULT_ENDPOINT, "holo 4&admin=1");
    assert!(url.contains("search=holo+4admin1&filter=gguf"), "{url}");
    let hits = hits_from_json(&json!([
        {"id": "Hcompany/Holo4-35B-A3B-GGUF", "downloads": 6029, "likes": 15},
        {"downloads": 1}
    ]));
    assert_eq!(hits.len(), 1);
    assert_eq!(
        (hits[0].repo.as_str(), hits[0].downloads),
        ("Hcompany/Holo4-35B-A3B-GGUF", 6029)
    );
}

#[test]
fn pulled_entries_are_saved_next_to_the_users_own() {
    let dir = std::env::temp_dir().join(format!("kai-store-{}", std::process::id()));
    let path = dir.join("models.json");
    let _ = std::fs::remove_dir_all(&dir);
    let mut a = pull(
        "o/alpha-7b-GGUF",
        &[file("a-Q4_K_M.gguf", 4_000_000_000), file("mmproj.gguf", 1)],
    )
    .unwrap();
    let b = pull(
        "o/beta-GGUF",
        &[file("b-Q4_0.gguf", 2_000_000_000), file("mmproj.gguf", 1)],
    )
    .unwrap();
    Catalog::save_user_entry(&path, &a).unwrap();
    Catalog::save_user_entry(&path, &b).unwrap();
    a.notes = "replaced".into();
    Catalog::save_user_entry(&path, &a).unwrap();
    let mut cat = Catalog::default();
    cat.merge_file(&path).unwrap();
    assert_eq!(cat.models.len(), 2);
    assert_eq!(cat.get("alpha-7b-q4_k_m").unwrap().notes, "replaced");
    assert_eq!(cat.get("alpha-7b-q4_k_m").unwrap().params_b, 7.0);
    // No size in the name: estimated from the file (2 GB at ~4.8 bits).
    assert_eq!(cat.get("beta-q4_0").unwrap().params_b, 3.3);
    assert!(!dir.join("models.json.tmp").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_pull_of_curated_weights_finds_the_curated_entry() {
    let pulled = entry_from_hub(
        DEFAULT_ENDPOINT,
        &parse_ref("Hcompany/Holo4-35B-A3B-GGUF").unwrap(),
        &info(),
        &files_from_tree(&holo4_tree()),
    )
    .unwrap();
    let same = Catalog::builtin()
        .same_weights(&pulled)
        .map(|m| m.id.clone());
    assert_eq!(same.as_deref(), Some("holo4-35b-a3b-q4"));
    let path = std::env::temp_dir().join(format!("kai-dup-{}/models.json", std::process::id()));
    let got = Catalog::builtin().add_pulled(&path, &pulled).unwrap();
    assert_eq!(got, super::Pulled::AlreadyHave("holo4-35b-a3b-q4".into()));
    assert!(!path.exists(), "nothing written for a duplicate");
}

#[test]
fn agent_trained_models_win_ties_between_untested_ones() {
    let base = Catalog::builtin().get("holo-3.1-4b-q4").unwrap().clone();
    let plain = super::ModelEntry {
        id: "plain".into(),
        agent_trained: false,
        ..base.clone()
    };
    let agent = super::ModelEntry {
        id: "agent".into(),
        agent_trained: true,
        ..base
    };
    let cat = Catalog {
        version: 1,
        models: vec![plain, agent],
    };
    let hw = HardwareSummary {
        ram_gb: 32.0,
        cpu_threads: 8,
        ..Default::default()
    };
    let ids: Vec<String> = rank(&cat, &hw).into_iter().map(|r| r.entry.id).collect();
    assert_eq!(ids, ["agent", "plain"]);
}
