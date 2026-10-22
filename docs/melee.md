# Melee

## Animation

Render side only (`crates/acr-game/src/ingame/strike_view.rs`, called from `ragdoll_view`
after the gait layer; `ACR_NO_STRIKE` turns it off).

- **Aimed strikes:** a man who attacks a unit (`AnimKind::Attack`, `PAttack`, `PStand` with
  an `Order::Attack` target that is drawn) bends his trunk so that his weapon points at the
  victim. The weapon is taken to point level along the hips' facing (reading the right-hand
  socket's axis from the clip, `ragdoll_view::slot_direction`, gave an upright musket for a
  levelled bayonet and is not used yet); its point lies `REACH` (1.0 m) along it from the
  hand and turns with the hand as the trunk bends. The rotation that takes that point onto the target, seen
  from the lower spine, is shared between `spine` (55 %) and `spine2` (the rest), clamped to
  75° in all, and fades in and out with the attack stance (8/s). The clip keeps its own
  motion: the bayonet lunge, the sabre cut, the shot and the reload are the clip's, turned
  onto the victim.
- **Target point** (`strike_height`): a man on the ground (crawling, dying, dead) is struck at
  0.2 m over his ground point, a rider at 1.65 m, a standing man at 1.15 m. On foot, a blow
  at a man on the ground also sinks the attacker 0.2 m into his knees, his feet kept where
  the clip put them by leg IK (`gait::Rig::two_bone_ik`).
- **Riders:** prepared but not active. Mounted models can be mapped onto skeleton H with the
  `rider.` prefix (`Mapping::with_prefix`; the horse's bones ride on the rider's hips, which
  the layer never moves), but `three_d.rs` passes only models with a skeleton-H `rig` to
  `ragdoll_view`, so riders never reach the layer yet: a rider does not lean down at a man
  on the ground.
- **Socket animation kept:** the bent pose goes back into the palette as each bone's change
  (`ragdoll_view::repose`), so a socket keeps its own keyed orientation relative to the hand
  (the musket's "world" keys), unlike the ragdoll's and the gait's `palette_of`.
- **Contract with the sim** (`HitRecord::strike`, not on main yet): `strike_view::StrikeHeight`
  is a local copy of the sim's `StrikeHeight`; `strike_height(victim, strike)` prefers the
  sim's height when given (`TODO(m2-melee)`: pass the attacker's last strike, and `Low` for
  a kneeling man, which the renderer cannot see itself).

Bench: `ACR-analysis/melee-anim/melee_bench.py` (white blank field, HUD off; an `EngM18`
ordered to attack a standing or crawling `EngM18`, and the same for an `EngD18`), sheets
`ACR-analysis/melee-anim/on/sheet_*.png`. Against a crawler the bayonet now points down
by about 15–20° instead of slightly up; it does not yet reach him (he crawls out of reach,
and the bend is small): it reads as "lowered", not yet as a stab down.

### Not yet

- No new keyframed clips: every melee strike still plays the unit's `ATTACK` clip (the
  musketeer's two bayonet lunges, the rider's cut up and cut down, the lance thrust) and is
  only turned onto the target. Weapon-specific clips (butt smash, overhead and diagonal
  cuts, a club's hack, a point-blank shot down) and choosing among them by weapon and
  target height need asset builds and a clip-name scheme the game can pick from.
- Cavalry with a carbine (`EngD18`) still swings the carbine in `ATTACK` (the sabre stays at
  the hip): the fix belongs in `cavalry.add_clips` (draw the sabre into the right hand,
  hide the gun) and needs an asset build.
- The hit moment is not matched to the sim's strike tick; the bend holds through the whole
  clip, so the clip's lunge reaches the target whenever it lunges.
- The weapon's reach is one length for all (1.0 m from the hand); the left hand of a
  two-handed weapon follows the chest, not the weapon.
