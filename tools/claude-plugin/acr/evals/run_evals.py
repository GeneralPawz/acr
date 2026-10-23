"""Runs the acr agents on known-answer cases with several models and grades them.

    python run_evals.py [--cases GLOB] [--models haiku,sonnet,opus] [--runs N] [-j N] [--out DIR]

Each run is a fresh headless session: `claude -p <prompt> --agent acr:<agent> --model <m>
--output-format json`, so the installed acr plugin (agents, skills, hooks, bin/) is what is
tested, and the JSON gives exact tokens, cost, turns and time. Cases are `cases/*.json`:

    {"agent": "scout", "prompt": "...", "cwd": "C:/...",          # read-only cases
     "grade": {"regex_all": ["pat", ...], "regex_none": [...]},
     "budget_usd": 1.0, "timeout_s": 600}
    {"agent": "sim-engineer", "seed": "eval-c1-base", ...,         # coding cases: a fresh
     "grade": {"diff_regex": [...], "vm_test": "cargo test -q -p acr-game editor"}}

A coding case runs in its own worktree `ACR-wt/eval-<case>-<model>-<n>` made from the seed
branch, graded by the diff against the seed and a test run on the VM, then removed.
Results go to `<out>/results.jsonl` and a table in `<out>/summary.md`.
"""

import argparse
import concurrent.futures as cf
import glob
import json
import os
import re
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = "C:/Users/verwalter/WIP/ACR"
WT = "C:/Users/verwalter/WIP/ACR-wt"
VM_CARGO = os.path.join(HERE, "..", "bin", "vm_cargo.py")
TOOLS_RO = ["Read", "Grep", "Glob", "Skill"]
TOOLS_RW = ["Read", "Grep", "Glob", "Skill", "Edit", "Write", "Bash"]


def git(*args, cwd=REPO):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True)


def run_case(case, model, n, out):
    name = case["name"]
    tag = f"{name}-{model}-{n}"
    cwd = case.get("cwd")
    wt = None
    if case.get("seed"):
        wt = f"{WT}/eval-{tag}"
        if os.path.exists(wt):
            git("worktree", "remove", "--force", wt)
        r = git("worktree", "add", "-q", "-b", f"eval-{tag}", wt, case["seed"])
        if r.returncode:
            git("branch", "-D", f"eval-{tag}")
            git("worktree", "add", "-q", "-b", f"eval-{tag}", wt, case["seed"])
        cwd = wt
    prompt = case["prompt"].replace("{worktree}", cwd or "")
    tools = TOOLS_RW if case.get("write") else TOOLS_RO + (["Bash"] if case.get("bash") else [])
    cmd = ["claude", "-p", prompt, "--agent", f"acr:{case['agent']}", "--model", model,
           "--output-format", "json", "--no-session-persistence",
           "--max-budget-usd", str(case.get("budget_usd", 1.0)),
           "--allowedTools", *tools]
    for d in case.get("add_dirs", []):
        cmd += ["--add-dir", d]
    t0 = time.time()
    try:
        p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, encoding="utf-8",
                           timeout=case.get("timeout_s", 600))
        data = json.loads(p.stdout) if p.stdout.strip().startswith("{") else {"result": p.stdout + p.stderr, "is_error": True}
    except subprocess.TimeoutExpired:
        data = {"result": "TIMEOUT", "is_error": True}
    wall = time.time() - t0
    text = data.get("result") or ""
    usage = data.get("usage") or {}
    rec = {
        "case": name, "agent": case["agent"], "model": model, "run": n,
        "cost_usd": data.get("total_cost_usd"), "turns": data.get("num_turns"),
        "wall_s": round(wall, 1), "error": data.get("is_error", False),
        "in_tokens": (usage.get("input_tokens") or 0) + (usage.get("cache_creation_input_tokens") or 0)
        + (usage.get("cache_read_input_tokens") or 0),
        "out_tokens": usage.get("output_tokens"),
    }
    checks = grade(case, text, wt)
    rec["checks"] = checks
    rec["pass"] = bool(checks) and all(checks.values())
    rec["answer"] = text[-1500:]
    with open(os.path.join(out, "results.jsonl"), "a", encoding="utf-8") as f:
        f.write(json.dumps(rec) + "\n")
    if wt and not case.get("keep"):
        git("worktree", "remove", "--force", wt)
        git("branch", "-D", f"eval-{tag}")
    print(f"{'PASS' if rec['pass'] else 'FAIL'} {tag} ${rec['cost_usd']} {rec['wall_s']}s {checks}", flush=True)
    return rec


