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
            TouchIcon {
                id,
                role,
                size: [ICON, ICON],
                draws,
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
