//! The body model of the damage model (#262, `docs/damage-model.md` section 1): only with
//! [`crate::Rules::hit_locations`]. A hit lands somewhere on a limb or the trunk and
//! wounds, breaks or takes the limb off at any level; heavy blows tear bodies in two. What a
//! man can still do (walk, crawl, hold his weapon, fight) follows from what is left of him.
//!
//! A limb is one chain measured by a level from its root (shoulder or hip, 0) to its tip
//! (fingertips or toes, 255). An unhurt [`Body`] hashes to nothing, so games without the
//! rule keep their hashes.

use std::hash::{Hash, Hasher};

use crate::World;
use crate::combat::DeathCause;
use crate::wounds::HitRegion;

/// Levels along a limb (0 root .. 255 tip): the joints between its segments.
pub const ELBOW: u8 = 110;
pub const WRIST: u8 = 205;
pub const KNEE: u8 = 120;
pub const ANKLE: u8 = 215;
/// The level of a whole limb ([`Limb::lost`]).
pub const WHOLE: u8 = 255;

/// The four limbs, in the order of [`Body::limbs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LimbId {
    ArmL = 0,
    ArmR = 1,
    LegL = 2,
    LegR = 3,
}

impl LimbId {
    pub fn is_leg(self) -> bool {
        matches!(self, Self::LegL | Self::LegR)
    }
}

/// A broken or dislocated limb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FractureKind {
    Closed,
    /// The bone is through the skin: bleeds ([`BLEED_OPEN_FRACTURE`]).
    Open,
    /// A joint out of place (knee, ankle, elbow, wrist).
    Dislocated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fracture {
    pub level: u8,
    pub kind: FractureKind,
    /// How far the limb is bent at the break (degrees / 2), for the renderer.
    pub bend: i8,
}

/// One limb: how much is left of it, a break and flesh wounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Limb {
    /// [`WHOLE`], or the limb ends at this level.
    pub lost: u8,
    pub fracture: Option<Fracture>,
    /// Flesh wounds (count, saturating).
    pub wounds: u8,
}

impl Default for Limb {
    fn default() -> Self {
        Self { lost: WHOLE, fracture: None, wounds: 0 }
    }
}

impl Limb {
    /// The limb is no longer whole.
    pub fn severed(&self) -> bool {
        self.lost != WHOLE
    }
}

/// How a body was torn apart at death ([`crate::combat::DeathRecord::torn`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Torn {
    /// Split through the trunk at this level (0 shoulders .. 255 hips).
    InTwo {
        level: u8,
    },
    /// Nothing whole left.
    Pulverized,
    Headless,
}

/// The state of a body (field `Unit::body`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Body {
    /// [`LimbId`] order: left arm, right arm, left leg, right leg.
    pub limbs: [Limb; 4],
    /// Guts out: mortal, heavy bleeding.
    pub belly_open: bool,
    pub head_wounds: u8,
    pub chest_wounds: u8,
    /// Seconds of burning left after standing in a shell fire (`crate::ground_fire`).
    pub burn: u8,
    /// The long weapon (musket, pike) was dropped; set once, the renderer leaves it lying.
    pub dropped: bool,
    /// A rider (set at his first injury): the horse carries him, so his legs never make him
    /// crawl or slow him (falling from the saddle comes with the body simulation, #276).
    pub mounted: bool,
    /// Knocked down by an animal (mauling, #276 phase 4): ticks left on the ground. While
    /// down he cannot move, attack or hit back.
    pub down: u8,
}

impl Hash for Body {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if *self != Self::default() {
            (self.limbs, self.belly_open, self.head_wounds, self.chest_wounds, self.burn, self.dropped).hash(state);
            self.mounted.hash(state);
            self.down.hash(state);
        }
    }
}

