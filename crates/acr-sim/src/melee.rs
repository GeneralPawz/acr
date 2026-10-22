//! Melee and critical hits (`docs/melee.md`), only with [`crate::Rules::hit_locations`]:
//! how a blow was struck ([`Strike`]: a thrust, a cut, a smash, a shot or a bite, and at
//! which height, down at a man on the ground included) and whether it found a vital spot
//! ([`Vital`]: the brain, the neck, the heart, a lung, the liver, the spine, the femoral or
//! the brachial artery).
//!
//! The strike is derived, it draws nothing. A critical hit draws one `rando()` per injury
//! of a man (never an animal, never a mauling). The values are constants pinned by tests.

use crate::World;
use crate::body::{Body, ELBOW, Injury, KNEE, LimbId, Outcome, Part};
use crate::wounds::HitRegion;

/// How a blow is delivered (the animation picks its clip by it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrikeKind {
    /// Bayonet, pike, lance, spear, a sword's point: reaches deep vitals.
    Thrust,
    /// Sabre, tomahawk edge, sword edge: opens arteries and limbs.
    Cut,
    /// Musket butt, club, the tomahawk's blunt side, a stone.
    Smash,
    /// A ball or an arrow (any projectile; point-blank shots included).
    Shot,
    /// An animal's attack.
    Bite,
}

/// Where the blow is aimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrikeHeight {
    /// The head (and the neck).
    High,
    /// The trunk and the arms.
    Mid,
    /// The legs.
    Low,
    /// At a man on the ground (crawling, writhing or knocked down): a downward blow.
    Down,
}

/// How a hit was struck (`HitRecord::strike`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Strike {
    pub kind: StrikeKind,
    pub height: StrikeHeight,
    /// The attacker rides: a rider's blow comes from above (a lance thrust down, a sabre cut
    /// down).
    pub mounted: bool,
}

/// A vital zone within a region (`HitRecord::critical`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vital {
    /// Head: killed outright.
    Brain,
    /// Head: the carotid; bleeds out in seconds ([`NECK_SECONDS`]).
    Neck,
    /// Chest: the heart and the great vessels; killed or dead within seconds.
    Heart,
    /// Chest: a lung; he goes down for good and bleeds fast ([`LUNG_SECONDS`]).
    Lung,
    /// Belly: the liver or a kidney; heavy bleeding ([`LIVER_SECONDS`]).
    Liver,
    /// Trunk: the spine; paralysed (from the chest: all four limbs, he writhes; lower: the
    /// legs, he crawls on his arms).
    Spine,
    /// Thigh (or groin): bleeds out in [`FEMORAL_SECONDS`].
    Femoral,
    /// Upper arm: bleeds out in [`BRACHIAL_SECONDS`].
    Brachial,
}

/// A man on the ground is searched for as a target only within this reach of the searcher
/// (world pixels): men finish off the wounded next to them, they do not walk or shoot at
/// crawlers further off.
pub const DOWN_REACH: i32 = 150;
/// At most this many enemies go for one man on the ground at a time (no swarming).
pub const DOWN_ATTACKERS: u8 = 2;

/// Whether a man lies on the ground (crawling, writhing or knocked down): blows at him come
/// down, and he is a target of lower priority.
pub fn on_ground(b: &Body) -> bool {
    b.animal == 0 && !b.mounted && (b.crawling() || b.downed())
}

/// A rider ([`crate::spacing::is_rider`]: USAGE FASTHORSE, HARDHORCE, HORSE-STRELOK, or a
/// horse-sized body, so dragoons, their officers and native riders too).
pub fn rides(w: &World, i: usize) -> bool {
    crate::spacing::is_rider(w.catalog.get(w.units[i].kind))
}

/// A rider whose blow is a lance's or a spear's thrust (by the type's `.MD` name), not a
/// sabre's cut.
pub fn lancer(md_name: &str) -> bool {
    let n = md_name.to_ascii_lowercase();
    ["lan", "kop", "pik", "spear", "lnc"].iter().any(|k| n.contains(k))
}

/// The kind of a blow by `weapon` (`NRES.DAT [WEAPONS]` kind) of `sender`.
pub fn strike_kind(w: &World, sender: usize, weapon: &str, victim_down: bool) -> StrikeKind {
    let t = w.catalog.get(w.units[sender].kind);
    if crate::body::Beast::of(&t.md_name).is_some() {
        return StrikeKind::Bite;
    }
    match weapon {
        "PIKA" | "KOPIE" => StrikeKind::Thrust,
        "TAMAGAVK" => StrikeKind::Cut,
        "KAMEN" => StrikeKind::Smash,
        "MECH" => {
            if rides(w, sender) {
                if lancer(&t.md_name) { StrikeKind::Thrust } else { StrikeKind::Cut }
            } else if t.officer {
                // An officer cuts; at a man on the ground he stabs down with the point.
                if victim_down { StrikeKind::Thrust } else { StrikeKind::Cut }
            } else if t.attacks.iter().any(|a| a.weapon.is_some()) {
                // A musket: the bayonet.
                StrikeKind::Thrust
            } else if t.role.usage == crate::catalog::usage::PEASANT {
                StrikeKind::Smash
            } else {
                // Tomahawks, swords, axes.
                StrikeKind::Cut
            }
        }
        _ => StrikeKind::Shot,
    }
}

