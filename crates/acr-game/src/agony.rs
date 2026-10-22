//! Agony of the dying, fearful screams of panicking troops and the din of a bloody battle
//! (#248, not in the original), on top of the hit and pain sounds of [`crate::hit_sounds`].
//!
//! - **Agony:** a share of the soldiers killed by a hit (`World::hits`) screams as he falls
//!   ([`SCREAM`]) and then moans and gasps ([`MOAN`], [`GASP`]) ever quieter for 3–12 s
//!   where he lies. Who does is fixed by the unit index and the cause ([`agony_ticks`]):
//!   a musket ball often kills outright, grapeshot, shells and blades seldom. A man struck
//!   dead by a cannonball is torn apart and makes no sound: only the wounded scream.
//!   Only the [`MAX_AUDIBLE`] agonies nearest the view centre make sounds; the others wait
//!   silently while they last, also when they are out of hearing.
//! - **Panic:** units that start a flight leg in panic (`World::shouts`, 0x600bc8) shout in
//!   fear ([`FEAR`]): one shout per brigade (or loose unit) every [`PANIC_GROUP_TICKS`], and
//!   one in all every [`PANIC_GLOBAL_TICKS`], the one nearest the view centre.
//! - **Din:** with many recent kills in hearing ([`DIN_WINDOW`]), now and then a distant
//!   scream or shout from somewhere at the edge of the view, quiet and panned.
//!
//! Randomness comes from hashes of unit indices and ticks; none of it reaches the
//! simulation.

use std::collections::{HashMap, VecDeque};

use acr_sim::combat::{DeathCause, HitRecord};
use acr_sim::fog::Fog;

use crate::unit_sounds::{self, SoundEvent};

/// A dying soldier's scream as he falls.
pub const SCREAM: &str = "ACR_AGONY_SCREAM";
/// The moans of the dying.
pub const MOAN: &str = "ACR_AGONY_MOAN";
/// The short gasps of the dying.
pub const GASP: &str = "ACR_AGONY_GASP";
/// Fearful shouts of panicking troops.
pub const FEAR: &str = "ACR_FEAR";

/// The sound groups and their files below [`crate::hit_sounds::DIR`].
pub const GROUPS: &[(&str, &[&str])] = &[
    (SCREAM, &["scream1.wav", "scream2.wav", "scream3.wav", "scream4.wav", "scream5.wav", "scream6.wav"]),
    (MOAN, &["moan1.wav", "moan2.wav", "moan3.wav", "moan4.wav", "moan5.wav"]),
    (GASP, &["gasp1.wav", "gasp2.wav", "gasp3.wav"]),
    (FEAR, &["fear1.wav", "fear2.wav", "fear3.wav", "fear4.wav", "fear5.wav", "fear6.wav", "fear7.wav"]),
];

/// Simulation ticks per second (`acr_sim::ORIGINAL_TICKS_PER_SECOND`).
const SECOND: u64 = 10;
/// Agonies that make sounds at once at most.
pub const MAX_AUDIBLE: usize = 4;
/// A brigade (or a loose unit) shouts in fear at most once in this many ticks.
pub const PANIC_GROUP_TICKS: u64 = 4 * SECOND;
/// Fearful shouts in all at most once in this many ticks.
pub const PANIC_GLOBAL_TICKS: u64 = 12;
/// Kills in hearing within this many ticks feed the din.
pub const DIN_WINDOW: u64 = 15 * SECOND;
/// Kills in [`DIN_WINDOW`] before any din is heard.
pub const DIN_MIN_KILLS: usize = 6;

/// A stable 32-bit hash of `a` and `b` (any mixer: it never reaches the simulation).
fn hash(a: u64, b: u64) -> u32 {
    let mut x = a.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ b.wrapping_add(0x632b_e59b_d9b4_e019);
    x ^= x >> 31;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 29;
    (x >> 16) as u32
}

/// How long the unit `victim` killed by `cause` lies dying (ticks), or `None` if he dies at
/// once. The share and the length grow with the wound: per cent dying and seconds.
pub fn agony_ticks(victim: u32, cause: DeathCause) -> Option<u64> {
    use DeathCause::*;
    let (share, min_s, max_s) = match cause {
        Gunshot => (25, 3, 8),
        Arrow | Thrown => (40, 3, 10),
        Melee => (45, 4, 10),
        Grapeshot => (55, 5, 12),
        Explosion => (35, 4, 10),
        // Killed outright: a cannonball tears a man apart (who survives one is wounded and
        // screams in pain, `hit_sounds`).
        _ => return None,
    };
    let h = hash(u64::from(victim), 0xa90);
    if h % 100 >= share {
        return None;
    }
    let span = (max_s - min_s) * SECOND + 1;
    Some(min_s * SECOND + u64::from(h >> 8) % span)
}

