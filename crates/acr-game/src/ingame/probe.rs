//! What the test driver (`crate::drive`) reads from a running game and the shortcuts it
//! takes: HUD icons in window pixels, units on the screen, the selection, commands.

use acr_sim::world::EntityId;
use acr_sim::{Command, Unit, World};

use super::InGame;
use super::interface::tick_rate;
use crate::hud::Hud;

/// Slots of the icon grid (12 × 3); higher slots are not drawn in the grid.
const GRID_SLOTS: usize = 36;

/// An icon of the HUD's grid as the driver reports it.
pub struct HudIcon {
    /// `hud/<slot>` for the grid, `army/<slot>` for the army icons at the top left.
    pub id: String,
    /// Window pixels: x, y, width, height.
    pub rect: [f32; 4],
    pub hint: Vec<u8>,
    pub disabled: bool,
    pub selected: bool,
    /// Drawn gray: the player cannot afford it.
    pub gray: bool,
    pub count: Option<i32>,
    /// The number at the bottom right (a formation's number).
    pub number: Option<i32>,
    /// A left click does something.
    pub clickable: bool,
    /// A right click does something (cancelling production).
    pub right_clickable: bool,
}

impl InGame {
    pub fn world(&self) -> Option<&World> {
        self.world.as_ref()
    }

    /// The world to change directly: the close-up bench's units and scripted hits.
    pub fn world_mut(&mut self) -> Option<&mut World> {
        self.world.as_mut()
    }

    /// The driver's camera (bench, #277): the point `lift` true pixels above the ground at
    /// world pixel (`x`, `y`) in the middle of the window, at `zoom` (output pixels per
    /// original pixel, beyond the wheel's range if asked); with an `orbit` the free camera,
    /// turned and tilted so. The view is not kept inside the map.
    pub fn set_camera(
        &mut self,
        x: i32,
        y: i32,
        lift: f32,
        zoom: Option<f32>,
        orbit: Option<acr_render::view3d::Orbit>,
    ) {
        use acr_render::view3d::COS_E;
        if let Some(z) = zoom {
            self.camera.scale = z;
            self.interface.modern.zoom = z;
        }
        if let Some(o) = orbit {
            self.three_d.set_free(o);
        }
        let window = self.interface.window;
        let s = self.camera.scale;
        let h = self.terrain.as_ref().map_or(0, |t| t.height_at(x, y));
        // The point to show: on the model's ground (`view3d::ground_point`), lifted.
        let p = [x as f32, ((y >> 1) * 2) as f32, h as f32 / COS_E + lift];
        let target = match self.three_d.free_view(self.camera, window) {
            // The free camera turns about a point at z = 0: the one behind `p` on the view ray.
            Some(view) => {
                let f = view.basis().forward;
                let t = p[2] / f[2];
                [p[0] - t * f[0], p[1] - t * f[1]]
            }
            // The fixed camera draws height as a shift up the screen.
            None => {
                let up = lift * COS_E;
                [p[0], p[1] - 2.0 * (h as f32 + up)]
            }
        };
        self.camera.x = target[0] - window[0] / s / 2.0;
        self.camera.y = target[1] / 2.0 - window[1] / s / 2.0;
        self.centred = true;
    }

    /// The current camera: scale and, with the free camera, yaw and pitch.
    pub fn camera_scale(&self) -> f32 {
        self.camera.scale
    }

    pub fn player(&self) -> u8 {
        self.local_player()
    }

    pub fn selection(&self) -> &[EntityId] {
        &self.selected
    }

    /// Replaces the selection with the given own, living units.
    pub fn set_selection(&mut self, ids: Vec<EntityId>) {
        let own = self.local_player();
        let world = self.world.as_ref();
        self.selected = ids
            .into_iter()
            .filter(|&id| world.and_then(|w| w.unit(id)).is_some_and(|u| u.alive && u.owner == own))
            .collect();
    }

    /// Queues a command for the next tick, as the interface does.
    pub fn push_command(&mut self, command: Command) {
        self.commands.push(command);
    }

