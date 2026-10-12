//! LocalManager end to end against a mock HTTP server and a fake runtime:
//! resumable downloads, checksum failures, cancel + resume, unpacking, and
//! start / health / stop of a fake `llama-server` (no real llama.cpp runs).

mod local_support;

use kai_core::models::{Catalog, ModelEntry, ModelFile};
use kai_os::local::download::{CancelToken, fetch};
use kai_os::local::runtime::{RuntimeAsset, RuntimeSpec, Variant};
use kai_os::local::{Cancelled, LocalManager, Progress};
use local_support::{Route, bytes, fake_runtime_tgz, fake_runtime_zip, serve, sha256};
use std::collections::HashMap;
use std::sync::Mutex;
#[cfg(target_os = "linux")]
use std::time::Duration;

const MIB: usize = 1 << 20;

fn entry(base: &str, model: &[u8], mmproj: &[u8], model_sha: Option<String>) -> ModelEntry {
    let mut e = Catalog::builtin().get("gemma-4-e2b-it-q4").unwrap().clone();
    e.id = "test-model".into();
    e.served_name = "test-served".into();
    e.files = vec![
        ModelFile {
            url: format!("{base}/m/model.gguf"),
            sha256: model_sha,
            role: "model".into(),
            size_bytes: Some(model.len() as u64),
        },
        ModelFile {
            url: format!("{base}/m/mmproj.gguf"),
            sha256: Some(sha256(mmproj)),
            role: "mmproj".into(),
            size_bytes: Some(mmproj.len() as u64),
        },
    ];
    e
}

fn runtime(base: &str, archive: &[u8], name: &str, file: &str, exe: &str) -> RuntimeSpec {
    RuntimeSpec {
        name: name.into(),
        variant: Variant::Cpu,
        assets: vec![RuntimeAsset {
            url: format!("{base}/rt/{file}"),
            sha256: Some(sha256(archive)),
            size_bytes: Some(archive.len() as u64),
        }],
        server_exe: exe.into(),
    }
}

fn routes(model: &[u8], mmproj: &[u8], rt: &[u8]) -> HashMap<String, Route> {
    let r = |b: &[u8]| Route {
        body: b.to_vec(),
        ..Default::default()
    };
    HashMap::from([
        ("/m/model.gguf".to_string(), r(model)),
        ("/m/mmproj.gguf".to_string(), r(mmproj)),
        ("/rt/rt.tar.gz".to_string(), r(rt)),
        ("/rt/rt.zip".to_string(), r(&fake_runtime_zip())),
    ])
}

