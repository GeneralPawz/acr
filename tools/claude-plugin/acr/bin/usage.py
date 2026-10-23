"""Token and cost ledger from Claude Code transcripts (no hooks needed, works backwards).

    acr-usage [--since 2026-10-01] [--by day|week] [--project SUBSTR] [--agents] [--jsonl OUT]

Reads ~/.claude/projects/*/<session>.jsonl (main sessions) and
~/.claude/projects/*/<session>/subagents/agent-*.jsonl (+ .meta.json: agent type), counts each
API message once (by message id), and prices it with the list prices below (estimate; the
CLI's own cost reports are authoritative). Prints runs, tokens and cost per period x agent
type x model, so a change to the agent setup can be compared over fixed time frames.
"""

import argparse
import collections
import datetime as dt
import glob
import json
import os

# USD per million tokens: (input, output). Cache write = input x 1.25 (5 min) or x 2 (1 h);
# cache read = input x 0.1 (x 0.05 on Opus 5.5 / Sonnet 5.5). Haiku 5.5 doubles... see LONG.
PRICES = {
    "claude-haiku-5-5": (0.10, 0.50),
    "claude-sonnet-5-5": (2.0, 10.0),
    "claude-opus-5-5": (4.0, 20.0),
    "claude-fable-5-1": (4.0, 20.0),
}
READ_MULT = {"claude-opus-5-5": 0.05, "claude-sonnet-5-5": 0.05}
LONG = {"claude-haiku-5-5": (100_000, (0.50, 2.50))}  # prompts above this size: whole request at this rate


def price(model, u):
    base = next((v for k, v in PRICES.items() if model and model.startswith(k)), None)
    if not base:
        return 0.0
    cw = u.get("cache_creation") or {}
    w1h = cw.get("ephemeral_1h_input_tokens", 0) or 0
    w5m = (u.get("cache_creation_input_tokens") or 0) - w1h
    rd = u.get("cache_read_input_tokens") or 0
    inp = u.get("input_tokens") or 0
    out = u.get("output_tokens") or 0
    pin, pout = base
    lim = LONG.get(model)
    if lim and inp + w5m + w1h + rd > lim[0]:
        pin, pout = lim[1]
    rm = READ_MULT.get(model, 0.1)
    return (inp * pin + w5m * pin * 1.25 + w1h * pin * 2 + rd * pin * rm + out * pout) / 1e6


def scan(path, kind, agent):
    seen = {}
    try:
        for line in open(path, encoding="utf-8"):
            try:
                r = json.loads(line)
            except ValueError:
                continue
            m = r.get("message")
            if r.get("type") != "assistant" or not isinstance(m, dict) or not m.get("usage"):
                continue
            seen[m.get("id") or id(r)] = (r.get("timestamp", ""), m.get("model"), m["usage"])
    except OSError:
        return []
    return [(ts, kind, agent, model, u) for ts, model, u in seen.values()]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--since", default="")
    ap.add_argument("--by", choices=["day", "week"], default="day")
    ap.add_argument("--project", default="")
    ap.add_argument("--jsonl", default="")
    a = ap.parse_args()
    root = os.path.expanduser("~/.claude/projects")
    rows = []
    for proj in glob.glob(os.path.join(root, "*")):
        if a.project and a.project.lower() not in proj.lower():
            continue
        for f in glob.glob(os.path.join(proj, "*.jsonl")):
            rows += scan(f, "main", "main")
        for f in glob.glob(os.path.join(proj, "*", "subagents", "agent-*.jsonl")):
            meta = f[:-6] + ".meta.json"
            try:
                agent = json.load(open(meta, encoding="utf-8")).get("agentType", "?")
            except (OSError, ValueError):
                agent = "?"
            rows += scan(f, "sub", agent)
    agg = collections.defaultdict(lambda: [0, 0, 0, 0.0, set()])
    out = open(a.jsonl, "w", encoding="utf-8") if a.jsonl else None
    for ts, kind, agent, model, u in rows:
        day = ts[:10]
        if not day or day < a.since:
            continue
        period = day if a.by == "day" else dt.date.fromisoformat(day).strftime("%G-W%V")
        tin = sum(u.get(k) or 0 for k in ("input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"))
        c = price(model, u)
        g = agg[(period, agent, model)]
        g[0] += 1
        g[1] += tin
        g[2] += u.get("output_tokens") or 0
        g[3] += c
        if out:
            out.write(json.dumps({"ts": ts, "kind": kind, "agent": agent, "model": model, "in": tin,
                                  "out": u.get("output_tokens"), "cost": round(c, 5)}) + "\n")
    print(f"{'period':10} {'agent':28} {'model':20} {'calls':>6} {'in Mtok':>8} {'out ktok':>8} {'est $':>8}")
    tot = collections.defaultdict(float)
    for (period, agent, model), (n, tin, tout, c, _) in sorted(agg.items()):
        print(f"{period:10} {agent[:28]:28} {str(model)[:20]:20} {n:6} {tin/1e6:8.2f} {tout/1e3:8.1f} {c:8.2f}")
        tot[period] += c
    for p, c in sorted(tot.items()):
        print(f"TOTAL {p}: ${c:.2f}")


if __name__ == "__main__":
    main()
