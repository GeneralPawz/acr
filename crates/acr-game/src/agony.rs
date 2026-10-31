//! Agony of the dying, fearful screams of panicking troops and the din of a bloody battle
//! (#248, not in the original), on top of the hit and pain sounds of [`crate::hit_sounds`].
//!
//! - **Agony:** a share of the soldiers killed by a hit (`World::hits`) screams as he falls
//!   ([`SCREAM`]) and then moans and gasps ([`MOAN`], [`GASP`]) ever quieter for 3–12 s
//!   where he lies. Who does is fixed by the unit index and the cause ([`agony_ticks`]):
//!   a musket ball often kills outright, grapeshot, shells and blades seldom. A man struck
//!   dead by a cannonball is torn apart and makes no sound: only the wounded scream.
//!   Only the [`Mix::max_dying`] agonies nearest the view centre make sounds; the others wait
//!   silently while they last, also when they are out of hearing.
//! - **Wounded by shell fragments** (hit-locations rule, `acr_sim::shrapnel`): a man a
//!   fragment struck but did not kill cries out like the dying. A critical wound (a vital
//!   hit, a limb off, the belly open, an open break) keeps him screaming and moaning until
//!   he dies ([`CRY_ON_TICKS`] between sounds at most); a light one stops after
//!   [`LIGHT_CRY_TICKS`]. They share the [`Mix::max_dying`] voices with the dying.
//! - **Badly wounded survivors** (hit-locations rule, #253): a living man whose body
//!   ([`severity`]) is at least [`Mix::severity`] and whose life is not full moans and gasps
//!   now and then where he is, less often than the dying ([`Mix::wounded_pause`]); he falls
//!   silent when he is healed (full life), dead or out of hearing.
//! - **Burning men** (`Unit::burning`, #253) scream again and again at full strength
//!   ([`Mix::burning_pause`]) with the agony screams (there are no fire samples). Only the
//!   [`Mix::max_wounded`] and [`Mix::max_burning`] nearest are heard, their sounds in all at
//!   most once in [`Mix::suffer_gap`] ticks, and none in an update the dying are heard in.
//! - **Panic:** units that start a flight leg in panic (`World::shouts`, 0x600bc8) shout in
//!   fear ([`FEAR`]): one shout per brigade (or loose unit) every [`Mix::panic_group`], and
//!   one in all every [`Mix::panic_gap`], the one nearest the view centre.
//! - **Din:** with many recent kills in hearing ([`Mix::din_window`]), now and then a distant
//!   scream or shout from somewhere at the edge of the view, quiet and panned.
//!
//! All the numbers of the mix are in [`Mix`]; natively `ACR_AGONY_MIX` overrides them
//! (`max_dying=3,din_min_kills=10`, [`Mix::from_env`]) so the mix can be tuned by ear
//! without a build, and a debug log line per second of game time counts what played
//! (`agony mix`).
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
/// How long a man lightly wounded by a shell fragment cries out (ticks): 3–5 s.
pub const LIGHT_CRY_TICKS: (u64, u64) = (3 * SECOND, 5 * SECOND);
/// A critically wounded man cries out until he dies; at most this long (ticks) without a
/// death seen, so a lost unit does not cry for ever.
pub const CRY_ON_TICKS: u64 = 600 * SECOND;

/// The mix of the agony, panic and din voices (#253): caps, rates and volumes in one place
/// to tune by ear. Times are simulation ticks (10 a second at normal speed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mix {
    /// All these sounds together: on average at most this many a second, and at most
    /// [`Mix::burst`] at once; the lowest in rank give way (the din, then the wounded and
    /// burning, the moans of the dying, fear, the screams of the falling).
    pub max_per_second: f32,
    pub burst: f32,
    /// The dying (and the fragment-wounded) heard at once at most, the nearest.
    pub max_dying: usize,
    /// Badly wounded survivors heard at once at most, the nearest.
    pub max_wounded: usize,
    /// Burning men heard at once at most, the nearest.
    pub max_burning: usize,
    /// Wounded and burning men's sounds in all at most once in this many ticks.
    pub suffer_gap: u64,
    /// Ticks between a badly wounded man's sounds (from, to).
    pub wounded_pause: (u64, u64),
    /// Ticks between a burning man's screams (from, to).
    pub burning_pause: (u64, u64),
    /// Volume factor of a badly wounded man's moans.
    pub wounded_volume: f32,
    /// Volume factor of a burning man's screams.
    pub burning_volume: f32,
    /// The [`severity`] from which a living man moans.
    pub severity: u32,
    /// A brigade (or a loose unit) shouts in fear at most once in this many ticks.
    pub panic_group: u64,
    /// Fearful shouts in all at most once in this many ticks.
    pub panic_gap: u64,
    /// Kills in hearing within this many ticks feed the din.
    pub din_window: u64,
    /// Kills in [`Mix::din_window`] before any din is heard.
    pub din_min_kills: usize,
    /// The din's pause is `din_ticks / kills`, clamped to [`Mix::din_pause`], plus up to
    /// 0.7 s.
    pub din_ticks: u64,
    pub din_pause: (u64, u64),
}

