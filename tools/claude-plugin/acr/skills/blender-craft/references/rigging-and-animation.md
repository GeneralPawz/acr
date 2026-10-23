# Rigging, IK, contacts and animation

## Weights

- **Normalise to 1 and limit to 4 influences per vertex**, after every weight operation (transfer, smooth, mirror). Use `vertex_group_limit_total(limit=4)` then `vertex_group_normalize_all(lock_active=False)`, or do both in Python on the weight lists. Why: real-time skinning reads a fixed 4 joints; an unnormalised sum scales the vertex towards or away from the origin.
- **Prune tiny weights** (under ~0.01) before limiting: they steal one of the 4 slots and add nothing.
- **No weights on non-deforming bones** (sockets, IK targets, roots). Mark them `use_deform = False` so automatic and transfer tools skip them.
- **Smooth falloff across a joint, rigid inside a segment.** A 1/d^n falloff from bone segments (n ≈ 4) gives a short blend at the joint; wider blends make limbs rubbery.
- **Mirror-exact weights** on a symmetric body: weight one side and mirror by name (`_l`/`_r`), then check the centre line sums.
- **Clothes and straps inherit from the surface beneath** (nearest face, interpolated barycentrically, restricted to the same side of the body), not from bones by distance. Exclude regions whose deformation is wrong for cloth (a baked fist curls fingers in at the thigh: a skirt there would follow the hand).
- **Corrective shares** where free cloth hangs over a moving limb: a coat skirt keeps its own bones but takes a share of the thigh behind it (more at the hem, less at the back), so a stride does not pass through it.
- **Linear blend skinning loses volume in twist** (candy wrapper at the wrist and the shoulder) and in deep bends. Remedies, cheapest first: distribute twist over the forearm in the pose itself; more loops at the bend; a corrective morph keyed with the pose; twist bones (costs bones and channels).
- **Check deformation in the extremes**: a deep lunge, prone with arms over the head, a crawl, a kneel. Standing poses hide every weighting error.

## IK, FK and contact locking

- **FK for arcs and timing, IK for contacts.** Author swings, blows and gestures in FK (clean arcs); solve hands on a weapon, feet on the ground and hands on a wheel with IK.
- **Bake IK to plain rotation keys before export.** glTF and most engines have no constraints. A two-bone analytic solve (law of cosines, with a pole vector for the elbow or knee) is exact, fast and deterministic; iterative solvers need a fixed iteration count for determinism.
- **Never fully straighten an IK chain.** At full extension the solve flips or pops; keep the target ≥ 1–2 % inside reach, or step the body towards the goal first (move the root, then solve the arm).
- **Pole vectors decide the elbow**: elbows out and down for a musket at the shoulder, behind for a thrust, in for a drum. Wrong poles give chicken-wing elbows that read even at 45 px.
- **Solve every frame of a contact, not only the keys.** Interpolating two solved keys drifts off the contact in between (rotation interpolation is not position interpolation). Key per frame while a hand holds or a foot is planted; let the free phases interpolate.
- **Foot locking:** during a plant the foot's world position and yaw stay constant while the root moves; measure the foot's world drift over the contact frames (target under ~1 cm). Sliding feet are the most visible error in a walk.
- **Two-handed weapons:** the main hand owns the weapon; the off hand is solved onto the weapon's axis each frame and turned so its knuckle line runs along it. Measure the fist-to-axis gap per frame.
- **Grip by the hand, not by the socket.** If a clip asks for a weapon orientation, turn the hand so the weapon stays in the palm; turning the socket alone lets the item slide through the fingers.

## Animation principles for drills and combat

- **Key poses first** (contact, extreme, release), then breakdowns, then in-betweens. Check the key poses as a contact sheet before timing.
- **Timing comes from the game.** Frame counts and event frames are fixed (shot, hit, release); place the anticipation before and the follow-through after the event frame, and never shift the event.
- **Anticipation** before every effort: a wind-up before a blow, a dip before lifting a trail, a breath before the throw. Without it heavy actions look weightless.
- **Weight:** heavy objects (a 4.5 kg musket, a 1.5 t gun) accelerate slowly, overshoot little and settle; the body counter-balances (hips back when lifting, lean into a push). Light objects (sabre, drumstick) can snap.
- **Arcs:** hands, weapon tips and heads travel in arcs, not straight lines. Check the tip's path; a straight-line in-between reads mechanical.
- **Slow in and slow out** at holds; linear interpolation only for constant motion (marching cadence) or where the game wants exact sprite matching.
- **Follow-through and overlapping action:** coat skirts, sashes, hair, slung gear lag the body and settle after it stops (lag bones or runtime springs are cheaper than keyed overlap).
- **Military drills are precise, not loose.** A manual of arms has fixed positions held for a count; motion between them is brisk and economical. Use period manuals (British 1764 Manual Exercise, French and Spanish ordonnances) for positions and counts.
- **Silhouette from the game camera** (30° down, small): each key pose must read as a shape; arms crossing the body vanish.
- **Loops:** last frame equals first; check velocity continuity across the seam, not just the pose.
- **Deaths and falls:** the fall ends where the corpse sprite lies; the body slides only during the stumble, never after it is down; the last frame of `DEATH` equals the lying pose.
