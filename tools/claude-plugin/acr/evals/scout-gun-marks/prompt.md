---
name: scout-gun-marks
description: Known-answer scout task (ground truth checked on origin/main e0cfd78, 2026-10-23).
tags: [scout, read-only]
runs: 3
max_turns: 25
allowed_tools: [Read, Grep, Glob, Agent]
---

Use the `acr:scout` agent to answer this, then repeat its answer verbatim.

In the ACR repo at C:\Users\verwalter\WIP\ACR-wt\acr-eval-repo: which source file draws the cannon fire-mode overlays on the ground, and what is the value of the constant for the ground-fire lane length?