impl Mix {
    pub const DEFAULT: Mix = Mix {
        max_per_second: 2.0,
        burst: 4.0,
        max_dying: 4,
        max_wounded: 2,
        max_burning: 2,
        suffer_gap: 15,
        wounded_pause: (5 * SECOND, 10 * SECOND),
        burning_pause: (12, 22),
        wounded_volume: 0.6,
        burning_volume: 1.0,
        severity: 4,
        panic_group: 4 * SECOND,
        panic_gap: 12,
        din_window: 15 * SECOND,
        din_min_kills: 6,
        din_ticks: 24 * SECOND,
        din_pause: (SECOND, 4 * SECOND),
    };

    /// [`Mix::DEFAULT`] with the `key=value` pairs of `text` over it (comma or space
    /// separated, the field names; ranges as `wounded_pause=40-90`); bad pairs are logged.
    pub fn parse(text: &str) -> Mix {
        let mut m = Mix::DEFAULT;
        for pair in text.split([',', ' ']).filter(|p| !p.is_empty()) {
            let Some((k, v)) = pair.split_once('=') else {
                log::warn!("ACR_AGONY_MIX: cannot use {pair:?}");
                continue;
            };
            let n = |v: &str| v.trim().parse::<u64>().ok();
            let range = |v: &str| {
                let (a, b) = v.split_once('-')?;
                let (a, b) = (n(a)?, n(b)?);
                Some((a, b.max(a)))
            };
            let f = |v: &str| v.trim().parse::<f32>().ok();
            let ok = match k.trim() {
                "max_per_second" => f(v).map(|x| m.max_per_second = x),
                "burst" => f(v).map(|x| m.burst = x),
                "max_dying" => n(v).map(|x| m.max_dying = x as usize),
                "max_wounded" => n(v).map(|x| m.max_wounded = x as usize),
                "max_burning" => n(v).map(|x| m.max_burning = x as usize),
                "suffer_gap" => n(v).map(|x| m.suffer_gap = x),
                "wounded_pause" => range(v).map(|x| m.wounded_pause = x),
                "burning_pause" => range(v).map(|x| m.burning_pause = x),
                "wounded_volume" => f(v).map(|x| m.wounded_volume = x),
                "burning_volume" => f(v).map(|x| m.burning_volume = x),
                "severity" => n(v).map(|x| m.severity = x as u32),
                "panic_group" => n(v).map(|x| m.panic_group = x),
                "panic_gap" => n(v).map(|x| m.panic_gap = x),
                "din_window" => n(v).map(|x| m.din_window = x),
                "din_min_kills" => n(v).map(|x| m.din_min_kills = x as usize),
                "din_ticks" => n(v).map(|x| m.din_ticks = x),
                "din_pause" => range(v).map(|x| m.din_pause = x),
                _ => None,
            };
            if ok.is_none() {
                log::warn!("ACR_AGONY_MIX: cannot use {pair:?}");
            }
        }
        m
    }

    /// The mix of `ACR_AGONY_MIX` natively, else [`Mix::DEFAULT`] (also in the browser).
    pub fn from_env() -> Mix {
        match std::env::var("ACR_AGONY_MIX") {
            Ok(text) => {
                let m = Mix::parse(&text);
                log::info!("agony mix {m:?}");
                m
            }
            Err(_) => Mix::DEFAULT,
        }
    }
}

impl Default for Mix {
    fn default() -> Self {
        Mix::DEFAULT
    }
}

/// How badly a man is hurt, from his body (hit-locations rule; 0 unhurt): a limb off 3, an
/// open break 3, a closed one 2, a joint out 1, a flesh wound 1 (3 a limb at most), a head
/// or chest wound 2, the belly open 6, a vital hit bleeding out 4, the spine 4, a lung 3,
/// bleeding fragment wounds 1. From [`Mix::severity`] 4 on (a limb off and a wound, two
/// chest wounds, an open break and a wound) a living man moans.
pub fn severity(body: &acr_sim::body::Body) -> u32 {
    use acr_sim::body::FractureKind;
    let mut s = 0;
    for limb in &body.limbs {
        if limb.severed() {
            s += 3;
        }
        s += match limb.fracture.map(|f| f.kind) {
            Some(FractureKind::Open) => 3,
            Some(FractureKind::Closed) => 2,
            Some(FractureKind::Dislocated) => 1,
            None => 0,
        };
        s += u32::from(limb.wounds.min(3));
    }
    s += 2 * u32::from(body.head_wounds) + 2 * u32::from(body.chest_wounds);
    s += 6 * u32::from(body.belly_open) + 3 * u32::from(body.collapsed);
    s += if body.bleed_out > 0 { 4 } else { 0 } + if body.spine > 0 { 4 } else { 0 };
    s + u32::from(body.seep_s > 0)
}