/// The height of a blow on `region` of a man (`down`: on the ground).
pub fn strike_height(region: HitRegion, down: bool) -> StrikeHeight {
    match region {
        _ if down => StrikeHeight::Down,
        HitRegion::Head => StrikeHeight::High,
        HitRegion::Legs => StrikeHeight::Low,
        HitRegion::Torso | HitRegion::Arms => StrikeHeight::Mid,
    }
}

/// Region weights (torso, legs, arms, head; sum 100) of a melee blow at a man on the
/// ground: aimed at the trunk and the head, the arms come up to shield.
pub const DOWN_REGIONS: [i32; 4] = [55, 10, 15, 20];

/// The region of a downward blow by percentage `roll` ([`DOWN_REGIONS`]).
pub fn down_region(roll: i32) -> HitRegion {
    let mut r = roll;
    for (region, weight) in
        [HitRegion::Torso, HitRegion::Legs, HitRegion::Arms, HitRegion::Head].into_iter().zip(DOWN_REGIONS)
    {
        if r < weight {
            return region;
        }
        r -= weight;
    }
    HitRegion::Torso
}

/// Chances in percent of a vital hit per part and strike kind, against a man on his feet:
/// `(vital, thrust, cut, smash, shot)`. Tested in this order; at most one is hit.
pub const HEAD_VITALS: [(Vital, [i32; 4]); 2] = [(Vital::Brain, [35, 10, 30, 0]), (Vital::Neck, [20, 30, 0, 12])];
pub const CHEST_VITALS: [(Vital, [i32; 4]); 3] =
    [(Vital::Heart, [12, 2, 0, 8]), (Vital::Lung, [18, 5, 4, 15]), (Vital::Spine, [3, 2, 2, 3])];
pub const BELLY_VITALS: [(Vital, [i32; 4]); 2] = [(Vital::Liver, [20, 8, 4, 14]), (Vital::Spine, [4, 2, 2, 4])];
pub const PELVIS_VITALS: [(Vital, [i32; 4]); 2] = [(Vital::Femoral, [8, 6, 0, 6]), (Vital::Spine, [3, 1, 2, 3])];
/// The thigh (above [`KNEE`]).
pub const THIGH_VITALS: [(Vital, [i32; 4]); 1] = [(Vital::Femoral, [10, 15, 0, 8])];
/// The upper arm (above [`ELBOW`]).
pub const UPPER_ARM_VITALS: [(Vital, [i32; 4]); 1] = [(Vital::Brachial, [8, 14, 0, 6])];

/// A man on the ground is helpless: each vital chance is multiplied by this (percent).
pub const DOWN_VITAL: i32 = 200;
/// An officer's blow is surer (percent added to each chance).
pub const OFFICER_VITAL: i32 = 20;
/// Each kill adds this (percent) to the chances, up to [`KILLS_VITAL_MAX`].
pub const KILLS_VITAL: i32 = 2;
pub const KILLS_VITAL_MAX: i32 = 20;
/// No vital chance exceeds this.
pub const VITAL_CAP: i32 = 90;

/// Seconds until death (bleeding out), from the first to the last of a range picked by
/// the roll: the carotid, a lung, the liver or a kidney, the femoral and the brachial
/// artery; the heart when it does not kill at once ([`HEART_KILLS`]).
pub const NECK_SECONDS: (u16, u16) = (5, 12);
pub const HEART_SECONDS: (u16, u16) = (2, 5);
pub const LUNG_SECONDS: (u16, u16) = (30, 60);
pub const LIVER_SECONDS: (u16, u16) = (45, 90);
pub const FEMORAL_SECONDS: (u16, u16) = (20, 40);
pub const BRACHIAL_SECONDS: (u16, u16) = (40, 80);
/// Percent of heart hits that kill outright (the rest within [`HEART_SECONDS`]).
pub const HEART_KILLS: i32 = 60;

