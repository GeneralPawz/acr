---
name: acr-coordinator
description: "Playbook for the session that coordinates ACR agents. Covers which acr agent and model to pick per job, the short brief template, landing branches with acr-land, updating the showcase, and talking to the user. Load when planning or delegating ACR work."
---

# Coordinating ACR work

You plan, delegate, verify and land. Agents do the work in their own worktrees and push their branch; you land it on main.

## Pick the agent

| Job | Agent (`subagent_type`) | Model | Notes |
|---|---|---|---|
| "Where is X / what does doc Y say" | `acr:scout` | haiku (pinned) | read-only; seconds, about $0.003 |
| "What does the original do" (data files, `.MD`, sprites, disassembly notes) | `acr:original-researcher` | haiku (pinned) | read-only; cites values and addresses; about $0.01 |
| Does a picture show X? Before/after judgement | `acr:visual-verifier` | haiku (pinned); pass `model: sonnet` for subtle criteria or after an UNSURE | live studio (MCP) or `acr-vm-drive`; PASS/FAIL/UNSURE per criterion |
| Rebuild named assets and install them in the showcase | `acr:asset-builder` | haiku (pinned) | mechanical; reports stats deltas |
| Game rules in `acr-sim` (determinism, optional rules, commands) | `acr:sim-engineer` | see evals/RESULTS.md (inherit until decided) | unit tests and acr-check |
| 3D views, gore, decals, particles, physics, performance | `acr:render-engineer` | see evals/RESULTS.md (inherit until decided) | profile numbers and sheets |
| HUD, menus, dialogs, settings, editor, controls | `acr:ui-engineer` | see evals/RESULTS.md (inherit until decided) | driver `ui` and screenshots |
| Models, rigs, clips, gear, crews (Blender) | `acr:blender-artist` | see evals/RESULTS.md (inherit until decided) | VM Blender builds, studio sheets and measures |
| Garments from historical patterns: trace (Waugh etc.), grade, sew, drape, check | `acr:garment-tailor` | inherit (not evaluated yet) | traced JSON stays in `ACR_SOURCES`; drape stats, `model_layers`, studio sheets |
| Anything else | general-purpose | — | brief it with the `acr-workflow` skill name |

The eval results that set these models are in `tools/claude-plugin/acr/evals/RESULTS.md`. When an agent of a cheaper model fails a task, rerun it once with the next model up via the Agent tool's `model` parameter, and note it in RESULTS.md.

## Brief (the agents already know the workflow, the VM, the driver and their domain)

```
Task: <one sentence, the user's words if they matter>
Worktree/branch: <name> (create from origin/main)
Context: <only what is not in the repo or the skills: the user's report, earlier findings, commits>
Done when: <measurable: tests, numbers, a sheet showing X>
Out of scope: <files or areas other agents are working on right now>
Slice: <minutes>
```

Don't restate the workflow, the VM commands, the commit trailers, the never-do list or the paths: the preloaded skills carry them. A brief is 5–15 lines.

## Delegation and budgets

- The engineers and the garment-tailor may call `acr:scout`, `acr:visual-verifier` and `acr:asset-builder` themselves (haiku). The haiku agents can't delegate, so the depth is at most 2.
- **A job that must not exceed a budget:** run it headless with `acr-job <agent> <worktree> <usd> <brief file>`. It applies the `--max-budget-usd` cap and appends one record to `~/.claude/acr-jobs.jsonl`. You get one report at the end; there is no messaging while it runs.
- **Spend over time:** `acr-usage --by week --project American-Conquest` reports per agent type and model from the transcripts. Compare weeks before and after a change to this plugin.

## Land

`acr-land C:/Users/verwalter/WIP/ACR-wt/<name>`:
1. Rebases onto origin/main, aborting on conflicts and refusing leftover markers.
2. Runs `acr-check` on the VM.
3. Pushes HEAD to main only if all five steps pass.

On a conflict: resolve it in the worktree, then run `acr-land` again. Never push with failing checks. Remove only the worktrees you or your agents created, without `--force`.

## Showcase (the user's play build)

- **`acr-showcase`:** builds the Windows exe from origin/main on this PC (the only local build allowed), then installs it as `showcase\AC-showcase.exe`. While the game runs it is staged as `AC-showcase-next.exe` instead.
- **New glbs:** copy them into `showcase\models\` with a backup in `showcase\backup-<topic>\`. Break hard links first.
- **Starting it for the user:** `Start-Showcase.cmd` (sound on, controls=modern).

## Look at results cheaply

- Ask agents for sheets (the studio `sheet` command, or `acr-shots sheet`) and numbers, then look at one sheet, not frame dumps.
- Trim large screenshots with `acr-shots trim` before reading them.
- A question about a picture goes to `acr:visual-verifier` with explicit criteria.

## Talking to the user

- **Reports:** what changed, where (paths, commits), what's open. Plain English; keep the game's German labels as they appear in the game.
- **Before the user acts:** say whether the showcase was swapped or staged.
- **Tracking:** file issues for deferred ideas (`gh issue create`, labels `area: *`, `enhancement`, `bug`, `tracking`).
