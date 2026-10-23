# Model evals for the acr agents

Each agent role is tried with haiku, sonnet and opus on tasks whose right answer is known. The cheapest model that is reliably right becomes the role's default in its frontmatter. Results: [RESULTS.md](RESULTS.md).

## Two harnesses

1. **Read-only cases** (`evals/<case>/prompt.md` plus `graders/`). These are the scout and original-researcher questions.
   - Run them with `claude plugin eval tools/claude-plugin/acr --case "<glob>" --model <m>`. In this format the prompt names the `acr:<agent>` agent, so the agent's own frontmatter model applies; that makes it a regression suite for the plugin as shipped.
   - The repo snapshot they ask about is the worktree `C:\Users\verwalter\WIP\ACR-wt\acr-eval-repo`, detached at e0cfd78. Recreate it with `git worktree add --detach <path> e0cfd78`.
2. **Agent cases with Bash** (below). These cover coding through the VM and visual checks.
   - Native Windows has no sandbox for `claude plugin eval` with Bash, so the coordinator runs them through the Agent tool: `subagent_type: acr:<agent>`, `model: haiku|sonnet|opus`, one fresh worktree per run.
   - Each run is graded by the listed check. Tokens and time come from the task notification.

## Agent cases

The seed branches are local branches in the main repo, never pushed. Each holds one deliberate bug on top of origin/main e0cfd78.

| Case | Agent | Seed branch | Prompt (a user report, not the answer) | Pass check |
|---|---|---|---|---|
| c1-editor-half | sim-engineer | `eval-c1-base` | "Editor: the H (Halb) button applies too few upgrades for a type with an odd number of them, e.g. 3 of 7; it should be the first half rounded up (4 of 7). Fix it." | the diff restores `ups.div_ceil(2)` (or equivalent); `cargo test -p acr-game editor` passes on the VM; nothing else changed |
| c2-life-upgrade | sim-engineer | `eval-c2-base` | "Editor: placing pikemen at level V (TOUGH +50 % life) leaves the pikemen already standing at 100/150 life instead of 150/150; researched LIFE upgrades in a real game also don't heal existing units any more. Fix it." | the LIFE branch of `apply_unit` again sets existing objects' `max_life` and `life`; `cargo test -p acr-sim --test upgrade` passes |
| r1-crew-flicker | render-engineer | `eval-r1-base` | "While a cannon turns, its crew snaps to the standing pose for one tick between the 22.5° steps (flicker). Find the cause and fix it." | `AnimKind::PStand(1)` maps to `TRAIL_WALK` held again; `cargo test -p acr-game crew_view` passes |
| v1-black-bear | visual-verifier | images `ACR-analysis\evals\bears_new\bears.trim.png` (PASS) and `bears_old\bears.trim.png` (FAIL) | "Criterion: the right-hand bear is a distinct American black bear: mostly black fur, lighter muzzle, not a smaller copy of the brown bear on the left." | PASS for the new image, FAIL for the old one |

Each coding run gets its own worktree from the seed branch: `ACR-wt/eval-<case>-<model>`. Delete these after grading. The VM target dirs named after them can be removed with `rm -rf ~/target/eval-*` on the VM.

**Also graded:** whether the agent stayed in the workflow, i.e. used `acr-vm-cargo` (the guard hook blocks local cargo), did not push to main, and gave a short report.