/// A living man who suffers: badly wounded or burning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Suffering {
    pub victim: u32,
    /// Original screen pixels (`y` halved).
    pub pos: (f32, f32),
    /// His [`severity`].
    pub severity: u32,
    pub burning: bool,
}

/// What played in one second of game time (the `agony mix` log line).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Counts {
    pub scream: u32,
    pub moan: u32,
    pub gasp: u32,
    pub wounded: u32,
    pub burning: u32,
    pub fear: u32,
    pub din: u32,
    /// Sounds left out over the budget ([`Mix::max_per_second`]).
    pub dropped: u32,
    /// The most dying, badly wounded and burning men in hearing at once in that second.
    pub heard: [u32; 3],
}

/// Where a sound comes from, for [`Counts`].
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Dying,
    Suffer,
    Fear,
    Din,
}

impl Counts {
    /// All sounds that played.
    pub fn events(&self) -> u32 {
        self.scream + self.moan + self.gasp + self.wounded + self.burning + self.fear + self.din
    }
}

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
    /// Wounded, not dying (shell fragments): his cries go on until `end` or his death, at
    /// full strength.
    wounded: bool,
}

/// How long a man wounded but not killed by `hit` cries out (ticks), if he does: a shell
/// fragment's critical wound until he dies, a light one a few seconds.
pub fn cry_ticks(hit: &HitRecord) -> Option<u64> {
    use acr_sim::body::{FractureKind, Outcome};
    if hit.killed || hit.building || hit.cause != DeathCause::Explosion {
        return None;
    }
    let injury = hit.injury?;
    let critical = hit.critical.is_some()
        || matches!(injury.outcome, Outcome::Off | Outcome::BellyOpen | Outcome::Fracture(FractureKind::Open));
    if critical {
        return Some(CRY_ON_TICKS);
    }
    let (min, max) = LIGHT_CRY_TICKS;
    Some(min + u64::from(hash(u64::from(hit.victim), hit.tick)) % (max - min + 1))
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
    mix: Mix,
    dying: Vec<Dying>,
    /// The tick of the last hit looked at.
    cursor: u64,
    /// The last fearful shout per group, and of all.
    panic_group: HashMap<(u8, u32), u64>,
    panic_last: Option<u64>,
    /// Ticks of recent kills in hearing.
    kills: VecDeque<u64>,
    din_next: u64,
    /// The next sound of each suffering man (badly wounded or burning) still suffering.
    suffer_next: HashMap<u32, u64>,
    /// The tick of the last wounded or burning sound.
    suffer_last: Option<u64>,
    /// The second of game time counted, and its counts.
    second: u64,
    counts: Counts,
    /// Sounds left in the budget, and its tick.
    budget: Option<(f32, u64)>,
}

fn distance(pos: (f32, f32), view: [f32; 4]) -> f32 {
    let centre = (view[0] + view[2] / 2.0, view[1] + view[3] / 2.0);
    (pos.0 - centre.0).powi(2) + (pos.1 - centre.1).powi(2)
}

impl Agony {
    pub fn new(mix: Mix) -> Self {
        Self { mix, ..Self::default() }
    }

