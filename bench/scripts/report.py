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
            group.sort(key=lambda r: r["rps"], reverse=True)
            baseline = next((r for r in group if r["target"] == "pixtimize"), None)
            print(f"### {scenario} · {image} · {group[0]['vus']} concurrent clients\n")
            print("| target | req/s | vs pixtimize | p50 ms | p95 ms | p99 ms | errors | avg output KB |")
            print("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |")
            for r in group:
                ratio = f"{r['rps'] / baseline['rps']:.2f}x" if baseline and baseline["rps"] else "-"
                print(
                    f"| {r['target']} | {r['rps']:.1f} | {ratio} | {r['p50_ms']:.1f} | "
                    f"{r['p95_ms']:.1f} | {r['p99_ms']:.1f} | {r['error_rate'] * 100:.2f}% | "
                    f"{r['avg_bytes'] / 1024:.1f} |"
                )
            print()


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "results")