/// Life lost per second by a stump above the knee or the elbow.
pub const BLEED_ABOVE: i32 = 6;
/// … below the knee or the elbow.
pub const BLEED_BELOW: i32 = 4;
/// … a lost hand or foot.
pub const BLEED_HAND_FOOT: i32 = 2;
pub const BLEED_OPEN_FRACTURE: i32 = 2;
pub const BLEED_BELLY: i32 = 5;

/// Crawling speed in percent with two, one and no working arms.
pub const CRAWL_SPEED: [i32; 3] = [15, 8, 0];

impl Body {
    pub fn limb(&self, l: LimbId) -> &Limb {
        &self.limbs[l as usize]
    }

    /// The function of a leg, 0..=100: what it still carries.
    pub fn leg(&self, l: LimbId) -> i32 {
        let limb = self.limb(l);
        if limb.lost < ANKLE {
            return 0;
        }
        let mut f = 100 - 35 * i32::from(limb.wounds.min(3));
        if limb.wounds > 0 {
            f = f.max(30);
        }
        if limb.lost != WHOLE {
            f = f.min(15); // hobbling on the stump
        }
        match limb.fracture.map(|b| b.kind) {
            Some(FractureKind::Open) => 0,
            Some(FractureKind::Closed) if limb.fracture.is_some_and(|b| b.level < ANKLE) => 0,
            Some(_) => f.min(20),
            None => f,
        }
    }

    /// The function of an arm, 0..=100: any loss or break makes it useless.
    pub fn arm(&self, l: LimbId) -> i32 {
        let limb = self.limb(l);
        if limb.severed() || limb.fracture.is_some() {
            return 0;
        }
        (100 - 40 * i32::from(limb.wounds)).max(0)
    }

    /// Arms with a function above 0.
    pub fn working_arms(&self) -> usize {
        [LimbId::ArmL, LimbId::ArmR].into_iter().filter(|&a| self.arm(a) > 0).count()
    }

    /// A leg gave out: the man crawls.
    pub fn crawling(&self) -> bool {
        !self.mounted && (self.leg(LimbId::LegL) == 0 || self.leg(LimbId::LegR) == 0)
    }

    /// Moving speed in percent: walking on both legs, else crawling by the arms (0: he can
    /// only writhe).
    pub fn move_percent(&self) -> i32 {
        if self.mounted {
            return 100;
        }
        let (l, r) = (self.leg(LimbId::LegL), self.leg(LimbId::LegR));
        if l > 0 && r > 0 {
            // min × (0.6 + 0.4 × max / 100)
            l.min(r) * (150 + l.max(r)) / 250
        } else {
            CRAWL_SPEED[2 - self.working_arms()]
        }
    }

    /// The walking `rate` (16ths) scaled by [`Self::move_percent`]: at least 1 unless he
    /// cannot move at all.
    pub fn walk_rate(&self, rate: i32) -> i32 {
        match self.move_percent() {
            100 => rate,
            0 => 0,
            p => (rate * p / 100).max(1),
        }
    }

    /// A long weapon (musket, pike) needs both arms and a man on his feet.
    pub fn drops_long_weapon(&self) -> bool {
        self.crawling() || self.working_arms() < 2
    }

    /// A one-handed weapon (sabre, pistol) needs one working arm.
    pub fn holds_one_handed(&self) -> bool {
        self.working_arms() > 0
    }

    /// The unit can no longer fire its firearm or bow (it dropped it).
    pub fn disarmed(&self) -> bool {
        self.dropped || self.drops_long_weapon()
    }

    /// A crawler or a man without a working arm: takes no orders, attacks nothing and drags
    /// himself away from the enemy.
    pub fn out_of_fight(&self) -> bool {
        self.crawling() || self.working_arms() == 0
    }

