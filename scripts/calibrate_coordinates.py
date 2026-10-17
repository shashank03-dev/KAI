#!/usr/bin/env python3
"""Score raw coordinates from a model using the local calibration page.

No model is called. See docs/CALIBRATION.md for the capture protocol.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import struct


VIEWPORTS = ((1280, 800), (1440, 900))
HALF_TARGET = 32
FRAMES = ("pixels", "norm1000", "smart-resize")
SMART_FACTOR = 28
SMART_MIN_PIXELS = 100 * 28 * 28
SMART_MAX_PIXELS = 16384 * 28 * 28


def smart_resize(width, height):
    """Qwen2.5-style dimensions, an unverified hypothesis for Holo3.1."""
    factor = SMART_FACTOR
    w = round(width / factor) * factor
    h = round(height / factor) * factor
    if h * w > SMART_MAX_PIXELS:
        beta = math.sqrt(height * width / SMART_MAX_PIXELS)
        h = math.floor(height / beta / factor) * factor
        w = math.floor(width / beta / factor) * factor
    elif h * w < SMART_MIN_PIXELS:
        beta = math.sqrt(SMART_MIN_PIXELS / (height * width))
        h = math.ceil(height * beta / factor) * factor
        w = math.ceil(width * beta / factor) * factor
    return w, h


def to_pixels(x, y, width, height, frame):
    if x < 0 or y < 0:
        return None
    if frame == "pixels":
        return x, y
    if frame == "norm1000":
        if x > 1000 or y > 1000:
            return None
        return x * width / 1000, y * height / 1000
    if frame == "smart-resize":
        resized_w, resized_h = smart_resize(width, height)
        if x > resized_w or y > resized_h:
            return None
        return x * width / resized_w, y * height / resized_h
    raise ValueError(f"unknown frame: {frame}")


def png_dimensions(path):
    with path.open("rb") as image:
        header = image.read(24)
    if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n" or header[12:16] != b"IHDR":
        raise ValueError(f"not a PNG image: {path}")
    return struct.unpack(">II", header[16:24])


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(128 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_samples(path):
    samples = []
    with path.open(encoding="utf-8") as stream:
        for lineno, line in enumerate(stream, 1):
            if not line.strip():
                continue
            try:
                sample = json.loads(line)
                if not isinstance(sample, dict):
                    raise ValueError("expected object")
                w, h = sample["width"], sample["height"]
                x, y = sample["x"], sample["y"]
                if type(w) is not int or type(h) is not int or (w, h) not in VIEWPORTS:
                    raise ValueError(f"unsupported viewport: {w}x{h}")
                if any(type(v) not in (int, float) or not math.isfinite(v) for v in (x, y)):
                    raise ValueError("coordinates must be finite numbers")
                parsed = {"width": w, "height": h, "x": x, "y": y}
                if "screenshot" in sample:
                    screenshot = (path.parent / sample["screenshot"]).resolve()
                    if png_dimensions(screenshot) != (w, h):
                        raise ValueError(f"screenshot dimensions differ from {w}x{h}")
                    parsed["screenshot_sha256"] = checksum(screenshot)
                samples.append(parsed)
            except (KeyError, TypeError, ValueError, OSError, json.JSONDecodeError) as error:
                raise ValueError(f"{path}:{lineno}: {error}") from error
    if not samples:
        raise ValueError("input contains no predictions")
    return samples


def score(samples, frame):
    by_size = []
    all_errors = []
    total_hits = 0
    for width, height in VIEWPORTS:
        matched = [s for s in samples if (s["width"], s["height"]) == (width, height)]
        errors = []
        hits = 0
        valid = 0
        for sample in matched:
            pixel = to_pixels(sample["x"], sample["y"], width, height, frame)
            if pixel is None:
                errors.append(math.hypot(width, height))
                continue
            valid += 1
            dx = pixel[0] - 0.75 * width
            dy = pixel[1] - 0.60 * height
            errors.append(math.hypot(dx, dy))
            hits += abs(dx) <= HALF_TARGET and abs(dy) <= HALF_TARGET
        all_errors.extend(errors)
        total_hits += hits
        by_size.append({
            "width": width,
            "height": height,
            "trials": len(matched),
            "valid_coordinates": valid,
            "hits": hits,
            "mean_error_px": round(sum(errors) / len(errors), 3) if errors else None,
        })
    return {
        "frame": frame,
        "hits": total_hits,
        "mean_error_px": round(sum(all_errors) / len(all_errors), 3),
        "by_viewport": by_size,
    }


def evaluate(samples, model, input_sha256):
    scores = sorted((score(samples, f) for f in FRAMES), key=lambda s: s["mean_error_px"])
    best, runner_up = scores[:2]
    enough = all(
        sum((s["width"], s["height"]) == size for s in samples) >= 10
        for size in VIEWPORTS
    )
    sufficient_hits = all(
        row["trials"] >= 10 and row["hits"] / row["trials"] >= 0.9
        for row in best["by_viewport"]
    )
    separated = runner_up["mean_error_px"] - best["mean_error_px"] >= 10
    accepted = enough and sufficient_hits and best["mean_error_px"] <= 16 and separated
    status = "calibrated" if accepted else "insufficient_data" if not enough else "ambiguous"
    reason = (
        "frame meets hit-rate, error and separation thresholds"
        if accepted else "requires at least 10 trials at each viewport"
        if not enough else "accuracy or candidate separation threshold not met"
    )
    return {
        "status": status,
        "selected_frame": best["frame"] if accepted else None,
        "reason": reason,
        "model": model,
        "input_sha256": input_sha256,
        "fixture": "bench/fixtures/site/calibration.html",
        "target": {"width": 64, "height": 64, "center_x_fraction": 0.75, "center_y_fraction": 0.60},
        "smart_resize_assumption": {
            "factor": SMART_FACTOR, "min_pixels": SMART_MIN_PIXELS,
            "max_pixels": SMART_MAX_PIXELS,
        },
        "scores": scores,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True, help="one JSON object per raw reply")
    parser.add_argument("--model", required=True, help="exact served model identifier")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = evaluate(load_samples(args.input), args.model, checksum(args.input))
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    except (OSError, ValueError) as error:
        parser.exit(2, f"calibration failed: {error}\n")
    print(f"{result['status']}: {result['reason']}; report: {args.out}")
    return 0 if result["status"] == "calibrated" else 1


if __name__ == "__main__":
    raise SystemExit(main())
