#!/usr/bin/env python3
"""Summarize xctrace metal-gpu-intervals for complete synthetic benchmark frames.

Export the table first:
  xcrun xctrace export --input run.trace --xpath \
    '/trace-toc/run[@number="1"]/data/table[@schema="metal-gpu-intervals"]' \
    --output intervals.xml
Then pass --pid from the trace's target metadata. Other processes are discarded.
GPU channel intervals overlap; their union, not their sum, measures covered time.
This is GPU stage coverage, not shader cycles, power, or utilization percentage.
"""
import argparse
import bisect
import collections
import gzip
import json
import statistics
import xml.etree.ElementTree as ET
from pathlib import Path


def read_intervals(path, pid):
    ids, result = {}, []
    for _, element in ET.iterparse(path, events=("end",)):
        if "id" in element.attrib:
            ids[element.attrib["id"]] = (element.text, element.attrib.get("fmt", ""))
        if element.tag != "row":
            continue
        columns = list(element)

        def value(item):
            return ids.get(item.attrib.get("ref", ""), (item.text, item.attrib.get("fmt", "")))

        if f"({pid})" in value(columns[10])[1]:
            result.append({
                "start": int(value(columns[0])[0] or 0),
                "duration": int(value(columns[1])[0] or 0),
                "channel": value(columns[2])[1],
                "depth": value(columns[5])[0],
                "label": value(columns[6])[1].split("  (")[0],
                "command": value(columns[15])[0],
            })
        element.clear()
    return result


def union_ms(intervals):
    end, total = 0, 0
    for a, b in sorted((row["start"], row["start"] + row["duration"]) for row in intervals):
        if a >= end:
            total += b - a
            end = b
        elif b > end:
            total += b - end
            end = b
    return total / 1_000_000


def distribution(values):
    values = sorted(values)
    if not values:
        return None
    return {"median": statistics.median(values), "p95": values[round(.95 * (len(values) - 1))],
            "min": values[0], "max": values[-1]}


def summarize(intervals, surface_count, warmup):
    # Depth is a timeline placement lane; valid commands may exist only on a
    # higher lane. Union every interval so overlaps are not double-counted.
    starts = {}
    for row in intervals:
        if row["label"].startswith("main_pass:"):
            starts[row["command"]] = min(row["start"], starts.get(row["command"], row["start"]))
    starts = sorted(starts.values())
    groups = [[] for _ in starts]
    for row in intervals:
        if row["label"].startswith(("main_pass:", "glass-", "after_backdrop_filter:", "scene_blit:")):
            index = bisect.bisect_right(starts, row["start"]) - 1
            if index >= 0:
                groups[index].append(row)
    frames = []
    for rows in groups:
        composites = len({row["command"] for row in rows if row["label"].startswith("glass-composite:")})
        baseline_only = all(row["label"].startswith(("main_pass:", "scene_blit:")) for row in rows)
        kind = "effect" if composites == surface_count else "baseline" if composites == 0 and baseline_only else "incomplete"
        start = min(row["start"] for row in rows)
        frames.append({"kind": kind, "start_s": start / 1e9, "composites": composites,
                       "busy_ms": union_ms(rows),
                       "span_ms": (max(row["start"] + row["duration"] for row in rows) - start) / 1e6,
                       "channels": {channel: union_ms([row for row in rows if row["channel"] == channel])
                                    for channel in {row["channel"] for row in rows}},
                       "labels": {label: union_ms([row for row in rows if row["label"].split(":")[0] == label])
                                  for label in {row["label"].split(":")[0] for row in rows}}})
    summary = {}
    for kind in ("effect", "baseline"):
        selected = [frame for frame in frames if frame["kind"] == kind][warmup:]
        summary[kind] = {"count": len(selected)}
        for key in ("busy_ms", "span_ms"):
            summary[kind][key] = distribution(frame[key] for frame in selected)
        for channel in ("Vertex", "Fragment", "Compute"):
            summary[kind][channel] = distribution(frame["channels"].get(channel, 0) for frame in selected)
        summary[kind]["labels"] = {
            label: distribution(frame["labels"].get(label, 0) for frame in selected)
            for label in sorted({label for frame in selected for label in frame["labels"]})
        }
    return {"method": "Union of actual Instruments GPU stage intervals per complete synthetic frame; channels overlap. Excludes PendingWrites/Transit and native timing markers. Incomplete frames discarded. Profiling overhead and active desktop apply. This is not a power or GPU utilization measurement.",
            "warmup_frames_per_kind": warmup, "surface_count": surface_count,
            "frame_counts": dict(collections.Counter(frame["kind"] for frame in frames)),
            "summary": summary, "frames": frames}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--surfaces", type=int, default=48)
    parser.add_argument("--warmup", type=int, default=13)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--retain-intervals", type=Path, help="Optional gzip JSON of only this PID's rows")
    args = parser.parse_args()
    intervals = read_intervals(args.input, args.pid)
    if args.retain_intervals:
        with gzip.open(args.retain_intervals, "wt") as output:
            json.dump(intervals, output, separators=(",", ":"))
    report = summarize(intervals, args.surfaces, args.warmup)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"frame_counts": report["frame_counts"], "summary": report["summary"]}, indent=2))


if __name__ == "__main__":
    main()
