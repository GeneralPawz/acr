---
name: acr-garments
description: "Digitising historical sewing patterns for ACR: the sources (Waugh's The Cut of Men's Clothes and its page map, Reconstructing History, J.P. Ryan) and their copyright rule, tracing panels from a PDF into the acr-pattern/1 JSON, grading to the MakeHuman body, layer order, the drape checks and studio sheet, and period notes per nation. Load before tracing, grading or draping a garment."
---

# Garments from historical patterns

The pipeline (`docs/clothing.md`): flat panels in cm → placed round the MakeHuman body → sewn and settled in Blender's cloth sim → welded, skinned, split per material key. Patterns live in `garments.py` (pure, drafts as code) and are draped by `drape.py`. This skill adds the step before: getting real period cuts into that format.

## Sources and the copyright rule

| source | where | use |
|---|---|---|
| Norah Waugh, *The Cut of Men's Clothes 1600–1900* | `ACR_SOURCES/patterns/waugh/The_Cut_sewing_patterns_only.pdf` (locally `D:\ACR-models\patterns\waugh\`, VM `/home/claude/sources/patterns/waugh/`) | scaled cutting diagrams of surviving garments 1600–1680 and 1680–1800, plus period tailors' layouts. Page map, offsets and a diagram-per-garment table: [references/waugh.md](references/waugh.md) |
| Reconstructing History, J.P. Ryan (planned) | `ACR_SOURCES/patterns/<rh|jpryan>/` | full-size commercial patterns; same rule. They usually include seam allowances: read the instruction sheet and remove them |
| the existing drafts | `garments.py` | the 1768 regimental coat, waistcoat, breeches, gaiters; 17th-c. doublet, buff coat, hose, slops; tunics, cloaks, bands |