#[tokio::test]
async fn resumes_after_a_dropped_connection_and_verifies() {
    let data = bytes(3 * MIB + 123, 1);
    let mut r = HashMap::new();
    r.insert(
        "/f".to_string(),
        Route {
            body: data.clone(),
            cut_first: Some(MIB + 7),
            ..Default::default()
        },
    );
    let mock = serve(r).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("f.gguf");
    let http = reqwest::Client::new();
    let mut seen = 0;
    fetch(
        &http,
        &format!("{}/f", mock.base),
        &dest,
        Some(&sha256(&data)),
        Some(data.len() as u64),
        &CancelToken::new(),
        &mut |b, _| seen = b,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert_eq!(seen, data.len() as u64);
    let log = mock.log.lock().unwrap().clone();
    assert_eq!(log.len(), 2, "{log:?}");
    assert_eq!(log[0].1, None);
    assert_eq!(
        log[1].1.as_deref(),
        Some(format!("bytes={}-", MIB + 7).as_str())
    );
}

#[tokio::test]
async fn restarts_when_the_server_ignores_ranges() {
    let data = bytes(MIB, 2);
    let mut r = HashMap::new();
    r.insert(
        "/f".to_string(),
        Route {
            body: data.clone(),
            ignore_range: true,
            ..Default::default()
        },
    );
    let mock = serve(r).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("f.bin");
    // A stale partial file from an earlier run.
    std::fs::write(dir.path().join("f.bin.part"), &data[..1000]).unwrap();
    fetch(
        &reqwest::Client::new(),
        &format!("{}/f", mock.base),
        &dest,
        Some(&sha256(&data)),
        Some(data.len() as u64),
        &CancelToken::new(),
        &mut |_, _| {},
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
}

#[tokio::test]
async fn checksum_mismatch_deletes_the_download() {
    let model = bytes(MIB, 3);
    let mmproj = bytes(1000, 4);
    let rt = fake_runtime_tgz();
    let mock = serve(routes(&model, &mmproj, &rt)).await;
    let dir = tempfile::tempdir().unwrap();
    let m = LocalManager::new(dir.path()).with_runtime(runtime(
        &mock.base,
        &rt,
        "fake-rt",
        "rt.tar.gz",
        "llama-server",
    ));
    let e = entry(&mock.base, &model, &mmproj, Some("0".repeat(64)));
    let err = m.install(&e, &|_| {}).await.unwrap_err().to_string();
    assert!(err.contains("checksum mismatch"), "{err}");
    let md = m.model_dir("test-model");
    assert!(!md.join("model.gguf").exists());
    assert!(!md.join("model.gguf.part").exists());
    assert!(!m.is_installed("test-model"));
}

#[tokio::test]
async fn cancel_keeps_the_part_and_the_next_install_resumes() {
    let model = bytes(4 * MIB, 5);
    let mmproj = bytes(5000, 6);
    let rt = fake_runtime_tgz();
    let mut r = routes(&model, &mmproj, &rt);
    r.get_mut("/m/model.gguf").unwrap().slow = true;
    let mock = serve(r).await;
    let dir = tempfile::tempdir().unwrap();
    let m = LocalManager::new(dir.path()).with_runtime(runtime(
        &mock.base,
        &rt,
        "fake-rt",
        "rt.tar.gz",
        "llama-server",
    ));
    let e = entry(&mock.base, &model, &mmproj, Some(sha256(&model)));
    let canceller = m.clone();
    let err = m
        .install(&e, &move |p: Progress| {
            if p.stage == "model" && p.bytes >= MIB as u64 {
                canceller.cancel();
            }
        })
        .await
        .unwrap_err();
    assert!(err.downcast_ref::<Cancelled>().is_some(), "{err:#}");
    let part = m.model_dir("test-model").join("model.gguf.part");
    let have = std::fs::metadata(&part).unwrap().len();
    assert!(have >= MIB as u64 && have < model.len() as u64, "{have}");
    assert!(!m.is_installed("test-model"));

    let stages = Mutex::new(Vec::new());
    m.install(&e, &|p: Progress| stages.lock().unwrap().push(p))
        .await
        .unwrap();
    assert!(m.is_installed("test-model"));
    assert_eq!(m.installed(), vec!["test-model".to_string()]);
    let md = m.model_dir("test-model");
    assert_eq!(std::fs::read(md.join("model.gguf")).unwrap(), model);
    assert_eq!(std::fs::read(md.join("mmproj.gguf")).unwrap(), mmproj);
    assert_eq!(m.installed_entry("test-model").unwrap(), e);
    let log = mock.log.lock().unwrap().clone();
    assert!(
        log.iter()
            .any(|(p, r)| p == "/m/model.gguf" && r.as_deref() == Some(&format!("bytes={have}-"))),
        "resumed with a range: {log:?}"
    );
    // The runtime was fetched once, not again after the cancel.
    assert_eq!(log.iter().filter(|(p, _)| p == "/rt/rt.tar.gz").count(), 1);
    let stages = stages.into_inner().unwrap();
    assert_eq!(stages.last().unwrap().stage, "done");
    assert!(stages.iter().any(|p| p.stage == "verify"));
    let last_model = stages.iter().rev().find(|p| p.stage == "model").unwrap();
    assert_eq!(last_model.total, Some((model.len() + mmproj.len()) as u64));

    m.remove("test-model").unwrap();
    assert!(m.installed().is_empty());
}

#[tokio::test]
async fn unpacks_a_flat_zip_runtime() {
    let model = bytes(1000, 7);
    let mmproj = bytes(1000, 8);
    let mock = serve(routes(&model, &mmproj, &[])).await;
    let dir = tempfile::tempdir().unwrap();
    let zip = fake_runtime_zip();
    let m = LocalManager::new(dir.path()).with_runtime(runtime(
        &mock.base,
        &zip,
        "zip-rt",
        "rt.zip",
        "llama-server.exe",
    ));
    m.install(&entry(&mock.base, &model, &mmproj, None), &|_| {})
        .await
        .unwrap();
    let server = m.server_path().unwrap();
    assert!(
        server.ends_with("runtime/zip-rt/llama-server.exe"),
        "{server:?}"
    );
    assert!(
        !dir.path().join("downloads/rt.zip").exists(),
        "archive cleaned up"
    );
}

#[cfg(target_os = "linux")]
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn starts_waits_for_health_and_stops_the_fake_server() {
    let model = bytes(2000, 9);
    let mmproj = bytes(2000, 10);
    let rt = fake_runtime_tgz();
    let mock = serve(routes(&model, &mmproj, &rt)).await;
    let dir = tempfile::tempdir().unwrap();
    let m = LocalManager::new(dir.path())
        .with_runtime(runtime(
            &mock.base,
            &rt,
            "fake-rt",
            "rt.tar.gz",
            "llama-server",
        ))
        .with_health_timeout(Duration::from_secs(20));
    let e = entry(&mock.base, &model, &mmproj, Some(sha256(&model)));
    let port = free_port();
    let err = m.start(&e, port).await.unwrap_err().to_string();
    assert!(err.contains("not installed"), "{err}");
    m.install(&e, &|_| {}).await.unwrap();

    let server = m.start(&e, port).await.unwrap();
    assert_eq!(server.base_url, format!("http://127.0.0.1:{port}/v1"));
    assert!(server.pid > 0);
    let args =
        std::fs::read_to_string(m.server_path().unwrap().with_file_name("args.txt")).unwrap();
    let args: Vec<&str> = args.lines().collect();
    let after = |f: &str| args[args.iter().position(|a| *a == f).unwrap() + 1];
    assert_eq!(after("--host"), "127.0.0.1");
    assert_eq!(after("--alias"), "test-served");
    assert!(after("-m").ends_with("models/test-model/model.gguf"));
    assert!(after("--mmproj").ends_with("models/test-model/mmproj.gguf"));
    let st = m.status().await.unwrap();
    assert!(st.healthy);
    assert_eq!(st.info.id, "test-model");
    assert_eq!(st.info.port, port);

    // Drop kills it.
    drop(server);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!kai_os::local::server::health(port).await);
    assert!(m.status().await.is_none());

    // Detached: survives the handle; stop() via server.json ends it.
    let server = m.start(&e, port).await.unwrap();
    server.detach();
    assert!(m.status().await.unwrap().healthy);
    assert!(m.stop().unwrap());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!kai_os::local::server::health(port).await);
    assert!(!m.stop().unwrap());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_server_that_dies_while_loading_reports_its_log() {
    let model = bytes(100, 11);
    let mmproj = bytes(100, 12);
    let rt = fake_runtime_tgz();
    let mock = serve(routes(&model, &mmproj, &rt)).await;
    let dir = tempfile::tempdir().unwrap();
    let m = LocalManager::new(dir.path())
        .with_runtime(runtime(
            &mock.base,
            &rt,
            "fake-rt",
            "rt.tar.gz",
            "llama-server",
        ))
        .with_health_timeout(Duration::from_secs(20));
    let e = entry(&mock.base, &model, &mmproj, None);
    m.install(&e, &|_| {}).await.unwrap();
    // Break the fake server: it exits at once with a message.
    let py = m.server_path().unwrap().with_file_name("fake_server.py");
    std::fs::write(
        &py,
        "import sys\nprint('error: model is corrupt')\nsys.exit(3)\n",
    )
    .unwrap();
    let err = m.start(&e, free_port()).await.unwrap_err().to_string();
    assert!(
        err.contains("stopped while loading") && err.contains("model is corrupt"),
        "{err}"
    );
}
