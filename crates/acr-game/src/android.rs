//! Android entry point (`NativeActivity`): find the original game files on the device, then run.
//!
//! The game folder is looked up in the app's own folder on the shared storage
//! (`Android/data/<package>/files`, itself or its `American Conquest` folder), then in
//! `American Conquest` at the top of the shared storage, which needs "All files access": when
//! no folder is found, the app opens that permission's system page and looks again on return.
//! Logs go to logcat under the tag `acr`.

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use acr_app::NativeAssets;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;
use winit::window::WindowId;

use crate::ui::settings::Orientation;
use crate::{Event, Game};

/// Marker file of an American Conquest installation.
const MARKER: &str = "ALL.GSC";

/// The top of the shared storage ("Internal storage" in the phone's file manager).
const SHARED: &str = "/storage/emulated/0";

/// The Java class for the back gesture and the orientation (`tools/android/java`).
const SHELL: &str = "io.github.generalpawz.acr.Shell";

static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
static BACK_PROXY: std::sync::Mutex<Option<EventLoopProxy<Event>>> = std::sync::Mutex::new(None);

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
    let mut launcher = Launcher {
        app,
        proxy,
        game: None,
        asked: false,
        pointer_hidden: false,
        hints_tried: false,
        shell: None,
        orientation: None,
    };
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
    /// The performance hint session was tried (once, at the first resume with a game).
    hints_tried: bool,
    /// The `Shell` class, once loaded (back gesture and orientation).
    shell: Option<jni::refs::Global<jni::objects::JClass<'static>>>,
    /// The orientation last applied to the activity.
    orientation: Option<Orientation>,
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
        // `native_ui=svelte` (docs/webui.md "Native host (Android, #479)"): the Svelte UI in a WebView
        // over the game; read at start, and only with the bundle embedded.
        if native_ui_wanted() {
            match crate::webui::android::Host::new(&self.app, self.proxy.clone()) {
                Ok(host) => game.set_native_host(Box::new(host)),
                Err(e) => log::error!("webui: no native host, the engine draws: {e}"),
            }
        }
        Ok(game)
    }
}

impl Launcher {
    /// Sets the activity's orientation when the `orientation` setting differs from the one applied.
    fn apply_orientation(&mut self) {
        let (Some(game), Some(shell)) = (&self.game, &self.shell) else { return };
        let wanted = game.orientation();
        if self.orientation == Some(wanted) {
            return;
        }
        match set_orientation(&self.app, shell, wanted) {
            Ok(()) => {
                log::info!("orientation: {}", wanted.name());
                self.orientation = Some(wanted);
            }
            Err(e) => {
                log::error!("cannot set the orientation: {e}");
                // Do not retry every loop turn.
                self.orientation = Some(wanted);
            }
        }
    }
}

/// Loads `Shell` through the activity's class loader (a native thread's `FindClass` cannot see
/// app classes), registers its native and installs the back handling.
fn load_shell(
    app: &AndroidApp,
    proxy: &EventLoopProxy<Event>,
) -> Result<jni::refs::Global<jni::objects::JClass<'static>>, jni::errors::Error> {
    use jni::objects::{JClass, JObject, JValue};
    use jni::{NativeMethod, jni_sig, jni_str};
    if let Ok(mut slot) = BACK_PROXY.lock() {
        *slot = Some(proxy.clone());
    }
    // SAFETY: both pointers come from the running activity and stay valid while it runs.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| {
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
        let loader =
            env.call_method(&activity, jni_str!("getClassLoader"), jni_sig!(() -> java.lang.ClassLoader), &[])?.l()?;
        let name = env.new_string(SHELL)?;
        let class = env
            .call_method(
                &loader,
                jni_str!("loadClass"),
                jni_sig!((java.lang.String) -> java.lang.Class),
                &[JValue::Object(&name)],
            )?
            .l()?;
        let class = env.cast_local::<JClass>(class)?;
        // SAFETY: the signature of the function matches the descriptor.
        let methods = unsafe {
            [NativeMethod::from_raw_parts(jni_str!("nativeBack"), jni_str!("()V"), native_back as *mut c_void)]
        };
        // SAFETY: as above.
        unsafe { env.register_native_methods(&class, &methods)? };
        env.call_static_method(
            &class,
            jni_str!("installBack"),
            jni_sig!((android.app.Activity) -> void),
            &[JValue::Object(&activity)],
        )?;
        env.new_global_ref(&class)
    })
}

/// `Shell.nativeBack()`: the back gesture, on the UI thread.
extern "system" fn native_back<'c>(_env: jni::EnvUnowned<'c>, _class: jni::objects::JClass<'c>) {
    if let Ok(slot) = BACK_PROXY.lock()
        && let Some(proxy) = slot.as_ref()
        && proxy.send_event(Event::Back).is_err()
    {
        log::debug!("back dropped: the event loop is gone");
    }
}

/// `ActivityInfo.SCREEN_ORIENTATION_*` for a setting: the sensor-driven variants, so a flipped
/// phone stays upright (landscape is what the manifest's `sensorLandscape` was).
fn requested_orientation(orientation: Orientation) -> i32 {
    match orientation {
        Orientation::Landscape => 6,
        Orientation::Auto => 10,
        Orientation::Portrait => 7,
    }
}