/// The vitals of a struck part at `level` (limbs: only the upper segment).
fn vitals(part: Part, level: u8) -> &'static [(Vital, [i32; 4])] {
    match part {
        Part::Head => &HEAD_VITALS,
        Part::Chest => &CHEST_VITALS,
        Part::Belly => &BELLY_VITALS,
        Part::Pelvis => &PELVIS_VITALS,
        Part::Limb(l) if l.is_leg() && level < KNEE => &THIGH_VITALS,
        Part::Limb(LimbId::ArmL | LimbId::ArmR) if level < ELBOW => &UPPER_ARM_VITALS,
        Part::Limb(_) => &[],
    }
}

/// The column of a strike kind in the vital tables; `None` for bites (animals have their own
/// tables, `crate::body::maul`).
fn column(kind: StrikeKind) -> Option<usize> {
    match kind {
        StrikeKind::Thrust => Some(0),
        StrikeKind::Cut => Some(1),
        StrikeKind::Smash => Some(2),
        StrikeKind::Shot => Some(3),
        StrikeKind::Bite => None,
    }
}

/// The chance in percent that a strike of `kind` on `part` at `level` finds `vital`:
/// the table's, doubled against a man on the ground (`down`), plus `bonus` percent of it
/// (the attacker), capped at [`VITAL_CAP`].
pub fn vital_chance(kind: StrikeKind, part: Part, level: u8, vital: Vital, down: bool, bonus: i32) -> i32 {
    let Some(c) = column(kind) else { return 0 };
    let base = vitals(part, level).iter().find(|(v, _)| *v == vital).map_or(0, |(_, p)| p[c]);
    let state = if down { DOWN_VITAL } else { 100 };
    (base * state / 100 * (100 + bonus) / 100).min(VITAL_CAP)
}

/// The vital a strike finds by percentage `roll`, if any: the part's vitals tested in order.
pub fn pick_vital(kind: StrikeKind, part: Part, level: u8, down: bool, bonus: i32, roll: i32) -> Option<Vital> {
    let mut r = roll;
    for &(v, _) in vitals(part, level) {
        let p = vital_chance(kind, part, level, v, down, bonus);
        if r < p {
            return Some(v);
        }
        r -= p;
    }
    None
}

/// The attacker's bonus to the vital chances (percent): an officer's, and his kills'.
pub fn attacker_bonus(w: &World, sender: usize) -> i32 {
    let u = &w.units[sender];
    let officer = if w.catalog.get(u.kind).officer { OFFICER_VITAL } else { 0 };
    officer + (KILLS_VITAL * i32::from(u.kills)).min(KILLS_VITAL_MAX)
}

/// A second percentage from `roll`, decorrelated from it (the timing of a vital hit).
fn second(roll: i32) -> i32 {
    (roll * 61 + 23) % 100
}

/// Seconds within `range` by percentage `r`.
fn within(range: (u16, u16), r: i32) -> u16 {
    range.0 + ((i32::from(range.1 - range.0) * r) / 99) as u16
}

/// Leaves a vital hit on the body; the injury's outcome becomes a kill where it kills
/// outright (the brain, most hearts). `roll` is the percentage that found it.
pub fn apply_vital(body: &mut Body, injury: &mut Injury, vital: Vital, roll: i32) {
    let r = second(roll);
    let mut bleed = |range: (u16, u16)| {
        let s = within(range, r).max(1);
        body.bleed_out = if body.bleed_out == 0 { s } else { body.bleed_out.min(s) };
    };
    match vital {
        Vital::Brain => injury.outcome = Outcome::Kill,
        Vital::Heart if r < HEART_KILLS => injury.outcome = Outcome::Kill,
        Vital::Heart => bleed(HEART_SECONDS),
        Vital::Neck => bleed(NECK_SECONDS),
        Vital::Lung => {
            bleed(LUNG_SECONDS);
            body.collapsed = true;
        }
        Vital::Liver => bleed(LIVER_SECONDS),
        Vital::Femoral => bleed(FEMORAL_SECONDS),
        Vital::Brachial => bleed(BRACHIAL_SECONDS),
        Vital::Spine => {
            let level = if injury.part == Part::Chest { 2 } else { 1 };
            body.spine = body.spine.max(level);
        }
    }
    if body.drops_long_weapon() {
        body.dropped = true;
    }
}

