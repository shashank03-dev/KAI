//! End to end: record two mock bench runs through the real engine, then
//! export and summarise them.

use super::export::{self, Filter};
use super::{Format, stats};
use crate::bench::record::Recording;
use crate::bench::runner::run_mock;
use crate::bench::tasks::parse_tasks;
use std::path::{Path, PathBuf};

const TASKS: &str = r#"
- id: good
  title: Good
  prompt: list the files
  check:
    linux: "true"
  mock:
    - '{"thought":"list","action":"shell","text":"ls","risk":"none","expect":"files listed"}'
    - '{"thought":"done","action":"done","risk":"none"}'
- id: loopy
  title: Loopy
  prompt: click save until it works
  mock:
    - '{"thought":"click \"Save\"","action":"click","target":4,"risk":"none","expect":"saved"}'
    - '{"thought":"click \"Save\"","action":"click","target":4,"risk":"none","expect":"saved"}'
    - '{"thought":"click \"Save\"","action":"click","target":4,"risk":"none","expect":"saved"}'
"#;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kai-data-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

async fn record(root: &Path) {
    let tasks = parse_tasks(TASKS).unwrap();
    let rec = Recording::new(root, "2026-10-10", "mock", "recorded");
    let runs = run_mock(&tasks, &[], kai_core::Config::default(), Some(&rec), None).await;
    assert_eq!(runs.len(), 2);
}

fn lines(p: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(p)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[tokio::test]
async fn bench_runs_are_recorded_in_the_contract_layout() {
    let root = tmp("layout");
    record(&root).await;
    let dir = root.join("2026-10-10-mock-recorded/good");
    for f in [
        "trace.json",
        "meta.json",
        "system.txt",
        "frames/step-000.jpg",
    ] {
        assert!(dir.join(f).is_file(), "missing {f}");
    }
    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("meta.json")).unwrap()).unwrap();
    assert_eq!(meta["schema"], "kai-trajectory/1");
    assert_eq!(meta["task_id"], "good");
    assert_eq!(meta["source"], "bench");
    assert_eq!(meta["outcome"], "success");
    assert!(meta["failure"].is_null());
    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("trace.json")).unwrap()).unwrap();
    let s0 = &trace["steps"][0];
    assert_eq!(s0["screenshot"], "frames/step-000.jpg");
    assert_eq!(s0["model"], "recorded");
    assert!(
        s0["prompt_user"]
            .as_str()
            .unwrap()
            .contains("list the files")
    );
    assert!(s0["reply_raw"].as_str().unwrap().contains("\"shell\""));
    assert_eq!(s0["prompt_system_hash"].as_str().unwrap().len(), 16);

    let bad: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("2026-10-10-mock-recorded/loopy/meta.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(bad["failure"]["category"], "loop", "{bad}");

    // A second run of the same task never overwrites the first.
    record(&root).await;
    assert!(
        root.join("2026-10-10-mock-recorded/good.2/meta.json")
            .is_file()
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn export_formats_and_stats() {
    let root = tmp("export");
    record(&root).await;
    let out = root.join("out");

    let all = out.join("all.jsonl");
    let n = export::export(&root, &all, Format::All, Filter::default(), &[]).unwrap();
    // good: shell + done; loopy: three clicks (then the mock runs dry).
    assert_eq!(n, 5);
    let rows = lines(&all);
    let r0 = &rows[0];
    assert_eq!(r0["id"], "2026-10-10-mock-recorded/good/0");
    assert_eq!(r0["task_id"], "good");
    assert_eq!(r0["decision"]["action"], "shell");
    assert_eq!(r0["run_outcome"], "success");
    assert!(r0["system"].as_str().unwrap().len() > 100);
    let img = r0["images"][0].as_str().unwrap();
    assert!(
        Path::new(img).is_absolute() && Path::new(img).is_file(),
        "{img}"
    );

    let sft = out.join("sft.jsonl");
    let n = export::export(&root, &sft, Format::Sft, Filter::default(), &[]).unwrap();
    assert_eq!(n, 2, "only the passing run");
    assert!(lines(&sft).iter().all(|r| r["task_id"] == "good"));

    let only = Filter {
        only_success: false,
        min_check: Some(true),
    };
    let n = export::export(&root, &out.join("c.jsonl"), Format::All, only, &[]).unwrap();
    assert_eq!(n, 0, "mock runs have no check");

    let tasks_file = root.join("tasks.yaml");
    std::fs::write(&tasks_file, TASKS).unwrap();
    let grpo = out.join("grpo.jsonl");
    let n = export::export(&root, &grpo, Format::Grpo, Filter::default(), &[tasks_file]).unwrap();
    assert_eq!(n, 2);
    let g = lines(&grpo);
    assert_eq!(g[0]["task_id"], "good");
    assert_eq!(g[0]["check"]["linux"], "true");
    assert_eq!(g[0]["passed"], 1);
    assert_eq!(g[1]["failures"]["loop"], 1);

    let s = stats::stats(&root).unwrap();
    assert_eq!((s.runs, s.passed, s.steps, s.model_calls), (2, 1, 5, 5));
    assert_eq!(s.failures.get("loop"), Some(&1));
    assert!((s.success_rate - 0.5).abs() < 1e-9);
    assert!(stats::text(&s).contains("loop"));
    let _ = std::fs::remove_dir_all(&root);
}