    /// Life lost per second (0: not bleeding).
    pub fn bleeding(&self) -> i32 {
        let mut sum = if self.belly_open { BLEED_BELLY } else { 0 };
        for (k, limb) in self.limbs.iter().enumerate() {
            let joint = if k < 2 { ELBOW } else { KNEE };
            let tip = if k < 2 { WRIST } else { ANKLE };
            if limb.severed() {
                sum += if limb.lost < joint {
                    BLEED_ABOVE
                } else if limb.lost < tip {
                    BLEED_BELOW
                } else {
                    BLEED_HAND_FOOT
                };
            }
            if limb.fracture.is_some_and(|b| b.kind == FractureKind::Open) {
                sum += BLEED_OPEN_FRACTURE;
            }
        }
        sum
    }
}

/// What struck, for the outcome table ([`limb_outcome`] and the trunk and head rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cause {
    /// Musket ball (and arrows, and anything unknown).
    Musket,
    Grapeshot,
    Cannonball,
    /// A bursting shell.
    Shell,
    /// Cavalry melee.
    Sabre,
    /// Bayonet and pike.
    Bayonet,
    /// Thrown axes, tomahawks, spears and stones.
    Thrown,
    /// An animal's claws (bear, jaguar): deep parallel gashes.
    Claw,
    /// An animal's jaws (alligator, jaguar, bear): crushing.
    Bite,
    /// Antlers or horns (elk, bison): punctures, thrown down.
    Gore,
}

impl Cause {
    /// The cause of a blow by weapon cause `c`; `mounted`: the attacker rides.
    pub fn of(c: DeathCause, mounted: bool) -> Self {
        match c {
            DeathCause::Grapeshot => Self::Grapeshot,
            DeathCause::Cannonball => Self::Cannonball,
            DeathCause::Explosion => Self::Shell,
            DeathCause::Melee if mounted => Self::Sabre,
            DeathCause::Melee => Self::Bayonet,
            DeathCause::Thrown => Self::Thrown,
            _ => Self::Musket,
        }
    }
}

/// Where a hit landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    Limb(LimbId),
    Head,
    Chest,
    Belly,
    Pelvis,
}

/// What a hit did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    Wound,
    /// A deep cut: two flesh wounds.
    DeepWound,
    Fracture(FractureKind),
    /// The limb is off at the hit's level.
    Off,
    BellyOpen,
    /// Killed outright (head, crushed chest).
    Kill,
    Torn(Torn),
}

impl Outcome {
    /// The outcome kills whatever the damage.
    pub fn kills(self) -> bool {
        matches!(self, Self::Kill | Self::Torn(_))
    }

    pub fn torn(self) -> Option<Torn> {
        if let Self::Torn(t) = self { Some(t) } else { None }
    }
}

/// One hit on a body (`HitRecord::injury`): where, at which level, what it did and the
/// energy of the blow (0..=100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Injury {
    pub part: Part,
    pub level: u8,
    pub outcome: Outcome,
    pub energy: u8,
    /// What struck (the renderer shapes the wound by it: claw gashes, a bite, a puncture).
    pub cause: Cause,
}

/// A percentage roll from the world RNG (0..100).
fn pct(w: &mut World) -> i32 {
    (w.random.rando() * 100) >> 15
}

/// Cannonball energy (percent) from which a limb hit takes the limb off.
pub const BALL_SEVERS: u8 = 30;
/// … and from which a trunk hit tears the body in two.
pub const BALL_TEARS: u8 = 60;
/// … and from which nothing whole is left.
pub const BALL_PULVERIZES: u8 = 90;

/// Musket-ball energy (percent, by range) from which a ball tears chunks: a limb off at the
/// hand, forearm or foot, a belly open.
pub const BALL_CLOSE: u8 = 75;

/// Percent of musket-ball limb hits that shatter the bone (18th-century balls are soft,
/// heavy and slow: they flatten and splinter what they strike).
pub const MUSKET_FRACTURE: i32 = 45;
/// … that take the limb off at the hand, forearm or foot at close range ([`BALL_CLOSE`]),
/// and beyond it (at the tip only).
pub const MUSKET_OFF: [i32; 2] = [6, 2];
/// … of trunk hits that open the belly at close range.
pub const MUSKET_BELLY: i32 = 8;
/// Thrown axes and tomahawks: hand (or foot) off, belly open, head kill.
pub const THROWN_OFF: i32 = 12;
pub const THROWN_BELLY: i32 = 10;
pub const THROWN_HEAD_KILL: i32 = 60;
/// A bayonet thrust into the belly opens it.
pub const BAYONET_BELLY: i32 = 20;

