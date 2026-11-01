//! The interface for the Svelte touch skin (docs/webui.md "Touch skin"): the pieces it lays out
//! itself, each as the original art's draws in a box of its own, instead of the canvas groups the
//! classic skin copies. Only what the pieces show lives here; taps come back as intents.

use super::*;

/// A resource of the top bar: icon and number.
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub key: &'static str,
    pub value: u32,
    pub size: [i32; 2],
    pub draws: Vec<Draw>,
}

/// What a tapped icon asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Select,
    Command,
    Army,
}

/// An icon of the command grid or the army list.
#[derive(Debug, Clone, PartialEq)]
pub struct TouchIcon {
    pub id: String,
    pub role: Role,
    pub size: [i32; 2],
    pub draws: Vec<Draw>,
    /// The touch skin's own drawing of the command (see [`glyph`]); `None`: the original's
    /// picture, enlarged, is in `draws`.
    pub glyph: Option<String>,
    pub hint: Vec<u8>,
    pub disabled: bool,
    pub selected: bool,
    pub clickable: bool,
    /// A unit production icon: Ctrl+click is an endless order.
    pub endless: bool,
}

/// The pieces of one frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TouchView {
    pub resources: Vec<Resource>,
    pub icons: Vec<TouchIcon>,
    pub army: Vec<TouchIcon>,
    pub paused: bool,
    pub minimap: bool,
    /// The game speed slider's position, `100 - FPSTime` (set by the game, which owns the setting).
    pub speed: i32,
    /// `SLOWMOD` and `FASTMOD`.
    pub speed_text: [Vec<u8>; 2],
}

/// How many times the touch skin enlarges the original's pictures that have no glyph (the
/// pixel-art scaler, `webui::upscale`): a 40 pixel icon is shown at 150 to 200 device pixels.
pub const PORTRAIT_SCALE: u32 = 4;

