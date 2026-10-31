//! The editor's second view, "Schlacht" (#404): two nations' army presets (`presets.rs`)
//! put facing each other across the middle of the map, in one click. Not in the original.
//!
//! The window switches between the unit list and this view with a button in its title row (a
//! combo box line would shift the nation indices the rest of the editor counts by). Seite 1
//! is player 1 and faces east, Seite 2 is player 2 and faces west. The map is square, so
//! which axis the line of battle runs along makes no difference to what fits; it runs
//! west-east, the armies' widths across the north-south axis. The fronts stand 1.5 times the
//! longest range of the armies' main infantry apart (at least [`MIN_GAP`]). A second
//! "Aufstellen" first removes the units the first one placed, so a battle can be re-rolled.

use acr_formats::m3d::MapSize;
use acr_sim::World;
use acr_sim::formation;

use super::presets::{self, Block, Era, Error, Role, Size};
use super::{Act as EditorAct, ENTRIES, Editor};
use super::{FIRST_EXTRA, Item, Look, OY, ROW, W, X0, sides};
use crate::ui::draw::Rect;

/// The fronts never stand closer than this, world pixels.
const MIN_GAP: i32 = 400;
/// The facings of the two sides: east and west.
const FACING: [u8; 2] = [0, 128];

/// A click on the battle view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Switches between the unit list and the battle view.
    Toggle,
    /// Another nation for side `0` or `1`, `d` lines down the list.
    Nation(usize, i32),
    /// Another century for the side.
    Era(usize, i32),
    Size(i32),
    /// Fighting starts at once (otherwise the editor's Kampf switch is left as it is).
    Fight,
    /// Places both armies.
    Go,
}

/// The battle view's state.
#[derive(Debug, Clone)]
pub struct Battle {
    pub on: bool,
    nation: [usize; 2],
    era: [Option<Era>; 2],
    size: Size,
    fight: bool,
    /// The units the last "Aufstellen" placed.
    placed: Vec<acr_sim::EntityId>,
}

impl Default for Battle {
    fn default() -> Self {
        Self { on: false, nation: [2, 3], era: [None, None], size: Size::Medium, fight: false, placed: Vec::new() }
    }
}

impl Battle {
    /// The century in use for a side: the chosen one if the nation has it, else the default.
    fn era_of(&self, side: usize) -> Option<Era> {
        let all = presets::eras(self.nation[side]);
        self.era[side].filter(|e| all.contains(e)).or_else(|| presets::default_era(self.nation[side]))
    }
}

/// How far an army reaches around its origin in its own frame (forward and right-hand side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent {
    pub front: i32,
    pub back: i32,
    pub left: i32,
    pub right: i32,
}

impl Extent {
    pub fn of(blocks: &[Block]) -> Self {
        let mut e = Extent { front: i32::MIN, back: i32::MAX, left: i32::MAX, right: i32::MIN };
        for b in blocks {
            e.front = e.front.max(b.forward + b.reach.front);
            e.back = e.back.min(b.forward + b.reach.back);
            e.left = e.left.min(b.side + b.reach.left);
            e.right = e.right.max(b.side + b.reach.right);
        }
        e
    }
}

/// Where the two armies' origins stand: side 0 facing east at the left of the middle, side 1
/// facing west at the right, their fronts `gap` apart and each centred across the line, all
/// of both at least [`presets::MAP_MARGIN`] inside a map `side` pixels wide.
pub fn origins(ext: [Extent; 2], gap: i32, side: i32) -> Result<[(i32, i32); 2], Error> {
    let mid = side / 2;
    let across = |e: Extent| (e.left + e.right) / 2;
    let a = (mid - gap / 2 - ext[0].front, mid - across(ext[0]));
    // Facing west the frame is turned half way round: forward and side both flip.
    let b = (mid + gap / 2 + ext[1].front, mid + across(ext[1]));
    let inside = |v: i32| (presets::MAP_MARGIN..=side - presets::MAP_MARGIN).contains(&v);
    let fits = inside(a.0 + ext[0].back)
        && inside(b.0 - ext[1].back)
        && inside(a.1 + ext[0].left)
        && inside(a.1 + ext[0].right)
        && inside(b.1 - ext[1].right)
        && inside(b.1 - ext[1].left);
    if fits { Ok([a, b]) } else { Err(Error::OffMap) }
}

