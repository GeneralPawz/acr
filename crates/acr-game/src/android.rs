//! Android entry point (`NativeActivity`): find the original game files on the device, then run.
//!
//! The game folder is looked up in the app's own folder on the shared storage
//! (`Android/data/<package>/files`, itself or its `American Conquest` folder), then in
//! `American Conquest` at the top of the shared storage, which needs "All files access": when
//! no folder is found, the app opens that permission's system page and looks again on return.
//! Logs go to logcat under the tag `acr`.

use std::path::{Path, PathBuf};

use acr_app::NativeAssets;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;
use winit::window::WindowId;

use crate::{Event, Game};

/// Marker file of an American Conquest installation.
const MARKER: &str = "ALL.GSC";

/// The top of the shared storage ("Internal storage" in the phone's file manager).
const SHARED: &str = "/storage/emulated/0";

static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_tag("acr").with_max_level(log::LevelFilter::Info));
    std::panic::set_hook(Box::new(|info| {
        log::error!("{info}\n{}", std::backtrace::Backtrace::force_capture());
    }));
    if let Some(dir) = app.external_data_path().or_else(|| app.internal_data_path()) {
        let _ = DATA_DIR.set(dir);
    }
    if let Err(e) = run(app) {
        log::error!("{e}");
    }
    // The event loop ends when the activity is destroyed (back, task removed). Android keeps
    // the process cached and on the next launch creates a new activity in it, but
    // `android_main` runs once per process, so the app would open to nothing. Ending the
    // process makes every launch a cold start.
    log::info!("event loop ended; exiting the process");
    std::process::exit(0);
}

/// The app's own folder: settings and performance snapshots.
pub(crate) fn data_dir() -> Option<PathBuf> {
    DATA_DIR.get().cloned()
}

fn run(app: AndroidApp) -> Result<(), String> {
    let event_loop =
        EventLoop::<Event>::with_user_event().with_android_app(app.clone()).build().map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();
    let mut launcher = Launcher { app, proxy, game: None, asked: false, pointer_hidden: false };
    event_loop.run_app(&mut launcher).map_err(|e| e.to_string())
}

/// The game once its folder is found. Until then each resume looks again, and the first one
/// opens the system's "All files access" page for this app.
struct Launcher {
    app: AndroidApp,
    proxy: EventLoopProxy<Event>,
    game: Option<Game>,
    asked: bool,
    /// The system pointer is hidden (the game draws its own for a mouse).
    pointer_hidden: bool,
}

impl Launcher {
    fn start(&self, dir: &Path) -> Result<Game, String> {
        log::info!("game data: {}", dir.display());
        let assets = NativeAssets::open(dir)?;
        let mut game = Game::new(self.proxy.clone(), Box::new(assets));
        game.set_saves(Box::new(acr_app::saves::FolderSaves::new(dir)));
        game.set_profiles(Box::new(acr_app::saves::FolderSaves::files(dir, "Players", "txt")));
        if let Some(settings) = data_dir() {
            let place = settings.join("settings.txt").display().to_string();
            game.set_settings_store(Box::new(acr_app::saves::FolderSaves::files(&settings, "", "txt")), &place);
        }
        Ok(game)
    }
}