/// The glyph (`ui/src/lib/icons/`) that stands for `icon`, chosen by what it does, never by its
/// picture: the touch skin draws these itself. Unit, building and upgrade portraits, resources
/// and anything unknown have none and fall back to the original's picture. A formation's
/// glyph follows its `NRES.DAT [ORDERICONS]` group, found by the icon's frame.
pub fn glyph(icon: &Icon, data: &HudData) -> Option<String> {
    use HudCommand as C;
    let name = match icon.action.as_ref()? {
        C::StandGround(true) => "stand-ground",
        C::StandGround(false) => "advance",
        C::AttackMode => "attack-move",
        C::PatrolMode => "patrol",
        C::AttackGroundMode => "attack-ground",
        C::MortarZoneMode => "mortar-zone",
        C::MortarClearZones => "mortar-clear",
        C::MortarFire(true) => "mortar-fire",
        C::MortarFire(false) => "mortar-cease",
        C::AimGuides => "aim-guide",
        C::HoldFire(true) => "hold-fire",
        C::HoldFire(false) => "open-fire",
        C::FriendlyFire(true) => "friendly-fire",
        C::FriendlyFire(false) => "friendly-safe",
        C::MeleeOnly(true) => "melee-only",
        C::MeleeOnly(false) => "any-attack",
        C::StopProduction(true) => "production-stop",
        C::StopProduction(false) => "production-resume",
        C::Canister => "canister-fire",
        C::LoadCanister => "canister-load",
        C::StopFishing(true) => "fishing-stop",
        C::StopFishing(false) => "fishing-resume",
        C::StopSowing(true) => "sowing-stop",
        C::StopSowing(false) => "sowing-resume",
        C::Disband => "disband",
        C::Group(true) => "group",
        C::Group(false) => "ungroup",
        C::Rally => "rally",
        C::SelectIdle(false) => "idle-peasant",
        C::SelectIdle(true) => "idle-mine",
        C::ArmyList => "army-list",
        C::FormSize(true) => "form-smaller",
        C::FormSize(false) => "form-larger",
        C::Fill => "form-fill",
        C::Page(p) if *p < 3 => return Some(format!("page-{}", p + 1)),
        C::MarketTrade => "market-trade",
        C::MarketClear => "market-clear",
        C::MarketAmount(n) if *n != 0 => {
            let sign = if *n > 0 { "plus" } else { "minus" };
            return Some(format!("amount-{sign}-{}", n.unsigned_abs().ilog10()));
        }
        C::FormGroup(_) | C::Reform(_) => {
            let group = &data.orders.iter().find(|o| o.icon.frame == icon.frame)?.group;
            match group.trim_end_matches('2').to_ascii_uppercase().as_str() {
                "LINE" => "formation-line",
                "SQUARE" => "formation-column",
                "KARE" => "formation-square",
                "PRUS" => "formation-crescent",
                "TRI" => "formation-wedge",
                "SHER" => "formation-cavalry-line",
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(name.to_owned())
}

/// Names of the resource bar's slots, left to right (the classic bar swaps `FOOD` and `GOLD`
/// against `NRES.DAT`, 0x4ece10).
const SLOTS: [&str; touch::ROWS] = ["wood", "food", "stone", "gold", "iron", "coal"];
/// Gap between a resource's icon and its number.
const NUMBER_GAP: i32 = 4;

impl Hud {
    /// The touch skin's pieces for `view`; `None` while the game's palette is not loaded.
    pub fn touch_view(&self, ctx: &Ctx, view: &View) -> Option<TouchView> {
        let palette = ctx.lib.palette(GAME_PALETTE)?;
        let resources = SLOTS
            .iter()
            .enumerate()
            .map(|(slot, &key)| {
                let value = view.resources[match slot {
                    1 => 3,
                    3 => 1,
                    s => s,
                }];
                let lot;
                let text: &[u8] = if value >= 10_000_000 {
                    ctx.texts.get("RES_LOT_MONEY")
                } else {
                    lot = value.to_string();
                    lot.as_bytes()
                };
                let sx = TOUCH_ICON_X + slot as i32 * RES_STEP;
                let clip = Rect::new(0, 0, touch::ICON_W, touch::BODY_H);
                let x = touch::ICON_W + NUMBER_GAP;
                let width = ctx.fonts.width(ctx.lib, font::YELLOW, text);
                let draws = vec![
                    Draw::game_frame(gp::RESPANEL, 0, -sx, -1, 0).clipped(clip),
                    Draw::game_text(font::YELLOW, text, x, 6),
                ];
                Resource { key, value, size: [x + width, touch::BODY_H], draws }
            })
            .collect();
        let icon = |i: &Icon, id: String, role: Role| {
            let mut draws = Vec::new();
            draw_icon(ctx, i, 0, 0, false, &palette, &mut draws);
            let glyph = self.data.as_deref().and_then(|data| glyph(i, data));
            if glyph.is_some() {
                // The first draw is the original's picture; the glyph takes its place and the
                // outline, life bar and numbers stay.
                draws.remove(0);
            }
            TouchIcon {
                id,
                role,
                size: [ICON, ICON],
                draws,
                glyph,
                hint: i.hint.clone(),
                disabled: i.disabled,
                selected: i.selected,
                clickable: i.action.is_some() && !i.disabled,
                endless: matches!(i.action, Some(HudCommand::Produce(_))),
            }
        };
        let icons = view
            .icons
            .iter()
            .filter(|i| Self::shown(i.slot))
            .map(|i| {
                let role = match i.action {
                    Some(HudCommand::SelectType(_) | HudCommand::SelectBrigade(_)) => Role::Select,
                    _ => Role::Command,
                };
                icon(i, format!("hud/{}", i.slot), role)
            })
            .collect();
        let army = Self::army_icons(view).map(|i| icon(i, format!("army/{}", i.slot), Role::Army)).collect();
        Some(TouchView {
            resources,
            icons,
            army,
            paused: view.paused,
            minimap: self.minimap.is_some(),
            speed: 50,
            speed_text: [ctx.texts.get("SLOWMOD").to_vec(), ctx.texts.get("FASTMOD").to_vec()],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(action: HudCommand) -> Icon {
        Icon { frame: 69, action: Some(action), ..Icon::default() }
    }

    fn data() -> HudData {
        HudData::parse(
            b"A\r\nFormation_Line\r\nFormation_Kavalery_C\r\n",
            b"[ORDERICONS]\r\nLINE 8 0 Formation_Line X\r\nPRUS2 8 2 Formation_Kavalery_C Y\r\n[END]\r\n",
        )
    }

    #[test]
    fn commands_get_glyphs_by_what_they_do() {
        let d = data();
        let name = |c| glyph(&with(c), &d);
        assert_eq!(name(HudCommand::AttackMode).as_deref(), Some("attack-move"));
        assert_eq!(name(HudCommand::StopProduction(true)).as_deref(), Some("production-stop"));
        assert_eq!(name(HudCommand::StopProduction(false)).as_deref(), Some("production-resume"));
        assert_eq!(name(HudCommand::Page(2)).as_deref(), Some("page-3"));
        assert_eq!(name(HudCommand::MarketAmount(-1000)).as_deref(), Some("amount-minus-3"));
        assert_eq!(name(HudCommand::MarketAmount(10)).as_deref(), Some("amount-plus-1"));
    }

    #[test]
    fn portraits_and_unknowns_have_no_glyph() {
        let d = data();
        for c in [
            HudCommand::SelectType(1),
            HudCommand::Produce(3),
            HudCommand::Place(3),
            HudCommand::Leave(4),
            HudCommand::Upgrade(2),
            HudCommand::MarketSell(1),
            HudCommand::Page(3),
            HudCommand::MarketAmount(0),
        ] {
            assert_eq!(glyph(&with(c.clone()), &d), None, "{c:?}");
        }
        assert_eq!(glyph(&Icon::default(), &d), None, "no action");
    }

    #[test]
    fn formations_follow_their_group_by_the_icon() {
        let d = data();
        let mut line = with(HudCommand::FormGroup(0));
        line.frame = 1;
        assert_eq!(glyph(&line, &d).as_deref(), Some("formation-line"));
        let mut cavalry = with(HudCommand::Reform(7));
        cavalry.frame = 2;
        assert_eq!(glyph(&cavalry, &d).as_deref(), Some("formation-crescent"), "PRUS2 is PRUS");
        let mut unknown = with(HudCommand::Reform(7));
        unknown.frame = 40;
        assert_eq!(glyph(&unknown, &d), None);
    }
}