/// The longest range of a side's main infantry, world pixels.
fn range(world: &World, blocks: &[Block]) -> i32 {
    blocks
        .iter()
        .filter(|b| b.role == Role::Front)
        .flat_map(|b| world.catalog.get(b.kind).attacks.iter().map(|a| a.radius.1))
        .max()
        .unwrap_or(0)
}

/// The gap between the fronts: 1.5 times the longer range, at least [`MIN_GAP`].
pub fn gap_of(ranges: [i32; 2]) -> i32 {
    (ranges[0].max(ranges[1]) * 3 / 2).max(MIN_GAP)
}

/// Places both armies of `battle` across the middle of the map; returns the soldiers placed
/// per side. Nothing stays placed when either army does not fit or stands in water.
fn deploy(world: &mut World, battle: &mut Battle) -> Result<[usize; 2], Error> {
    // The previous battle goes first, its units' ids kept (the slots are only freed).
    undo(world, std::mem::take(&mut battle.placed));
    let plans = [0, 1].map(|s| presets::blocks(world, battle.nation[s], battle.size, battle.era_of(s)));
    let [Ok(p0), Ok(p1)] = plans else { return Err(plans.into_iter().find_map(Result::err).expect("an error")) };
    let gap = gap_of([range(world, &p0), range(world, &p1)]);
    let side = MapSize { shift: world.map.shift }.playable_size();
    let at = origins([Extent::of(&p0), Extent::of(&p1)], gap, side)?;
    let before: Vec<bool> = world.units.iter().map(|u| u.alive).collect();
    let mut men = [0; 2];
    for s in 0..2 {
        match presets::place_preset(world, battle.nation[s], battle.size, battle.era_of(s), s as u8, at[s], FACING[s]) {
            Ok(n) => men[s] = n,
            Err(e) => {
                undo(world, fresh(world, &before));
                return Err(e);
            }
        }
    }
    battle.placed = fresh(world, &before);
    Ok(men)
}

/// The units alive now that were not before.
fn fresh(world: &World, before: &[bool]) -> Vec<acr_sim::EntityId> {
    world
        .units
        .iter()
        .enumerate()
        .filter(|(i, u)| u.alive && !before.get(*i).copied().unwrap_or(false))
        .map(|(_, u)| u.id)
        .collect()
}

fn undo(world: &mut World, ids: Vec<acr_sim::EntityId>) {
    for id in ids {
        if let Some(b) = world.units.get(id.0 as usize).and_then(|u| u.brigade) {
            formation::dissolve(world, usize::from(b));
        }
        world.remove(id);
    }
}

impl Editor {
    /// A click on the battle view.
    pub(super) fn battle_act(&mut self, world: &mut World, act: Act) {
        let b = &mut self.battle;
        let step = |v: usize, d: i32| (v as i32 + d).rem_euclid(FIRST_EXTRA as i32) as usize;
        match act {
            Act::Toggle => b.on = !b.on,
            Act::Nation(s, d) => {
                b.nation[s] = step(b.nation[s], d);
                b.era[s] = None;
            }
            Act::Era(s, d) => {
                let all = presets::eras(b.nation[s]);
                if let Some(i) = all.iter().position(|&e| Some(e) == b.era_of(s)) {
                    b.era[s] = Some(all[(i as i32 + d).rem_euclid(all.len() as i32) as usize]);
                }
            }
            Act::Size(d) => {
                let i = Size::ALL.iter().position(|&s| s == b.size).unwrap_or(0) as i32;
                b.size = Size::ALL[(i + d).rem_euclid(Size::ALL.len() as i32) as usize];
            }
            Act::Fight => b.fight = !b.fight,
            Act::Go => {
                let result = deploy(world, b);
                if b.fight && self.sides == sides::Mode::Off {
                    self.sides = sides::Mode::Free;
                }
                self.apply_sides(world);
                self.status = match result {
                    Ok([a, c]) => {
                        let side = MapSize { shift: world.map.shift }.playable_size();
                        self.camera_to = Some((side / 2, side / 2));
                        format!("Schlacht: {a} gegen {c} Mann").into_bytes()
                    }
                    Err(e) => e.to_string().into_bytes(),
                };
            }
        }
    }