/// The critical roll of an injury dealt by `strike` (hit-locations rule): one `rando()` for
/// a man struck by a man, nothing for animals. `down`: the victim was on the ground when
/// struck; `bonus`: [`attacker_bonus`].
pub fn critical(
    w: &mut World,
    victim: usize,
    injury: &mut Injury,
    strike: Strike,
    down: bool,
    bonus: i32,
) -> Option<Vital> {
    if column(strike.kind).is_none() || w.units[victim].body.animal != 0 || injury.outcome.torn().is_some() {
        return None;
    }
    let roll = (w.random.rando() * 100) >> 15;
    let vital = pick_vital(strike.kind, injury.part, injury.level, down, bonus, roll)?;
    // A head already split by a blow that kills is no critical hit.
    if injury.outcome == Outcome::Kill && vital != Vital::Brain {
        return None;
    }
    apply_vital(&mut w.units[victim].body, injury, vital, roll);
    Some(vital)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn share(kind: StrikeKind, part: Part, level: u8, down: bool, f: impl Fn(Option<Vital>) -> bool) -> usize {
        (0..100).filter(|&r| f(pick_vital(kind, part, level, down, 0, r))).count()
    }

    #[test]
    fn thrusts_reach_deep_vitals_cuts_open_arteries() {
        let heart = |k, down| share(k, Part::Chest, 40, down, |v| v == Some(Vital::Heart));
        assert_eq!(heart(StrikeKind::Thrust, false), 12);
        assert_eq!(heart(StrikeKind::Cut, false), 2);
        assert_eq!(heart(StrikeKind::Thrust, true), 24, "a man on the ground");
        let femoral = |k| share(k, Part::Limb(LimbId::LegL), 60, false, |v| v == Some(Vital::Femoral));
        assert!(femoral(StrikeKind::Cut) > femoral(StrikeKind::Thrust));
        assert_eq!(femoral(StrikeKind::Smash), 0);
        // Below the knee there is no great artery.
        assert_eq!(share(StrikeKind::Cut, Part::Limb(LimbId::LegL), KNEE + 10, false, |v| v.is_some()), 0);
        // Any vital in the chest: a third of thrusts at a man on the ground.
        let any = share(StrikeKind::Thrust, Part::Chest, 40, true, |v| v.is_some());
        assert!((60..=70).contains(&any), "{any}");
        assert_eq!(share(StrikeKind::Bite, Part::Head, 0, true, |v| v.is_some()), 0);
    }

    #[test]
    fn chances_stay_capped_and_bounded() {
        for kind in [StrikeKind::Thrust, StrikeKind::Cut, StrikeKind::Smash, StrikeKind::Shot] {
            for part in [Part::Head, Part::Chest, Part::Belly, Part::Pelvis, Part::Limb(LimbId::ArmR)] {
                for &(v, _) in vitals(part, 50) {
                    let p = vital_chance(kind, part, 50, v, true, OFFICER_VITAL + KILLS_VITAL_MAX);
                    assert!((0..=VITAL_CAP).contains(&p), "{kind:?} {part:?} {v:?} {p}");
                    assert!(p >= vital_chance(kind, part, 50, v, false, 0));
                }
            }
        }
        assert_eq!(vital_chance(StrikeKind::Thrust, Part::Head, 0, Vital::Brain, true, 100), VITAL_CAP);
    }

    #[test]
    fn timed_vitals_bleed_within_their_ranges() {
        let inj =
            |part| Injury { part, level: 60, outcome: Outcome::Wound, energy: 100, cause: crate::body::Cause::Bayonet };
        for roll in 0..100 {
            let mut b = Body::default();
            apply_vital(&mut b, &mut inj(Part::Limb(LimbId::LegR)), Vital::Femoral, roll);
            assert!((20..=40).contains(&b.bleed_out), "{}", b.bleed_out);
            let mut b = Body::default();
            apply_vital(&mut b, &mut inj(Part::Head), Vital::Neck, roll);
            assert!((5..=12).contains(&b.bleed_out));
        }
        let mut b = Body::default();
        let mut i = inj(Part::Chest);
        apply_vital(&mut b, &mut i, Vital::Lung, 0);
        assert!(b.collapsed && b.crawling() && b.bleed_out >= 30);
        let mut b = Body::default();
        apply_vital(&mut b, &mut inj(Part::Chest), Vital::Spine, 0);
        assert_eq!(b.move_percent(), 0, "spine at the chest: he writhes");
        let mut b = Body::default();
        apply_vital(&mut b, &mut inj(Part::Belly), Vital::Spine, 0);
        assert!(b.crawling() && b.move_percent() > 0, "lower spine: he crawls on his arms");
        let kills = (0..100)
            .filter(|&r| {
                let mut i = inj(Part::Chest);
                apply_vital(&mut Body::default(), &mut i, Vital::Heart, r);
                i.outcome == Outcome::Kill
            })
            .count();
        assert!((50..=70).contains(&kills), "{kills}");
    }

    #[test]
    fn down_regions_sum_to_100() {
        assert_eq!(DOWN_REGIONS.iter().sum::<i32>(), 100);
        assert_eq!(down_region(0), HitRegion::Torso);
        assert_eq!(down_region(99), HitRegion::Head);
        assert_eq!(strike_height(HitRegion::Legs, true), StrikeHeight::Down);
        assert_eq!(strike_height(HitRegion::Head, false), StrikeHeight::High);
        assert!(lancer("SpLancer") && lancer("IndKop") && !lancer("EngDrag"));
    }
}
