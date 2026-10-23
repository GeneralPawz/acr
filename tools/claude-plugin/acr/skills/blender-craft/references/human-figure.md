# Human figure: adult male, about 1760

Sources: Richer, "Artistic Anatomy"; Loomis, "Figure Drawing for All It's Worth"; Drillis and Contini, "Body Segment Parameters" (segment lengths as fractions of height H); Steckel and Prince on the stature of Plains Indians (American Economic Review); Blender Manual (Rigging). Ratios are population averages (+-5 %). Skeleton H and garments: `acr-blender-pipeline` and `cloth.md`.

## Stature by population (adult men, approximate)

| group | height | notes |
|---|---|---|
| British soldier | 1.68-1.72 m | recruit minimum about 1.65 m (5'5"); grenadiers and guards 1.75+ |
| French, Spanish, German infantry | 1.64-1.70 m | Spanish and southern French on the low side |
| Conquistador (c. 1520) | 1.60-1.67 m | sources thin; do not exceed 1.70 m |
| Eastern Woodland, Great Lakes | 1.68-1.74 m | lean, long-limbed |
| Plains (Sioux, Cheyenne, Comanche) | 1.70-1.77 m | among the tallest populations of their time |
| Southeast, Southwest, Pueblo | 1.62-1.70 m | Pueblo toward the low end |
| Mesoamerican (Aztec, Maya) | 1.52-1.62 m | |
| Andean (Inca) | 1.55-1.63 m | broad chest, strong calves |

Individuals scatter +-4 % around the family mean. Scale the whole rig, but keep head and hand size nearly fixed (a taller man's head is only a little bigger).

## Proportions (fraction of H)

| landmark | ratio | at H = 1.72 m |
|---|---|---|
| head, chin to crown | 0.13 (7.5 heads real; 8 is heroic) | 0.22-0.23 m |
| chin / acromion / elbow / crotch / knee height | 0.87 / 0.82 / 0.63 / 0.47-0.50 / 0.28 | |
| biacromial (bone to bone) | 0.23 | 0.39 m |
| bideltoid (over the muscle) | 0.26-0.28 | 0.45-0.48 m, about 3 head widths |
| hip (bitrochanteric) | 0.19-0.20 | 0.33-0.35 m |
| arm span, fingertip to fingertip | 1.00-1.03 | |
| upper arm / forearm / hand | 0.186 / 0.146 / 0.108 | hand length about equals face length, 0.18-0.19 m |
| thigh / shank / foot height | 0.245 / 0.246 / 0.039 | |
| foot length | 0.15 | 0.26 m, about 1.4x the hand |
| neck width / head width | 0.6-0.7 | neck circumference about 0.36-0.40 m |
| chest depth | about 0.12 | 0.21 m; shoulders are about 1.8x wider than the chest is deep |

Standing, the wrist is at crotch height and the fingertips reach mid-thigh. Bideltoid is about 1.4x the hip width.

## Landmarks for deformation and weights

- **Shoulder girdle:** the clavicle is a strut, the scapula slides on the ribs. Raising the arm 180 deg is about 120 deg at the joint plus 60 deg scapula (2:1 rhythm). Give `clavicle` roughly a third of the elevation above about 30 deg (up and slightly back), not zero: a fixed clavicle sinks the shoulder into the chest. Reaching forward moves the shoulder blade outward.
- **Shoulder joint** sits about 4-5 cm below the acromion tip and 5-6 cm inside the deltoid outline. The visible outline is the deltoid, not the joint: weight the deltoid to the upper arm and let the trapezius follow the clavicle.
- **Elbow carrying angle:** about 5-15 deg valgus; the forearm angles out from the upper arm when the palm faces forward and straightens in pronation. Do not rig dead-straight arms.
- **Forearm twist:** up to about 80-90 deg of pronation lives along the forearm (radius crossing ulna), not at the wrist. Spread it over the forearm weights (candy wrapper, see `rigging-and-animation.md`).
- **Hip joint** centre is near pubic-symphysis height (about 0.52 H) and deep (about 7 cm) inside the flesh; the visible hip is the greater trochanter, about a hand lower than the joint's top. Fade leg weights over buttock and groin, not along a straight crease.
- **Knee:** thigh bones slope inward (femorotibial angle about 5-7 deg); a hinge from 0 to about 140 deg whose axis shifts back during flexion. Lock-out is 0, never negative.
- **Spine:** thoracic kyphosis, lumbar lordosis, pelvis tilted forward about 10-15 deg. The neck pivots at C7 (the bump at the base of the neck) and leans forward from there; the head pivots near the ear hole, not mid-neck.
- **Ankle:** hinge plus subtalar roll; feet point 10-15 deg outward.

## Skin at game scale

sRGB albedo (approximate; the renderer shows base colour only, so bake AO and cavity in at moderate strength, see `mesh-and-materials.md`):

| complexion | R, G, B |
|---|---|
| pale northern European (covered body) | 205-230, 160-185, 140-165 |
| ruddy, weathered soldier face and hands | 190-210, 120-150, 100-125 |
| olive southern European | 170-195, 125-145, 95-115 |
| Native American, light to medium brown | 150-180, 105-130, 80-100 |
| Andean, Mesoamerican, darker brown | 110-145, 75-100, 55-80 |
| very dark | 70-100, 50-65, 40-55 |

- **Tan map:** face, neck V, forearms and hands 10-20 % darker and redder than the covered torso; the forehead under a hat brim is paler; a clear collar line at the neck and wrist.
- **Avoid plastic:** no flat colour. Add low-frequency blotch (+-6 % value with a red/yellow shift), redder cheeks, nose tip, ears, knuckles, elbows and knees; darker, more saturated shadows in cavities (nostrils, eye sockets, under the chin); lips only about 20 % redder than skin; teeth cream (200-215), not white; eye whites 210-225 with a darker rim. No painted sheen: it is wrong when the figure turns.
- **At 45 px** the face is 8-9 px tall: the hat shadow, eye line and nose shade carry it; beyond that only tone and hands read.

## Age and individual variation

- Soldiers 18-45: older men have thicker waists, lower shoulder lines, jowls, grey in the queue. Natives 16-45: lean, strong deltoids and calves; tattoos and paint go in the albedo.
- Builds: lean, average, stocky (shoulders about 10 % wider, neck about 15 % thicker). Morph 5-10 % only; larger changes break garment patterns.
- Posture: drilled men stand straight, chest up, chin level over the stock; levies slouch (head forward about 5 cm, shoulders rounded about 10 deg); hard-used men stoop under a pack; the old drop the head.
- Scars and marks (smallpox pits, a cheek scar, a missing ear): texture, one or two per face at most; they read only in the close camera.
- Break symmetry by 1-2 deg (one shoulder lower, head tilt); perfect symmetry looks like a mannequin.

## Hair and facial hair

| item | at 45 px | up close | method |
|---|---|---|---|
| soldier hair (queue, powder, under a hat) | dark or white mass under the hat | tied back, ribbon, side curls | geometry for the queue and hair mass; strands in the albedo |
| long hair (frontier, native) | silhouette on the shoulders | strands | sculpted mass of 150-300 tris; alpha MASK cards only for loose ends |
| scalp lock, roach | standing crest | | geometry ridge, 100-200 tris |
| beard, moustache | a darker jaw | stubble | albedo; mass geometry only for full beards (conquistadors, frontiersmen) |
| brows, lashes | none | | albedo |

Use MASK, not BLEND, for any cards (sorting, overdraw). Hair albedo about 30-90 for dark, 120-170 for brown, 190-215 for grey or powder; never 0 or 255.

## Common mistakes

1. **Arms too long:** fingertips stop at mid-thigh; wrist at the crotch.
2. **Neck too thin or long:** 60-70 % of head width with trapezius slope; a thin long neck reads as a child.
3. **Hands too small:** a hand covers the face; small hands ruin held weapons and close-ups.
4. **T-pose shoulder collapse:** weights tuned in a T-pose collapse the deltoid and trapezius in the A-pose. Weight and test in the rest A-pose with raised-arm extremes; use the clavicle share and 2-3 loops across the shoulder.
5. **Straight limbs:** use the carrying angle, soft knees and an S-curved spine.
6. **Feet too small:** boots are about 1.1x the foot length with 3-4 cm soles and heels.
