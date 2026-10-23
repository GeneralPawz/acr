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
    /// An animal's body (set at its first injury): its size ([`Beast::size`], 1..=4), 0 for
    /// a man. Four legs, the front ones in the arm slots: wounds slow it, a leg that gives
    /// out makes it limp, heavy wounds make it flee; it never crawls, drops nothing and is
    /// never knocked down.
    pub animal: u8,
    /// Seconds left until he bleeds out from a vital hit (`crate::melee`, the carotid, an
    /// artery, the liver, a lung); 0: none. Counted down by `crate::wounds::bleed`.
    pub bleed_out: u16,
    /// The spine hit (`crate::melee::Vital::Spine`): 1 the legs are paralysed (he crawls on
    /// his arms), 2 all four limbs (he writhes).
    pub spine: u8,
    /// A lung hit: he went down and does not get up again.
    pub collapsed: bool,
    /// Shell fragment wounds (`crate::shrapnel`): life lost per second while
    /// [`Body::seep_s`] lasts, on top of [`Body::bleeding`]'s stumps and open breaks.
    pub seep: u8,
    /// Seconds the fragment wounds still bleed (counted down by `crate::wounds::bleed`).
    pub seep_s: u8,
}

impl Hash for Body {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if *self != Self::default() {
            (self.limbs, self.belly_open, self.head_wounds, self.chest_wounds, self.burn, self.dropped).hash(state);
            self.mounted.hash(state);
            self.down.hash(state);
            if self.animal != 0 {
                self.animal.hash(state);
            }
            if self.bleed_out != 0 || self.spine != 0 || self.collapsed {
                (self.bleed_out, self.spine, self.collapsed).hash(state);
            }
            if self.seep_s != 0 {
                (self.seep, self.seep_s).hash(state);
            }
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

/// Energy below which a shell fragment takes no limb off (`crate::shrapnel`).
pub const FRAGMENT_SEVERS: u8 = 45;
/// Energy from which a shell fragment to the head kills; below it a deep wound.
pub const FRAGMENT_HEAD_KILL: u8 = 40;

/// Crawling speed in percent with two, one and no working arms.
pub const CRAWL_SPEED: [i32; 3] = [15, 8, 0];

impl Body {
    pub fn limb(&self, l: LimbId) -> &Limb {
        &self.limbs[l as usize]
    }

