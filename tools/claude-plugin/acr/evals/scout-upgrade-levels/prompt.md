---
name: scout-upgrade-levels
description: Known-answer scout task (ground truth checked on origin/main e0cfd78, 2026-10-23).
tags: [scout, read-only]
runs: 3
max_turns: 25
allowed_tools: [Read, Grep, Glob, Agent]
---

Use the `acr:scout` agent to answer this, then repeat its answer verbatim.

In the ACR repo at C:\Users\verwalter\WIP\ACR-wt\acr-eval-repo: which functions implement the editor's B/H/V upgrade levels (applying none, the first half, or all of a unit type's upgrades), and in which files?
