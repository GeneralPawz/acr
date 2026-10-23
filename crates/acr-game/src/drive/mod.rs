//! The test driver: `AmericanConquest --drive` reads JSON commands from stdin, one per
//! line, and answers each with one JSON line on stdout. The protocol is documented in
//! `docs/testing.md`; `cargo xtask e2e` runs scenario files through it.
//!
//! The game only advances while a command needs it, in fixed steps as fast as it can: one
//! simulation tick per frame in a game, one menu tick (1/60 s) otherwise, with no waiting
//! for real time or the screen. Without `--drive-gpu` there is no window and no GPU at all:
//! menus, interface and simulation run, nothing is drawn. With it a hidden window holds the
//! GPU, and `screenshot` draws a frame offscreen.
//!
//! Input goes through the same handlers as the window's events, so a `click` on a label
//! is a real mouse move, press and release at the widget's centre. After every input the
//! driver *settles*: it keeps stepping until requested files are loaded, the top screen is
//! built and idle and a started game is shown. Modal mission messages are pressed away
//! (`dialogs auto`, the default; each is reported) or left open and reported as `blocking`.

mod bench;
mod logger;
mod protocol;
mod shot;
mod state;
mod tree;

pub use logger::install as install_logger;
pub use protocol::DialogPolicy;

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

use crate::ui::setup::{Control, PlayerSetup, RandomMapSetup};
use crate::{Game, Mode, ui};
use protocol::{Button, Mods, Request, Start, Target, Wait};

/// Version of the line protocol, reported in the `ready` line.
pub const PROTOCOL: u32 = 1;
/// Settling gives up after this many frames or this much time (and says so).
const SETTLE_FRAMES: u64 = 30_000;
const SETTLE_TIME: Duration = Duration::from_secs(180);
/// Frames run per pass of the event loop; between passes loaded files arrive.
const BURST_FRAMES: u32 = 64;
const BURST_TIME: Duration = Duration::from_millis(25);

/// How the driver was asked to run.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// A hidden window with a GPU (screenshots), else no window and no GPU.
    pub gpu: bool,
    /// The window size the game lays out for.
    pub size: [u32; 2],
    pub dialogs: DialogPolicy,
}

impl Options {
    /// `--drive` or `ACR_DRIVE=1`; `--drive-gpu` or `ACR_DRIVE_GPU=1` with a GPU;
    /// `--size WxH` (default 1024x768); `--dialogs auto|report`.
    pub fn from_args(args: &[String]) -> Option<Self> {
        let env = |name: &str| std::env::var(name).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        let gpu = args.iter().any(|a| a == "--drive-gpu") || env("ACR_DRIVE_GPU");
        if !(gpu || args.iter().any(|a| a == "--drive") || env("ACR_DRIVE")) {
            return None;
        }
        let value = |name: &str| args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone());
        let size = value("--size")
            .and_then(|s| {
                let (w, h) = s.split_once('x')?;
                Some([w.parse().ok()?, h.parse().ok()?])
            })
            .unwrap_or([1024, 768]);
        let dialogs = value("--dialogs").as_deref().and_then(DialogPolicy::parse).unwrap_or(DialogPolicy::Auto);
        Some(Self { gpu, size, dialogs })
    }
}

/// The command line without the driver's flags (the game folder may follow them).
pub fn strip_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--drive" | "--drive-gpu" => {}
            "--size" | "--dialogs" => {
                it.next();
            }
            _ => out.push(a.clone()),
        }
    }
    out
}

/// One input or a frame of a command's script.
#[derive(Debug, Clone)]
enum Step {
    Move([f32; 2]),
    Button(MouseButton, ElementState),
    Key(Key, Option<String>, ElementState),
    Mods(ModifiersState),
    Wheel(f32),
    /// Run one frame before the next step.
    Frame,
}

/// What a resolved pointer target is used for.
#[derive(Debug, Clone)]
enum Pointer {
    Click { button: Button, double: bool, mods: Mods },
    Move,
}