    /// The function of a leg, 0..=100: what it still carries.
    pub fn leg(&self, l: LimbId) -> i32 {
        let limb = self.limb(l);
        if limb.lost < ANKLE || (self.animal == 0 && (self.spine > 0 || self.collapsed)) {
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
        if limb.severed() || limb.fracture.is_some() || (self.animal == 0 && self.spine >= 2) {
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
        !self.mounted && self.animal == 0 && (self.leg(LimbId::LegL) == 0 || self.leg(LimbId::LegR) == 0)
    }

    /// Moving speed in percent: walking on both legs, else crawling by the arms (0: he can
    /// only writhe).
    pub fn move_percent(&self) -> i32 {
        if self.mounted {
            return 100;
        }
        if self.animal != 0 {
            return self.beast_move_percent();
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
        if self.downed() {
            return 0;
        }
        match self.move_percent() {
            100 => rate,
            0 => 0,
            p => (rate * p / 100).max(1),
        }
    }

    /// Knocked down ([`Body::down`]): he cannot move, attack or hit back until he is up.
    pub fn downed(&self) -> bool {
        self.down > 0
    }

    /// An animal's speed in percent: its worst leg slows it (60 % at none left whole), one
    /// leg that gives out makes it limp on three ([`BEAST_LIMP`]), two leave it barely
    /// moving (10 %); an open belly halves it.
    fn beast_move_percent(&self) -> i32 {
        let legs = [LimbId::ArmL, LimbId::ArmR, LimbId::LegL, LimbId::LegR].map(|l| self.leg(l));
        let out = legs.iter().filter(|&&f| f == 0).count();
        let worst = legs.into_iter().min().unwrap_or(100);
        let p = match out {
            0 => 60 + 40 * worst / 100,
            1 => BEAST_LIMP,
            _ => 10,
        };
        if self.belly_open { p.min(50) } else { p }
    }

    /// An animal limps: a leg gave out.
    pub fn limping(&self) -> bool {
        self.animal != 0
            && [LimbId::ArmL, LimbId::ArmR, LimbId::LegL, LimbId::LegR].into_iter().any(|l| self.leg(l) == 0)
    }

    /// An animal's heavy wounds make it flee: an open belly, more trunk and head wounds
    /// than its size, or wounds in all from 2 + 2 × its size.
    pub fn flees(&self) -> bool {
        if self.animal == 0 {
            return false;
        }
        let size = i32::from(self.animal);
        let trunk = i32::from(self.chest_wounds) + i32::from(self.head_wounds);
        let limbs: i32 = self.limbs.iter().map(|l| i32::from(l.wounds)).sum();
        self.belly_open || trunk > size || trunk + limbs >= 2 + 2 * size
    }

    /// A long weapon (musket, pike) needs both arms and a man on his feet.
    pub fn drops_long_weapon(&self) -> bool {
        self.animal == 0 && (self.crawling() || self.working_arms() < 2)
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
        if self.animal != 0 {
            return self.flees();
        }
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
        if self.seep_s > 0 {
            sum += i32::from(self.seep);
        }
        // A bigger body has more blood to lose: an animal loses it by its size.
        sum * i32::from(self.animal.max(1))
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

    /// An animal's claws, jaws or horns.
    pub fn mauls(self) -> bool {
        matches!(self, Self::Claw | Self::Bite | Self::Gore)
    }
}

/// An animal attacker, by its type's `.MD` name (`AnmMed`, `AnmCroSm`, ...): how it mauls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Beast {
    /// `AnmMed*`: claws mostly, bites.
    Bear,
    /// `AnmJug`: claws and skull bites.
    Jaguar,
    /// `AnmCro*`: bites, tears limbs off.
    Alligator,
    /// `AnmOln*`: antlers.
    Elk,
    /// `AnmBiz`: horns.
    Bison,
    /// Any other `Anm*` (the eagle): claws.
    Other,
}

impl Beast {
    /// The beast of a type's `.MD` name, `None` for anything that is not an animal.
    pub fn of(md_name: &str) -> Option<Self> {
        let n = md_name.to_ascii_lowercase();
        let rest = n.strip_prefix("anm")?;
        Some(if rest.starts_with("med") {
            Self::Bear
        } else if rest.starts_with("jug") {
            Self::Jaguar
        } else if rest.starts_with("cro") {
            Self::Alligator
        } else if rest.starts_with("oln") {
            Self::Elk
        } else if rest.starts_with("biz") {
            Self::Bison
        } else {
            Self::Other
        })
    }

    /// The size of its body against a man's (1..=4): how much blood it has to lose, how hard
    /// it is to kill outright ([`beast_outcome`]); the young one size less.
    pub fn size(self, small: bool) -> u8 {
        let s: u8 = match self {
            Self::Bison => 4,
            Self::Bear | Self::Alligator => 3,
            Self::Elk => 2,
            Self::Jaguar | Self::Other => 1,
        };
        if small { (s - 1).max(1) } else { s }
    }

    /// The young of a kind (`AnmMedSm`, `AnmMed_Sm`, `AnmCroSm`, `AnmOlnS`): weaker blows.
    pub fn small(md_name: &str) -> bool {
        let n = md_name.to_ascii_lowercase();
        n.ends_with("sm") || n == "anmolns"
    }

    /// The cause of one attack by percentage `roll`: a bear claws 70 % (bites 30), a
    /// jaguar claws or bites half and half, an alligator bites, elk and bison gore.
    pub fn cause(self, roll: i32) -> Cause {
        match self {
            Self::Bear if roll < BEAR_CLAWS => Cause::Claw,
            Self::Jaguar if roll < JAGUAR_CLAWS => Cause::Claw,
            Self::Bear | Self::Jaguar | Self::Alligator => Cause::Bite,
            Self::Elk | Self::Bison => Cause::Gore,
            Self::Other => Cause::Claw,
        }
    }

    /// The energy (0..=100) of its `cause`; `small`: the young ([`Beast::small`]).
    pub fn energy(self, cause: Cause, small: bool) -> u8 {
        let e: u8 = match (self, cause) {
            (Self::Bear, Cause::Claw) => 100,
            (Self::Bear, _) => 80,
            (Self::Jaguar, Cause::Claw) => 75,
            (Self::Jaguar, _) => 85,
            (Self::Alligator, _) => 95,
            (Self::Elk, _) => 80,
            (Self::Bison, _) => 100,
            (Self::Other, _) => 40,
        };
        if small { e * 2 / 3 } else { e }
    }

    /// Weights of the regions it strikes (torso, legs, arms, head; sum 100); `down`: the
    /// victim lies on the ground (the beast goes for the head, the neck and the belly; the
    /// arms come up to shield the face).
    pub fn regions(self, down: bool) -> [i32; 4] {
        match (self, down) {
            (Self::Bear, false) => [40, 10, 30, 20],
            (Self::Bear, true) => [45, 5, 20, 30],
            (Self::Jaguar, false) => [30, 10, 25, 35],
            (Self::Jaguar, true) => [25, 5, 20, 50],
            (Self::Alligator, false) => [20, 55, 20, 5],
            (Self::Alligator, true) => [35, 30, 20, 15],
            (Self::Elk | Self::Bison, false) => [70, 25, 5, 0],
            (Self::Elk | Self::Bison, true) => [75, 20, 5, 0],
            (Self::Other, _) => [30, 10, 30, 30],
        }
    }

    /// The region of an attack by percentage `roll` ([`Beast::regions`]).
    pub fn region(self, down: bool, roll: i32) -> HitRegion {
        let mut r = roll;
        for (region, weight) in
            [HitRegion::Torso, HitRegion::Legs, HitRegion::Arms, HitRegion::Head].into_iter().zip(self.regions(down))
        {
            if r < weight {
                return region;
            }
            r -= weight;
        }
        HitRegion::Torso
    }
}

/// Percent of a bear's and a jaguar's attacks that are claws (the rest bites).
pub const BEAR_CLAWS: i32 = 70;
/// An animal limping on three legs (percent of its speed).
pub const BEAST_LIMP: i32 = 45;
pub const JAGUAR_CLAWS: i32 = 50;

/// An animal's attack on `victim` (hit-locations rule): the beast of the attacker's `.MD`
/// name, or `None` (no random number drawn) for anything else. Two `rando()`: the cause
/// ([`Beast::cause`]) and the region ([`Beast::region`], by whether the victim is down);
/// the energy is the beast's.
pub fn maul(w: &mut World, sender: usize, victim: usize) -> Option<(Cause, u8, HitRegion)> {
    let name = &w.catalog.get(w.units[sender].kind).md_name;
    let beast = Beast::of(name)?;
    let small = Beast::small(name);
    let cause = beast.cause(pct(w));
    let region = beast.region(w.units[victim].body.downed(), pct(w));
    Some((cause, beast.energy(cause, small), region))
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

    /// A flesh wound made deep (mauling gashes and punctures).
    fn deep(self) -> Self {
        if self == Self::Wound { Self::DeepWound } else { self }
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

/// Cannonball energy (percent) from which a limb hit takes the limb off. A ball's energy is
/// what the bodies before it left of it (`projectile::Plough`: 100, 80, 60, 40, 20); a 6 to
/// 12 pdr roundshot keeps enough to kill over its whole flight and through several men, so
/// only the last, spent ball leaves a man in one piece (#262, the user's report "a direct
/// hit just tumbled him over").
pub const BALL_SEVERS: u8 = 15;
/// … and from which any hit on a man tears him apart: a trunk hit tears the body in two, a
/// leg hit takes the legs and throws the torso (torn in two at the hips), an arm hit goes
/// on through the chest beside it.
pub const BALL_TEARS: u8 = 40;
/// … and from which a trunk (or arm) hit leaves nothing whole by [`BALL_PULVERIZE_SHARE`]
/// percent (the rest are torn in two).
pub const BALL_PULVERIZES: u8 = 70;
pub const BALL_PULVERIZE_SHARE: i32 = 65;
/// Trunk levels (0 shoulders .. 255 hips) of a body torn by a ball through the legs (at the
/// hips: the legs stay, the torso is thrown) and through an arm (across the chest).
pub const BALL_HIPS: u8 = 236;
pub const BALL_CHEST: u8 = 70;

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

/// Claw energy from which a swipe tears an arm off (at the forearm or above the elbow) and
/// the share of arm hits that do. The share of belly and pelvis hits that tear the belly
/// open (from [`CLAW_TEARS`] and below it), of chest hits; head hits that kill (the throat
/// or the skull; from [`CLAW_TEARS`] and below), the rest tear the face and scalp.
pub const CLAW_TEARS: u8 = 80;
pub const CLAW_ARM_OFF: i32 = 15;
pub const CLAW_BELLY: [i32; 2] = [35, 12];
pub const CLAW_CHEST_BELLY: i32 = 10;
pub const CLAW_HEAD_KILL: [i32; 2] = [25, 10];
/// Bite energy from which the jaws take a limb off at the bite (an alligator's death roll),
/// and the share of limb hits that do; head and neck bites that kill.
pub const BITE_SEVERS: u8 = 90;
pub const BITE_OFF: i32 = 30;
pub const BITE_HEAD_KILL: i32 = 70;
/// Chest bites from [`BITE_SEVERS`] that crush the chest (kill); belly bites that open it.
pub const BITE_CHEST_KILL: i32 = 20;
pub const BITE_BELLY: i32 = 25;
/// Gore energy from which antlers and horns go deep: belly and pelvis hits that open the
/// belly (from it and below), chest hits that kill (heart or lungs), head hits that kill.
pub const GORE_DEEP: u8 = 80;
pub const GORE_BELLY: [i32; 2] = [45, 25];
pub const GORE_CHEST_KILL: i32 = 15;
pub const GORE_HEAD_KILL: i32 = 40;

/// Ticks a mauling hit keeps the victim down ([`Body::down`]): a claw from [`CLAW_KNOCKS`]
/// energy, a bite by [`BITE_KNOCKS`] percent (from [`BITE_SEVERS`] and below), a gore
/// always. A new hit while down renews it.
pub const DOWN_CLAW: u8 = 40;
pub const DOWN_BITE: u8 = 30;
pub const DOWN_GORE: u8 = 50;
pub const CLAW_KNOCKS: u8 = 70;
pub const BITE_KNOCKS: [i32; 2] = [60, 30];

/// The ticks a mauling hit by `cause` with `energy` and percentage `roll` (the outcome's)
/// knocks the victim down for; 0 for none.
pub fn knock_down(cause: Cause, energy: u8, roll: i32) -> u8 {
    match cause {
        Cause::Claw if energy >= CLAW_KNOCKS => DOWN_CLAW,
        // Decorrelated from the outcome, from the same roll.
        Cause::Bite if (roll * 37 + 11) % 100 < BITE_KNOCKS[usize::from(energy < BITE_SEVERS)] => DOWN_BITE,
        Cause::Gore => DOWN_GORE,
        _ => 0,
    }
}

/// Counts down the knocked-down ([`Body::down`]) once a tick; only with the hit-locations
/// rule (else nobody is ever down).
pub fn get_up(w: &mut World) {
    if !w.rules.hit_locations {
        return;
    }
    for u in &mut w.units {
        if u.body.down > 0 {
            u.body.down -= 1;
        }
    }
}

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
        // A shell's blast (energy 100) or one of its fragments (`crate::shrapnel`): the
        // weaker the fragment, the more often a flesh wound; below [`FRAGMENT_SEVERS`] it
        // takes nothing off.
        Cause::Shell => {
            let weak = (100 - i32::from(energy.min(100))) / 2;
            let off = 80 + weak * 2;
            match roll {
                r if r < 50 + weak => (Outcome::Wound, level),
                r if r < off || energy < FRAGMENT_SEVERS => (Outcome::Fracture(ball_break(50 + weak, 30)), level),
                _ => (Outcome::Off, level),
            }
        }
        Cause::Sabre => match roll {
            0..20 => (Outcome::Off, level.max(joint)),
            20..80 => (Outcome::DeepWound, level),
            _ => (Outcome::Fracture(FractureKind::Closed), level),
        },
        // Deep parallel gashes; a swipe at high energy tears the arm off at the forearm or
        // above the elbow; the paw's blow breaks the bone, often through the gashes.
        Cause::Claw => {
            let off = if !leg && energy >= CLAW_TEARS { CLAW_ARM_OFF } else { 0 };
            match roll - off {
                r if r < 0 => (Outcome::Off, level.max(ELBOW / 3)),
                r if r < 65 => (Outcome::DeepWound, level),
                r if r < 75 => (Outcome::Fracture(FractureKind::Open), level),
                r if r < 85 => (Outcome::Fracture(FractureKind::Closed), level),
                _ => (Outcome::Wound, level),
            }
        }
        // Crushing: the bone breaks under the jaws (mostly open, the teeth go through); an
        // alligator's death roll takes the limb off at the bite.
        Cause::Bite => {
            let off = if energy >= BITE_SEVERS { BITE_OFF } else { 0 };
            match roll - off {
                r if r < 0 => (Outcome::Off, level),
                r if r < 40 => (Outcome::Fracture(FractureKind::Open), level),
                r if r < 65 => (Outcome::Fracture(FractureKind::Closed), level),
                _ => (Outcome::DeepWound, level),
            }
        }
        // A puncture; thrown and trampled, the bone breaks.
        Cause::Gore => match roll {
            0..60 => (Outcome::DeepWound, level),
            60..85 => (Outcome::Fracture(FractureKind::Closed), level),
            _ => (Outcome::Fracture(FractureKind::Open), level),
        },
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

/// A cannonball with [`BALL_TEARS`] energy or more striking a man's limb does not stop at
/// it: through the legs it takes both and throws the torso (torn in two at the hips,
/// [`BALL_HIPS`]); through an arm it goes on across the chest ([`BALL_CHEST`]: pulverized
/// from [`BALL_PULVERIZES`] by [`BALL_PULVERIZE_SHARE`] percent, else torn in two there).
/// `None`: the limb table stands (a spent ball, or not a ball).
pub fn ball_through(cause: Cause, energy: u8, leg: bool, roll: i32) -> Option<(Part, u8, Outcome)> {
    if cause != Cause::Cannonball || energy < BALL_TEARS {
        return None;
    }
    Some(if leg {
        (Part::Pelvis, BALL_HIPS, Outcome::Torn(Torn::InTwo { level: BALL_HIPS }))
    } else {
        (Part::Chest, BALL_CHEST, trunk_outcome(cause, energy, Part::Chest, BALL_CHEST, roll))
    })
}

/// [`limb_outcome`] for a hit on `limb`, or on a man (not an animal) the ball going through
/// ([`ball_through`]).
fn limb_injury(w: &World, victim: usize, cause: Cause, energy: u8, limb: LimbId, level: u8, roll: i32) -> Injury {
    let man = w.units[victim].body.animal == 0;
    if let Some((part, level, outcome)) = ball_through(cause, energy, limb.is_leg(), roll).filter(|_| man) {
        return Injury { part, level, outcome, energy, cause };
    }
    let (outcome, level) = limb_outcome(cause, energy, limb.is_leg(), level, roll);
    Injury { part: Part::Limb(limb), level, outcome, energy, cause }
}

/// The outcome of a trunk hit on `part` by percentage `roll`.
pub fn trunk_outcome(cause: Cause, energy: u8, part: Part, level: u8, roll: i32) -> Outcome {
    let belly = |chance: i32| if roll < chance { Outcome::BellyOpen } else { Outcome::Wound };
    match cause {
        Cause::Musket if energy >= BALL_CLOSE => belly(MUSKET_BELLY),
        Cause::Musket => Outcome::Wound,
        Cause::Grapeshot | Cause::Sabre => belly(10),
        Cause::Shell => belly(20 * i32::from(energy.min(100)) / 100),
        // A deep stab or gash.
        Cause::Bayonet if roll < BAYONET_BELLY => Outcome::BellyOpen,
        Cause::Thrown if roll < THROWN_BELLY => Outcome::BellyOpen,
        Cause::Bayonet | Cause::Thrown => Outcome::DeepWound,
        Cause::Cannonball if energy >= BALL_PULVERIZES && roll < BALL_PULVERIZE_SHARE => {
            Outcome::Torn(Torn::Pulverized)
        }
        Cause::Cannonball if energy >= BALL_TEARS => Outcome::Torn(Torn::InTwo { level }),
        Cause::Cannonball if part == Part::Chest => Outcome::Kill,
        Cause::Cannonball => Outcome::BellyOpen,
        // Gashes across the trunk; the belly torn open.
        Cause::Claw => {
            let chance = match part {
                Part::Chest => CLAW_CHEST_BELLY,
                _ => CLAW_BELLY[usize::from(energy < CLAW_TEARS)],
            };
            belly(chance).deep()
        }
        Cause::Bite if part == Part::Chest && energy >= BITE_SEVERS && roll < BITE_CHEST_KILL => Outcome::Kill,
        Cause::Bite if part == Part::Belly => belly(BITE_BELLY).deep(),
        Cause::Bite => Outcome::DeepWound,
        // Antlers or horns through the chest or into the belly.
        Cause::Gore if part == Part::Chest && energy >= GORE_DEEP && roll < GORE_CHEST_KILL => Outcome::Kill,
        Cause::Gore if part == Part::Chest => Outcome::DeepWound,
        Cause::Gore => belly(GORE_BELLY[usize::from(energy < GORE_DEEP)]).deep(),
    }
}

/// The outcome of a head hit by percentage `roll`.
pub fn head_outcome(cause: Cause, roll: i32) -> Outcome {
    head_outcome_e(cause, 100, roll)
}

/// [`head_outcome`] with the blow's energy (mauling): claws tear the face and scalp and kill
/// by [`CLAW_HEAD_KILL`]; a bite to the head or the neck kills by [`BITE_HEAD_KILL`]; a horn
/// by [`GORE_HEAD_KILL`].
pub fn head_outcome_e(cause: Cause, energy: u8, roll: i32) -> Outcome {
    match cause {
        Cause::Cannonball => Outcome::Torn(Torn::Headless),
        Cause::Sabre if roll >= 50 => Outcome::Wound,
        Cause::Thrown if roll >= THROWN_HEAD_KILL => Outcome::DeepWound,
        Cause::Claw if roll >= CLAW_HEAD_KILL[usize::from(energy < CLAW_TEARS)] => Outcome::DeepWound,
        Cause::Bite if roll >= BITE_HEAD_KILL => Outcome::DeepWound,
        Cause::Gore if roll >= GORE_HEAD_KILL => Outcome::DeepWound,
        // A small shell fragment cuts the scalp or the face (no roll: the blast always kills).
        Cause::Shell if energy < FRAGMENT_HEAD_KILL => Outcome::DeepWound,
        _ => Outcome::Kill,
    }
}

/// Whether a head hit by `cause` draws a roll (the others always kill).
fn head_rolls(cause: Cause) -> bool {
    matches!(cause, Cause::Sabre | Cause::Thrown) || cause.mauls()
}

/// `injure`: the injury of a hit on `victim`'s `region` by `cause` with `energy`
/// (0..=100), applied to his body. Draws from the world RNG: for a limb its side, the level
/// and the outcome; for the trunk the level and the outcome; for the head the outcome.
pub fn injure(w: &mut World, victim: usize, cause: Cause, energy: u8, region: HitRegion) -> Injury {
    mark_animal(w, victim);
    let level_roll = |w: &mut World| ((w.random.rando() << 8) >> 15) as u8;
    let (injury, roll) = match region {
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
            (limb_injury(w, victim, cause, energy, limb, level, roll), roll)
        }
        HitRegion::Torso => {
            let level = level_roll(w);
            let part = match level {
                0..96 => Part::Chest,
                96..192 => Part::Belly,
                _ => Part::Pelvis,
            };
            let roll = pct(w);
            (Injury { part, level, outcome: trunk_outcome(cause, energy, part, level, roll), energy, cause }, roll)
        }
        HitRegion::Head => {
            let roll = if head_rolls(cause) || w.units[victim].body.animal != 0 { pct(w) } else { 0 };
            (Injury { part: Part::Head, level: 0, outcome: head_outcome_e(cause, energy, roll), energy, cause }, roll)
        }
    };
    let size = w.units[victim].body.animal;
    let injury = if size != 0 { Injury { outcome: beast_outcome(&injury, size, roll), ..injury } } else { injury };
    apply(&mut w.units[victim].body, &injury);
    knock(&mut w.units[victim].body, &injury, roll);
    injury
}

/// An injury aimed at `part` (the close-up bench): at `level`, or a level drawn as in
/// [`injure`]; the outcome is rolled as in battle.
pub fn injure_aimed(w: &mut World, victim: usize, cause: Cause, energy: u8, part: Part, level: Option<u8>) -> Injury {
    mark_animal(w, victim);
    let level_roll = |w: &mut World| ((w.random.rando() << 8) >> 15) as u8;
    let level = match level {
        Some(l) => l,
        None => level_roll(w),
    };
    let (injury, roll) = match part {
        Part::Limb(limb) => {
            let lost = w.units[victim].body.limb(limb).lost;
            let level = level.min(lost.saturating_sub(1));
            let roll = pct(w);
            (limb_injury(w, victim, cause, energy, limb, level, roll), roll)
        }
        Part::Head => {
            let roll = if head_rolls(cause) || w.units[victim].body.animal != 0 { pct(w) } else { 0 };
            (Injury { part, level: 0, outcome: head_outcome_e(cause, energy, roll), energy, cause }, roll)
        }
        _ => {
            let roll = pct(w);
            (Injury { part, level, outcome: trunk_outcome(cause, energy, part, level, roll), energy, cause }, roll)
        }
    };
    let size = w.units[victim].body.animal;
    let injury = if size != 0 { Injury { outcome: beast_outcome(&injury, size, roll), ..injury } } else { injury };
    apply(&mut w.units[victim].body, &injury);
    knock(&mut w.units[victim].body, &injury, roll);
    injury
}

/// Marks an animal's body ([`Body::animal`]) at its first injury, by its type's `.MD` name.
fn mark_animal(w: &mut World, victim: usize) {
    if w.units[victim].body.animal != 0 {
        return;
    }
    let name = &w.catalog.get(w.units[victim].kind).md_name;
    if let Some(beast) = Beast::of(name) {
        w.units[victim].body.animal = beast.size(Beast::small(name));
    }
}

/// An animal's outcome of an injury rolled on the tables of men (percentage `roll`, the
/// outcome's): no ball, blade, claw or jaw takes a leg off (it is shattered, or deeply
/// wounded), only cannonballs and shells; a head hit kills one of size `size` by
/// 100 / `size` percent (a deep wound otherwise); a big body (size 3 and up) is killed
/// rather than torn in two or pulverized.
pub fn beast_outcome(injury: &Injury, size: u8, roll: i32) -> Outcome {
    let size = i32::from(size.max(1));
    match (injury.part, injury.outcome) {
        (Part::Limb(_), Outcome::Off) if !matches!(injury.cause, Cause::Cannonball | Cause::Shell) => {
            if roll % 2 == 0 {
                Outcome::Fracture(FractureKind::Open)
            } else {
                Outcome::DeepWound
            }
        }
        // Decorrelated from the outcome's roll.
        (Part::Head, Outcome::Kill) if (roll * 53 + 17) % 100 >= 100 / size => Outcome::DeepWound,
        (_, Outcome::Torn(Torn::InTwo { .. } | Torn::Pulverized)) if size >= 3 => Outcome::Kill,
        (_, o) => o,
    }
}

/// A mauling hit knocks the victim down ([`knock_down`]), or keeps him down longer; never
/// an animal.
fn knock(body: &mut Body, injury: &Injury, roll: i32) {
    if body.animal != 0 {
        return;
    }
    let down = knock_down(injury.cause, injury.energy, roll);
    body.down = body.down.max(down);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn animal(size: u8) -> Body {
        Body { animal: size, ..Body::default() }
    }

    #[test]
    fn an_animal_limps_and_never_crawls() {
        let mut b = animal(3);
        b.limbs[LimbId::ArmL as usize].fracture = Some(Fracture { level: 100, kind: FractureKind::Open, bend: 0 });
        assert!(!b.crawling());
        assert!(b.limping());
        assert_eq!(b.move_percent(), BEAST_LIMP);
        assert!(!b.drops_long_weapon());
        // Two legs gone: it barely moves, still not crawling.
        b.limbs[LimbId::LegR as usize].lost = KNEE;
        assert_eq!(b.move_percent(), 10);
        assert!(!b.crawling());
    }

    #[test]
    fn wounds_slow_an_animal_and_heavy_ones_make_it_flee() {
        let mut b = animal(3);
        b.limbs[LimbId::LegL as usize].wounds = 1;
        assert!(b.move_percent() < 100 && b.move_percent() >= 60);
        assert!(!b.flees() && !b.out_of_fight());
        b.chest_wounds = 4;
        assert!(b.flees() && b.out_of_fight());
        let mut belly = animal(4);
        belly.belly_open = true;
        assert!(belly.flees());
        assert!(belly.move_percent() <= 50);
    }

    #[test]
    fn an_animal_bleeds_by_its_size() {
        let man = Body { belly_open: true, ..Body::default() };
        let bison = Body { belly_open: true, ..animal(4) };
        assert_eq!(bison.bleeding(), 4 * man.bleeding());
    }

    #[test]
    fn balls_do_not_take_an_animals_leg_off() {
        let i = |part, outcome, cause| Injury { part, level: 100, outcome, energy: 100, cause };
        let off = i(Part::Limb(LimbId::LegL), Outcome::Off, Cause::Musket);
        assert_ne!(beast_outcome(&off, 3, 4), Outcome::Off);
        let ball = i(Part::Limb(LimbId::LegL), Outcome::Off, Cause::Cannonball);
        assert_eq!(beast_outcome(&ball, 3, 4), Outcome::Off);
        let torn = i(Part::Chest, Outcome::Torn(Torn::InTwo { level: 90 }), Cause::Cannonball);
        assert_eq!(beast_outcome(&torn, 4, 0), Outcome::Kill);
        assert_eq!(beast_outcome(&torn, 1, 0), torn.outcome);
        // A head shot kills a jaguar always, a bison by a quarter.
        let head = i(Part::Head, Outcome::Kill, Cause::Musket);
        assert!((0..100).all(|r| beast_outcome(&head, 1, r) == Outcome::Kill));
        let kills = (0..100).filter(|&r| beast_outcome(&head, 4, r) == Outcome::Kill).count();
        assert_eq!(kills, 25);
    }

    #[test]
    fn a_man_hashes_as_before() {
        use std::hash::DefaultHasher;
        let h = |b: &Body| {
            let mut s = DefaultHasher::new();
            b.hash(&mut s);
            s.finish()
        };
        let man = Body { chest_wounds: 1, ..Body::default() };
        let mut manual = DefaultHasher::new();
        (man.limbs, man.belly_open, man.head_wounds, man.chest_wounds, man.burn, man.dropped).hash(&mut manual);
        man.mounted.hash(&mut manual);
        man.down.hash(&mut manual);
        assert_eq!(h(&man), manual.finish());
    }
}