/// The outcome of a limb hit at `level` by percentage `roll`, and the level it applies at.
pub fn limb_outcome(cause: Cause, energy: u8, leg: bool, level: u8, roll: i32) -> (Outcome, u8) {
    let (joint, tip) = if leg { (KNEE, ANKLE) } else { (ELBOW, WRIST) };
    // Torn off by a ball at close range: the hand or forearm, or the foot.
    let chunk = if leg { ANKLE } else { ELBOW };
    // Two thirds of the breaks by balls are open.
    let ball_break = |from: i32, n: i32| {
        if roll - from < n * 2 / 3 { FractureKind::Open } else { FractureKind::Closed }
    };
    match cause {
        Cause::Musket => {
            let close = energy >= BALL_CLOSE;
            let off = MUSKET_OFF[usize::from(!close)];
            match roll {
                r if r < off => (Outcome::Off, level.max(if close { chunk } else { tip })),
                r if r < off + MUSKET_FRACTURE => (Outcome::Fracture(ball_break(off, MUSKET_FRACTURE)), level),
                _ => (Outcome::Wound, level),
            }
        }
        // Deep gashes; a hand (or foot) chopped off.
        Cause::Thrown => match roll {
            r if r < THROWN_OFF => (Outcome::Off, level.max(tip)),
            r if r < THROWN_OFF + 70 => (Outcome::DeepWound, level),
            _ => (Outcome::Fracture(FractureKind::Closed), level),
        },
        Cause::Grapeshot => match roll {
            0..40 => (Outcome::Wound, level),
            40..75 => (Outcome::Fracture(ball_break(40, 35)), level),
            _ => (Outcome::Off, level.max(joint)),
        },
        Cause::Cannonball if energy >= BALL_SEVERS && roll < 85 => (Outcome::Off, level),
        Cause::Cannonball => (Outcome::Fracture(FractureKind::Open), level),
        Cause::Shell => match roll {
            0..50 => (Outcome::Wound, level),
            50..80 => (Outcome::Fracture(ball_break(50, 30)), level),
            _ => (Outcome::Off, level),
        },
        Cause::Sabre => match roll {
            0..20 => (Outcome::Off, level.max(joint)),
            20..80 => (Outcome::DeepWound, level),
            _ => (Outcome::Fracture(FractureKind::Closed), level),
        },
        // Mauling (stub: filled by the mauling tables).
        Cause::Claw | Cause::Bite | Cause::Gore => (Outcome::DeepWound, level),
        Cause::Bayonet => match roll {
            0..85 => (Outcome::Wound, level),
            // A thrust into a joint puts it out.
            _ if level.abs_diff(joint) < 12 || level.abs_diff(tip) < 12 => {
                (Outcome::Fracture(FractureKind::Dislocated), level)
            }
            _ => (Outcome::Fracture(FractureKind::Closed), level),
        },
    }
}