/// `record`: screenshots every `every` frames.
struct Recording {
    dir: String,
    prefix: String,
    count: u32,
    every: u64,
    crop: Option<[u32; 4]>,
    primed: bool,
    /// The job frame of the next picture.
    next: u64,
    paths: Vec<String>,
}

/// The command being carried out.
struct Job {
    seq: Option<Value>,
    started: Instant,
    frames: u64,
    /// A click waiting for its target to appear, with its timeout in frames.
    resolve: Option<(Target, Pointer, u64)>,
    steps: VecDeque<Step>,
    wait: Option<(Wait, Option<u64>)>,
    settle: bool,
    settle_frames: u64,
    /// A screenshot to take once drawing has asked for its files: path, crop, primed.
    shot: Option<(String, Option<[u32; 4]>, bool)>,
    /// A recording in progress (`record`).
    record: Option<Recording>,
    reply: Map<String, Value>,
    /// The first answer: the `ready` line.
    hello: bool,
}

impl Job {
    fn new(seq: Option<Value>) -> Self {
        Self {
            seq,
            started: Instant::now(),
            frames: 0,
            resolve: None,
            steps: VecDeque::new(),
            wait: None,
            settle: false,
            settle_frames: 0,
            shot: None,
            record: None,
            reply: Map::new(),
            hello: false,
        }
    }
}

pub struct Driver {
    options: Options,
    lines: Receiver<String>,
    job: Option<Job>,
    /// Frames run since the start.
    frames: u64,
    policy: DialogPolicy,
    /// Messages pressed away since the last answer.
    dismissed: Vec<Value>,
    quit: bool,
}

impl Driver {
    fn new(options: Options) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                if !line.trim().is_empty() && tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut hello = Job::new(None);
        hello.settle = true;
        hello.hello = true;
        Self {
            policy: options.dialogs,
            options,
            lines: rx,
            job: Some(hello),
            frames: 0,
            dismissed: Vec::new(),
            quit: false,
        }
    }

    /// The hidden window of the GPU mode at the driver's size.
    pub fn window_attributes(&self, attributes: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
        let [w, h] = self.options.size;
        attributes
            .with_title("American Conquest (driver)")
            .with_visible(false)
            .with_inner_size(winit::dpi::PhysicalSize::new(w, h))
    }
}

fn send(value: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

fn modifiers(m: Mods) -> ModifiersState {
    let mut s = ModifiersState::empty();
    s.set(ModifiersState::CONTROL, m.ctrl);
    s.set(ModifiersState::SHIFT, m.shift);
    s.set(ModifiersState::ALT, m.alt);
    s
}

fn mouse_button(b: Button) -> MouseButton {
    match b {
        Button::Left => MouseButton::Left,
        Button::Right => MouseButton::Right,
        Button::Middle => MouseButton::Middle,
    }
}

/// A key name (`Enter`, `F12`, `a`, ...) as the window would report it, with its text.
fn parse_key(name: &str) -> Result<(Key, Option<String>), String> {
    let named = match name.to_ascii_lowercase().as_str() {
        "enter" | "return" => NamedKey::Enter,
        "escape" | "esc" => NamedKey::Escape,
        "space" => return Ok((Key::Named(NamedKey::Space), Some(" ".into()))),
        "tab" => NamedKey::Tab,
        "pause" => NamedKey::Pause,
        "backspace" => NamedKey::Backspace,
        "delete" | "del" => NamedKey::Delete,
        "insert" => NamedKey::Insert,
        "up" | "arrowup" => NamedKey::ArrowUp,
        "down" | "arrowdown" => NamedKey::ArrowDown,
        "left" | "arrowleft" => NamedKey::ArrowLeft,
        "right" | "arrowright" => NamedKey::ArrowRight,
        "pageup" => NamedKey::PageUp,
        "pagedown" => NamedKey::PageDown,
        "home" => NamedKey::Home,
        "end" => NamedKey::End,
        "f1" => NamedKey::F1,
        "f2" => NamedKey::F2,
        "f3" => NamedKey::F3,
        "f4" => NamedKey::F4,
        "f5" => NamedKey::F5,
        "f6" => NamedKey::F6,
        "f7" => NamedKey::F7,
        "f8" => NamedKey::F8,
        "f9" => NamedKey::F9,
        "f10" => NamedKey::F10,
        "f11" => NamedKey::F11,
        "f12" => NamedKey::F12,
        _ => {
            let mut chars = name.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) => Ok((Key::Character(SmolStr::new(c.to_string())), Some(c.to_string()))),
                _ => Err(format!("unknown key {name:?}")),
            };
        }
    };
    Ok((Key::Named(named), None))
}

