//! Impact sounds of hits and pain screams of the wounded (#243, not in the original), driven
//! by the hits the simulation records for the renderer (`World::hits`).
//!
//! Projectiles that hit a soldier thud ([`BODY`], [`HEAVY`] for cannonballs and shells), on
//! buildings they knock on wood ([`WOOD`]); a soldier who survives a hit (also in melee)
//! may cry out ([`PAIN`]). Kills keep the original's death sounds (`SOUND 3 #DEATH ...`) and
//! only add the impact. A volley must not turn into a hundred sounds: per tick only the
//! [`MAX_IMPACTS`] impacts and [`MAX_PAINS`] screams nearest the view centre play, and
//! [`crate::unit_sounds::UnitSounds::play`] skips a group's variants that still play.
//!
//! The samples are CC0 files of ACR (`assets/sounds`, see its `CREDITS.txt`), looked up as
//! the game files `ACR\Sounds\<file>` (docs/simulation.md, "Hit and pain sounds").

use std::collections::BTreeMap;

use acr_sim::combat::{DeathCause, HitRecord};
use acr_sim::fog::Fog;

use crate::unit_sounds::{self, SoundEvent};

/// Musket balls, arrows, thrown weapons and grapeshot hitting a body.
pub const BODY: &str = "ACR_HIT_BODY";
/// Cannonballs and bursting shells hitting a body.
pub const HEAVY: &str = "ACR_HIT_HEAVY";
/// Any projectile hitting a building.
pub const WOOD: &str = "ACR_HIT_WOOD";
/// A soldier wounded but alive.
pub const PAIN: &str = "ACR_PAIN";

/// The sound groups and their files below [`DIR`].
pub const GROUPS: &[(&str, &[&str])] = &[
    (BODY, &["hit_body1.ogg", "hit_body2.ogg", "hit_body3.ogg"]),
    (HEAVY, &["hit_heavy1.ogg", "hit_heavy2.ogg", "hit_heavy3.ogg"]),
    (WOOD, &["hit_wood1.ogg", "hit_wood2.ogg", "hit_wood3.ogg"]),
    (
        PAIN,
        &[
            "pain1.wav",
            "pain2.wav",
            "pain3.wav",
            "pain4.wav",
            "pain5.wav",
            "pain6.wav",
            "pain7.wav",
            "pain8.wav",
            "pain9.wav",
        ],
    ),
];

/// The game folder of ACR's own sounds.
pub const DIR: &str = r"ACR\Sounds";

/// Impact sounds started per tick at most.
pub const MAX_IMPACTS: usize = 3;
/// Pain screams started per tick at most.
pub const MAX_PAINS: usize = 1;

/// ACR's own sound groups: these and those of [`crate::agony`].
pub fn all_groups() -> impl Iterator<Item = &'static (&'static str, &'static [&'static str])> {
    GROUPS.iter().chain(crate::agony::GROUPS)
}

/// The game paths of a group's files.
pub fn files(group: &str) -> Option<Vec<String>> {
    let (_, files) = all_groups().find(|(g, _)| *g == group)?;
    Some(files.iter().map(|f| format!(r"{DIR}\{f}")).collect())
}

fn projectile(cause: DeathCause) -> bool {
    use DeathCause::*;
    matches!(cause, Gunshot | Arrow | Thrown | Cannonball | Grapeshot | Explosion)
}

/// The impact group of a hit, if it makes one.
fn impact(hit: &HitRecord) -> Option<&'static str> {
    if !projectile(hit.cause) {
        return None;
    }
    Some(match (hit.building, hit.cause) {
        (true, _) => WOOD,
        (false, DeathCause::Cannonball | DeathCause::Explosion) => HEAVY,
        _ => BODY,
    })
}

fn pain(hit: &HitRecord) -> bool {
    !hit.killed && !hit.building && (projectile(hit.cause) || hit.cause == DeathCause::Melee)
}