    /// The sounds up to tick `now`: new agonies from the kills in `hits` after the last
    /// call (`human` tells soldiers from animals by unit index), the next sounds of the
    /// nearest ones and of the `suffering` men, the fearful shouts of `fleeing` and the din,
    /// heard in the view `[left, top, width, height]` under `fog`.
    #[allow(clippy::too_many_arguments)]
    pub fn update<'a>(
        &mut self,
        hits: impl IntoIterator<Item = &'a HitRecord>,
        fleeing: &[Fleeing],
        suffering: &[Suffering],
        human: impl Fn(u32) -> bool,
        alive: impl Fn(u32) -> bool,
        now: u64,
        view: [f32; 4],
        fog: Option<&Fog>,
    ) -> Vec<SoundEvent> {
        // A loaded game may start earlier.
        if self.cursor > now || self.second > now / SECOND {
            *self = Self::new(self.mix);
        }
        let mut out = Vec::new();
        let after = self.cursor;
        for hit in hits.into_iter().filter(|h| h.tick > after) {
            self.cursor = self.cursor.max(hit.tick);
            if !hit.killed
                && human(hit.victim)
                && let Some(len) = cry_ticks(hit)
            {
                let end = hit.tick + len;
                match self.dying.iter_mut().find(|d| d.victim == hit.victim) {
                    // A worse wound cries longer.
                    Some(d) => d.end = d.end.max(end),
                    None => self.dying.push(Dying {
                        victim: hit.victim,
                        pos: (hit.pos.0 as f32, hit.pos.1 as f32 / 2.0),
                        start: hit.tick,
                        end,
                        next: hit.tick,
                        sounds: 0,
                        wounded: true,
                    }),
                }
                continue;
            }
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
                    wounded: false,
                });
            }
        }
        // The wounded stop crying when they die (a death of their own may start an agony).
        self.dying.retain(|d| !d.wounded || alive(d.victim));
        let dying = self.agonies(now, view, fog);
        let suffer = self.suffer(suffering, !dying.is_empty(), now, view, fog);
        let fear = self.panic(fleeing, view, fog);
        let din = self.din(now);
        // In the order they give way to the budget: the screams of the falling, fear, the
        // moans of the dying, the burning and the wounded, the din.
        let (screams, moans): (Vec<_>, Vec<_>) = dying.into_iter().partition(|e| e.name == SCREAM);
        let tag = |kind: Kind, v: Vec<SoundEvent>| v.into_iter().map(move |e| (kind, e));
        let mut all: Vec<(Kind, SoundEvent)> = tag(Kind::Dying, screams)
            .chain(tag(Kind::Fear, fear))
            .chain(tag(Kind::Dying, moans))
            .chain(tag(Kind::Suffer, suffer))
            .chain(tag(Kind::Din, din))
            .collect();
        let mix = self.mix;
        let budget = match self.budget {
            Some((b, t)) => (b + now.saturating_sub(t) as f32 * mix.max_per_second / SECOND as f32).min(mix.burst),
            None => mix.burst,
        };
        let keep = (budget.max(0.0).floor() as usize).min(all.len());
        self.budget = Some((budget - keep as f32, now));
        let dropped = all.len() - keep;
        all.truncate(keep);
        self.count(now, &all, dropped);
        out.extend(all.into_iter().map(|(_, e)| e));
        out
    }

    /// Counts what played per second of game time and logs each second with any sound.
    fn count(&mut self, now: u64, played: &[(Kind, SoundEvent)], dropped: usize) {
        let second = now / SECOND;
        if second != self.second {
            let c = self.counts;
            if c.events() > 0 {
                log::debug!(
                    "agony mix s {} events {} (scream {} moan {} gasp {} wounded {} burning {} fear {} din {}) \
                     over budget {} heard dying {} wounded {} burning {}",
                    self.second,
                    c.events(),
                    c.scream,
                    c.moan,
                    c.gasp,
                    c.wounded,
                    c.burning,
                    c.fear,
                    c.din,
                    c.dropped,
                    c.heard[0],
                    c.heard[1],
                    c.heard[2],
                );
            }
            self.second = second;
            self.counts = Counts::default();
        }
        let c = &mut self.counts;
        c.dropped += dropped as u32;
        for (kind, e) in played {
            match (kind, e.name.as_str()) {
                (Kind::Dying, SCREAM) => c.scream += 1,
                (Kind::Dying, GASP) => c.gasp += 1,
                (Kind::Dying, _) => c.moan += 1,
                (Kind::Suffer, SCREAM) => c.burning += 1,
                (Kind::Suffer, _) => c.wounded += 1,
                (Kind::Fear, _) => c.fear += 1,
                (Kind::Din, _) => c.din += 1,
            }
        }
    }

    /// The next sound of the badly wounded and burning men among `suffering`: only the
    /// [`Mix::max_wounded`] and [`Mix::max_burning`] heard nearest the view centre, one
    /// sound in all at most every [`Mix::suffer_gap`], none while `dying_heard`.
    fn suffer(
        &mut self,
        suffering: &[Suffering],
        dying_heard: bool,
        now: u64,
        view: [f32; 4],
        fog: Option<&Fog>,
    ) -> Vec<SoundEvent> {
        let mix = self.mix;
        // Healed, dead or no longer burning: forgotten (a new wound starts afresh).
        let suffers = |s: &&Suffering| s.burning || s.severity >= mix.severity;
        let now_suffering: Vec<&Suffering> = suffering
            .iter()
            .filter(suffers)
            // The dying and the fragment-wounded have their own voices.
            .filter(|s| !self.dying.iter().any(|d| d.victim == s.victim))
            .collect();
        self.suffer_next.retain(|v, _| now_suffering.iter().any(|s| s.victim == *v));
        let mut heard: Vec<(bool, f32, &Suffering, f32, f32)> = Vec::new();
        for s in now_suffering {
            // The first sound comes at a random time within his pause, so that a crowd does
            // not start together.
            self.suffer_next.entry(s.victim).or_insert_with(|| {
                let (a, b) = if s.burning { (0, mix.burning_pause.0) } else { mix.wounded_pause };
                now + u64::from(hash(u64::from(s.victim), 0x5f)) % (b - a + 1) + a
            });
            let Some((volume, pan)) = unit_sounds::placed(s.pos.0, s.pos.1, view, fog) else { continue };
            heard.push((s.burning, distance(s.pos, view), s, volume, pan));
        }
        heard.sort_by(|a, b| a.0.cmp(&b.0).reverse().then(a.1.total_cmp(&b.1)).then(a.2.victim.cmp(&b.2.victim)));
        let burning = heard.iter().filter(|h| h.0).count();
        let voiced: Vec<_> = heard
            .iter()
            .take(burning.min(mix.max_burning))
            .chain(heard.iter().skip(burning).take(mix.max_wounded))
            .collect();
        let c = &mut self.counts.heard;
        c[1] = c[1].max((heard.len() - burning) as u32);
        c[2] = c[2].max(burning as u32);
        let gap = self.suffer_last.is_some_and(|t| now < t + mix.suffer_gap);
        // The burning first, then the nearest whose time has come.
        let due = voiced.iter().find(|h| self.suffer_next.get(&h.2.victim).is_some_and(|&t| t <= now));
        let mut out = Vec::new();
        if let Some(&&(burns, _, s, volume, pan)) = due
            && !gap
            && !dying_heard
        {
            let h = hash(u64::from(s.victim), now);
            let (name, pause, gain) = if burns {
                (SCREAM, mix.burning_pause, mix.burning_volume)
            } else {
                // Worse wounds louder: 70 % at the threshold, full from twice it.
                let worse = (s.severity as f32 / mix.severity.max(1) as f32 - 1.0).clamp(0.0, 1.0);
                (
                    if h.is_multiple_of(3) { GASP } else { MOAN },
                    mix.wounded_pause,
                    mix.wounded_volume * (0.7 + 0.3 * worse),
                )
            };
            out.push(SoundEvent { name: name.to_owned(), pan, volume: (volume * gain).min(1.0), at: None });
            self.suffer_last = Some(now);
            let (a, b) = pause;
            self.suffer_next.insert(s.victim, now + a + u64::from(h >> 8) % (b - a + 1));
        }
        // The voiced whose time has come try again in the next update; the others (out of
        // hearing, or others are nearer) wait without a backlog.
        let voiced: Vec<u32> = voiced.iter().map(|h| h.2.victim).collect();
        for (v, next) in self.suffer_next.iter_mut() {
            if *next <= now && !voiced.contains(v) {
                *next = now + SECOND;
            }
        }
        out
    }

    /// The next sounds of the [`Mix::max_dying`] heard agonies nearest the view centre.
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
        let voiced: Vec<_> = heard.iter().take(self.mix.max_dying).map(|h| h.1).collect();
        self.counts.heard[0] = self.counts.heard[0].max(heard.len() as u32);
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
            } else if d.wounded {
                // The wounded scream and moan on, not ebbing away.
                (if h.is_multiple_of(3) { SCREAM } else { MOAN }, 0.7 + 0.3 * ((h >> 4) % 4) as f32 / 3.0)
            } else {
                let left = (d.end - now) as f32 / (d.end - d.start).max(1) as f32;
                (if h.is_multiple_of(3) { GASP } else { MOAN }, 0.25 + 0.6 * left)
            };
            out.push(SoundEvent { name: name.to_owned(), pan, volume: volume * fade, at: None });
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
            if self.panic_group.get(&f.group).is_some_and(|&t| f.tick < t + self.mix.panic_group) {
                continue;
            }
            if self.panic_last.is_some_and(|t| f.tick < t + self.mix.panic_gap) {
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
        let panic_group = self.mix.panic_group;
        self.panic_group.retain(|_, t| *t + panic_group > f.tick);
        vec![SoundEvent { name: FEAR.to_owned(), pan, volume, at: None }]
    }

    /// Now and then a distant scream or shout while many have died nearby lately.
    fn din(&mut self, now: u64) -> Vec<SoundEvent> {
        while self.kills.front().is_some_and(|&t| t + self.mix.din_window <= now) {
            self.kills.pop_front();
        }
        let kills = self.kills.len();
        if kills < self.mix.din_min_kills || now < self.din_next {
            return Vec::new();
        }
        let h = hash(now, 0xd1);
        // The more dead, the sooner the next: 6 kills about every 4 s, 30 and more every 1 s.
        let (min, max) = self.mix.din_pause;
        let pause = (self.mix.din_ticks / kills as u64).clamp(min, max.max(min));
        self.din_next = now + pause + u64::from(h % 8);
        let name = if h & 0x100 == 0 { SCREAM } else { FEAR };
        // Somewhere to the left or right, beyond the view: quiet and panned.
        let side = if h & 0x200 == 0 { -1.0 } else { 1.0 };
        let pan = side * (0.5 + (h >> 10 & 0xff) as f32 / 255.0 * 0.4);
        let volume = 0.15 + (kills.min(30) as f32 / 30.0) * 0.2;
        vec![SoundEvent { name: name.to_owned(), pan, volume, at: None }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The deep, slowed-down moans sounded like a zombie (#344): no growl or roar sample among
    /// the agony voices, and none lowered by more than 10 % (under two semitones), as
    /// `assets/sounds/CREDITS.txt` records each one's speed.
    #[test]
    fn agony_voices_are_human_pitched() {
        for (_, names) in GROUPS {
            for name in *names {
                assert!(!name.contains("growl") && !name.contains("roar"), "{name} is not a human voice");
            }
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sounds/CREDITS.txt");
        let credits = std::fs::read_to_string(path).unwrap();
        let mut factors = 0;
        for (i, m) in credits.match_indices("slowed to ") {
            let rest = &credits[i + m.len()..];
            let end = rest.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(rest.len());
            let f: f32 = rest[..end].parse().unwrap();
            assert!(f >= 0.9, "a sample slowed to {f} sounds like a growl");
            factors += 1;
        }
        assert!(factors > 0, "the speed notes in CREDITS.txt are not found");
        assert!(credits.contains("moan1-5.wav") && !credits.contains("0.65-0.8 (lower)"));
    }

    const VIEW: [f32; 4] = [1000.0, 500.0, 1024.0, 768.0];

    fn kill(tick: u64, victim: u32, x: i32, cause: DeathCause) -> HitRecord {
        // y = 2 × 884: the view centre's row.
        HitRecord {
            tick,
            victim,
            pos: (x, 1768),
            dir: (1, 0),
            cause,
            killed: true,
            building: false,
            region: None,
            injury: None,
            strike: None,
            critical: None,
            fire: false,
        }
    }

    fn fragment(tick: u64, victim: u32, outcome: acr_sim::body::Outcome) -> HitRecord {
        use acr_sim::body::{Cause, Injury, LimbId, Part};
        let injury = Injury { part: Part::Limb(LimbId::LegL), level: 100, outcome, energy: 40, cause: Cause::Shell };
        HitRecord { killed: false, injury: Some(injury), ..kill(tick, victim, 1512, DeathCause::Explosion) }
    }

    #[test]
    fn a_fragment_wound_cries_briefly_a_critical_one_until_death() {
        use acr_sim::body::Outcome;
        // Light: a few seconds, then silence while he lives on.
        let hits = [fragment(1, 3, Outcome::Wound)];
        let mut a = Agony::default();
        let mut sounds = Vec::new();
        for now in 1..200 {
            sounds.extend(a.update(&hits, &[], &[], |_| true, |_| true, now, VIEW, None));
        }
        assert!(!sounds.is_empty() && sounds.len() <= 4, "{}", sounds.len());
        assert!(a.dying.is_empty());
        // Critical (a leg off): on and on while he lives, silent once he is dead.
        let hits = [fragment(1, 4, Outcome::Off)];
        let mut a = Agony::default();
        let mut n = 0;
        for now in 1..600 {
            n += a.update(&hits, &[], &[], |_| true, |_| now < 400, now, VIEW, None).len();
        }
        assert!(n > 15, "{n}");
        assert!(a.dying.is_empty());
        // Not for the killed, not for other causes.
        assert_eq!(cry_ticks(&HitRecord { cause: DeathCause::Gunshot, ..fragment(1, 5, Outcome::Wound) }), None);
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
        let hits: Vec<_> =
            (0..Mix::DEFAULT.din_min_kills as u32 - 1).map(|v| kill(1, v, 1512, DeathCause::Cannonball)).collect();
        let mut a = Agony::default();
        let sounds = a.update(&hits, &[], &[], |_| true, |_| true, 1, VIEW, None);
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
            sounds.extend(a.update(&hits, &[], &[], |_| true, |_| true, now, VIEW, None));
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
        let victims: Vec<_> = dying(DeathCause::Grapeshot).into_iter().take(Mix::DEFAULT.din_min_kills - 1).collect();
        // Five dying on a line to the right of the centre; the nearest are the first.
        let hits: Vec<_> =
            victims.iter().enumerate().map(|(i, &v)| kill(1, v, 1512 + 10 * i as i32, DeathCause::Grapeshot)).collect();
        let mut a = Agony::default();
        let first = a.update(&hits, &[], &[], |_| true, |_| true, 1, VIEW, None);
        assert_eq!(names(&first), [SCREAM; Mix::DEFAULT.max_dying]);
        // Not the farthest, 40 px off: pan 80, its left channel would lose 0.8 dB.
        assert!(first.iter().all(|e| e.pan < 0.08));
        // Animals and units out of hearing do not count.
        let mut b = Agony::default();
        assert!(b.update(&hits, &[], &[], |_| false, |_| true, 1, VIEW, None).is_empty());
        let far: Vec<_> = victims.iter().map(|&v| kill(1, v, 5000, DeathCause::Grapeshot)).collect();
        assert!(Agony::default().update(&far, &[], &[], |_| true, |_| true, 1, VIEW, None).is_empty());
    }

    #[test]
    fn panic_shouts_per_group_and_in_all() {
        let flee = |tick, group, x| Fleeing { tick, group: (1, group), pos: (x, 884.0) };
        let mut a = Agony::default();
        let none: [HitRecord; 0] = [];
        // A whole brigade flees: one shout, from the nearest.
        let out = a.update(
            &none,
            &[flee(10, 7, 1600.0), flee(10, 7, 1520.0), flee(10, 8, 1700.0)],
            &[],
            |_| true,
            |_| true,
            10,
            VIEW,
            None,
        );
        assert_eq!(names(&out), [FEAR]);
        assert!(out[0].pan < 0.2);
        // Too soon for anyone.
        assert!(a.update(&none, &[flee(12, 8, 1512.0)], &[], |_| true, |_| true, 12, VIEW, None).is_empty());
        // Another group after the global pause; brigade 7 still waits.
        assert!(a.update(&none, &[flee(22, 7, 1512.0)], &[], |_| true, |_| true, 22, VIEW, None).is_empty());
        assert_eq!(a.update(&none, &[flee(22, 8, 1512.0)], &[], |_| true, |_| true, 22, VIEW, None).len(), 1);
        assert_eq!(a.update(&none, &[flee(50, 7, 1512.0)], &[], |_| true, |_| true, 50, VIEW, None).len(), 1);
    }

    #[test]
    fn many_kills_raise_a_din() {
        let mut a = Agony::default();
        // Killed by gunshots that do not linger, out of the agony.
        let quick: Vec<_> = (0..1000).filter(|&v| agony_ticks(v, DeathCause::Gunshot).is_none()).take(20).collect();
        let few: Vec<_> = quick[..3].iter().map(|&v| kill(1, v, 1512, DeathCause::Gunshot)).collect();
        assert!((1..100).all(|now| a.update(&few, &[], &[], |_| true, |_| true, now, VIEW, None).is_empty()));
        let mut b = Agony::default();
        let many: Vec<_> = quick.iter().map(|&v| kill(1, v, 1512, DeathCause::Gunshot)).collect();
        let din: Vec<_> =
            (1..100).flat_map(|now| b.update(&many, &[], &[], |_| true, |_| true, now, VIEW, None)).collect();
        assert!(din.len() >= 5, "{}", din.len());
        assert!(din.iter().all(|e| e.volume < 0.5 && e.pan.abs() >= 0.5));
        // It dies down once the kills are old.
        assert!((200..300).all(|now| b.update(&many, &[], &[], |_| true, |_| true, now, VIEW, None).is_empty()));
    }

    fn suffering(victim: u32, x: f32, severity: u32, burning: bool) -> Suffering {
        Suffering { victim, pos: (x, 884.0), severity, burning }
    }

    /// The sounds of `who` from tick 1 to `ticks`, with nobody dying.
    fn suffer_run(who: &[Suffering], ticks: u64) -> Vec<(u64, SoundEvent)> {
        let mut a = Agony::default();
        let none: [HitRecord; 0] = [];
        let mut out = Vec::new();
        for now in 1..ticks {
            out.extend(a.update(&none, &[], who, |_| true, |_| true, now, VIEW, None).into_iter().map(|e| (now, e)));
        }
        out
    }

    #[test]
    fn badly_wounded_survivors_moan_now_and_then() {
        // Below the threshold: silent.
        assert!(suffer_run(&[suffering(1, 1512.0, 3, false)], 300).is_empty());
        // One badly wounded man: moans and gasps every 5–10 s, not like the dying.
        let one = suffer_run(&[suffering(1, 1512.0, 6, false)], 600);
        assert!((4..=12).contains(&one.len()), "{}", one.len());
        assert!(one.iter().all(|(_, e)| e.name == MOAN || e.name == GASP));
        assert!(one.windows(2).all(|w| w[1].0 - w[0].0 >= Mix::DEFAULT.wounded_pause.0));
        // Ten of them: only the nearest two are heard, and not more often than the gap.
        let ten: Vec<_> = (0..10).map(|v| suffering(v, 1512.0 + 20.0 * v as f32, 8, false)).collect();
        let heard = suffer_run(&ten, 600);
        assert!(heard.iter().all(|(_, e)| e.pan < 0.1), "only the nearest");
        assert!(heard.windows(2).all(|w| w[1].0 - w[0].0 >= Mix::DEFAULT.suffer_gap));
        assert!(heard.len() <= 2 * one.len() + 2, "{}", heard.len());
        // Out of hearing: silent.
        assert!(suffer_run(&[suffering(1, 5000.0, 9, false)], 300).is_empty());
    }

    #[test]
    fn suffering_stops_when_healed_or_dead_and_waits_for_the_dying() {
        let mut a = Agony::default();
        let none: [HitRecord; 0] = [];
        let man = [suffering(1, 1512.0, 9, false)];
        let n: usize = (1..300).map(|now| a.update(&none, &[], &man, |_| true, |_| true, now, VIEW, None).len()).sum();
        assert!(n > 0);
        // Healed or dead: he is no longer in the list.
        assert!((300..600).all(|now| a.update(&none, &[], &[], |_| true, |_| true, now, VIEW, None).is_empty()));
        assert!(a.suffer_next.is_empty());
        // A man in his agony is not heard twice.
        let victim = dying(DeathCause::Grapeshot)[0];
        let hits = [kill(1, victim, 1512, DeathCause::Grapeshot)];
        let mut b = Agony::default();
        let me = [suffering(victim, 1512.0, 9, false)];
        for now in 1..40 {
            b.update(&hits, &[], &me, |_| true, |_| true, now, VIEW, None);
            assert!(b.suffer_next.is_empty());
        }
    }

    #[test]
    fn burning_men_scream_often_and_loud() {
        let burning = suffer_run(&[suffering(1, 1512.0, 0, true)], 300);
        assert!(burning.len() >= 10, "{}", burning.len());
        assert!(burning.iter().all(|(_, e)| e.name == SCREAM && e.volume > 0.9));
        assert!(burning[0].0 <= 1 + Mix::DEFAULT.burning_pause.0, "screams at once");
        // A burning man goes before the wounded nearer the centre.
        let both = suffer_run(&[suffering(1, 1512.0, 9, false), suffering(2, 1700.0, 0, true)], 100);
        assert_eq!(both[0].1.name, SCREAM);
    }

    #[test]
    fn a_massacre_stays_within_the_budget() {
        let victims = dying(DeathCause::Grapeshot);
        let hits: Vec<_> = victims.iter().map(|&v| kill(1, v, 1512, DeathCause::Grapeshot)).collect();
        let wounded: Vec<_> = (2000..2040).map(|v| suffering(v, 1512.0, 9, v % 4 == 0)).collect();
        let mut a = Agony::default();
        let mut per_tick = Vec::new();
        for now in 1..200 {
            per_tick.push(a.update(&hits, &[], &wounded, |_| true, |_| true, now, VIEW, None).len());
        }
        let m = Mix::DEFAULT;
        assert!(per_tick.iter().all(|&n| n as f32 <= m.burst));
        let total: usize = per_tick.iter().sum();
        assert!(total as f32 <= m.burst + m.max_per_second * 20.0, "{total}");
        assert!(total as f32 >= m.max_per_second * 15.0, "{total}");
    }

    #[test]
    fn the_mix_reads_overrides() {
        let m = Mix::parse("max_dying=2, wounded_pause=30-60,burning_volume=0.8 bogus=1 din_min_kills=x");
        assert_eq!(m.max_dying, 2);
        assert_eq!(m.wounded_pause, (30, 60));
        assert_eq!(m.burning_volume, 0.8);
        assert_eq!(m.din_min_kills, Mix::DEFAULT.din_min_kills);
        assert_eq!(Mix::parse(""), Mix::DEFAULT);
    }

    #[test]
    fn severity_counts_the_bad_wounds() {
        use acr_sim::body::{Body, Limb};
        let whole = Limb::default();
        assert_eq!(severity(&Body::default()), 0);
        let one = Body { limbs: [Limb { wounds: 1, ..whole }, whole, whole, whole], ..Body::default() };
        assert!(severity(&one) < Mix::DEFAULT.severity);
        let stump = Body { limbs: [Limb { lost: 100, wounds: 1, ..whole }, whole, whole, whole], ..Body::default() };
        assert!(severity(&stump) >= Mix::DEFAULT.severity);
        assert!(severity(&Body { belly_open: true, ..Body::default() }) >= Mix::DEFAULT.severity);
        assert!(severity(&Body { chest_wounds: 2, ..Body::default() }) >= Mix::DEFAULT.severity);
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
