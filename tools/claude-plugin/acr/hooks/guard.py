"""PreToolUse guard for ACR work (Bash and PowerShell tools).

Blocks, with a reason the model sees (exit code 2, message on stderr):
- running the original game's executables (dmcr.exe, vopl.exe);
- `cargo xtask assets --help` (it builds every asset);
- local cargo builds, tests, clippy and xtask runs in ACR worktrees: they belong on the
  render VM (`python tools/vm/vm_cargo.py -- ...`), unless the command sets ACR_LOCAL_BUILD=1;
- force pushes and pushes to main from subagents (the coordinator lands branches);
- `git worktree remove --force` (other sessions' worktrees live next to yours).
Everything else passes untouched. Any error in this script lets the command through.
"""

import json
import re
import shlex
import sys

ORIGINAL_EXE = re.compile(r"(?i)(^|[\\/])(dmcr|vopl)\.exe$")
SEGMENT_SPLIT = re.compile(r"&&|\|\||[;|\n]")
LAUNCHERS = {"&", "start", "start-process", "call", "cmd", "/c", "exec", "nohup", "timeout", "env"}
CARGO_HEAVY = {"build", "test", "clippy", "check", "run", "xtask", "bench", "doc", "nextest"}


def tokens(segment):
    """Words of a command segment; quoted words with spaces stay one word, backslashes stay."""
    lex = shlex.shlex(segment.strip(), posix=False)
    lex.whitespace_split = True
    lex.commenters = ""
    try:
        words = list(lex)
    except ValueError:
        words = segment.split()
    return [w.strip("\"'`") for w in words if w.strip("\"'`")]


def first_program(toks):
    for t in toks:
        low = t.lower()
        if low in LAUNCHERS or "=" in t and not t.startswith(("/", "\\")) and ":" not in t.split("=")[0]:
            continue
        if low.startswith("-"):
            continue
        return t
    return ""


def block(msg):
    sys.stderr.write(msg + "\n")
    sys.exit(2)


def main():
    try:
        data = json.load(sys.stdin)
    except Exception:
        return 0
    if data.get("tool_name") not in ("Bash", "PowerShell"):
        return 0
    cmd = (data.get("tool_input") or {}).get("command") or ""
    cwd = data.get("cwd") or ""
    is_subagent = bool(data.get("agent_id") or data.get("agent_type"))
    in_acr = bool(re.search(r"(?i)[\\/]ACR(-wt[\\/][^\\/\s\"']+|[\\/]|$|\s|\")", cwd + " " + cmd)) or "ACR-wt" in cwd

    # Anywhere in the command, also inside a quoted VM command (acr-vm-cargo -- '...').
    if re.search(r"xtask\s+assets\b[^;&|\n]*\s(--help|-h)\b", cmd):
        block("Blocked: `cargo xtask assets --help` builds every asset (also on the VM). Read xtask/src/assets.rs instead.")
    for seg in SEGMENT_SPLIT.split(cmd):
        toks = tokens(seg)
        if not toks:
            continue
        prog = first_program(toks)
        if ORIGINAL_EXE.search(prog) or (toks[0].lower() == "start-process" and any(ORIGINAL_EXE.search(t) for t in toks)):
            block("Blocked: never run the original game (dmcr.exe/vopl.exe). Read it as data instead "
                  "(skill acr-original).")
        if prog.lower().endswith(("cargo", "cargo.exe")) and len(toks) > 1:
            rest = [t.lower() for t in toks[toks.index(prog) + 1:] if not t.startswith("+")]
            sub = rest[0] if rest else ""
            if sub == "xtask" and len(rest) > 2 and rest[1] == "assets" and any(t in ("--help", "-h") for t in rest[2:]):
                block("Blocked: `cargo xtask assets --help` builds every asset. Read xtask/src/assets.rs instead.")
            local = not any(k in cmd for k in ("vm_cargo", "vm-cargo", "vm_drive", "vm-drive")) and "ACR_LOCAL_BUILD=1" not in cmd
            if in_acr and local and sub in CARGO_HEAVY:
                block("Blocked: no local cargo builds/tests in ACR (16 GB ARM laptop, ~10 min builds). Run it on "
                      "the render VM from your worktree root: acr-vm-cargo -- '" + seg.strip() +
                      "' (skill acr-vm). Coordinator-only local builds: prefix ACR_LOCAL_BUILD=1.")
        if prog.lower() in ("git", "git.exe") and len(toks) > 1:
            g = [t.lower() for t in toks[toks.index(prog) + 1:]]
            if g[:1] == ["push"]:
                if any(t in ("-f", "--force", "--force-with-lease") or t.startswith("+") for t in g[1:]):
                    block("Blocked: no force pushes in ACR. Push a new commit instead.")
                targets_main = any(t in ("main", "head:main", "master") or t.endswith(":main") for t in g[1:])
                if is_subagent and targets_main:
                    block("Blocked: subagents push only their own branch (git push origin <branch>). "
                          "The coordinator lands it on main.")
            if g[:2] == ["worktree", "remove"] and any(t in ("--force", "-f") for t in g[2:]):
                block("Blocked: remove worktrees without --force, and only the ones you created.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except SystemExit:
        raise
    except Exception:
        sys.exit(0)