    /// The battle view's lines inside the window.
    pub(super) fn battle_items(&self) -> Vec<Item> {
        let b = &self.battle;
        let mut out = Vec::new();
        let mut y = OY + 10;
        let text = |y: i32, text: String, name: &str, hot: bool, act: Option<Act>| Item {
            rect: Rect::new(X0, y, W, ROW),
            text: text.into_bytes(),
            name: name.into(),
            hot,
            act: act.map(EditorAct::Battle),
            look: Look::Text,
        };
        // A value with an arrow to each side, like the settings under the unit list.
        let pair = |out: &mut Vec<Item>, y: i32, value: String, name: &str, less: Act, more: Act| {
            let arrow = |x: i32, t: &str, d: &str, act: Act| Item {
                rect: Rect::new(x, y, 24, ROW),
                text: t.as_bytes().to_vec(),
                name: format!("{name} {d}"),
                hot: false,
                act: Some(EditorAct::Battle(act)),
                look: Look::Text,
            };
            out.push(arrow(X0, "<", "-", less));
            out.push(Item { rect: Rect::new(X0 + 24, y, W - 48, ROW), ..text(y, value, name, false, None) });
            out.push(arrow(X0 + W - 24, ">", "+", more));
        };
        for s in 0..2 {
            let n = s + 1;
            let nation = format!("Seite {n}: {}", ENTRIES[b.nation[s]].0);
            pair(&mut out, y, nation, &format!("Seite {n} Nation"), Act::Nation(s, -1), Act::Nation(s, 1));
            y += ROW;
            let era = b.era_of(s).map_or("ein Heer", Era::label).to_owned();
            if presets::eras(b.nation[s]).len() > 1 {
                pair(&mut out, y, era, &format!("Seite {n} Jahrhundert"), Act::Era(s, -1), Act::Era(s, 1));
            } else {
                out.push(Item { rect: Rect::new(X0 + 24, y, W - 48, ROW), ..text(y, era, "", false, None) });
            }
            y += ROW;
        }
        // The window's divider lies under the two sides; the rest stands below it.
        y = OY + 80;
        pair(&mut out, y, format!("Groesse: {}", b.size.label()), "Groesse", Act::Size(-1), Act::Size(1));
        y += ROW;
        let fight = if b.fight { "Kampf sofort: AN" } else { "Kampf sofort: AUS" };
        out.push(text(y, fight.into(), "Kampf sofort", b.fight, Some(Act::Fight)));
        y += ROW + 6;
        out.push(text(y, "Aufstellen".into(), "Aufstellen", false, Some(Act::Go)));
        y += ROW + 6;
        let mut status = self.status.clone();
        status.truncate(44);
        out.push(Item { text: status, ..text(y, String::new(), "", false, None) });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDE: i32 = 15360;

    fn ext(front: i32, back: i32, half: i32) -> Extent {
        Extent { front, back, left: -half, right: half }
    }

    #[test]
    fn the_fronts_stand_a_gap_apart_facing_each_other_across_the_middle() {
        let e = [ext(300, -900, 2000), ext(250, -1200, 3500)];
        let [a, b] = origins(e, 1200, SIDE).unwrap();
        // Facing east and west: the front edges.
        let (fa, fb) = (a.0 + e[0].front, b.0 - e[1].front);
        assert_eq!(fb - fa, 1200);
        assert_eq!((fa + fb) / 2, SIDE / 2);
        // Each is centred across the line.
        assert_eq!(a.1, SIDE / 2);
        assert_eq!(b.1, SIDE / 2);
        assert_eq!(FACING[1].wrapping_sub(FACING[0]), 128);
    }

    #[test]
    fn the_widest_army_still_fits_the_map() {
        // A Gross army is 4000 to 8700 px wide.
        let wide = ext(400, -1500, 4350);
        let [a, b] = origins([wide, wide], gap_of([700, 700]), SIDE).unwrap();
        assert!(a.0 - 1500 >= presets::MAP_MARGIN && b.0 + 1500 <= SIDE - presets::MAP_MARGIN);
        assert!(a.1 - 4350 >= presets::MAP_MARGIN && a.1 + 4350 <= SIDE - presets::MAP_MARGIN);
    }

    #[test]
    fn an_army_wider_than_the_map_is_refused() {
        assert_eq!(origins([ext(10, -10, 7600), ext(10, -10, 100)], 400, SIDE), Err(Error::OffMap));
    }

    #[test]
    fn the_gap_is_one_and_a_half_ranges_but_never_small() {
        assert_eq!(gap_of([640, 800]), 1200);
        assert_eq!(gap_of([0, 50]), MIN_GAP);
    }

    /// Every roster against itself in every size, on the 15360 px map: both armies inside the
    /// map on their own half, the fronts at least the gap apart, and a second "Aufstellen"
    /// replaces the first one's units (the same count stands, none of the old ones).
    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR)"]
    fn every_roster_deploys_inside_the_map_with_the_fronts_apart() {
        let base = presets::tests::world();
        let side = MapSize { shift: base.map.shift }.playable_size();
        let inside = |v: i32| (presets::MAP_MARGIN..=side - presets::MAP_MARGIN).contains(&v);
        for r in presets::ROSTERS {
            for size in Size::ALL {
                let mut w = base.clone();
                let mut battle = Battle { nation: [r.nation; 2], era: [r.era; 2], size, ..Battle::default() };
                let name = format!("{} {:?} {size:?}", ENTRIES[r.nation].0, r.era);
                let men = deploy(&mut w, &mut battle).expect(&name);
                assert!(men[0] > 0 && men[0] == men[1], "{name}: {men:?}");
                let first = battle.placed.clone();
                let x_of = |w: &World, owner: u8| -> Vec<i32> {
                    w.units.iter().filter(|u| u.alive && u.owner == owner).map(|u| u.pos.x >> 4).collect()
                };
                for owner in 0..2u8 {
                    let ys: Vec<_> =
                        w.units.iter().filter(|u| u.alive && u.owner == owner).map(|u| u.pos.y >> 4).collect();
                    assert!(x_of(&w, owner).into_iter().chain(ys).all(inside), "{name}: player {owner} off the map");
                }
                let (a, b) = (x_of(&w, 0), x_of(&w, 1));
                assert!(a.iter().all(|&x| x < side / 2) && b.iter().all(|&x| x > side / 2), "{name}: halves");
                let reach = range(&w, &presets::blocks(&w, r.nation, size, r.era).unwrap());
                let want = gap_of([reach; 2]);
                let gap = a.iter().max().zip(b.iter().min()).map(|(a, b)| b - a).unwrap();
                assert!(gap >= want, "{name}: fronts {gap} apart, wanted {want}");
                // Again: the first battle's units are gone, the count is the same.
                let again = deploy(&mut w, &mut battle).expect(&name);
                assert_eq!(again, men, "{name}");
                assert_eq!(w.units.iter().filter(|u| u.alive).count(), first.len(), "{name}: re-roll");
            }
        }
    }
}