/// The outcome of a trunk hit on `part` by percentage `roll`.
pub fn trunk_outcome(cause: Cause, energy: u8, part: Part, level: u8, roll: i32) -> Outcome {
    let belly = |chance: i32| if roll < chance { Outcome::BellyOpen } else { Outcome::Wound };
    match cause {
        Cause::Musket if energy >= BALL_CLOSE => belly(MUSKET_BELLY),
        Cause::Musket => Outcome::Wound,
        Cause::Grapeshot | Cause::Sabre => belly(10),
        Cause::Shell => belly(20),
        // A deep stab or gash.
        Cause::Bayonet if roll < BAYONET_BELLY => Outcome::BellyOpen,
        Cause::Thrown if roll < THROWN_BELLY => Outcome::BellyOpen,
        Cause::Bayonet | Cause::Thrown => Outcome::DeepWound,
        Cause::Cannonball if energy >= BALL_PULVERIZES => Outcome::Torn(Torn::Pulverized),
        Cause::Cannonball if energy >= BALL_TEARS => Outcome::Torn(Torn::InTwo { level }),
        Cause::Cannonball if part == Part::Chest => Outcome::Kill,
        Cause::Cannonball => Outcome::BellyOpen,
        Cause::Claw | Cause::Bite | Cause::Gore => Outcome::DeepWound,
    }
}

/// The outcome of a head hit by percentage `roll`.
pub fn head_outcome(cause: Cause, roll: i32) -> Outcome {
    match cause {
        Cause::Cannonball => Outcome::Torn(Torn::Headless),
        Cause::Sabre if roll >= 50 => Outcome::Wound,
        Cause::Thrown if roll >= THROWN_HEAD_KILL => Outcome::DeepWound,
        _ => Outcome::Kill,
    }
}

/// `injure`: the injury of a hit on `victim`'s `region` by `cause` with `energy`
/// (0..=100), applied to his body. Draws from the world RNG: for a limb its side, the level
/// and the outcome; for the trunk the level and the outcome; for the head the outcome.
pub fn injure(w: &mut World, victim: usize, cause: Cause, energy: u8, region: HitRegion) -> Injury {
    let level_roll = |w: &mut World| ((w.random.rando() << 8) >> 15) as u8;
    let injury = match region {
        HitRegion::Legs | HitRegion::Arms => {
            let right = w.random.rando() >= 1 << 14;
            let limb = match (region, right) {
                (HitRegion::Legs, false) => LimbId::LegL,
                (HitRegion::Legs, true) => LimbId::LegR,
                (_, false) => LimbId::ArmL,
                (_, true) => LimbId::ArmR,
            };
            // The hit lands on what is left of the limb.
            let lost = w.units[victim].body.limb(limb).lost;
            let level = (u32::from(level_roll(w)) * u32::from(lost) / 255) as u8;
            let roll = pct(w);
            let (outcome, level) = limb_outcome(cause, energy, limb.is_leg(), level, roll);
            Injury { part: Part::Limb(limb), level, outcome, energy, cause }
        }
        HitRegion::Torso => {
            let level = level_roll(w);
            let part = match level {
                0..96 => Part::Chest,
                96..192 => Part::Belly,
                _ => Part::Pelvis,
            };
            let roll = pct(w);
            Injury { part, level, outcome: trunk_outcome(cause, energy, part, level, roll), energy, cause }
        }
        HitRegion::Head => {
            let roll = if matches!(cause, Cause::Sabre | Cause::Thrown) { pct(w) } else { 0 };
            Injury { part: Part::Head, level: 0, outcome: head_outcome(cause, roll), energy, cause }
        }
    };
    apply(&mut w.units[victim].body, &injury);
    injury
}

/// An injury aimed at `part` (the close-up bench): at `level`, or a level drawn as in
/// [`injure`]; the outcome is rolled as in battle.
pub fn injure_aimed(w: &mut World, victim: usize, cause: Cause, energy: u8, part: Part, level: Option<u8>) -> Injury {
    let level_roll = |w: &mut World| ((w.random.rando() << 8) >> 15) as u8;
    let level = match level {
        Some(l) => l,
        None => level_roll(w),
    };
    let injury = match part {
        Part::Limb(limb) => {
            let lost = w.units[victim].body.limb(limb).lost;
            let level = level.min(lost.saturating_sub(1));
            let roll = pct(w);
            let (outcome, level) = limb_outcome(cause, energy, limb.is_leg(), level, roll);
            Injury { part, level, outcome, energy, cause }
        }
        Part::Head => {
            let roll = if matches!(cause, Cause::Sabre | Cause::Thrown) { pct(w) } else { 0 };
            Injury { part, level: 0, outcome: head_outcome(cause, roll), energy, cause }
        }
        _ => {
            let roll = pct(w);
            Injury { part, level, outcome: trunk_outcome(cause, energy, part, level, roll), energy, cause }
        }
    };
    apply(&mut w.units[victim].body, &injury);
    injury
}