/// The sounds of the hits after tick `after`, heard in the view `[left, top, width, height]`
/// (original screen pixels, as for [`unit_sounds::placed`]) under the fog `fog`: per tick
/// the [`MAX_IMPACTS`] impacts and [`MAX_PAINS`] screams nearest the view centre, in tick
/// order.
pub fn hit_sounds<'a>(
    hits: impl IntoIterator<Item = &'a HitRecord>,
    after: u64,
    view: [f32; 4],
    fog: Option<&Fog>,
) -> Vec<SoundEvent> {
    let centre = (view[0] + view[2] / 2.0, view[1] + view[3] / 2.0);
    // Per tick: (distance², group, volume, pan) of impacts and of screams.
    type Heard = Vec<(f32, &'static str, f32, f32)>;
    let mut ticks: BTreeMap<u64, (Heard, Heard)> = BTreeMap::new();
    for hit in hits.into_iter().filter(|h| h.tick > after) {
        let (x, y) = (hit.pos.0 as f32, hit.pos.1 as f32 / 2.0);
        let Some((volume, pan)) = unit_sounds::placed(x, y, view, fog) else { continue };
        let d = (x - centre.0).powi(2) + (y - centre.1).powi(2);
        let (impacts, pains) = ticks.entry(hit.tick).or_default();
        if let Some(group) = impact(hit) {
            impacts.push((d, group, volume, pan));
        }
        if pain(hit) {
            pains.push((d, PAIN, volume, pan));
        }
    }
    let mut out = Vec::new();
    for (_, (mut impacts, mut pains)) in ticks {
        for (list, max) in [(&mut impacts, MAX_IMPACTS), (&mut pains, MAX_PAINS)] {
            list.sort_by(|a, b| a.0.total_cmp(&b.0));
            out.extend(list.iter().take(max).map(|&(_, name, volume, pan)| SoundEvent {
                name: name.to_owned(),
                pan,
                volume,
            }));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: [f32; 4] = [1000.0, 500.0, 1024.0, 768.0];

    fn hit(tick: u64, x: i32, cause: DeathCause, killed: bool, building: bool) -> HitRecord {
        // y = 2 × 884: the view centre's row.
        HitRecord {
            tick,
            victim: 0,
            pos: (x, 1768),
            dir: (1, 0),
            cause,
            killed,
            building,
            region: None,
            injury: None,
            strike: None,
            critical: None,
        }
    }

    fn names(events: &[SoundEvent]) -> Vec<&str> {
        events.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn groups_by_cause_and_victim() {
        use DeathCause::*;
        let hits = [
            hit(1, 1512, Gunshot, false, false),
            hit(2, 1512, Cannonball, true, false),
            hit(3, 1512, Grapeshot, false, true),
            hit(4, 1512, Melee, false, false),
            hit(5, 1512, Burned, false, false),
        ];
        assert_eq!(names(&hit_sounds(&hits, 0, VIEW, None)), [BODY, PAIN, HEAVY, WOOD, PAIN]);
        // The view centre plays at full volume, unpanned.
        let first = &hit_sounds(&hits, 0, VIEW, None)[0];
        assert_eq!((first.volume, first.pan), (1.0, 0.0));
    }

    #[test]
    fn only_hits_after_the_cursor() {
        let hits = [hit(7, 1512, DeathCause::Arrow, true, false), hit(8, 1512, DeathCause::Arrow, true, false)];
        assert_eq!(hit_sounds(&hits, 7, VIEW, None).len(), 1);
        assert!(hit_sounds(&hits, 8, VIEW, None).is_empty());
    }

    #[test]
    fn a_volley_plays_the_nearest_few_per_tick() {
        // 100 musket hits in one tick, spread to the right of the centre.
        let volley: Vec<_> = (0..100).map(|i| hit(3, 1512 + 5 * (99 - i), DeathCause::Gunshot, false, false)).collect();
        let events = hit_sounds(&volley, 0, VIEW, None);
        assert_eq!(names(&events), [BODY, BODY, BODY, PAIN]);
        // The nearest ones: little pan.
        assert!(events.iter().all(|e| e.pan < 0.05));
        // A second tick adds its own share.
        let mut two = volley.clone();
        two.extend((0..10).map(|_| hit(4, 1512, DeathCause::Gunshot, true, false)));
        assert_eq!(names(&hit_sounds(&two, 0, VIEW, None)), [BODY, BODY, BODY, PAIN, BODY, BODY, BODY]);
    }

    #[test]
    fn far_hits_are_not_heard() {
        let hits = [hit(1, 1000 + 1024 + 1025, DeathCause::Gunshot, false, false)];
        assert!(hit_sounds(&hits, 0, VIEW, None).is_empty());
    }

    #[test]
    fn every_group_has_files() {
        let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sounds");
        for (group, names) in GROUPS {
            assert!(files(group).is_some_and(|f| !f.is_empty() && f.iter().all(|p| p.starts_with(DIR))));
            for name in *names {
                let bytes = std::fs::read(shipped.join(name)).unwrap();
                acr_audio::Sound::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            }
        }
        assert_eq!(files("NOPE"), None);
    }
}