    /// Seconds of one simulation tick at the current game speed.
    pub fn tick_seconds(&self) -> f64 {
        1.0 / f64::from(tick_rate(self.frame_time))
    }

    /// The icon grid of the last drawn interface.
    pub fn hud_icons(&self) -> Vec<HudIcon> {
        let screen = self.interface.hud.screen();
        let canvas = self.interface.canvas;
        self.interface
            .view
            .icons
            .iter()
            .filter(|i| i.slot < GRID_SLOTS)
            .map(|i| {
                let r = Hud::icon_rect(screen, i.slot);
                HudIcon {
                    id: format!("hud/{}", i.slot),
                    rect: canvas.to_window([r.x0 as f32, r.y0 as f32, r.width() as f32, r.height() as f32]),
                    hint: i.hint.clone(),
                    disabled: i.disabled,
                    selected: i.selected,
                    gray: i.gray,
                    count: i.count,
                    number: i.number,
                    clickable: i.action.is_some() && !i.disabled,
                    right_clickable: i.right.is_some() && !i.disabled,
                }
            })
            .chain(Hud::army_icons(&self.interface.view).map(|i| {
                let r = Hud::army_rect(i.slot);
                // The army icons have no hints; the driver names them `army` (the list
                // button) and `army <number>`.
                let label = match i.number {
                    Some(n) => format!("army {n}"),
                    None => "army".to_owned(),
                };
                HudIcon {
                    id: format!("army/{}", i.slot),
                    rect: canvas.to_window([r.x0 as f32, r.y0 as f32, r.width() as f32, r.height() as f32]),
                    hint: label.into_bytes(),
                    disabled: i.disabled,
                    selected: i.selected,
                    gray: i.gray,
                    count: i.count,
                    number: i.number,
                    clickable: i.action.is_some() && !i.disabled,
                    right_clickable: false,
                }
            }))
            .collect()
    }

    /// What a map click does now: `placing <type>`, `rally`, `patrol`, `attack_ground` or
    /// `normal`.
    pub fn cursor_mode(&self) -> String {
        if let (Some(kind), Some(world)) = (self.interface.placing, &self.world) {
            return format!("placing {}", world.catalog.get(kind).id);
        }
        if self.interface.rally_mode {
            return "rally".into();
        }
        if self.interface.patrol_mode {
            return "patrol".into();
        }
        if self.interface.attack_ground_mode {
            return "attack_ground".into();
        }
        "normal".into()
    }

    /// The window size the interface was last laid out for.
    pub fn window_size_px(&self) -> [f32; 2] {
        self.interface.window
    }

    /// A unit's selection box in window pixels: x, y, width, height.
    pub fn unit_window_box(&self, u: &Unit) -> [f32; 4] {
        let (x, y, w, h) = self.unit_box(u);
        let s = self.camera.scale;
        [(x as f32 - self.camera.x) * s, (y as f32 - self.camera.y) * s, w as f32 * s, h as f32 * s]
    }

    /// The window position of a world pixel on the ground.
    pub fn world_to_window(&self, x: i32, y: i32) -> [f32; 2] {
        let h = self.terrain.as_ref().map_or(0, |t| t.height_at(x, y));
        self.camera.project(x, y, h)
    }

    /// Centres the view on a world pixel.
    pub fn centre_on(&mut self, x: i32, y: i32) {
        let window = self.interface.window;
        let s = self.camera.scale;
        self.camera.x = x as f32 - window[0] / s / 2.0;
        self.camera.y = (y / 2) as f32 - window[1] / s / 2.0;
        self.centred = true;
        self.clamp_camera(window);
    }

    /// Whether the player sees a world pixel through the fog (always without fog).
    pub fn sees(&self, x: i32, y: i32) -> bool {
        match (self.mission.fog(), &self.world) {
            (Some(fog), Some(world)) => fog.visible(world, x, y),
            _ => true,
        }
    }

    /// The mission has objectives (F1 shows them).
    pub fn has_objectives(&self) -> bool {
        self.mission.objectives().is_some()
    }
}
