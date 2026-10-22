//! The diplomacy panel hooked into the game screen: DipServer's dialogs, clicks on tribe
//! buildings (`CheckDipBuilding`) and F8.

use super::InGame;
use crate::diplomacy::DipAction;
use crate::ui::Ctx;
use crate::ui::draw::Draw;
use crate::unit_sounds::SoundEvent;

impl InGame {
    /// Opens the pages DipServer sent and fires due `#time` requests.
    pub(super) fn update_diplomacy(&mut self, seconds: f64) {
        // Developer shortcut: show a page file in the panel at the start (layout checks).
        #[cfg(not(target_arch = "wasm32"))]
        if !self.dip_page_shown && self.mission.has_diplomacy() {
            self.dip_page_shown = true;
            if let Some(text) = std::env::var_os("ACR_DIP_PAGE").and_then(|p| std::fs::read(p).ok()) {
                self.diplomacy.activate(b"LW_file&page", &|_| Some(text.clone()));
            }
        }
        for command in self.mission.take_diplomacy() {
            let mission = &self.mission;
            self.diplomacy.activate(&command, &|p| mission.dip_file(p));
        }
        if let Some(action) = self.diplomacy.update(seconds) {
            self.dip_action(action);
        }
    }

    fn dip_action(&mut self, action: DipAction) {
        let Some(world) = &mut self.world else { return };
        match action {
            DipAction::Request(request) => {
                self.save.log.diplomacy(world.tick.0, crate::savegame::DipInput::Request(request.clone()));
                let answer = self.mission.dip_request(world, &request);
                let mission = &self.mission;
                self.diplomacy.execute(&answer, &|p| mission.dip_file(p));
                // A request may also open a page (`ActivateDipDialog`).
                for command in self.mission.take_diplomacy() {
                    let mission = &self.mission;
                    self.diplomacy.activate(&command, &|p| mission.dip_file(p));
                }
            }
        }
    }

    /// Left button pressed at canvas point `p`: true if the panel took it.
    pub(super) fn dip_press(&mut self, ctx: &Ctx, p: [f32; 2]) -> bool {
        if !self.diplomacy.covers(p) {
            return false;
        }
        let (action, sounds) = self.diplomacy.press(ctx, p);
        // A link click plays `DialogsSystem::ClickSound`, the `STANDART` group.
        let click = sounds.iter().any(|s| s == crate::ui::dialog::sound::CLICK);
        if click {
            self.sound_events.push(SoundEvent { name: "STANDART".into(), pan: 0.0, volume: 1.0 });
        }
        if let Some(action) = action {
            self.dip_action(action);
        }
        true
    }

    pub(super) fn dip_release(&mut self) {
        self.diplomacy.release();
    }

    /// A press of the left button on the map (0x4c446c): `CheckDipBuilding(MyNation, index)`
    /// with the building under the cursor; DipServer opens the dialog of an allied tribe whose
    /// building it is.
    pub(super) fn dip_check_building(&mut self, at: [f32; 2]) {
        if !self.mission.has_diplomacy() {
            return;
        }
        let Some(building) = self.unit_at(at, |u| u.building) else { return };
        let Some(world) = &mut self.world else { return };
        self.save.log.diplomacy(world.tick.0, crate::savegame::DipInput::Building(building.0 as u16));
        self.mission.dip_check_building(world, building.0 as u16);
    }

    /// F8 (key 0x77 → 0x5c3a60) opens or closes the panel.
    pub(super) fn dip_toggle(&mut self) {
        if !self.mission.has_diplomacy() {
            return;
        }
        if let Some(action) = self.diplomacy.toggle() {
            self.dip_action(action);
        }
    }

    pub(super) fn dip_draw(&mut self, ctx: &Ctx, out: &mut Vec<Draw>) {
        self.diplomacy.draw(ctx, out);
    }
}