fn click_steps(p: [f32; 2], button: Button, double: bool, mods: Mods) -> VecDeque<Step> {
    let b = mouse_button(button);
    let mut steps = VecDeque::from([
        Step::Mods(modifiers(mods)),
        Step::Move(p),
        Step::Frame,
        Step::Button(b, ElementState::Pressed),
        Step::Frame,
        Step::Button(b, ElementState::Released),
    ]);
    if double {
        steps.extend([
            Step::Frame,
            Step::Button(b, ElementState::Pressed),
            Step::Frame,
            Step::Button(b, ElementState::Released),
        ]);
    }
    steps.extend([Step::Mods(ModifiersState::empty()), Step::Frame]);
    steps
}

/// Press at `a`, move to `b` in four steps, release.
fn drag_steps(a: [f32; 2], b: [f32; 2], button: Button, mods: Mods) -> VecDeque<Step> {
    let mb = mouse_button(button);
    let mut steps = VecDeque::from([Step::Mods(modifiers(mods)), Step::Move(a), Step::Frame]);
    steps.extend([Step::Button(mb, ElementState::Pressed), Step::Frame]);
    for k in 1..=4 {
        let t = k as f32 / 4.0;
        steps.extend([Step::Move([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]), Step::Frame]);
    }
    steps.extend([Step::Button(mb, ElementState::Released), Step::Mods(ModifiersState::empty()), Step::Frame]);
    steps
}

impl Game {
    /// Runs the game under the test driver (see the module docs).
    pub fn set_driver(&mut self, options: Options) {
        if !options.gpu {
            self.headless = Some(options.size.map(|v| v as f32));
        }
        // No movies.
        self.mode = Mode::Menus;
        self.driver = Some(Box::new(Driver::new(options)));
    }

    /// One pass of the event loop under the driver.
    pub(crate) fn drive(&mut self, event_loop: &ActiveEventLoop) {
        let Some(mut driver) = self.driver.take() else { return };
        driver.run(self, event_loop);
        let quit = driver.quit;
        self.driver = Some(driver);
        if quit {
            self.audio.stop();
            event_loop.exit();
        }
    }
}