def grade(case, text, wt):
    g = case.get("grade", {})
    checks = {}
    for pat in g.get("regex_all", []):
        checks[f"has:{pat[:30]}"] = bool(re.search(pat, text))
    for pat in g.get("regex_none", []):
        checks[f"not:{pat[:30]}"] = not re.search(pat, text)
    if wt:
        seed = case["seed"]
        diff = git("diff", seed, cwd=wt).stdout + git("diff", seed, "HEAD", cwd=wt).stdout
        for pat in g.get("diff_regex", []):
            checks[f"diff:{pat[:30]}"] = bool(re.search(pat, diff, re.M))
        files = set(git("diff", "--name-only", seed, cwd=wt).stdout.split())
        if g.get("max_files"):
            checks["files<=%d" % g["max_files"]] = 0 < len(files) <= g["max_files"]
        if g.get("vm_test"):
            r = subprocess.run([sys.executable, VM_CARGO, "--wt", wt, "--", g["vm_test"] + " 2>&1 | grep -E 'test result|FAILED' | tail -3"],
                               capture_output=True, text=True)
            out = r.stdout
            checks["vm_test"] = "test result: ok" in out and "FAILED" not in out
        if g.get("vm_cmd"):
            r = subprocess.run([sys.executable, VM_CARGO, "--wt", wt, "--", g["vm_cmd"]], capture_output=True, text=True)
            checks["vm_cmd"] = bool(re.search(g["vm_ok_regex"], r.stdout + r.stderr, re.M))
        pushed = git("ls-remote", "origin", f"refs/heads/eval-*", cwd=wt).stdout.strip()
        checks["no_push"] = not pushed
    return checks


def summarize(out):
    rows = [json.loads(l) for l in open(os.path.join(out, "results.jsonl"), encoding="utf-8")]
    agg = {}
    for r in rows:
        k = (r["agent"], r["case"], r["model"])
        a = agg.setdefault(k, {"n": 0, "pass": 0, "cost": 0.0, "wall": 0.0, "tok": 0})
        a["n"] += 1
        a["pass"] += r["pass"]
        a["cost"] += r["cost_usd"] or 0
        a["wall"] += r["wall_s"]
        a["tok"] += (r["in_tokens"] or 0) + (r["out_tokens"] or 0)
    lines = ["| agent | case | model | pass | mean cost $ | mean s | mean tokens |", "|---|---|---|---|---|---|---|"]
    for (agent, case, model), a in sorted(agg.items()):
        lines.append(f"| {agent} | {case} | {model} | {a['pass']}/{a['n']} | {a['cost']/a['n']:.3f} | {a['wall']/a['n']:.0f} | {a['tok']//a['n']} |")
    open(os.path.join(out, "summary.md"), "w", encoding="utf-8").write("\n".join(lines) + "\n")
    print("\n".join(lines))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cases", default="*")
    ap.add_argument("--models", default="haiku,sonnet,opus")
    ap.add_argument("--runs", type=int, default=2)
    ap.add_argument("-j", type=int, default=4)
    ap.add_argument("--out", default="C:/Users/verwalter/WIP/ACR-analysis/evals/run")
    ap.add_argument("--summary-only", action="store_true")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    if a.summary_only:
        return summarize(a.out)
    cases = []
    for p in sorted(glob.glob(os.path.join(HERE, "cases", a.cases + ".json"))):
        c = json.load(open(p, encoding="utf-8"))
        c["name"] = os.path.splitext(os.path.basename(p))[0]
        cases.append(c)
    jobs = [(c, m, n) for c in cases for m in a.models.split(",") for n in range(1, a.runs + 1)]
    print(f"{len(jobs)} runs", flush=True)
    with cf.ThreadPoolExecutor(a.j) as ex:
        list(ex.map(lambda j: run_case(*j, a.out), jobs))
    summarize(a.out)


if __name__ == "__main__":
    main()