/// A soldier lying dying.
#[derive(Debug, Clone, PartialEq)]
struct Dying {
    victim: u32,
    /// Original screen pixels (`y` halved), as for [`unit_sounds::placed`].
    pos: (f32, f32),
    start: u64,
    end: u64,
    /// The tick of his next sound.
    next: u64,
    /// Sounds made so far.
    sounds: u32,
}

/// A unit that started a flight leg in panic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fleeing {
    pub tick: u64,
    /// The unit's brigade, or its own index for a loose unit.
    pub group: (u8, u32),
    /// Original screen pixels (`y` halved).
    pub pos: (f32, f32),
}

/// The agonies, the panic and the din of a running game.
#[derive(Debug, Default)]
pub struct Agony {
    dying: Vec<Dying>,
    /// The tick of the last hit looked at.
    cursor: u64,
    /// The last fearful shout per group, and of all.
    panic_group: HashMap<(u8, u32), u64>,
    panic_last: Option<u64>,
    /// Ticks of recent kills in hearing.
    kills: VecDeque<u64>,
    din_next: u64,
}

fn distance(pos: (f32, f32), view: [f32; 4]) -> f32 {
    let centre = (view[0] + view[2] / 2.0, view[1] + view[3] / 2.0);
    (pos.0 - centre.0).powi(2) + (pos.1 - centre.1).powi(2)
}

