//! The editor's training levels (docs/controls.md, "Editor"): a right click on a unit icon
//! opens a drop-down with its levels. Level 0 is "Basis" (no upgrade); level `n` applies the
//! first `n` upgrades of [`acr_sim::upgrade::of_type`] (the nation file's order), and is
//! named by the text of upgrade `n`, as the game's upgrade hint shows it ("Ausbildung I").

use acr_sim::tech::UpgradeId;
use acr_sim::{TypeId, World};

use super::{BOX_X1, ROW};
use crate::hud::SCREEN;
use crate::ui::draw::Rect;

/// The first line: the type as it is, no upgrade applied.
pub const BASIS: &str = "Basis";
/// The drop-down is as wide as its longest line needs, at about this many pixels per
/// character of the small font, within these limits (the upgrade texts name their effect).
const CHAR_W: i32 = 6;
const MIN_WIDTH: i32 = 150;
const MAX_WIDTH: i32 = 500;

/// An open drop-down: the type it picks and the icon cell it hangs from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Menu {
    pub kind: TypeId,
    pub anchor: Rect,
}

/// How many levels above Basis `kind` has (its upgrades).
pub fn steps(world: &World, kind: TypeId) -> usize {
    acr_sim::upgrade::of_type(&world.catalog, kind).len()
}

/// The names of the levels of `kind`: Basis, then one per upgrade. An upgrade without text
/// is named by its id.
pub fn names(world: &World, kind: TypeId, text: &dyn Fn(UpgradeId) -> Vec<u8>) -> Vec<Vec<u8>> {
    let mut out = vec![BASIS.as_bytes().to_vec()];
    for up in acr_sim::upgrade::of_type(&world.catalog, kind) {
        let name = text(up);
        out.push(if name.is_empty() { world.catalog.tech.upgrade(up).id.clone().into_bytes() } else { name });
    }
    out
}

/// Puts level `level` of `kind` in force for `owner`: exactly the first `level` upgrades,
/// on top of the type's own characteristics. Returns the level applied (at most the type's
/// upgrade count).
pub fn apply(world: &mut World, owner: u8, kind: TypeId, level: usize) -> usize {
    let level = level.min(steps(world, kind));
    acr_sim::upgrade::set_type_level(world, owner, kind, level);
    level
}

impl Menu {
    /// The lines' rectangles for `n` levels whose longest name has `chars` characters:
    /// under the icon, moved up or left to stay on the screen and clear of the window's edge.
    pub fn rows(&self, n: usize, chars: usize) -> Vec<Rect> {
        let h = n as i32 * ROW;
        let width = (chars as i32 * CHAR_W + 8).clamp(MIN_WIDTH, MAX_WIDTH);
        let x = self.anchor.x0.min(BOX_X1 - width - 2).max(0);
        let below = self.anchor.y1;
        let y = if below + h > SCREEN[1] { (self.anchor.y0 - h).max(0) } else { below };
        (0..n as i32).map(|i| Rect::new(x, y + i * ROW, width, ROW)).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use acr_formats::defs::{NationDef, UnitDef};
    use acr_sim::catalog::{Catalog, ObjectType};
    use acr_sim::formation::Formations;
    use acr_sim::map::MapGrid;
    use acr_sim::tech::Tech;

    use super::*;

    const PIKEMAN: &str = "NAME Pik\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\nATTACK_RADIUS 0 0 40\r\nDAMAGE 0 7\r\n\
        WEAPONKIND 0 PIKA\r\n[END]\r\n";
    const NATION: &[u8] = b"[UPGRADE]\r\n\
        DMG ICO #POSITION 0 0 #LEVEL 1 M #COST 0 #TIME 3\r\nDAMAGE WEAPONKIND PIKA Pik(EN) +2\r\n\
        TOUGH ICO #POSITION 1 0 #LEVEL 1 M #COST 0 #TIME 3\r\nLIFE GRP 1 Pik(EN) +50%\r\n\
        DMG2 ICO #POSITION 2 0 #LEVEL 2 M #COST 0 #TIME 3\r\nDAMAGE WEAPONKIND PIKA Pik(EN) +3\r\n\
        [ENABLED]\r\nPik(EN)\r\n[UPGRADEPLACE]\r\nPik(EN) 7 DMG\r\nTOUGH\r\nDMG2\r\n[END]\r\n";

    fn pikeman() -> (World, TypeId) {
        let mut c = Catalog::new();
        let pik = c.add(ObjectType::from_def("Pik(EN)", &UnitDef::parse(PIKEMAN.as_bytes())));
        let mut tech = Tech::default();
        tech.add_nation(&c, &NationDef::parse(NATION));
        c.tech = tech;
        (World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default())), pik)
    }

    #[test]
    fn each_level_applies_exactly_the_upgrades_up_to_it() {
        let (mut w, pik) = pikeman();
        assert_eq!(steps(&w, pik), 3);
        // Damage and life after the first n upgrades (DMG +2, TOUGH +50% life, DMG2 +3).
        let expect = [(7, 100), (9, 100), (9, 150), (12, 150)];
        // Step up and down: a level never keeps traces of a higher one.
        for n in [0, 1, 2, 3, 1, 0, 3, 2] {
            assert_eq!(apply(&mut w, 0, pik, n), n);
            let adv = w.adv(0, pik);
            assert_eq!((adv.attacks[0].damage, adv.life), expect[n], "level {n}");
            assert_eq!(w.adv(1, pik).attacks[0].damage, 7, "other players keep Basis");
        }
        assert_eq!(apply(&mut w, 0, pik, 9), 3, "beyond the last step is fully trained");
    }

    #[test]
    fn levels_are_named_basis_then_by_upgrade_text() {
        let (w, pik) = pikeman();
        let text = |u: UpgradeId| if u.0 == 1 { Vec::new() } else { format!("Ausbildung {}", u.0).into_bytes() };
        let names: Vec<String> = names(&w, pik, &text).into_iter().map(|n| String::from_utf8(n).unwrap()).collect();
        assert_eq!(names, ["Basis", "Ausbildung 0", "TOUGH", "Ausbildung 2"]);
    }

    #[test]
    fn the_drop_down_stays_on_screen() {
        let menu = Menu { kind: TypeId(0), anchor: Rect::new(900, 700, 41, 41) };
        let rows = menu.rows(4, 10);
        assert!(rows.iter().all(|r| r.x1 <= BOX_X1 && r.y1 <= SCREEN[1]));
        assert!(menu.rows(2, 70)[0].width() > rows[0].width(), "long texts widen it");
        assert_eq!(rows[1].y0 - rows[0].y0, ROW);
        let high = Menu { kind: TypeId(0), anchor: Rect::new(760, 100, 41, 41) };
        assert_eq!(high.rows(2, 10)[0].y0, 141);
    }
}
