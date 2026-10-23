---
name: acr-workflow
description: "Mandatory working rules for any change in the ACR repo (American Conquest Rust rewrite, GeneralPawz/acr). Covers worktrees, building and testing on the render VM instead of this PC, commits, pushing, reports and the hard never-do list. Load before touching ACR code, assets or docs."
---

# Working on ACR

ACR re-implements *American Conquest* 1.46 (German edition) in Rust from the original `dmcr.exe`. Milestone 2 adds realistic 3D models, animation, gore and optional realism rules on top. The original's behaviour stays the default.

## Places

| What | Where |
|---|---|
| Repo (main checkout, never work in it) | `C:\Users\verwalter\WIP\ACR` |
| Your worktree | `C:\Users\verwalter\WIP\ACR-wt\<name>` |
| Original game (read-only data) | `C:\Users\verwalter\WIP\American Conquest` |
| Analysis output (frames, sheets, scripts) | `C:\Users\verwalter\WIP\ACR-analysis\<topic>\` |
| Showcase (the user's play build) | `C:\Users\verwalter\WIP\ACR-analysis\showcase\` |
| Bought or downloaded model sources | `D:\ACR-models\`; on the VM `~/sources` via `ACR_SOURCES` |

## Start

```
git -C C:/Users/verwalter/WIP/ACR fetch -q origin
git -C C:/Users/verwalter/WIP/ACR worktree add C:/Users/verwalter/WIP/ACR-wt/<name> -b <name> origin/main
```

If `<name>` already exists, it belongs to someone else: pick another name. Never remove worktrees or branches you did not create.

## Build, test and render on the VM, not here

This PC is a 16 GB ARM laptop shared by many agents; a local release build takes about 10 minutes. The render VM does it in about 1 minute. Load the `acr-vm` skill for the commands. In short, from your worktree root:

- checks: `acr-vm-cargo -- acr-check`
- one test: `acr-vm-cargo -- cargo test -q -p acr-sim --test plough`
- assets: `acr-vm-cargo -- cargo xtask assets <name>`
- driver and screenshots: `acr-vm-drive --steps <json> --out <abs dir>`

Never create a local `ACR-target-*` dir, and never run local `cargo build`, `test` or `clippy`. A hook blocks them.

## Rules of the code

- Match the original's behaviour, and cite the `dmcr.exe` addresses you followed in the docs.
- New gameplay goes behind an optional rule, so the original stays the default. The pinned state hashes must not change with the rules off.
- Keep the browser build compiling: the wasm clippy check is part of `acr-check`.
- Write in the surrounding style, and update the doc that covers what you changed (`docs/*.md`).

## Finish

1. Run `acr-vm-cargo -- acr-check`. It must print `PASS` for fmt, clippy, clippy-wasm, test and blender-tests.
2. Commit with these trailer lines:
   ```
   Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>
   Claude-Session: https://claude.ai/code/session_012ZCjaLgo6m2LwGaz9EvcRE
   ```
   Use the model and session from your own context if they differ.
3. `git fetch origin && git rebase origin/main`, then re-run the checks, then `git push origin <name>`.
   - Push only your branch: no PR, never main, never force-push, never amend pushed commits. The coordinator lands it.
4. **Report:**
   - what changed, with file paths;
   - the evidence: check results, numbers, sheet paths;
   - what is open, most important first.

   Keep it short. Paths are absolute.

## Never

- Run `dmcr.exe` or `vopl.exe` (the original). Reading them as data is fine.
- Commit original game data, or bought or downloaded model sources (WildMesh, Sketchfab zips).
- Run `cargo xtask assets --help`: it builds every asset.
- Use relative paths in render or analysis tools. They once created stray folders at `C:\`.
- Run the game's exe from `target/` directly, or open a visible game window to test.
- Spawn further agents.
- Delete user data permanently.

## Time

Work in slices of about 45–75 minutes. When the slice is used up: commit, push your branch, and report what is left, rather than running on.
