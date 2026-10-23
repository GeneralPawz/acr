---
name: orig-mortar-shell
description: Known-answer original-researcher task (ground truth checked on origin/main e0cfd78, 2026-10-23).
tags: [original-researcher, read-only]
runs: 3
max_turns: 25
allowed_tools: [Read, Grep, Glob, Agent]
---

Use the `acr:original-researcher` agent to answer this, then repeat its answer verbatim.

Does the original game's WEAPON.NDS define a mortar shell projectile that no unit type uses? Name it, its launch angle, and which weapon entry fires it. Data: C:\Users\verwalter\WIP\ACR-analysis\gamedata.