/// `Shell.setOrientation(activity, orientation)`; the Java side hops to the UI thread.
fn set_orientation(
    app: &AndroidApp,
    shell: &jni::refs::Global<jni::objects::JClass<'static>>,
    orientation: Orientation,
) -> Result<(), jni::errors::Error> {
    use jni::objects::{JObject, JValue};
    use jni::{jni_sig, jni_str};
    // SAFETY: both pointers come from the running activity and stay valid while it runs.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| {
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
        env.call_static_method(
            shell,
            jni_str!("setOrientation"),
            jni_sig!((android.app.Activity, int) -> void),
            &[JValue::Object(&activity), JValue::Int(requested_orientation(orientation))],
        )?;
        Ok(())
    })
}

/// Whether the `native_ui` setting of the app's `settings.txt` asks for the Svelte UI and this build
/// can show it ([`crate::webui::native_ui_active`]).
fn native_ui_wanted() -> bool {
    let Some(text) = data_dir().and_then(|d| std::fs::read_to_string(d.join("settings.txt")).ok()) else {
        return false;
    };
    crate::webui::native_ui_active(crate::ui::settings::Settings::decode(&text).native_ui)
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
            if !self.hints_tried {
                self.hints_tried = true;
                // 60 Hz until the first frame reports the real budget.
                match HintSession::create(&self.app, std::time::Duration::from_micros(16_667)) {
                    Ok(Some(session)) => {
                        log::info!("performance hints: session created");
                        game.set_work_sink(Box::new(session));
                    }
                    Ok(None) => log::info!("performance hints: not available on this device"),
                    Err(e) => log::info!("performance hints: not available ({e})"),
                }
            }
            if self.shell.is_none() {
                match load_shell(&self.app, &self.proxy) {
                    Ok(class) => self.shell = Some(class),
                    Err(e) => log::error!("no back gesture handling: {e}"),
                }
            }
            self.apply_orientation();
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
        self.apply_orientation();
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

/// Android's performance hint session (ADPF, `PerformanceHintManager`, API 31+) of the main
/// thread, through JNI: the work the thread did per frame and the budget, so the scheduler can
/// pick efficient cores and clocks. Created once; when the device has no manager or refuses a
/// session, there is none and the game runs as before.
struct HintSession {
    vm: jni::JavaVM,
    session: jni::refs::Global<jni::objects::JObject<'static>>,
    /// A call failed: stop calling.
    failed: bool,
}

impl HintSession {
    /// The session for the calling thread aiming at `budget` per frame; `None` when the device
    /// has no such manager (before API 31) or gives no session.
    fn create(app: &AndroidApp, budget: std::time::Duration) -> Result<Option<Self>, jni::errors::Error> {
        use jni::objects::{JObject, JValue};
        use jni::{jni_sig, jni_str};
        /// `Build.VERSION_CODES.S`.
        const API_S: i32 = 31;
        // SAFETY: the pointer comes from the running activity and stays valid while it runs.
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
        let session = vm.attach_current_thread(|env| {
            let sdk =
                env.get_static_field(jni_str!("android/os/Build$VERSION"), jni_str!("SDK_INT"), jni_sig!("I"))?.i()?;
            if sdk < API_S {
                return Ok(None);
            }
            let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
            let name = env.new_string("performance_hint")?;
            let manager = env
                .call_method(
                    &activity,
                    jni_str!("getSystemService"),
                    jni_sig!("(Ljava/lang/String;)Ljava/lang/Object;"),
                    &[JValue::Object(&name)],
                )?
                .l()?;
            if manager.is_null() {
                return Ok(None);
            }
            let tid =
                env.call_static_method(jni_str!("android/os/Process"), jni_str!("myTid"), jni_sig!("()I"), &[])?.i()?;
            let tids = env.new_int_array(1)?;
            tids.set_region(env, 0, &[tid])?;
            let session = env
                .call_method(
                    &manager,
                    jni_str!("createHintSession"),
                    jni_sig!("([IJ)Landroid/os/PerformanceHintManager$Session;"),
                    &[JValue::Object(&tids), JValue::Long(nanos(budget))],
                )?
                .l()?;
            if session.is_null() {
                return Ok(None);
            }
            Ok::<_, jni::errors::Error>(Some(env.new_global_ref(session)?))
        })?;
        Ok(session.map(|session| Self { vm, session, failed: false }))
    }

    /// `Session.<method>(nanos)`.
    fn call(&mut self, method: &'static jni::strings::JNIStr, nanos: i64) {
        use jni::jni_sig;
        use jni::objects::JValue;
        if self.failed {
            return;
        }
        let result = self.vm.attach_current_thread(|env| {
            env.call_method(&self.session, method, jni_sig!("(J)V"), &[JValue::Long(nanos)]).map(|_| ())
        });
        if let Err(e) = result {
            log::warn!("performance hints stop: {e}");
            self.failed = true;
        }
    }
}

fn nanos(d: std::time::Duration) -> i64 {
    i64::try_from(d.as_nanos()).unwrap_or(i64::MAX)
}

impl crate::pacing::WorkSink for HintSession {
    fn target(&mut self, budget: std::time::Duration) {
        self.call(jni::jni_str!("updateTargetWorkDuration"), nanos(budget));
    }

    fn report(&mut self, work: std::time::Duration) {
        self.call(jni::jni_str!("reportActualWorkDuration"), nanos(work));
    }
}
