//! "Spotify mitmischen" (music choice 2): the game plays no music of its own and lowers the
//! volume of Spotify's audio sessions (the Windows volume mixer) while the battle is loud,
//! raising it again slowly when it calms down. On leaving the mode or the game Spotify gets
//! its volume back. Only on Windows; elsewhere the choice just turns the game's music off.

use std::sync::{Arc, Mutex};

use acr_audio::Mixer;

/// How Spotify's volume follows the loudness of the game's sound effects.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ducking {
    /// The loudness at which the music starts to go down.
    pub threshold: f32,
    /// The loudness above `threshold` at which it is fully down.
    pub range: f32,
    /// How far down (0..=1 of the music's own volume).
    pub depth: f32,
    /// Time constants of the loudness envelope, in seconds.
    pub attack: f32,
    pub release: f32,
}

impl Default for Ducking {
    fn default() -> Self {
        Self { threshold: 0.02, range: 0.12, depth: 0.6, attack: 0.15, release: 2.0 }
    }
}

/// The envelope of the effects' loudness and the volume factor for the music.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Default)]
pub struct Envelope {
    level: f32,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl Envelope {
    /// Follows `loudness` (RMS 0..=1) over `dt` seconds; returns the factor (1 = untouched,
    /// `1 - depth` = fully ducked) for the music's volume.
    pub fn step(&mut self, d: &Ducking, loudness: f32, dt: f32) -> f32 {
        let tau = if loudness > self.level { d.attack } else { d.release };
        let k = 1.0 - (-dt / tau.max(1e-3)).exp();
        self.level += (loudness - self.level) * k;
        let duck = ((self.level - d.threshold) / d.range.max(1e-6)).clamp(0.0, 1.0);
        1.0 - d.depth * duck
    }
}

/// The running ducker; dropping it gives Spotify its volume back.
pub struct SpotifyDucker {
    #[cfg(windows)]
    stop: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(windows)]
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SpotifyDucker {
    /// Starts following the effects' loudness of `mixer`.
    pub fn start(mixer: Arc<Mutex<Mixer>>) -> Self {
        #[cfg(windows)]
        {
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = stop.clone();
            let thread = std::thread::Builder::new()
                .name("spotify-ducker".into())
                .spawn(move || win::run(&mixer, &flag, Ducking::default()))
                .map_err(|e| log::warn!("spotify: {e}"))
                .ok();
            Self { stop, thread }
        }
        #[cfg(not(windows))]
        {
            let _ = mixer;
            Self {}
        }
    }
}

impl Drop for SpotifyDucker {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }
}

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use acr_audio::Mixer;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Media::Audio::{
        DEVICE_STATE_ACTIVE, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
        MMDeviceEnumerator, eRender,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::core::{Interface, PWSTR};

    use super::{Ducking, Envelope};

    /// One of Spotify's sessions: its volume control, the user's volume and what was set.
    struct Session {
        pid: u32,
        volume: ISimpleAudioVolume,
        base: f32,
        set: f32,
    }

    pub(super) fn run(mixer: &Arc<Mutex<Mixer>>, stop: &AtomicBool, ducking: Ducking) {
        // SAFETY: COM is initialised for this thread only and released at its end.
        if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
            log::warn!("spotify: COM unavailable");
            return;
        }
        let mut sessions: Vec<Session> = Vec::new();
        let mut envelope = Envelope::default();
        let mut last_scan: Option<Instant> = None;
        let tick = Duration::from_millis(50);
        while !stop.load(Ordering::Relaxed) {
            if last_scan.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
                last_scan = Some(Instant::now());
                for s in spotify_sessions().unwrap_or_default() {
                    if !sessions.iter().any(|k| k.pid == s.1 && k.volume == s.0) {
                        // SAFETY: a valid session volume interface.
                        let base = unsafe { s.0.GetMasterVolume() }.unwrap_or(1.0);
                        sessions.push(Session { pid: s.1, volume: s.0, base, set: base });
                    }
                }
            }
            let loudness = mixer.lock().map(|m| m.sfx_level).unwrap_or(0.0);
            let factor = envelope.step(&ducking, loudness, tick.as_secs_f32());
            sessions.retain_mut(|s| {
                // SAFETY: valid interfaces; a closed session reports an error and is dropped.
                let Ok(now) = (unsafe { s.volume.GetMasterVolume() }) else { return false };
                if (now - s.set).abs() > 0.02 {
                    // The user moved Spotify's volume: that is the new full volume.
                    s.base = (now / (s.set / s.base.max(1e-3)).max(0.05)).min(1.0);
                }
                let target = (s.base * factor).clamp(0.0, 1.0);
                if (target - s.set).abs() > 0.004 {
                    // SAFETY: as above.
                    if unsafe { s.volume.SetMasterVolume(target, std::ptr::null()) }.is_err() {
                        return false;
                    }
                    s.set = target;
                }
                true
            });
            std::thread::sleep(tick);
        }
        for s in &sessions {
            // SAFETY: as above; give the user's volume back.
            let _ = unsafe { s.volume.SetMasterVolume(s.base, std::ptr::null()) };
        }
        drop(sessions);
        // SAFETY: matches the CoInitializeEx above.
        unsafe { CoUninitialize() };
    }

    /// The volume controls of all audio sessions of `Spotify.exe` on the active outputs.
    pub(super) fn spotify_sessions() -> windows::core::Result<Vec<(ISimpleAudioVolume, u32)>> {
        let mut found = Vec::new();
        // SAFETY: COM calls on interfaces obtained from COM on this thread.
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            for d in 0..devices.GetCount()? {
                let Ok(device) = devices.Item(d) else { continue };
                let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
                let Ok(list) = manager.GetSessionEnumerator() else { continue };
                for i in 0..list.GetCount().unwrap_or(0) {
                    let Ok(control) = list.GetSession(i) else { continue };
                    let Ok(control) = control.cast::<IAudioSessionControl2>() else { continue };
                    let Ok(pid) = control.GetProcessId() else { continue };
                    if pid == 0 || !is_spotify(pid) {
                        continue;
                    }
                    if let Ok(volume) = control.cast::<ISimpleAudioVolume>() {
                        found.push((volume, pid));
                    }
                }
            }
        }
        Ok(found)
    }

    fn is_spotify(pid: u32) -> bool {
        // SAFETY: the handle is closed below; the buffer outlives the call.
        unsafe {
            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(process);
            ok && String::from_utf16_lossy(&buf[..len as usize]).to_ascii_lowercase().ends_with("\\spotify.exe")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_leaves_the_music_alone() {
        let mut e = Envelope::default();
        for _ in 0..100 {
            assert_eq!(e.step(&Ducking::default(), 0.005, 0.05), 1.0);
        }
    }

    #[test]
    fn a_loud_battle_ducks_fast_and_lets_go_slowly() {
        let d = Ducking::default();
        let mut e = Envelope::default();
        let mut f = 1.0;
        for _ in 0..10 {
            f = e.step(&d, 0.3, 0.05); // half a second of volleys
        }
        assert!((f - (1.0 - d.depth)).abs() < 0.05, "fully down: {f}");
        for _ in 0..20 {
            f = e.step(&d, 0.0, 0.05); // one quiet second
        }
        assert!(f < 0.8, "still partly down after a second: {f}");
        for _ in 0..200 {
            f = e.step(&d, 0.0, 0.05);
        }
        assert!(f > 0.99, "back up after ten seconds: {f}");
    }

    /// Lists Spotify's audio sessions and their volumes without changing them (needs a
    /// running Spotify): `cargo test -p acr-game spotify -- --ignored --nocapture`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn finds_spotify_sessions() {
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
        // SAFETY: COM for the test thread.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let sessions = win::spotify_sessions().expect("the audio sessions");
        for (volume, pid) in &sessions {
            // SAFETY: a valid session volume interface.
            println!("Spotify pid {pid}: volume {:?}", unsafe { volume.GetMasterVolume() });
        }
        assert!(!sessions.is_empty(), "no Spotify session (is it running and has it played?)");
    }
}
