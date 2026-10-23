"""Runs one acr agent job headless, with a hard cost cap and an exact cost record.

    acr-job <agent> <worktree> <budget_usd> <brief file | -> [--model M] [--timeout S] [--studio]

`claude -p <brief> --agent acr:<agent> --max-budget-usd <budget>` in the worktree, with the
agent's own model unless --model is given. At the cap the run stops (the request in flight
finishes, so leave headroom). Prints the agent's final report, then one JSON line with cost,
tokens, turns and time; the same line is appended to ~/.claude/acr-jobs.jsonl for
`acr-usage`-style comparisons. --studio keeps the acr-studio MCP server (live game);
otherwise no MCP servers are started.

Use it instead of the Agent tool when a job must not exceed a budget; the trade-off is no
messaging with the agent while it runs (it reports once, at the end).
"""

import argparse
import datetime as dt
import json
import os
import subprocess
import sys
import time

TOOLS = ["Read", "Grep", "Glob", "Skill", "Edit", "Write", "Bash", "Agent"]
STUDIO = [f"mcp__plugin_acr_acr-studio__{t}" for t in ("open_session", "send", "shot", "fetch_image", "close_session")]
LEDGER = os.path.expanduser("~/.claude/acr-jobs.jsonl")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("agent")
    ap.add_argument("worktree")
    ap.add_argument("budget_usd", type=float)
    ap.add_argument("brief")
    ap.add_argument("--model", default="")
    ap.add_argument("--timeout", type=int, default=5400)
    ap.add_argument("--studio", action="store_true")
    a = ap.parse_args()
    brief = sys.stdin.read() if a.brief == "-" else open(a.brief, encoding="utf-8").read()
    cmd = ["claude", "-p", brief, "--agent", f"acr:{a.agent}", "--output-format", "json",
           "--max-budget-usd", str(a.budget_usd), "--allowedTools", *TOOLS, *(STUDIO if a.studio else [])]
    if not a.studio:
        cmd.append("--strict-mcp-config")
    if a.model:
        cmd += ["--model", a.model]
    t0 = time.time()
    try:
        p = subprocess.run(cmd, cwd=a.worktree, capture_output=True, text=True, encoding="utf-8", timeout=a.timeout)
        data = json.loads(p.stdout) if p.stdout.strip().startswith("{") else {"result": p.stdout + p.stderr, "is_error": True}
    except subprocess.TimeoutExpired:
        data = {"result": "TIMEOUT", "is_error": True}
    u = data.get("usage") or {}
    rec = {
        "ts": dt.datetime.now().isoformat(timespec="seconds"), "agent": a.agent, "model": a.model or "default",
        "worktree": a.worktree, "budget_usd": a.budget_usd, "cost_usd": data.get("total_cost_usd"),
        "turns": data.get("num_turns"), "wall_s": round(time.time() - t0), "error": data.get("is_error", False),
        "subtype": data.get("subtype"), "models": list((data.get("modelUsage") or {}).keys()),
        "in_tokens": sum(u.get(k) or 0 for k in ("input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens")),
        "out_tokens": u.get("output_tokens"),
    }
    with open(LEDGER, "a", encoding="utf-8") as f:
        f.write(json.dumps(rec) + "\n")
    print(data.get("result", ""))
    print(json.dumps(rec))
    return 1 if rec["error"] else 0


if __name__ == "__main__":
    sys.exit(main())
