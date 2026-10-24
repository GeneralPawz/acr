//! Saving the running game and loading a saved one (#78, [`crate::savegame`]).
//!
//! While a game runs, the player's commands and closed mission dialogs go into a
//! [`Log`]. A loaded game starts like a new one; once the world and the scripts are there
//! it fast-forwards to the saved tick, a time budget per frame, without drawing or sound.
//! Then the state hash is compared, view and selection come back and the log continues
//! from the saved one, so the game can be saved again.

use acr_sim::World;
use web_time::{Duration, Instant};

use super::InGame;
use crate::mission::MissionLink;
use crate::savegame::{DipInput, FastForward, Log, Progress, SaveGame, SaveTime, Script};

/// Fast-forward time per frame: the window stays responsive while a long game loads.
const BUDGET: Duration = Duration::from_millis(100);
/// How long a warning stays over the map.
const WARNING_SECONDS: f64 = 12.0;

#[derive(Default)]
pub(super) struct SaveState {
    pub(super) log: Log,
    resume: Option<Resume>,
    /// A message over the map (the state differed after loading) and its remaining time.
    warning: Option<(Vec<u8>, f64)>,
}

struct Resume {
    save: Box<SaveGame>,
    ff: FastForward,
    started: Option<Instant>,
}

impl Script for MissionLink {
    fn before_step(&mut self, world: &mut World) -> Option<Vec<acr_sim::Command>> {
        MissionLink::before_step(self, world)
    }

    fn dialog_open(&self) -> bool {
        self.window_open()
    }

    fn close_dialog(&mut self, answer: u32) {
        self.answer_window(answer);
    }

    fn ended(&self) -> bool {
        self.ended.is_some()
    }

    fn diplomacy(&mut self, world: &mut World, input: &DipInput) {
        match input {
            DipInput::Request(request) => {
                self.dip_request(world, request);
            }
            DipInput::Building(index) => self.dip_check_building(world, *index),
        }
    }
}

impl InGame {
    /// The game of a savegame: starts like a new game, then fast-forwards to the saved tick.
    pub fn from_save(save: Box<SaveGame>) -> Self {
        let mut game = Self::new(Box::new(save.setup.clone()));
        let ff = FastForward::new(save.tick);
        game.save.resume = Some(Resume { save, ff, started: None });
        game
    }

    /// The running game as a savegame; `None` until it runs (or while it is still loading).
    pub fn snapshot(&self, name: &[u8], time: SaveTime) -> Option<SaveGame> {
        let world = self.world.as_ref()?;
        if self.save.resume.is_some() {
            return None;
        }
        Some(SaveGame {
            name: name.to_vec(),
            time,
            setup: (*self.setup).clone(),
            tick: world.tick.0,
            hash: world.state_hash(),
            log: self.save.log.clone(),
            camera: [self.camera.x.round() as i32, self.camera.y.round() as i32],
            selected: self.selected.clone(),
            groups: self.interface.groups.clone(),
        })
    }

    /// Fraction of a loaded game replayed so far, while it is replayed.
    pub fn loading_progress(&self) -> Option<f32> {
        let r = self.save.resume.as_ref()?;
        let tick = self.world.as_ref().map_or(0, |w| w.tick.0);
        Some(if r.ff.target == 0 { 1.0 } else { (tick as f32 / r.ff.target as f32).min(1.0) })
    }

    pub(super) fn loading_save(&self) -> bool {
        self.save.resume.is_some()
    }

    /// Replays a loaded game for one frame's budget; false when no game is being loaded.
    pub(super) fn fast_forward(&mut self, seconds: f32) -> bool {
        if let Some((_, left)) = &mut self.save.warning {
            *left -= f64::from(seconds);
            if *left <= 0.0 {
                self.save.warning = None;
            }
        }
        let Some(r) = &mut self.save.resume else { return false };
        let Some(world) = &mut self.world else { return true };
        let started = *r.started.get_or_insert_with(Instant::now);
        let deadline = Instant::now() + BUDGET;
        let progress = r.ff.run(&r.save.log, world, &mut self.mission, || Instant::now() < deadline);
        self.requests.borrow_mut().extend(self.mission.take_requests());
        if progress != Progress::Done {
            return true;
        }
        let r = self.save.resume.take().expect("resuming");
        let save = r.save;
        let (tick, hash) = (world.tick.0, world.state_hash());
        log::info!("savegame: replayed {tick} ticks in {:.1} s", started.elapsed().as_secs_f32());
        if tick != save.tick || hash != save.hash || r.ff.mismatch {
            log::warn!(
                "savegame: the game differs from the saved one (tick {tick}/{}, hash {hash:016x}/{:016x})",
                save.tick,
                save.hash
            );
            self.save.warning =
                Some((b"Der geladene Spielstand weicht vom gespeicherten ab.".to_vec(), WARNING_SECONDS));
        }
        let alive = |ids: &[acr_sim::world::EntityId]| -> Vec<_> {
            ids.iter().copied().filter(|&id| world.unit(id).is_some_and(|u| u.alive)).collect()
        };
        self.selected = alive(&save.selected);
        for (group, ids) in self.interface.groups.iter_mut().zip(&save.groups) {
            *group = alive(ids);
        }
        self.camera.x = save.camera[0] as f32;
        self.camera.y = save.camera[1] as f32;
        self.centred = true;
        let _ = self.mission.take_camera();
        // Pages DipServer opened on the way are history: the panel starts closed.
        let _ = self.mission.take_diplomacy();
        self.save.log = save.log;
        true
    }

