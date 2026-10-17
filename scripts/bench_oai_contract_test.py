"""Synthetic-only tests for command planning and result integrity gates."""

from argparse import Namespace
import json
from pathlib import Path
import tempfile
import unittest

import bench_oai_contract as bench


class PairedBenchTest(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.args = Namespace(
            root=self.root, backend="vllm", model="gemma-4-26b-a4b-it",
            frame="marks", only=["fill-form", "web-form-submit"], rounds=2,
            experiment="test", tasks=bench.ROOT / "bench/tasks.yaml",
            kai_bin="kai", browser=False, headless=False, yes=False, calibration=None,
        )

    def recorded_experiment(self):
        plan = bench.build_plan(self.args)
        root = Path(plan["experiment_dir"])
        for run in plan["runs"]:
            run["status"] = "done"
            run["returncode"] = 0
            output_dir = root / run["label"]
            output_dir.mkdir(parents=True, exist_ok=True)
            result = {
                "backend": self.args.backend,
                "model": self.args.model,
                "machine": {"os": "linux", "cpu": "test", "cores": 4,
                            "display": "test", "git_commit": "synthetic"},
                "ab": {"flags": {"oai_contract": "marks" if run["condition"] == "contract" else None},
                       "applied": True},
                "total": 2, "skipped": 0,
                "tasks": [
                    {"id": "fill-form", "pass": True, "steps": 3, "model_calls": 4, "seconds": 10.0},
                    {"id": "web-form-submit", "pass": False, "steps": 8, "model_calls": 9, "seconds": 30.0},
                ],
            }
            (output_dir / "result.json").write_text(json.dumps(result))
        bench.write_json(root / "manifest.json", plan)
        return root

    def test_plan_is_balanced_isolated_and_read_only(self):
        plan = bench.build_plan(self.args)
        self.assertEqual([r["label"] for r in plan["runs"]],
                         ["baseline-1", "contract-1", "baseline-2", "contract-2"])
        self.assertFalse(Path(plan["experiment_dir"]).exists())
        for run in plan["runs"]:
            self.assertEqual(run["command"].count("--only"), 2)
            self.assertEqual("--oai-contract" in run["command"], run["condition"] == "contract")
            self.assertIn("--no-record", run["command"])

    def test_pixel_run_needs_calibration(self):
        self.args.frame = "norm1000"
        with self.assertRaisesRegex(ValueError, "calibration"):
            bench.build_plan(self.args)

    def test_comparison_reads_all_four_measurement_files(self):
        result = bench.compare_experiment(self.recorded_experiment())
        self.assertEqual(result["baseline"]["passed"], 2)
        self.assertEqual(result["contract"]["total"], 4)
        self.assertEqual(result["baseline"]["mean_steps"], 5.5)
        self.assertEqual(result["contract"]["mean_model_calls"], 6.5)
        self.assertIsNone(result["contract"]["misclicks"])
        self.assertIn("unavailable", bench.markdown_report(result))

    def test_missing_or_mislabelled_results_cannot_be_reported(self):
        root = self.recorded_experiment()
        (root / "baseline-2/result.json").unlink()
        with self.assertRaisesRegex(ValueError, "one Kai result JSON"):
            bench.compare_experiment(root)
        root = self.recorded_experiment()
        path = root / "contract-1/result.json"
        result = json.loads(path.read_text())
        result["ab"]["flags"]["oai_contract"] = None
        path.write_text(json.dumps(result))
        with self.assertRaisesRegex(ValueError, "wrong --oai-contract"):
            bench.compare_experiment(root)

    def test_skipped_tasks_cannot_be_silently_dropped(self):
        root = self.recorded_experiment()
        path = root / "contract-1/result.json"
        result = json.loads(path.read_text())
        result["skipped"] = 1
        path.write_text(json.dumps(result))
        with self.assertRaisesRegex(ValueError, "skipped tasks"):
            bench.compare_experiment(root)

    def test_stopped_runs_are_reported_as_invalid_comparisons(self):
        root = self.recorded_experiment()
        path = root / "contract-1/result.json"
        result = json.loads(path.read_text())
        result["tasks"][0]["outcome"] = "stopped"
        path.write_text(json.dumps(result))
        comparison = bench.compare_experiment(root)
        self.assertFalse(comparison["valid_comparison"])
        self.assertIn("contract-1: fill-form ended stopped", comparison["interruptions"])
        self.assertIn("invalid due to interrupted", bench.markdown_report(comparison))


if __name__ == "__main__":
    unittest.main()
