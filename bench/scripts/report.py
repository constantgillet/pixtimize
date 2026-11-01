#!/usr/bin/env python3
"""Turns bench/results/*.json into a Markdown comparison table."""

import json
import sys
from pathlib import Path


def main(results_dir: str) -> None:
    rows = [json.loads(p.read_text()) for p in sorted(Path(results_dir).glob("*.json"))]
    if not rows:
        sys.exit("no results found")

    for scenario in ("cold", "warm"):
        for image in sorted({r["image"] for r in rows}):
            group = [r for r in rows if r["scenario"] == scenario and r["image"] == image]
            if not group:
                continue
            for r in group:
                # Failed requests are often fast; only count the ones that returned an image.
                r["ok_rps"] = r["rps"] * (1 - r["error_rate"])
            group.sort(key=lambda r: r["ok_rps"], reverse=True)
            baseline = next((r for r in group if r["target"] == "pixtimize"), None)
            print(f"### {scenario} · {image} · {group[0]['vus']} concurrent clients\n")
            print("| target | ok req/s | vs pixtimize | p50 ms | p95 ms | p99 ms | errors | output KB |")
            print("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |")
            for r in group:
                ratio = f"{r['ok_rps'] / baseline['ok_rps']:.2f}x" if baseline and baseline["ok_rps"] else "-"
                errors = f"{r['error_rate'] * 100:.2f}%" + (" ⚠️" if r["error_rate"] > 0.01 else "")
                # Cold runs mix many widths, so only warm sizes are comparable.
                size = f"{r['avg_bytes'] / 1024:.1f}" if scenario == "warm" else "-"
                print(
                    f"| {r['target']} | {r['ok_rps']:.1f} | {ratio} | {r['p50_ms']:.1f} | "
                    f"{r['p95_ms']:.1f} | {r['p99_ms']:.1f} | {errors} | {size} |"
                )
            print()


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "results")
