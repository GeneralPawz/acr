# Pattern JSON (`acr-pattern/1`) and its adapter

A traced pattern is one JSON file per garment under `ACR_SOURCES/patterns/<source>/<garment>.json` (locally `D:\ACR-models\patterns\...`, on the VM `/home/claude/sources/patterns/...`). It maps one to one onto `garments.Panel` and `garments.Seam` (`assets/blender/lib/acr_assets/garments.py`; `docs/clothing.md` "Pattern format"); everything else in it is provenance or grading data.

```json
{
  "schema": "acr-pattern/1",
  "garment": "justaucorps_1690",
  "source": {"ref": "Waugh, The Cut of Men's Clothes", "diagram": "XVI", "book_page": 60,
             "pdf": "waugh/The_Cut_sewing_patterns_only.pdf", "pdf_page": 44, "dpi": 300,
             "px_per_inch": 37.9, "scale_from": "margin ruler 0-20 in", "allowance_cm": 0.0},
  "period": [1680, 1700],
  "nations": ["Eng", "Frn"],
  "units": ["EngOf17", "FrnOf17"],
  "notes": "pleats flattened to flare; pocket flaps not cut",
  "base": {"chest": 94, "waist": 84, "seat": 98, "neck": 38, "nape_to_waist": 43,
           "arm": 60, "waist_to_knee": 52},
  "ease": 20.0,
  "cloth": "coat",
  "spacing": 3.5,
  "panels": [
    {"name": "front", "key": "coat",
     "outline": [[0.0, 0.0], [27.5, 0.0], [26.0, 24.5], "..."],
     "edges": {"waist": [0, 1], "side": [1, 2], "armhole": [2, 5], "shoulder": [5, 6],
               "neck": [6, 8], "cf": [8, 0]},
     "place": {"region": "torso", "theta": 0.0, "dir": 1, "u0": 0.0, "at": "waist", "gap": 2.5},
     "mirror": true, "skirt": false, "grain": [0.0, 1.0], "tack": null,
     "notches": {"sleeve_front": [26.0, 24.5]},
     "grade": {"u": [[-200, "waist"], [12, "chest"]], "v": "nape_to_waist"}}
  ],
  "seams": [
    {"a": [["front", "side"]], "b": [["back", "side"]], "ease": 0.15},
    {"a": [["back", "cb"]], "b": [["back", "cb", false, "R"]]},
    {"a": [["front", "cf"]], "b": [["front", "cf", false, "R"]], "close": 0.05}
  ]
}
```

## Fields

| field | meaning |
|---|---|
| `garment` | the name it registers under (`registry.garment`), used in a unit's `build["drape"]` |
| `source` | where it was traced: diagram, book and PDF page, render dpi, the scale read (`px_per_inch` or `px_per_cm`), `allowance_cm` removed (0 for Waugh) |
| `period`, `nations`, `units` | years, nation prefixes (`Eng`, `Frn`, `Spn`, `Usa`, `Iro`, …), the roster units it fits |
| `base` | the original wearer's measurements read off the pattern (cm, `garments.MEASURE` keys): what grading scales from |
| `ease` | cm of chest girth the pattern has over `base["chest"]` (or the girth its region uses) |
| `cloth` | a `garments.CLOTH` key to copy, or a full dict (`thickness`, `mass`, `tension`, `bending`, `shrink`, `margin`; optional `self`, `self_gap`, `arms_down`) |
| `spacing` | mesh spacing in cm (`drape.SPACING`) |
| `outline` | [[u, v]] cm, counter-clockwise, v along the grain up the body, u across; v = 0 at the landmark `place.at` (the waist line for torso panels, the wrist for sleeves, the knee or ankle for legs), u = 0 at centre front/back or the panel's seam named by `u0` |
| `edges` | {name: [i, j]}: the outline from vertex i forwards to j (wrapping), after the outline is CCW |
| `place` | as `Panel.place`: `region` (`torso`, `neck`, `arm`, `leg`, `cape`), `theta`, `dir`, `u0`, `at` (`waist`, `knee`, `ankle`, `wrist`, `collar`, `stock`), `lift`, `gap`, `layer`, `pin` (edge), cape: `turn`, `span`, `pin_v` |
| `mirror`, `skirt`, `grain`, `tack` | as `Panel` (drafted for the wearer's left; `skirt` = free cloth on the skirt bones; `tack` = inlay of that panel) |
| `notches` | {name: [u, v]}: balance marks; the adapter checks that paired notches land at the same seam fraction (within 1 cm) |
| `grade` | how the panel scales: `u` is a list of [v_from, measure] bands (the band whose `v_from` is the highest at or below the point's v sets the girth ratio), `v` one length measure; a missing `grade` scales u by chest and v by `nape_to_waist` (torso), `arm` (arm), `waist_to_knee`/`knee_to_ankle` (leg) |
| seam `a`, `b` | chains of [panel, edge, reversed?, side?] as `Seam` (`side` "L" default, "R" = the mirrored copy); `ease` (default 0.15), `close` (m) |

Coordinates are real cm of the garment, never pixels. Store the scan-space polyline only in a sidecar `<garment>.trace.json` (pixels, page, crop) so the overlay can be redrawn.

## The adapter (does not exist yet)

`garments.py` has only drafting functions in code; nothing reads JSON, and nothing grades a fixed outline. The first garment-tailor task that uses a traced pattern adds, in the shared module:

- `garments.load_pattern(path)` (pure): JSON → a dict checked against this schema (CCW outlines, edge indices in range, seam lengths within `ease` after grading, notches paired).
- `garments.traced(path)` (pure): returns a draft `draft(m=MEASURE) -> (panels, seams)` that grades every panel point (u × `m[band]/base[band]` with ease kept constant, not scaled; v × the length ratio) and builds `Panel` and `Seam` objects. Ease is kept: girth ratio = (m + ease) / (base + ease).
- Registration in the family module: `registry.garment(name, gm.traced(os.path.join(os.environ["ACR_SOURCES"], "patterns", ...)), cloth, ease, spacing)`, failing with a clear error when `ACR_SOURCES` is unset (as `animals.py` does). The build reads the JSON; the repo never holds it.
- Tests in `assets/blender/tests/test_garments.py` on a small synthetic pattern written by the test (a rectangle torso, two sleeves), plus a test that loads every JSON under `ACR_SOURCES/patterns` and skips when it is unset.

Gaps the adapter has to bridge (current code, check before relying on it):
- **Inseam** is not measured. `drape.Body.measure` has `waist_to_knee` and `knee_to_ankle`; `breeches` fixes the rise at 27 cm. Add `inseam` to `drape.Body` (crotch to ankle from the rig) and `garments.MEASURE`, or grade legs by `waist_to_knee` and a fixed rise.
- **One-piece fronts** (justaucorps fronts cut from shoulder to knee) must be split at the waist line into a body panel and a `skirt` panel with a welded seam (`close`): `skirt` is per panel and drives the skirt-bone weights.
- **Pleats** are not simulated: flatten them into flare (as `garments.coat` does) and note it in `notes`.
- **Grain** (`Panel.grain`) is stored but no drape code reads it; keep the outline's v on the grain.
- **Curved sleeves** (17th-c. sleeves cut with an elbow bend) are wrapped round the straight A-pose arm axis; expect the open sleeve seams `docs/clothing.md` "Known problems" lists, and straighten the sleeve's centre line before grading if they gape.
- **Landmarks** are only `waist`, `knee`, `ankle`, `wrist`, `collar`, `stock` (and the cape's neck ring); a panel anchored elsewhere uses `lift` from the nearest one.