**Copyright:** the books and patterns are copyrighted. The PDFs, page renders, traced panel data (JSON and trace sidecars) and overlays stay under `ACR_SOURCES` or `C:\Users\verwalter\WIP\ACR-analysis\garments\` and are never committed. The repo holds only code that reads them (and synthetic test patterns). Copy new JSON to the VM with `scp -i ~/.ssh/claude-render` to the same path under `/home/claude/sources/patterns/`. A new source gets a row in `assets/LICENSES.md` saying it is used locally only.

## Digitising

1. **Render the page** at 300 dpi: `fitz.open(pdf)[pdf_page-1].get_pixmap(dpi=300)` (PyMuPDF is on this PC; `pdftoppm -r 300 -f N -l N` also works). The scans are slightly skewed: measure the margin ruler's angle and rotate so it is vertical. Work in `C:\Users\verwalter\WIP\ACR-analysis\garments\<garment>\`.
2. **Read the scale.** Waugh: the margin inch ruler (pixel distance between the 0 and 20 in ticks / 20 = px per inch); Diagram XIII a cm bar. Commercial patterns are full size: scan dpi gives the scale; check it against a printed test square. Record it in `source`.
3. **Trace each panel** as a closed polyline: threshold the crop, take the outer contour (OpenCV `findContours` or skimage `find_contours`), simplify to ~0.3 cm (Douglas–Peucker), keep corners. Ignore hatching (padding), embroidery, button rows and dotted fold lines; a dotted turn-back line (cuff in one with the sleeve, a lapel) becomes an inlay panel with `tack`.
4. **Convert to panel space:** cm, v up the grain, v = 0 on the landmark line (waist, wrist, knee, ankle), u from centre front or back towards the wearer's left side; one side only, `mirror` for the other. Make it counter-clockwise (reverse if the signed area is negative) *before* numbering edges. Remove seam allowances by offsetting inwards (Waugh: none).
5. **Label** edges by vertex index (`cf`, `side`, `shoulder`, `armhole`, `waist`, `hem`, `cap_front`, …), notches, and seam pairs. Both chains of a seam run the same way on the body.
6. **Prove the trace:** draw the JSON back over the scan (outline, vertex numbers, edge names, scale bar) and have `acr:visual-verifier` check it: every panel covered, no edge off the drawn line by more than 2 mm at scale, labels on the right edges. Then check the pure numbers: seam lengths of each pair within `ease`, notches at matching seam fractions.
7. **Write** `<garment>.json` ([references/pattern-json.md](references/pattern-json.md)) with source page, garment, period, nation fit and the original wearer's `base` measurements (read chest, waist and back length off the pattern: girths = sum of the panels' widths at that line × 2, minus the period ease below).

The JSON schema, the adapter that reads it (`garments.traced`, not written yet) and the gaps in the current code are in `references/pattern-json.md`.

## Grading to the MakeHuman body

- `drape.Body` measures the hull at build time into `garments.MEASURE` keys: `chest`, `waist`, `seat`, `neck`, `nape_to_waist` (back length), `shoulder`, `arm` (shoulder to wrist + 4), `upper_arm`, `wrist`, `waist_to_knee`, `thigh`, `knee`, `calf`, `ankle`, `knee_to_ankle`. Inseam is not one yet (see the gaps). Defaults (build 1.0): chest 98, waist 84, back length 45, arm 58 cm.
- Grade girths with the ease kept constant: factor (m + ease) / (base + ease) on u, per band (chest above the armhole line, waist below it, seat on skirts); lengths by the ratio of the matching length. Never scale ease.
- **Ease per period** (cm of chest girth; `garments.EASE` has the existing ones):

| garment | ease | note |
|---|---|---|
| doublet 1620–50 | 10–12 | fitted, closed down the front; peascod padding not modelled |
| buff coat | 16 | stiff leather, stands off the body |
| casaque, cassock 1630–90 | 25–35 | loose, over armour or the doublet |
| justaucorps 1680–1720 | 20–24 | straight body, full pleated skirts |
| regimental coat 1740–80 | 18–20 | fitted, cutaway skirts (`coat`: 20) |
| waistcoat | 6–8 | under the coat (`waistcoat`: 8) |
| breeches 17th c. / 18th c. | 15–30 / 0.5–2 | full and gathered early; close fitting late |
| shirt, tunic, hunting shirt | 24–34 | loose (`tunic` 24–30) |

- **Layer order** (`garments.LAYER_RANK`, equal to the game's `acr_render::layers`): skin 0, legwear 1, gaiters/boots/slops/breechcloth 2, shirts 3, waistcoats/doublets 4, coats/cloaks/tunics 5, facings/sashes/mantles 6, belts 7, boxes 8. Drape inner garments first (the unit's `build["drape"]` order); a new material key needs a rank in both tables (test `test_layer_ranks_match_the_game`; the Rust side is reported, not changed) and margins that grow outwards (`CLOTH[...]["margin"]`).

## Checks (numbers first, then one sheet)

| check | how | good |
|---|---|---|
| drape stats | `stats.json` → `drape.<garment>`: `welded`/`sew`, `gap_mm_max`, `gap_mm_max_after_sewing`, `worst_seams_mm`, `unposed_m` | every pair welded or listed; worst seam < 45 mm (`WELD`) |
| layers | `model_layers <glb> --steps 8` (via `acr-vm-cargo`): `summary … poking N crossing M` | not worse than the baseline; state both |
| solid gear | `gear_volume <glb>` | no cloth inside boxes or quivers |
| silhouette | `cargo xtask assets <unit> --check` (IoU per `CHECKS`) or `assets/blender/tools/silhouette.py` | IoU not lower than before; coat length and skirt flare match the sprite |
| look | studio `sheet` (skill `acr-driver`): views front, three_quarter, back; clips STAND, a `MOTION_L` frame mid-stride, the fire or strike frame (`ATTACK1`) | judged by `acr:visual-verifier` against the sprite and the reference picture |

`ACR-analysis/clothing/harness/run.sh <worktree> <tag> [nosim]` renders a unit's garments coloured by key in about 15 s on the VM: use it while iterating, the studio sheet for the proof.

## Reference pictures

Generated pictures are style targets, not patterns: `C:\Users\verwalter\WIP\ACR-analysis\genai\refs-codex\archetypes\<nation>_<century>.png` (english, french, spanish, usa, peasants; 16th–18th c.) and `refs-openrouter\<model>\`. A picture model (or `acr:visual-verifier` with numbered questions) can read proportions off them against the figure's height: coat hem height over the knee, cuff depth, skirt flare at the hem, sleeve length, waistcoat length. Use these to set pattern parameters (`drop`, cuff height, flare) and to judge the sheet; the cut itself comes from the historical pattern.

## Period notes

What each family wears, by roster family (`docs/humans-roster.md` §2, `docs/families/<family>.md`):

| family | period | garments | pattern source |
|---|---|---|---|
| 1, 2 line infantry, drummers, flags | 1740–80 | regimental coat with lapels or plain fronts, turnbacks, round or slashed cuffs; waistcoat; breeches; gaiters; shirt cuffs, stock | `garments.coat` (1768); Waugh XIX, XXI, XXVIII for national variants (a different cut is an argument of the draft, `docs/clothing.md` "Adding a garment or a nation"; check lapels and cuff shape per nation on Kronoskaf). Colours: `docs/families/line.md` |
| 4 officers | 18th and 17th c. | finer coat, sash (band), gorget; 17th: doublet, buff coat, falling band | Waugh XV–XVII (justaucorps), V–IX |
| 5 cavalry | 17th and 18th c. | riders' coats (skirts split over the saddle), buff coat, sash over the shoulder, bucket boots | Waugh VII/IX, XV–XVII; `buffcoat` |
| 6 pike and shot | 1620–60 | doublet (tabbed), full breeches to the knee, hose, buff coat, falling band | Waugh I–XIV |
| 11 frontier | 18th c. | hunting shirt (fringed smock), leggings, waistcoat, breeches; buccaneer: shirt, wide breeches, sash, cape | Waugh XXXII (shirt), XXII (frock); `frn_*` drafts |
| 12 conquistadors | 16th c. | doublet, slashed slops, hose, buff jerkin, quilted escaupil | Waugh I–III (closest), Alcega 1589 (fig. 1) |
| 13 civilians | 18th c. | smock or blouse, breeches, stockings; the priest's habit or cassock with a cord, cappa | Waugh XXXII; fig. 2 soutane, fig. 6 robe |
| 3, 9, 10 natives | 17th–18th c. | breechcloth, side-seam leggings, trade shirt, matchcoat (blanket over one shoulder), hide war shirt (Plains), kilt, manta, sash, jerkin (Pueblo) | no Waugh source: rectangles and tubes cut from the cloth width (`tunic`, `cloak`, `band`); `docs/clothing.md` roll-out table |
| 7, 8 meso, Inca | 16th c. | maxtlatl, tilmatli, ichcahuipilli (quilted, thick), unku, yacolla | loom-width rectangles; `unku`, `tilmatli`, `yacolla` drafts |

## Pitfalls (from `docs/clothing.md`)

- Pair seams by arc-length fraction (`seam_fractions`), never by sample count: skewed pairs opened skin holes at the shins.
- Torso panels go over the shoulders; a panel placed far off the body crumples the shoulders and puffs the back.
- Garment weights from the layer beneath (`transfer_weights(..., under=)`), never from the hand faces (pipeline pitfall 3).
- A buttoned closure that must end shut needs `close`, or the pairs stay open past `WELD`.
- Loose cloaks lie on draped layers: wider margin, self-collision, and the pose-space drape (`ARMS_DOWN_GARMENTS` / `CLOTH[g]["arms_down"]`) with the unit's stand arms.
- Rest-pose pokes remain in every family; `drape.settle` catches part. Report `model_layers` numbers, don't claim "clean".
- Shell garments (body-hull offsets) leave teeth: replace them with drafts rather than patching.
