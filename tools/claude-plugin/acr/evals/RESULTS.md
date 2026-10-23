# Eval results

Model per agent role, chosen from known-answer runs (`run_evals.py`, cases in `cases/`). Each run is a fresh `claude -p --agent acr:<role> --model <m>` session with the installed plugin. The cost is the CLI's list-price estimate; the tokens include cache reads.

## Decisions (2026-10-23)

| Agent | Model | Evidence |
|---|---|---|
| scout | **haiku** | 8/8 correct at $0.002–0.004 per run (sonnet 7/8, opus 8/8 at $0.08–0.11) |
| original-researcher | **haiku** | 6/6 correct at $0.007–0.013 (opus $0.14–0.31 for the same answers) |
| visual-verifier | **haiku** for clear-cut criteria | 4/4 on the black-bear pass/fail pair at $0.001; escalate UNSURE or subtle questions to sonnet (no subtle case yet) |
| asset-builder | **haiku** | b1: rebuilt artpus on the VM and reported every grip number right, $0.011 (sonnet $0.14) |
| sim-engineer, render-engineer, ui-engineer, blender-artist | not pinned (inherit, i.e. the coordinator's model) | coding cases c1, c2, r1, b2 not finished: the batch was stopped by the OS's low-memory reaper after 2 of 20 runs. Rerun with `-j 1` or `-j 2` |

## Read-only runs (run-ro, 2 runs per model)

| agent | case | model | pass | mean cost $ | mean s | mean tokens |
|---|---|---|---|---|---|---|
| original-researcher | orig-animal-sounds | haiku | 2/2 | 0.007 | 20 | 120265 |
| original-researcher | orig-animal-sounds | opus | 2/2 | 0.189 | 19 | 56871 |
| original-researcher | orig-animal-sounds | sonnet | 2/2 | 0.097 | 21 | 57465 |
| original-researcher | orig-artpus-ranges | haiku | 2/2 | 0.008 | 23 | 136010 |
| original-researcher | orig-artpus-ranges | opus | 2/2 | 0.135 | 22 | 90625 |
| original-researcher | orig-artpus-ranges | sonnet | 2/2 | 0.094 | 26 | 98975 |
| original-researcher | orig-mortar-shell | haiku | 2/2 | 0.013 | 59 | 276363 |
| original-researcher | orig-mortar-shell | opus | 2/2 | 0.314 | 52 | 275134 |
| original-researcher | orig-mortar-shell | sonnet | 2/2 | 0.080 | 25 | 74868 |
| scout | scout-crew-hold | haiku | 2/2 | 0.004 | 16 | 57681 |
| scout | scout-crew-hold | opus | 2/2 | 0.110 | 16 | 46318 |
| scout | scout-crew-hold | sonnet | 2/2 | 0.057 | 30 | 51969 |
| scout | scout-decay-stain | haiku | 2/2 | 0.004 | 17 | 59970 |
| scout | scout-decay-stain | opus | 2/2 | 0.081 | 21 | 31273 |
| scout | scout-decay-stain | sonnet | 1/2 | 0.035 | 15 | 24293 |
| scout | scout-gun-marks | haiku | 2/2 | 0.002 | 13 | 31882 |
| scout | scout-gun-marks | opus | 2/2 | 0.076 | 21 | 40492 |
| scout | scout-gun-marks | sonnet | 2/2 | 0.033 | 14 | 25485 |
| scout | scout-upgrade-levels | haiku | 2/2 | 0.003 | 13 | 37695 |
| scout | scout-upgrade-levels | opus | 2/2 | 0.110 | 23 | 42896 |
| scout | scout-upgrade-levels | sonnet | 2/2 | 0.048 | 17 | 40223 |
| visual-verifier | verify-black-bear-new | haiku | 2/2 | 0.001 | 7 | 14541 |
| visual-verifier | verify-black-bear-new | opus | 2/2 | 0.041 | 10 | 14588 |
| visual-verifier | verify-black-bear-new | sonnet | 2/2 | 0.032 | 10 | 14345 |
| visual-verifier | verify-black-bear-old | haiku | 2/2 | 0.001 | 7 | 14486 |
| visual-verifier | verify-black-bear-old | opus | 2/2 | 0.023 | 9 | 14366 |
| visual-verifier | verify-black-bear-old | sonnet | 2/2 | 0.011 | 7 | 14347 |

The one sonnet miss on scout-decay-stain named the shader and the docs but not `seep.rs`.

## Coding and Blender runs (run-code, partial)

| case | model | result | cost $ | s |
|---|---|---|---|---|
| b1-grip-report | haiku | PASS | 0.011 | 247 |
| b1-grip-report | sonnet | PASS | 0.140 | 167 |

## How to rerun

```
cd tools/claude-plugin/acr/evals
python run_evals.py --cases "[cbr][12]-*" --models haiku,sonnet --runs 2 -j 2 --out C:/Users/verwalter/WIP/ACR-analysis/evals/run-code
python run_evals.py --cases "[cbr][12]-*" --models opus --runs 1 -j 2 --out C:/Users/verwalter/WIP/ACR-analysis/evals/run-code
python run_evals.py --summary-only --out C:/Users/verwalter/WIP/ACR-analysis/evals/run-code
```

Each `claude -p` child takes several hundred MB on this 16 GB PC; keep `-j` at 2 or less when other sessions run.