    /// The warning over the map (1024 × 768 canvas).
    pub(super) fn save_overlay(&self, out: &mut Vec<crate::ui::draw::Draw>) {
        if let Some((text, _)) = &self.save.warning {
            out.push(crate::ui::draw::Draw::text(crate::ui::fonts::FontId("fon16y5"), text, 40, 100));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use acr_formats::{M3dMap, Vfs};
    use acr_sim::Command;
    use acr_sim::assets::SimAssets;

    use super::*;
    use crate::ui::{Difficulty, GameKind, GameSetup, MissionSetup};

    /// Answers the link's file requests from the game folder until it asks for no more.
    fn feed(vfs: &mut Vfs, link: &mut MissionLink, mut queue: Vec<String>) {
        while let Some(p) = queue.pop() {
            let data = vfs.read(&p).map_err(|e| e.to_string());
            queue.extend(link.on_loaded(&p, &data).1);
        }
    }

    /// World and mission link of a mission, scripts loaded, as the game starts them.
    fn start(vfs: &mut Vfs, setup: &GameSetup) -> (World, MissionLink) {
        let GameKind::Mission(m) = &setup.kind else { unreachable!() };
        let paths: Vec<String> = vfs.list().map(|(p, _)| p.to_owned()).collect();
        let mut assets = SimAssets::default();
        let mut queue = assets.start(&paths);
        while let Some(p) = queue.pop() {
            let data = vfs.read(&p).ok();
            queue.extend(assets.add_file(&p, data.as_deref()));
        }
        let mut data = assets.build();
        if setup.rules.mortars {
            acr_sim::mortar::append_types(&mut data.catalog);
        }
        let map = M3dMap::parse(&vfs.read(&m.map).unwrap()).unwrap();
        let (mut world, _) = World::from_map(&map, &data.shapes, Arc::new(data.catalog), Arc::new(data.formations));
        world.rules = setup.rules;
        let mut link = MissionLink::new(setup);
        let first = link.start(&paths);
        feed(vfs, &mut link, first);
        for _ in 0..4 {
            let more = link.update(&map, &mut world, 0.0);
            feed(vfs, &mut link, more);
        }
        (world, link)
    }

    /// Plays the first Columbus mission for a while with orders and dialog answers, saves,
    /// loads the save into a fresh game and compares the state. Needs `ACR_GAME_DIR`;
    /// `ACR_SAVE_TICKS` sets the length (1500 ticks by default).
    #[test]
    #[ignore = "needs the original game files (ACR_GAME_DIR)"]
    fn mission_save_loads_to_the_same_state() {
        let dir = std::env::var_os("ACR_GAME_DIR").expect("ACR_GAME_DIR");
        let mut vfs = Vfs::open_game_dir(std::path::Path::new(&dir)).unwrap();
        let setup = GameSetup {
            kind: GameKind::Mission(MissionSetup {
                id: String::new(),
                name: Vec::new(),
                map: "Missions\\COLUMB\\Columb1\\columb1.m3d".into(),
                script: String::new(),
                texts: String::new(),
                difficulty: Difficulty::Normal,
                campaign: None,
            }),
            players: Vec::new(),
            local_player: 0,
            rules: Default::default(),
        };
        let (mut world, mut link) = start(&mut vfs, &setup);
        let mut log = Log::default();
        let mut dialogs = 0;
        let ticks: u64 = std::env::var("ACR_SAVE_TICKS").ok().and_then(|t| t.parse().ok()).unwrap_or(1500);
        let t0 = Instant::now();
        while world.tick.0 < ticks {
            let Some(orders) = link.before_step(&mut world) else {
                assert!(link.window_open(), "the game stands still at tick {}", world.tick.0);
                let answer = dialogs % 2;
                dialogs += 1;
                log.dialog(world.tick.0, answer);
                link.answer_window(answer);
                continue;
            };
            let tick = world.tick.0;
            let own: Vec<_> =
                world.units.iter().filter(|u| u.alive && u.owner == 0 && !u.building).map(|u| u.id).collect();
            let mine = match tick {
                200 | 700 | 3000 | 9000 if !own.is_empty() => {
                    let u = world.unit(own[0]).unwrap();
                    let (x, y) = ((u.pos.x >> 4) + 300, (u.pos.y >> 4) - 200);
                    vec![Command::Move { player: 0, units: own.clone(), x, y }]
                }
                1100 if own.len() > 1 => vec![Command::Stop { player: 0, units: own[1..].to_vec() }],
                _ => Vec::new(),
            };
            log.command(tick, &mine);
            let mut commands = mine;
            commands.extend(orders);
            world.step(&commands);
        }
        assert!(dialogs > 0, "the mission showed no dialog");
        let save = SaveGame {
            name: b"test".to_vec(),
            time: SaveTime::default(),
            setup: setup.clone(),
            tick: world.tick.0,
            hash: world.state_hash(),
            log,
            camera: [0, 0],
            selected: Vec::new(),
            groups: Vec::new(),
        };
        let save = SaveGame::decode(&save.encode()).unwrap();
        let (mut again, mut link) = start(&mut vfs, &save.setup);
        let played = t0.elapsed();
        let t0 = Instant::now();
        let mut ff = FastForward::new(save.tick);
        assert_eq!(ff.run(&save.log, &mut again, &mut link, || true), Progress::Done);
        eprintln!("{ticks} ticks: played in {played:?}, replayed in {:?}", t0.elapsed());
        assert!(!ff.mismatch);
        assert_eq!(again.tick.0, save.tick);
        assert_eq!(again.state_hash(), save.hash);
    }
}
