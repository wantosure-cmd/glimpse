"""Summarize measured diagnostics without equating UI callbacks with visible frames."""
import json
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parent.parent
label = sys.argv[1] if len(sys.argv) > 1 else "cold"

def load(suffix):
    return json.loads((root / "artifacts" / f"p2-scroll-{label}{suffix}.json").read_text(encoding="utf-8-sig"))

ui, process = load(""), load("-process")
assert process["exit_code"] == 0, process
assert not ui["error"], ui["error"]
expected_media = int(sys.argv[2]) if len(sys.argv) > 2 else 5000
assert ui["media"] == expected_media
samples = ui["samples"]
assert len(samples) >= 120
assert samples[-1]["elapsed_ms"] >= ui["requested_seconds"] * 1000
assert max(s["scroll_y"] for s in samples) > 1000, "No meaningful scrolling"
physical = [[round(s["viewport_points"][i] * s["pixels_per_point"]) for i in (0, 1)] for s in samples]
summary = {
    "label": label,
    "exe_sha256": process["exe_sha256"],
    "scope": ui["scope"],
    "media": ui["media"],
    "seconds": samples[-1]["elapsed_ms"] / 1000,
    "ui_callback_samples": len(samples),
    "callback_interval_p50_ms": ui["interval_p50_ms"],
    "callback_interval_p95_ms": ui["interval_p95_ms"],
    "callback_interval_p99_ms": ui["interval_p99_ms"],
    "physical_viewport_sizes": sorted({tuple(p) for p in physical}),
    "scale_factors": sorted({s["pixels_per_point"] for s in samples}),
    "peak_working_set_mib": process["peak_working_set_bytes"] / 1024**2,
    "peak_texture_mib": max(s["texture_bytes"] for s in samples) / 1024**2,
    "max_scroll_points": max(s["scroll_y"] for s in samples),
    "last_decoded": samples[-1]["decoded"],
    "last_cache_hits": samples[-1]["cache_hits"],
    "max_preview_failures": max(s["failures"] for s in samples),
    "memory_target_600mib_met": process["peak_working_set_bytes"] <= 600 * 1024**2,
    "callback_interval_target_20ms_met": ui["interval_p95_ms"] <= 20,
    "four_k_viewport": all(p[0] >= 3840 and p[1] >= 2160 for p in physical),
    "display_presentation_and_physical_input": "not measured",
}
path = root / "artifacts" / f"p2-scroll-{label}-summary.json"
path.write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf8")
print(json.dumps(summary, ensure_ascii=False, indent=2))