impl Driver {
    fn run(&mut self, game: &mut Game, event_loop: &ActiveEventLoop) {
        let start = Instant::now();
        let mut frames = 0;
        loop {
            if self.job.is_none() {
                match self.lines.try_recv() {
                    Ok(line) => self.begin(game, event_loop, &line),
                    Err(TryRecvError::Empty) => {
                        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(2)));
                        return;
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.quit = true;
                        return;
                    }
                }
                if self.quit {
                    return;
                }
                continue;
            }
            event_loop.set_control_flow(ControlFlow::Poll);
            let ran = self.advance_job(game, event_loop);
            if ran {
                frames += 1;
            }
            // Requested files arrive between passes.
            if game.in_flight.get() > 0 || frames >= BURST_FRAMES || start.elapsed() > BURST_TIME {
                return;
            }
        }
    }

    /// One frame of the game at the fixed step.
    fn frame(&mut self, game: &mut Game, event_loop: &ActiveEventLoop) {
        let dt = match &game.mode {
            Mode::InGame(g) if g.ready() && !game.ui.dialog_open() => g.tick_seconds(),
            _ => 1.0 / ui::TICK_RATE,
        };
        game.advance(dt, event_loop);
        self.frames += 1;
        if let Some(job) = &mut self.job {
            job.frames += 1;
        }
        if self.policy == DialogPolicy::Auto {
            self.dismiss(game, event_loop);
        }
    }

    /// Presses the default button of an open modal message.
    fn dismiss(&mut self, game: &mut Game, event_loop: &ActiveEventLoop) {
        let Some(modal) = tree::modal(game) else { return };
        let Some(button) = modal.default.clone() else { return };
        let mut entry = modal.to_json();
        let m = entry.as_object_mut().unwrap();
        m.insert("button".into(), button.label.clone().into());
        m.insert("tick".into(), state::tick(game).into());
        log::info!("driver: pressed {:?} of {} {:?}", button.label, modal.screen, modal.texts.join(" / "));
        self.dismissed.push(entry);
        let [x, y] = button.centre();
        game.cursor_moved(PhysicalPosition::new(f64::from(x), f64::from(y)), event_loop);
        game.mouse_input(ElementState::Pressed, MouseButton::Left, event_loop);
        game.mouse_input(ElementState::Released, MouseButton::Left, event_loop);
    }

    fn settled(&self, game: &Game) -> bool {
        let screen_ready = tree::top(game).is_none_or(|(s, built)| built && !s.busy());
        let game_ready = match &game.mode {
            Mode::Intro => false,
            Mode::Menus => true,
            Mode::InGame(g) => g.ready() || g.failure().is_some(),
        };
        let open_modal = self.policy == DialogPolicy::Auto && tree::modal(game).is_some();
        game.in_flight.get() == 0 && screen_ready && game_ready && !open_modal
    }

    fn apply(&self, game: &mut Game, event_loop: &ActiveEventLoop, step: Step) {
        match step {
            Step::Move([x, y]) => game.cursor_moved(PhysicalPosition::new(f64::from(x), f64::from(y)), event_loop),
            Step::Button(b, s) => game.mouse_input(s, b, event_loop),
            Step::Key(k, text, s) => game.keyboard_input(k, text.as_deref(), s, event_loop),
            Step::Mods(m) => game.modifiers_changed(m),
            Step::Wheel(l) => game.mouse_wheel(l, event_loop),
            Step::Frame => {}
        }
    }

    /// Carries the job one frame further; returns whether a frame ran.
    fn advance_job(&mut self, game: &mut Game, event_loop: &ActiveEventLoop) -> bool {
        let Some(mut job) = self.job.take() else { return false };
        let result = self.work(game, event_loop, &mut job);
        match result {
            Ok(None) => {
                self.job = Some(job);
                self.frame(game, event_loop);
                true
            }
            Ok(Some(())) => {
                self.finish(game, job, None);
                false
            }
            Err(e) => {
                self.finish(game, job, Some(e));
                false
            }
        }
    }

    /// The next part of a job: `Ok(None)` needs another frame, `Ok(Some(()))` is done.
    fn work(&mut self, game: &mut Game, event_loop: &ActiveEventLoop, job: &mut Job) -> Result<Option<()>, String> {
        if let Some((target, pointer, timeout)) = job.resolve.take() {
            if !self.settled(game) && job.frames < timeout {
                job.resolve = Some((target, pointer, timeout));
                return Ok(None);
            }
            match tree::resolve(game, &target) {
                Ok((p, hit)) => {
                    job.reply.insert("target".into(), hit);
                    job.steps = match pointer {
                        Pointer::Click { button, double, mods } => click_steps(p, button, double, mods),
                        Pointer::Move => VecDeque::from([Step::Move(p), Step::Frame]),
                    };
                }
                Err(_) if job.frames < timeout => {
                    job.resolve = Some((target, pointer, timeout));
                    return Ok(None);
                }
                Err(e) => {
                    let labels: Vec<String> =
                        tree::items(game, false).into_iter().filter(|i| i.clickable).map(|i| i.label).collect();
                    job.reply.insert("clickable".into(), labels.into());
                    return Err(e);
                }
            }
        }
        while let Some(step) = job.steps.pop_front() {
            if matches!(step, Step::Frame) {
                return Ok(None);
            }
            self.apply(game, event_loop, step);
        }
        if let Some((wait, until_tick)) = &job.wait {
            let frames_done = wait.frames.is_none_or(|f| job.frames >= f);
            let ticks_done = until_tick.is_none_or(|t| state::tick(game).is_some_and(|now| now >= t));
            let conds_done = wait.until.iter().all(|c| state::check(game, c));
            if frames_done && ticks_done && conds_done {
                job.wait = None;
            } else if job.frames >= wait.timeout {
                let failed: Vec<String> =
                    wait.until.iter().filter(|c| !state::check(game, c)).map(|c| format!("{c:?}")).collect();
                job.reply.insert("unmet".into(), failed.into());
                return Err(format!("timeout after {} frames", job.frames));
            } else {
                return Ok(None);
            }
        }
        if job.settle {
            if self.settled(game) && (job.settle_frames > 0 || job.hello) {
                job.settle = false;
            } else if job.settle_frames >= SETTLE_FRAMES || job.started.elapsed() > SETTLE_TIME {
                job.settle = false;
                job.reply.insert("settled".into(), false.into());
            } else {
                job.settle_frames += 1;
                return Ok(None);
            }
        }
        if let Some((path, crop, primed)) = job.shot.take() {
            if !primed {
                // The first draw asks for the sprites and fonts it lacks; they load while
                // the driver settles, then the real picture is taken.
                shot::prime(game)?;
                job.shot = Some((path, crop, true));
                job.settle = true;
                job.settle_frames = 0;
                return Ok(None);
            }
            let size = shot::screenshot(game, &path, crop)?;
            job.reply.insert("path".into(), path.into());
            job.reply.insert("size".into(), json!(size));
        }
        if let Some(mut rec) = job.record.take() {
            if !rec.primed {
                shot::prime(game)?;
                rec.primed = true;
                rec.next = job.frames;
                job.record = Some(rec);
                job.settle = true;
                job.settle_frames = 0;
                return Ok(None);
            }
            if job.frames >= rec.next {
                let path =
                    format!("{}/{}{:03}.png", rec.dir.trim_end_matches(['/', '\\']), rec.prefix, rec.paths.len());
                let size = shot::screenshot(game, &path, rec.crop)?;
                job.reply.insert("size".into(), json!(size));
                rec.paths.push(path);
                rec.next = job.frames + rec.every;
            }
            if rec.paths.len() < rec.count as usize {
                job.record = Some(rec);
                return Ok(None);
            }
            job.reply.insert("paths".into(), rec.paths.into());
        }
        Ok(Some(()))
    }

    fn finish(&mut self, game: &Game, job: Job, error: Option<String>) {
        let mut reply = job.reply;
        if job.hello {
            reply.insert("event".into(), "ready".into());
            reply.insert("protocol".into(), PROTOCOL.into());
            reply.insert("gpu".into(), self.options.gpu.into());
            reply.insert("size".into(), json!(self.options.size));
        }
        if let Some(seq) = job.seq {
            reply.insert("seq".into(), seq);
        }
        reply.insert("ok".into(), error.is_none().into());
        if let Some(e) = error {
            reply.insert("error".into(), e.into());
        }
        reply.insert("screen".into(), tree::screen_name(game).into());
        reply.insert("tick".into(), state::tick(game).into());
        reply.insert("frame".into(), self.frames.into());
        reply.insert("frames".into(), job.frames.into());
        reply.insert("ms".into(), (job.started.elapsed().as_millis() as u64).into());
        if !self.dismissed.is_empty() {
            reply.insert("dismissed".into(), std::mem::take(&mut self.dismissed).into());
        }
        if self.policy == DialogPolicy::Report
            && let Some(m) = tree::modal(game)
        {
            reply.insert("blocking".into(), m.to_json());
        }
        let log = logger::take();
        if !log.is_empty() {
            reply.insert("log".into(), log.into());
        }
        send(&Value::Object(reply));
    }

    /// Starts the command of one line; quick ones are answered right away.
    fn begin(&mut self, game: &mut Game, event_loop: &ActiveEventLoop, line: &str) {
        let (seq, request) = match protocol::parse(line) {
            Ok(r) => r,
            Err(e) => {
                let seq = serde_json::from_str::<Value>(line).ok().and_then(|v| v.get("seq").cloned());
                return self.finish(game, Job::new(seq), Some(e));
            }
        };
        let mut job = Job::new(seq);
        match self.prepare(game, event_loop, request, &mut job) {
            Ok(true) => self.job = Some(job),
            Ok(false) => self.finish(game, job, None),
            Err(e) => self.finish(game, job, Some(e)),
        }
    }

    /// Sets the job up; `Ok(true)` if it needs frames.
    fn prepare(
        &mut self,
        game: &mut Game,
        event_loop: &ActiveEventLoop,
        request: Request,
        job: &mut Job,
    ) -> Result<bool, String> {
        match request {
            Request::Ui { all } => {
                job.reply.insert("ui".into(), tree::describe(game, all));
                Ok(false)
            }
            Request::State { units, owner, visible_only, detail, kind } => {
                let listed = units || detail || kind.is_some();
                let st = state::state(game, listed, owner, visible_only, detail, kind.as_deref());
                job.reply.insert("state".into(), st);
                Ok(false)
            }
            Request::Click { target, button, double, mods, timeout } => {
                job.resolve = Some((target, Pointer::Click { button, double, mods }, timeout));
                job.settle = true;
                Ok(true)
            }
            Request::Move { target } => {
                job.resolve = Some((target, Pointer::Move, protocol::CLICK_TIMEOUT));
                Ok(true)
            }
            Request::Drag { from, to, button, mods } => {
                let (a, hit_a) = tree::resolve(game, &from)?;
                let (b, hit_b) = tree::resolve(game, &to)?;
                job.reply.insert("from".into(), hit_a);
                job.reply.insert("to".into(), hit_b);
                job.steps = drag_steps(a, b, button, mods);
                job.settle = true;
                Ok(true)
            }
            Request::DragAround { filter, max, mods } => {
                let (a, b, n) = tree::around(game, &filter, max)?;
                job.reply.insert("around".into(), n.into());
                job.reply.insert("rect".into(), json!([a, b]));
                job.steps = drag_steps(a, b, Button::Left, mods);
                job.settle = true;
                Ok(true)
            }
            Request::Key { key, mods } => {
                let (k, text) = parse_key(&key)?;
                job.steps.extend([
                    Step::Mods(modifiers(mods)),
                    Step::Key(k.clone(), text.clone(), ElementState::Pressed),
                    Step::Frame,
                    Step::Key(k, text, ElementState::Released),
                    Step::Mods(ModifiersState::empty()),
                    Step::Frame,
                ]);
                job.settle = true;
                Ok(true)
            }
            Request::Text { text } => {
                for c in text.chars() {
                    let s = c.to_string();
                    let k = Key::Character(SmolStr::new(&s));
                    job.steps.extend([
                        Step::Key(k.clone(), Some(s.clone()), ElementState::Pressed),
                        Step::Key(k, Some(s), ElementState::Released),
                        Step::Frame,
                    ]);
                }
                job.settle = true;
                Ok(true)
            }
            Request::Wheel { lines } => {
                job.steps.extend([Step::Wheel(lines), Step::Frame]);
                job.settle = true;
                Ok(true)
            }
            Request::Wait(wait) => {
                let until_tick = match wait.ticks {
                    Some(n) => Some(state::tick(game).ok_or("wait ticks: no game running")? + n),
                    None => None,
                };
                job.wait = Some((wait, until_tick));
                Ok(true)
            }
            Request::Command(v) => {
                let commands = state::command(game, &v)?;
                let n = commands.len();
                if let Mode::InGame(g) = &mut game.mode {
                    for c in commands {
                        g.push_command(c);
                    }
                }
                job.reply.insert("queued".into(), n.into());
                Ok(false)
            }
            Request::Select(ids) => {
                let Mode::InGame(g) = &mut game.mode else { return Err("no game".into()) };
                g.set_selection(ids.into_iter().map(acr_sim::world::EntityId).collect());
                job.reply.insert("selected".into(), g.selection().len().into());
                Ok(false)
            }
            Request::Screenshot { path, crop } => {
                if game.graphics.is_none() {
                    return Err("no GPU: start the driver with --drive-gpu for screenshots".into());
                }
                job.shot = Some((path, crop, false));
                Ok(true)
            }
            Request::Start(start) => {
                match start {
                    Start::Map(path) => game.start_map(&path),
                    Start::Blank { setup, players, nation, hit_locations, body_spacing, hud } => {
                        // Human players only: no AI gives orders, no start units come.
                        let players = (0..players)
                            .map(|i| PlayerSetup {
                                name: if i == 0 { b"Driver".to_vec() } else { b"Other".to_vec() },
                                nation: (nation + i) % 6,
                                color: i as u8,
                                team: 0,
                                control: Control::Human,
                            })
                            .collect();
                        let side = crate::blank_map::playable(setup.size);
                        job.reply.insert("centre".into(), json!([side / 2, side / 2]));
                        job.reply.insert("playable".into(), json!(side));
                        game.ui.show_loading();
                        game.start_game(Box::new(ui::GameSetup {
                            kind: ui::GameKind::Blank(setup),
                            players,
                            local_player: 0,
                            rules: acr_sim::Rules { hit_locations, body_spacing },
                        }));
                        if let Mode::InGame(g) = &mut game.mode {
                            g.hud_hidden = !hud;
                        }
                    }
                    Start::Random {
                        nation,
                        opponents,
                        seed,
                        resources,
                        size,
                        difficulty,
                        hit_locations,
                        body_spacing,
                    } => {
                        let mut players = vec![PlayerSetup {
                            name: b"Driver".to_vec(),
                            nation,
                            color: 0,
                            team: 0,
                            control: Control::Human,
                        }];
                        for i in 0..opponents.min(6) {
                            players.push(PlayerSetup {
                                name: Vec::new(),
                                nation: (nation + 1 + i) % 6,
                                color: i as u8 + 1,
                                team: 0,
                                control: Control::Computer(ui::Difficulty::from_index(difficulty)),
                            });
                        }
                        let map = RandomMapSetup {
                            seed,
                            resources,
                            size: match size {
                                1 => ui::setup::MapSize::Large,
                                2 => ui::setup::MapSize::Huge,
                                _ => ui::setup::MapSize::Normal,
                            },
                            climate: 1,
                            ..Default::default()
                        };
                        game.ui.show_loading();
                        game.start_game(Box::new(ui::GameSetup {
                            kind: ui::GameKind::RandomMap(map),
                            players,
                            local_player: 0,
                            rules: acr_sim::Rules { hit_locations, body_spacing },
                        }));
                    }
                }
                job.settle = true;
                Ok(true)
            }
            Request::Save { name } => {
                if !matches!(game.mode, Mode::InGame(_)) {
                    return Err("no game to save".into());
                }
                let bytes = acr_formats::CodePage::Cp1252.encode(&name).ok_or("name not in CP1252")?;
                game.save_game(&bytes);
                let file = ui::screens::save_load::file_name(&bytes);
                job.reply.insert("file".into(), file.into());
                Ok(false)
            }
            Request::Load { name } => {
                let bytes = acr_formats::CodePage::Cp1252.encode(&name).ok_or("name not in CP1252")?;
                let file = ui::screens::save_load::file_name(&bytes);
                let data = game.ui.saves_mut().read(&file).ok_or(format!("no savegame {file}"))?;
                let save = crate::savegame::SaveGame::decode(&data).map_err(|e| format!("{file}: {e:?}"))?;
                game.load_game(Box::new(save));
                job.settle = true;
                Ok(true)
            }
            Request::Dialogs(policy) => {
                self.policy = policy;
                job.reply.insert("policy".into(), format!("{policy:?}").to_lowercase().into());
                // Pressing away what is open now needs a frame.
                Ok(policy == DialogPolicy::Auto && tree::modal(game).is_some())
            }
            Request::Camera { x, y, zoom, lift, orbit } => {
                let Mode::InGame(g) = &mut game.mode else { return Err("no game".into()) };
                let orbit = orbit.map(|[yaw, pitch]| acr_render::view3d::Orbit { yaw, pitch });
                g.set_camera(x, y, lift, zoom, orbit);
                job.reply.insert("camera".into(), json!({"world": [x, y], "zoom": g.camera_scale()}));
                // One frame to draw the view (and settle the files it asks for).
                job.steps.push_back(Step::Frame);
                job.settle = true;
                Ok(true)
            }
            Request::Shadows(on) => {
                crate::ingame::three_d::set_shadows_override(on);
                job.reply.insert("shadows".into(), on.map_or(Value::Null, Value::Bool));
                job.steps.push_back(Step::Frame);
                Ok(true)
            }
            Request::Spawn(v) => {
                let unit = bench::spawn(game, &v)?;
                job.reply.insert("unit".into(), unit);
                Ok(false)
            }
            Request::Hit(v) => {
                let hit = bench::hit(game, &v)?;
                job.reply.insert("hit".into(), hit);
                Ok(false)
            }
            Request::Impact(v) => {
                let impact = bench::impact(game, &v)?;
                job.reply.insert("impact".into(), impact);
                Ok(false)
            }
            Request::GpuTime(frames) => {
                let timing = shot::gputime(game, frames)?;
                job.reply.insert("gputime".into(), timing);
                Ok(false)
            }
            Request::Record { dir, prefix, count, every, crop } => {
                if game.graphics.is_none() {
                    return Err("no GPU: start the driver with --drive-gpu to record".into());
                }
                job.record =
                    Some(Recording { dir, prefix, count, every, crop, primed: false, next: 0, paths: Vec::new() });
                Ok(true)
            }
            Request::Hud(on) => {
                let Mode::InGame(g) = &mut game.mode else { return Err("no game".into()) };
                g.hud_hidden = !on;
                Ok(false)
            }
            Request::Quit => {
                self.quit = true;
                let _ = event_loop;
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn options_from_the_command_line() {
        let o = Options::from_args(&args("game --drive --size 800x600 --dialogs report")).unwrap();
        assert_eq!(o, Options { gpu: false, size: [800, 600], dialogs: DialogPolicy::Report });
        let o = Options::from_args(&args("game --drive-gpu")).unwrap();
        assert!(o.gpu && o.size == [1024, 768] && o.dialogs == DialogPolicy::Auto);
        assert_eq!(strip_args(&args("--drive --size 800x600 C:\\AC")), args("C:\\AC"));
    }

    #[test]
    fn key_names() {
        assert_eq!(parse_key("Enter").unwrap().0, Key::Named(NamedKey::Enter));
        assert_eq!(parse_key("f12").unwrap().0, Key::Named(NamedKey::F12));
        let (k, text) = parse_key("b").unwrap();
        assert_eq!((k, text.as_deref()), (Key::Character(SmolStr::new("b")), Some("b")));
        assert!(parse_key("hyper").is_err());
    }

    #[test]
    fn a_click_moves_presses_and_releases() {
        let steps = click_steps([5.0, 6.0], Button::Right, false, Mods::default());
        let buttons: Vec<_> =
            steps.iter().filter_map(|s| if let Step::Button(b, st) = s { Some((*b, *st)) } else { None }).collect();
        assert_eq!(
            buttons,
            [(MouseButton::Right, ElementState::Pressed), (MouseButton::Right, ElementState::Released)]
        );
        assert!(matches!(steps[1], Step::Move([5.0, 6.0])));
    }
}