impl Agony {
    /// The sounds up to tick `now`: new agonies from the kills in `hits` after the last
    /// call (`human` tells soldiers from animals by unit index), the next sounds of the
    /// nearest ones, the fearful shouts of `fleeing` and the din, heard in the view
    /// `[left, top, width, height]` under `fog`.
    pub fn update<'a>(
        &mut self,
        hits: impl IntoIterator<Item = &'a HitRecord>,
        fleeing: &[Fleeing],
        human: impl Fn(u32) -> bool,
        now: u64,
        view: [f32; 4],
        fog: Option<&Fog>,
    ) -> Vec<SoundEvent> {
        // A loaded game may start earlier.
        if self.cursor > now {
            *self = Self::default();
        }
        let mut out = Vec::new();
        let after = self.cursor;
        for hit in hits.into_iter().filter(|h| h.tick > after) {
            self.cursor = self.cursor.max(hit.tick);
            if !hit.killed || hit.building || !human(hit.victim) {
                continue;
            }
            let pos = (hit.pos.0 as f32, hit.pos.1 as f32 / 2.0);
            if unit_sounds::placed(pos.0, pos.1, view, fog).is_some() {
                self.kills.push_back(hit.tick);
            }
            if let Some(len) = agony_ticks(hit.victim, hit.cause)
                && !self.dying.iter().any(|d| d.victim == hit.victim)
            {
                self.dying.push(Dying {
                    victim: hit.victim,
                    pos,
                    start: hit.tick,
                    end: hit.tick + len,
                    next: hit.tick,
                    sounds: 0,
                });
            }
        }
        out.extend(self.agonies(now, view, fog));
        out.extend(self.panic(fleeing, view, fog));
        out.extend(self.din(now));
        out
    }

    /// The next sounds of the [`MAX_AUDIBLE`] heard agonies nearest the view centre.
    fn agonies(&mut self, now: u64, view: [f32; 4], fog: Option<&Fog>) -> Vec<SoundEvent> {
        self.dying.retain(|d| d.end > now);
        let mut heard: Vec<(f32, usize, f32, f32)> = self
            .dying
            .iter()
            .enumerate()
            .filter_map(|(i, d)| {
                let (volume, pan) = unit_sounds::placed(d.pos.0, d.pos.1, view, fog)?;
                Some((distance(d.pos, view), i, volume, pan))
            })
            .collect();
        heard.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let voiced: Vec<_> = heard.iter().take(MAX_AUDIBLE).map(|h| h.1).collect();
        let mut out = Vec::new();
        for (i, d) in self.dying.iter_mut().enumerate() {
            if d.next > now {
                continue;
            }
            let Some(&(_, _, volume, pan)) = heard.iter().find(|h| h.1 == i).filter(|_| voiced.contains(&i)) else {
                // Silent while others are nearer or he is out of hearing: no backlog, and
                // no late scream.
                d.next = now + SECOND;
                d.sounds = d.sounds.max(1);
                continue;
            };
            let h = hash(u64::from(d.victim), u64::from(d.sounds));
            // The scream at the fall; then moans and gasps, quieter as life ebbs away.
            let (name, fade) = if d.sounds == 0 {
                (SCREAM, 1.0)
            } else {
                let left = (d.end - now) as f32 / (d.end - d.start).max(1) as f32;
                (if h.is_multiple_of(3) { GASP } else { MOAN }, 0.25 + 0.6 * left)
            };
            out.push(SoundEvent { name: name.to_owned(), pan, volume: volume * fade });
            d.sounds += 1;
            // 1.2–2.8 s to the next sound (the scream itself lasts up to 3 s).
            let pause = if d.sounds == 1 { 25 } else { 12 } + u64::from(h >> 8) % 17;
            d.next = now + pause;
        }
        out
    }

    /// One fearful shout per group and pause, the one nearest the view centre.
    fn panic(&mut self, fleeing: &[Fleeing], view: [f32; 4], fog: Option<&Fog>) -> Vec<SoundEvent> {
        let mut best: Option<(f32, &Fleeing, f32, f32)> = None;
        for f in fleeing {
            if self.panic_group.get(&f.group).is_some_and(|&t| f.tick < t + PANIC_GROUP_TICKS) {
                continue;
            }
            if self.panic_last.is_some_and(|t| f.tick < t + PANIC_GLOBAL_TICKS) {
                continue;
            }
            let Some((volume, pan)) = unit_sounds::placed(f.pos.0, f.pos.1, view, fog) else { continue };
            let d = distance(f.pos, view);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, f, volume, pan));
            }
        }
        let Some((_, f, volume, pan)) = best else { return Vec::new() };
        self.panic_group.insert(f.group, f.tick);
        self.panic_last = Some(f.tick);
        self.panic_group.retain(|_, t| *t + PANIC_GROUP_TICKS > f.tick);
        vec![SoundEvent { name: FEAR.to_owned(), pan, volume }]
    }

    /// Now and then a distant scream or shout while many have died nearby lately.
    fn din(&mut self, now: u64) -> Vec<SoundEvent> {
        while self.kills.front().is_some_and(|&t| t + DIN_WINDOW <= now) {
            self.kills.pop_front();
        }
        let kills = self.kills.len();
        if kills < DIN_MIN_KILLS || now < self.din_next {
            return Vec::new();
        }
        let h = hash(now, 0xd1);
        // The more dead, the sooner the next: 6 kills about every 4 s, 30 and more every 1 s.
        let pause = (24 * SECOND / kills as u64).clamp(SECOND, 4 * SECOND);
        self.din_next = now + pause + u64::from(h % 8);
        let name = if h & 0x100 == 0 { SCREAM } else { FEAR };
        // Somewhere to the left or right, beyond the view: quiet and panned.
        let side = if h & 0x200 == 0 { -1.0 } else { 1.0 };
        let pan = side * (0.5 + (h >> 10 & 0xff) as f32 / 255.0 * 0.4);
        let volume = 0.15 + (kills.min(30) as f32 / 30.0) * 0.2;
        vec![SoundEvent { name: name.to_owned(), pan, volume }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: [f32; 4] = [1000.0, 500.0, 1024.0, 768.0];

    fn kill(tick: u64, victim: u32, x: i32, cause: DeathCause) -> HitRecord {
        // y = 2 × 884: the view centre's row.
        HitRecord { tick, victim, pos: (x, 1768), dir: (1, 0), cause, killed: true, building: false }
    }

    fn dying(cause: DeathCause) -> Vec<u32> {
        (0..1000).filter(|&v| agony_ticks(v, cause).is_some()).collect()
    }

    #[test]
    fn heavy_wounds_die_slowly_more_often() {
        use DeathCause::*;
        let (gun, grape, melee) = (dying(Gunshot).len(), dying(Grapeshot).len(), dying(Melee).len());
        assert!((200..300).contains(&gun), "{gun}");
        assert!((500..600).contains(&grape), "{grape}");
        assert!(gun < melee && melee < grape);
        assert!(dying(Burned).is_empty() && dying(Hunger).is_empty());
        for v in dying(Grapeshot) {
            let t = agony_ticks(v, Grapeshot).unwrap();
            assert!((50..=120).contains(&t));
            // Fixed per unit.
            assert_eq!(agony_ticks(v, Grapeshot), Some(t));
        }
    }

    #[test]
    fn struck_dead_by_a_cannonball_is_silent() {
        assert!(dying(DeathCause::Cannonball).is_empty());
        // Fewer kills than the din needs.
        let hits: Vec<_> = (0..DIN_MIN_KILLS as u32 - 1).map(|v| kill(1, v, 1512, DeathCause::Cannonball)).collect();
        let mut a = Agony::default();
        let sounds = a.update(&hits, &[], |_| true, 1, VIEW, None);
        assert!(sounds.iter().all(|e| e.name != SCREAM && e.name != MOAN && e.name != GASP));
    }

    fn names(events: &[SoundEvent]) -> Vec<&str> {
        events.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn a_dying_soldier_screams_then_moans_ever_quieter() {
        let victim =
            dying(DeathCause::Grapeshot).into_iter().max_by_key(|&v| agony_ticks(v, DeathCause::Grapeshot)).unwrap();
        let len = agony_ticks(victim, DeathCause::Grapeshot).unwrap();
        let hits = [kill(5, victim, 1512, DeathCause::Grapeshot)];
        let mut a = Agony::default();
        let mut sounds = Vec::new();
        for now in 5..5 + len + 50 {
            sounds.extend(a.update(&hits, &[], |_| true, now, VIEW, None));
        }
        assert_eq!(sounds[0].name, SCREAM);
        assert!(sounds.len() >= 3, "{}", sounds.len());
        assert!(sounds[1..].iter().all(|e| e.name == MOAN || e.name == GASP));
        assert!(sounds.last().unwrap().volume < sounds[1].volume);
        // Over once he is dead.
        assert!(a.dying.is_empty());
    }

    #[test]
    fn only_the_nearest_few_are_heard() {
        // Fewer kills than the din needs.
        let victims: Vec<_> = dying(DeathCause::Grapeshot).into_iter().take(DIN_MIN_KILLS - 1).collect();
        // Five dying on a line to the right of the centre; the nearest are the first.
        let hits: Vec<_> =
            victims.iter().enumerate().map(|(i, &v)| kill(1, v, 1512 + 10 * i as i32, DeathCause::Grapeshot)).collect();
        let mut a = Agony::default();
        let first = a.update(&hits, &[], |_| true, 1, VIEW, None);
        assert_eq!(names(&first), [SCREAM; MAX_AUDIBLE]);
        // Not the farthest, 40 px off: pan 80, its left channel would lose 0.8 dB.
        assert!(first.iter().all(|e| e.pan < 0.08));
        // Animals and units out of hearing do not count.
        let mut b = Agony::default();
        assert!(b.update(&hits, &[], |_| false, 1, VIEW, None).is_empty());
        let far: Vec<_> = victims.iter().map(|&v| kill(1, v, 5000, DeathCause::Grapeshot)).collect();
        assert!(Agony::default().update(&far, &[], |_| true, 1, VIEW, None).is_empty());
    }

    #[test]
    fn panic_shouts_per_group_and_in_all() {
        let flee = |tick, group, x| Fleeing { tick, group: (1, group), pos: (x, 884.0) };
        let mut a = Agony::default();
        let none: [HitRecord; 0] = [];
        // A whole brigade flees: one shout, from the nearest.
        let out =
            a.update(&none, &[flee(10, 7, 1600.0), flee(10, 7, 1520.0), flee(10, 8, 1700.0)], |_| true, 10, VIEW, None);
        assert_eq!(names(&out), [FEAR]);
        assert!(out[0].pan < 0.2);
        // Too soon for anyone.
        assert!(a.update(&none, &[flee(12, 8, 1512.0)], |_| true, 12, VIEW, None).is_empty());
        // Another group after the global pause; brigade 7 still waits.
        assert!(a.update(&none, &[flee(22, 7, 1512.0)], |_| true, 22, VIEW, None).is_empty());
        assert_eq!(a.update(&none, &[flee(22, 8, 1512.0)], |_| true, 22, VIEW, None).len(), 1);
        assert_eq!(a.update(&none, &[flee(50, 7, 1512.0)], |_| true, 50, VIEW, None).len(), 1);
    }

    #[test]
    fn many_kills_raise_a_din() {
        let mut a = Agony::default();
        // Killed by gunshots that do not linger, out of the agony.
        let quick: Vec<_> = (0..1000).filter(|&v| agony_ticks(v, DeathCause::Gunshot).is_none()).take(20).collect();
        let few: Vec<_> = quick[..3].iter().map(|&v| kill(1, v, 1512, DeathCause::Gunshot)).collect();
        assert!((1..100).all(|now| a.update(&few, &[], |_| true, now, VIEW, None).is_empty()));
        let mut b = Agony::default();
        let many: Vec<_> = quick.iter().map(|&v| kill(1, v, 1512, DeathCause::Gunshot)).collect();
        let din: Vec<_> = (1..100).flat_map(|now| b.update(&many, &[], |_| true, now, VIEW, None)).collect();
        assert!(din.len() >= 5, "{}", din.len());
        assert!(din.iter().all(|e| e.volume < 0.5 && e.pan.abs() >= 0.5));
        // It dies down once the kills are old.
        assert!((200..300).all(|now| b.update(&many, &[], |_| true, now, VIEW, None).is_empty()));
    }

    #[test]
    fn every_group_has_files() {
        let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sounds");
        for (_, names) in GROUPS {
            for name in *names {
                let bytes = std::fs::read(shipped.join(name)).unwrap();
                acr_audio::Sound::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            }
        }
    }
}