impl ApplicationHandler<Event> for Launcher {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_none() {
            let candidates: Vec<PathBuf> = data_dir()
                .into_iter()
                .flat_map(|d| [d.clone(), d.join("American Conquest")])
                .chain([Path::new(SHARED).join("American Conquest")])
                .collect();
            match candidates.iter().find(|d| d.join(MARKER).is_file()) {
                Some(dir) => match self.start(dir) {
                    Ok(game) => self.game = Some(game),
                    Err(e) => {
                        log::error!("{e}");
                        return event_loop.exit();
                    }
                },
                None => {
                    let tried: Vec<String> = candidates.iter().map(|d| d.display().to_string()).collect();
                    log::error!("no {MARKER} in any of: {}", tried.join(", "));
                    if !self.asked {
                        self.asked = true;
                        if let Err(e) = ask_all_files_access(&self.app) {
                            log::error!("cannot open the All files access settings: {e}");
                        }
                    }
                    return;
                }
            }
        }
        if let Some(game) = &mut self.game {
            // The user may change the Game Mode in the system's game settings between runs.
            match game_mode_battery(&self.app) {
                Ok(on) => crate::pacing::set_battery_mode(on),
                Err(e) => log::warn!("cannot read the Game Mode: {e}"),
            }
            game.resumed(event_loop);
            if !self.pointer_hidden {
                self.pointer_hidden = true;
                if let Err(e) = hide_system_pointer(&self.app) {
                    log::error!("cannot hide the system pointer: {e}");
                }
            }
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(game) = &mut self.game {
            game.suspended(event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        if let Some(game) = &mut self.game {
            game.user_event(event_loop, event);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(game) = &mut self.game {
            game.window_event(event_loop, id, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(game) = &mut self.game {
            game.about_to_wait(event_loop);
        }
    }
}

/// Opens the system page that grants this app "All files access", unless it has it already.
fn ask_all_files_access(app: &AndroidApp) -> Result<(), jni::errors::Error> {
    use jni::objects::{JObject, JValue};
    use jni::{jni_sig, jni_str};
    // SAFETY: both pointers come from the running activity and stay valid while it runs.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| {
        let granted = env
            .call_static_method(
                jni_str!("android/os/Environment"),
                jni_str!("isExternalStorageManager"),
                jni_sig!(() -> bool),
                &[],
            )?
            .z()?;
        if granted {
            log::info!("All files access is granted; the folder is missing");
            return Ok(());
        }
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
        let action = env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?;
        let package =
            env.call_method(&activity, jni_str!("getPackageName"), jni_sig!(() -> java.lang.String), &[])?.l()?;
        let prefix = env.new_string("package:")?;
        let uri_text = env
            .call_method(
                &prefix,
                jni_str!("concat"),
                jni_sig!((java.lang.String) -> java.lang.String),
                &[JValue::Object(&package)],
            )?
            .l()?;
        let uri = env
            .call_static_method(
                jni_str!("android/net/Uri"),
                jni_str!("parse"),
                jni_sig!((java.lang.String) -> android.net.Uri),
                &[JValue::Object(&uri_text)],
            )?
            .l()?;
        let intent = env.new_object(
            jni_str!("android/content/Intent"),
            jni_sig!((java.lang.String, android.net.Uri) -> void),
            &[JValue::Object(&action), JValue::Object(&uri)],
        )?;
        env.call_method(
            &activity,
            jni_str!("startActivity"),
            jni_sig!((android.content.Intent) -> void),
            &[JValue::Object(&intent)],
        )?;
        Ok(())
    })
}

/// Hides Android's own mouse pointer over the activity's windows (`PointerIcon.TYPE_NULL`),
/// since the game draws its cursor. `setPointerIcon` only stores the icon in the view, which
/// the next hover looks up, so it needs no UI-thread hop.
fn hide_system_pointer(app: &AndroidApp) -> Result<(), jni::errors::Error> {
    use jni::objects::{JObject, JValue};
    use jni::{jni_sig, jni_str};
    /// `android.R.id.content`, the frame that holds the activity's content view.
    const CONTENT_ID: i32 = 0x0102_0002;
    /// `PointerIcon.TYPE_NULL`.
    const TYPE_NULL: i32 = 0;
    // SAFETY: both pointers come from the running activity and stay valid while it runs.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| {
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
        let icon = env
            .call_static_method(
                jni_str!("android/view/PointerIcon"),
                jni_str!("getSystemIcon"),
                jni_sig!((android.content.Context, int) -> android.view.PointerIcon),
                &[JValue::Object(&activity), JValue::Int(TYPE_NULL)],
            )?
            .l()?;
        let window =
            env.call_method(&activity, jni_str!("getWindow"), jni_sig!(() -> android.view.Window), &[])?.l()?;
        let decor = env.call_method(&window, jni_str!("getDecorView"), jni_sig!(() -> android.view.View), &[])?.l()?;
        let content = env
            .call_method(
                &activity,
                jni_str!("findViewById"),
                jni_sig!((int) -> android.view.View),
                &[JValue::Int(CONTENT_ID)],
            )?
            .l()?;
        for view in [decor, content] {
            if !view.is_null() {
                env.call_method(
                    &view,
                    jni_str!("setPointerIcon"),
                    jni_sig!((android.view.PointerIcon) -> void),
                    &[JValue::Object(&icon)],
                )?;
            }
        }
        Ok(())
    })
}

/// Whether Android's Game Mode is `GAME_MODE_BATTERY` (`GameManager.getGameMode()`, API 31+;
/// earlier versions have neither, which reads as not battery).
fn game_mode_battery(app: &AndroidApp) -> Result<bool, jni::errors::Error> {
    use jni::objects::{JObject, JValue};
    use jni::{jni_sig, jni_str};
    /// `Build.VERSION_CODES.S`.
    const API_S: i32 = 31;
    /// `GameManager.GAME_MODE_BATTERY`.
    const GAME_MODE_BATTERY: i32 = 3;
    // SAFETY: both pointers come from the running activity and stay valid while it runs.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| {
        let sdk =
            env.get_static_field(jni_str!("android/os/Build$VERSION"), jni_str!("SDK_INT"), jni_sig!(int))?.i()?;
        if sdk < API_S {
            return Ok(false);
        }
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
        let name = env.new_string("game")?;
        let manager = env
            .call_method(
                &activity,
                jni_str!("getSystemService"),
                jni_sig!((java.lang.String) -> java.lang.Object),
                &[JValue::Object(&name)],
            )?
            .l()?;
        if manager.is_null() {
            return Ok(false);
        }
        let mode = env.call_method(&manager, jni_str!("getGameMode"), jni_sig!(() -> int), &[])?.i()?;
        Ok(mode == GAME_MODE_BATTERY)
    })
}
