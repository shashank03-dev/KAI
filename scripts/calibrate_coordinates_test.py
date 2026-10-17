"""Focused tests for score calculations; synthetic data is never a measurement."""

import json
from pathlib import Path
import struct
import tempfile
import unittest

import calibrate_coordinates as calibration


def samples_for(frame, count=10):
    samples = []
    for w, h in calibration.VIEWPORTS:
        x, y = w * 0.75, h * 0.60
        if frame == "norm1000":
            x, y = 750, 600
        elif frame == "smart-resize":
            rw, rh = calibration.smart_resize(w, h)
            x, y = rw * 0.75, rh * 0.60
        samples += [{"width": w, "height": h, "x": x, "y": y}] * count
    return samples


class CalibrationTest(unittest.TestCase):
    def test_normalized_predictions_select_norm1000(self):
        report = calibration.evaluate(samples_for("norm1000"), "synthetic", "test")
        self.assertEqual(report["status"], "calibrated")
        self.assertEqual(report["selected_frame"], "norm1000")
        self.assertEqual(report["scores"][0]["hits"], 20)

    def test_pixel_and_smart_resized_points_do_not_overclaim(self):
        for frame in ("pixels", "smart-resize"):
            report = calibration.evaluate(samples_for(frame), "synthetic", "test")
            self.assertEqual(report["status"], "ambiguous")
            self.assertIsNone(report["selected_frame"])

    def test_short_runs_are_not_calibrated(self):
        report = calibration.evaluate(samples_for("norm1000", count=9), "synthetic", "test")
        self.assertEqual(report["status"], "insufficient_data")

    def test_screenshot_dimensions_and_nonfinite_rejected(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            png = root / "screen.png"
            png.write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00\x00\x00\rIHDR" + struct.pack(">II", 100, 100))
            raw = root / "raw.jsonl"
            raw.write_text(json.dumps({"width": 1280, "height": 800, "x": 750,
                                       "y": 600, "screenshot": "screen.png"}) + "\n")
            with self.assertRaisesRegex(ValueError, "dimensions differ"):
                calibration.load_samples(raw)
            raw.write_text('{"width":1280,"height":800,"x":NaN,"y":600}\n')
            with self.assertRaisesRegex(ValueError, "finite"):
                calibration.load_samples(raw)


if __name__ == "__main__":
    unittest.main()
