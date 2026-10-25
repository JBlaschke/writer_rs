#!/usr/bin/env python3
"""Recompute the task waves from the task files' "Depends on" rows.

Usage: python3 scripts/waves.py [docs/tasks]

Reads every docs/tasks/Txx-*.md, takes the `| Depends on | … |` row of its header table,
assigns wave = 0 for tasks without dependencies and 1 + max(wave of dependencies) otherwise,
checks that no task depends on a task in its own wave (by construction) and that every
dependency exists, and prints the wave table in the format docs/tasks/README.md uses.
"""
import re
import sys
from collections import defaultdict
from pathlib import Path

root = Path(sys.argv[1] if len(sys.argv) > 1 else "docs/tasks")
deps = {}
for path in sorted(root.glob("T*-*.md")):
    tid = path.name.split("-", 1)[0]
    text = path.read_text(encoding="utf-8")
    m = re.search(r"^\|\s*Depends on\s*\|\s*(.*?)\s*\|\s*$", text, re.M)
    if not m:
        sys.exit(f"{path}: no 'Depends on' row")
    cell = m.group(1)
    ids = re.findall(r"\bT[0-9A-F]{2}\b", cell)
    # expand ranges like T10–T15 / T10-T15
    for a, b in re.findall(r"\b(T[0-9A-F]{2})\s*[–-]\s*(T[0-9A-F]{2})\b", cell):
        lo, hi = int(a[1:], 16), int(b[1:], 16)
        ids += [f"T{n:02X}" for n in range(lo, hi + 1)]
    deps[tid] = sorted(set(i for i in ids if i != tid))

missing = {d for ds in deps.values() for d in ds if d not in deps}
if missing:
    sys.exit(f"unknown dependencies: {sorted(missing)}")

wave = {}
def w(t, stack=()):
    if t in stack:
        sys.exit(f"dependency cycle: {' -> '.join(stack + (t,))}")
    if t not in wave:
        wave[t] = 0 if not deps[t] else 1 + max(w(d, stack + (t,)) for d in deps[t])
    return wave[t]

for t in deps:
    w(t)
by_wave = defaultdict(list)
for t, v in wave.items():
    by_wave[v].append(t)
print("| wave | tasks | parallelism |")
print("|---|---|---|")
for v in sorted(by_wave):
    tasks = sorted(by_wave[v])
    print(f"| {v} | {', '.join(tasks)} | {len(tasks)} |")
