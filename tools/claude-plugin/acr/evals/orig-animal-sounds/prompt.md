---
name: orig-animal-sounds
description: Known-answer original-researcher task (ground truth checked on origin/main e0cfd78, 2026-10-23).
tags: [original-researcher, read-only]
runs: 3
max_turns: 25
allowed_tools: [Read, Grep, Glob, Agent]
---

Use the `acr:original-researcher` agent to answer this, then repeat its answer verbatim.

In the original game data at C:\Users\verwalter\WIP\ACR-analysis\gamedata, do any animal unit definitions (ANM*.MD) define sounds (SOUND lines)? Answer yes or no with evidence.