/// Leaves the injury on the body.
pub fn apply(body: &mut Body, injury: &Injury) {
    match (injury.part, injury.outcome) {
        (Part::Limb(l), outcome) => {
            let limb = &mut body.limbs[l as usize];
            match outcome {
                Outcome::Wound => limb.wounds = limb.wounds.saturating_add(1),
                Outcome::DeepWound => limb.wounds = limb.wounds.saturating_add(2),
                Outcome::Fracture(kind) => {
                    let level = match kind {
                        // A dislocation is at the nearest joint.
                        FractureKind::Dislocated => {
                            let (joint, tip) = if l.is_leg() { (KNEE, ANKLE) } else { (ELBOW, WRIST) };
                            if injury.level.abs_diff(joint) < injury.level.abs_diff(tip) { joint } else { tip }
                        }
                        _ => injury.level,
                    };
                    // The bend is placed by the level: −30..=30 (degrees / 2).
                    let bend = ((i32::from(injury.level) * 37) % 61 - 30) as i8;
                    if level < limb.lost && limb.fracture.is_none_or(|b| b.kind != FractureKind::Open) {
                        limb.fracture = Some(Fracture { level, kind, bend });
                    }
                }
                Outcome::Off => {
                    limb.lost = limb.lost.min(injury.level);
                    // A break beyond the cut went with the limb.
                    if limb.fracture.is_some_and(|b| b.level >= limb.lost) {
                        limb.fracture = None;
                    }
                }
                Outcome::BellyOpen | Outcome::Kill | Outcome::Torn(_) => {}
            }
        }
        (Part::Head, Outcome::Wound | Outcome::DeepWound) => body.head_wounds = body.head_wounds.saturating_add(1),
        (_, Outcome::BellyOpen) => body.belly_open = true,
        (Part::Head, _) => {}
        (_, Outcome::Wound | Outcome::DeepWound) => body.chest_wounds = body.chest_wounds.saturating_add(1),
        _ => {}
    }
    if body.drops_long_weapon() {
        body.dropped = true;
    }
}

/// How far a man out of the fight drags himself from the nearest enemy (world pixels).
pub const RETREAT: i32 = 600;

/// A man who just went out of the fight ([`Body::out_of_fight`]) drops his orders and, if
/// he can move at all, drags himself away from the nearest enemy.
pub fn leave_fight(w: &mut World, i: usize) {
    let u = &w.units[i];
    let (x, y) = u.pos.pixels();
    let nearest = w
        .units
        .iter()
        .filter(|e| e.living() && !e.building && u.hostile(e))
        .map(|e| e.pos.pixels())
        .min_by_key(|&(ex, ey)| crate::trig::norma(ex - x, ey - y));
    let can_move = u.body.move_percent() > 0;
    let (w_px, h_px) = (w.map.land.width * 16, w.map.land.height * 16);
    let u = &mut w.units[i];
    u.orders.clear();
    u.path.clear();
    if let Some((ex, ey)) = nearest.filter(|_| can_move) {
        let (dx, dy) = (x - ex, y - ey);
        let n = crate::trig::norma(dx, dy).max(1);
        let tx = (x + dx * RETREAT / n).clamp(64, (w_px - 64).max(64));
        let ty = (y + dy * RETREAT / n).clamp(64, (h_px - 64).max(64));
        crate::motion::send_to(u, crate::Pos::from_pixels(tx, ty), crate::motion::PLAYER_PRIO, true);
    }
}
