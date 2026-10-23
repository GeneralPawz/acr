# `acr`: the Claude Code plugin for working on ACR

This plugin is ACR's team kit for Claude Code. It gives fresh agents the project's knowledge and tools up front, so their briefs can be short, they stay on the right workflow, and cheaper models can do the narrow jobs.

| Part | What |
|---|---|
| `acr/agents/` | scoped agents, each with preloaded skills, a tool list and a default model chosen by evals: `scout`, `original-researcher`, `visual-verifier`, `asset-builder`, `sim-engineer`, `render-engineer`, `ui-engineer`, `blender-artist`, `garment-tailor`, `asset-harvester` |
| `acr/skills/` | knowledge loaded on demand or preloaded into agents: `acr-workflow`, `acr-vm`, `acr-driver`, `acr-map`, `acr-sim`, `acr-original`, `acr-render`, `acr-ui`, `acr-blender-pipeline`, `blender-craft`, `acr-garments`, `acr-genai`, `acr-coordinator` |
| `acr/bin/` | commands on every agent's Bash PATH: `acr-vm-cargo` (build, check, test on the render VM), `acr-vm-drive` (GPU driver runs), `acr-shots` (trim and sheet images), `acr-land` (rebase, VM checks, push to main), `acr-showcase` (the user's play build), and `acr-check` (installed on the VM) |
| `acr/mcp/acr_studio.py` | MCP server `acr-studio`: a live headless game on the VM GPU (`open_session`, `send`, `shot`, `fetch_image`, `close_session`) |
| `acr/hooks/` | `guard.py` (PreToolUse on Bash and PowerShell). It blocks local cargo builds in ACR worktrees, running `dmcr.exe`/`vopl.exe`, `xtask assets --help`, force pushes, subagent pushes to main, and `worktree remove --force`. Tests: `python acr/hooks/test_guard.py` |
| `acr/evals/` | known-answer cases and `run_evals.py`, which compares haiku, sonnet and opus per agent. Results: `acr/evals/RESULTS.md` |

## Install (once per machine)

The marketplace is this folder, read from a worktree that only ever holds a landed commit:

```
git -C C:/Users/verwalter/WIP/ACR worktree add --detach C:/Users/verwalter/WIP/ACR-wt/acr-plugin-live origin/main
claude plugin marketplace add C:\Users\verwalter\WIP\ACR-wt\acr-plugin-live\tools\claude-plugin
claude plugin install acr@acr-local --scope user
```

## Update

After a plugin change has landed:
1. Run `git -C C:/Users/verwalter/WIP/ACR-wt/acr-plugin-live checkout --detach origin/main` (after a fetch).
2. In a running session, run `/reload-plugins`; new sessions load the change by themselves.

## Context cost

- **Every session:** about 1.9k tokens (names and descriptions; `claude plugin details acr`). The MCP tools are deferred.
- **A spawned agent:** pays for its preloaded skills, about 4–10k tokens. In return its brief is 5–15 lines, and it doesn't rediscover the workflow, the VM or the docs.

## Why the MCP server is small

Builds, checks and scripted renders are plain commands (`bin/`): cheaper in context than tool schemas, and usable from scripts and hooks. The MCP server covers only what a command cannot do well: a game that stays alive between looks, with trimmed images returned inline.
