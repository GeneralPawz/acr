"""Cases for guard.py: python test_guard.py (prints one line per case, exits 1 on a miss)."""

import json
import os
import subprocess
import sys

GUARD = os.path.join(os.path.dirname(os.path.abspath(__file__)), "guard.py")
WT = "C:/Users/verwalter/WIP/ACR-wt/m2-x"
GAME = "C:/Users/verwalter/WIP/American Conquest"
CASES = [
    ("Bash", WT, "cargo test -q -p acr-sim", {}, 2),
    ("Bash", WT, "acr-vm-cargo -- cargo test -q -p acr-sim", {}, 0),
    ("Bash", WT, "python tools/vm/vm_cargo.py -- cargo test -q -p acr-sim", {}, 0),
    ("Bash", GAME, "cd /c/Users/verwalter/WIP/ACR-wt/x && export PATH=$HOME/.cargo/bin:$PATH && cargo clippy --workspace", {}, 2),
    ("Bash", "C:/x", "cd /c/Users/verwalter/WIP/ACR-wt/x && ACR_LOCAL_BUILD=1 cargo build --release", {}, 0),
    ("PowerShell", "C:/x", '& "' + GAME + '/dmcr.exe"', {}, 2),
    ("Bash", "C:/x", "cd /c/x && ./vopl.exe -x", {}, 2),
    ("Bash", "C:/x", "strings dmcr.exe | head", {}, 0),
    ("Bash", "C:/x", "python read_md.py " + GAME + "/dmcr.exe", {}, 0),
    ("Bash", "C:/x", "cargo xtask assets --help", {}, 2),
    ("Bash", WT, "acr-vm-cargo -- 'cargo xtask assets --help 2>&1 | head'", {}, 2),
    ("Bash", WT, "acr-vm-cargo -- cargo xtask assets artpus", {}, 0),
    ("Bash", WT, "git push origin HEAD:main", {"agent_id": "a1", "agent_type": "acr:sim-engineer"}, 2),
    ("Bash", WT, "git push origin m2-x", {"agent_id": "a1", "agent_type": "acr:sim-engineer"}, 0),
    ("Bash", WT, "git push origin HEAD:main", {}, 0),
    ("Bash", WT, "git push -f origin x", {}, 2),
    ("Bash", WT, "cargo fmt --all --check", {}, 0),
    ("Bash", "C:/Users/verwalter/WIP/other", "cargo build", {}, 0),
    ("Bash", WT, "acr-showcase", {}, 0),
    ("Bash", WT, "git worktree remove --force ../x", {}, 2),
    ("Read", WT, "cargo test", {}, 0),
]


def main():
    bad = 0
    for tool, cwd, cmd, extra, want in CASES:
        data = {"tool_name": tool, "cwd": cwd, "tool_input": {"command": cmd}, **extra}
        r = subprocess.run([sys.executable, GUARD], input=json.dumps(data), capture_output=True, text=True)
        ok = r.returncode == want
        bad += not ok
        print("ok " if ok else "BAD", r.returncode, cmd[:70], "|", r.stderr.strip()[:60])
    print("misses:", bad)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
