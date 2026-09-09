#!/usr/bin/env python3
"""Render the public token comparison from a measured summary, never invented data."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("summary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = json.loads(args.summary.read_text())
    assert report["facts"]["passed"] == report["facts"]["assertions"]
    values = [report["baseline_tokens"], report["maximum_tokens"]["l1"], report["maximum_tokens"]["l1_cli_json"]]
    labels = ["Read all source files", "AWR rendered work context", "AWR full CLI JSON"]
    fig, ax = plt.subplots(figsize=(11, 4.7), dpi=160)
    fig.patch.set_facecolor("#f8fafc")
    ax.set_facecolor("#f8fafc")
    bars = ax.barh(labels, values, color=["#64748b", "#087f8c", "#6755a8"], height=.48)
    ax.invert_yaxis()
    ax.set_xlim(0, max(values)*1.21)
    ax.set_xticks([0, 5000, 10000, 15000, 20000], ["0", "5k", "10k", "15k", "20k"])
    ax.set_xlabel("Input tokens · lower is smaller", color="#475569", labelpad=10)
    ax.tick_params(axis="both", length=0, labelsize=10, colors="#334155")
    ax.grid(axis="x", color="#e2e8f0", linewidth=.8)
    ax.set_axisbelow(True)
    for spine in ax.spines.values():
        spine.set_visible(False)
    for bar, value in zip(bars, values):
        suffix = "" if value == values[0] else f"  (−{100*(1-value/values[0]):.1f}%)"
        ax.text(value+200, bar.get_y()+bar.get_height()/2, f"{value:,}{suffix}", va="center", fontsize=10, color="#0f172a", weight="bold")
    fig.text(.045, .92, "Less project context to read", fontsize=19, weight="bold", color="#0f172a")
    fig.text(.045, .855, "150 synthetic tasks · all 39 active tasks measured · largest packet reported", fontsize=11, color="#475569")
    fig.text(.045, .085, "o200k_base · same source corpus · L1 budget: 5,000 tokens", fontsize=10, color="#475569")
    fig.text(.045, .038, "JSON includes CLI metadata. Model output, chat history and MCP framing are excluded; this is not a billing claim.", fontsize=9, color="#475569")
    fig.subplots_adjust(left=.29, right=.92, top=.76, bottom=.22)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    for extension in ("svg", "png"):
        fig.savefig(args.output.with_suffix("."+extension), facecolor=fig.get_facecolor(), metadata={"Creator": "AWR public benchmark"} if extension == "svg" else None)
    svg = args.output.with_suffix(".svg")
    svg.write_text("\n".join(line.rstrip() for line in svg.read_text().splitlines()) + "\n")
    plt.close(fig)


if __name__ == "__main__":
    main()
