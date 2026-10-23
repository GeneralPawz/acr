# Known open defects and workarounds in the humans program

Recorded by the coordinator on 2026-10-23. **Verify each one in the code before you rely on it**: some may be fixed since. Fix shared bugs in the shared module, never per family, then rebuild one unit of every family you touch.

| Area | Defect | Where |
|---|---|---|
| Poses | `clips.STOOP` and the `lean` in `sword_strikes`/`club_strikes` lean backwards (positive spine X bends forward on skeleton H); officers.py and native.py carry local forward versions to drop after the fix | `clips.py`, `officers.py`, `native.py` |
| Paint | `outfit.dress` paints one colour per painter (civil.py registers separate painters) | `outfit.py`, `civil.py` |
| Paint | `masks.paint_face_half` covers half the trunk; native faces get European stubble | `masks.py` |
| Skin | the body skin renders dark brown though the stored flesh colour is right (faces are fine after `native.clean_shaven`); suspect the atlas skin painting or the renderer's skin tint. Trunk patches on natives come from the same cause | `outfit.py`, renderer tint |
| Weapons | `line.make` accepts only musket, drum and colour, so gun units can't draw a sword or tomahawk for melee | `line.py` |
| Weapons | `strike_view::weapon` (Rust) takes `socket.hand_r`; archers with a club on `socket.club_r` miss by 2+ m (prefer the socket that is visible in the clip) | `crates/acr-game/src/ingame/strike_view.rs`, `native.py` |
| Drummer | the idle is STAND (sticks raised); the sim picks REST about 1 in 64. The stick jab length needs a tip argument through `clips.py` and `line.py`, then an EngB18 rebuild | `clips.py`, `line.py` |
| 17th c. | no 17th-century clothing (doublet, buff coat, falling band, boots, matchlock with rest); pike, officers 17, cavalry 17 and conquistadors use the 18th-century coat or shells | families |
| Legs | stockings and leggings show skin holes at the shin and knee (the gaiter-pattern seam; civil, frontier, pike) | `garments.py` / families |
| Pueblo | PueVsd has no script of its own (its .MD equals PueVsda's), and the hair is blocky boxes | `pueblo.py` |
| Colours | EngD17 and EngKOf17 sprites show red coats, but they were built in buff | cavalry, officers |
| Shells | shell garments derived from the body hull leave "teeth"; convert them to drafts (`docs/clothing.md` roll-out table): Delkri trade shirt, quiver strap patches, SpnKri/FrnKri blouse opening at the waist | `drape.py`, `civil.py` |
| Grips | rebuilt with fists for 18 foot units only; other units and the riders still have open hands until rebuilt. The rammer's left hand on the staff misses by about 0.2 m (`staff_l`) | `grips.py`, `guncrew.py` |
| Cavalry | limber and deploy clips for horse-drawn guns are missing | `guncrew.py` |

**Rebuild debt:** every unit built before 9883ad7 (multi-material slots) or before the grips landing (e9f200c) needs a rebuild to pick up the fixes. Use `acr:asset-builder` with the list of names.
