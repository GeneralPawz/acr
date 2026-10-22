//! Sound effects of units: the `SOUNDLIST.TXT` groups their definitions name.
//!
//! The original plays a group through `PlayEffect` (0x5863c4): it starts at a random
//! variant and takes the first one whose buffer is not playing, so a busy group does not
//! stack copies of the same sample; if every variant plays, nothing is started. Sounds in
//! the world go through 0x5868d3, see [`placed`].
//!
//! Ambient sounds (0x4ecef0, once per game frame from 0x4a7198): up to ten random cells of
//! the view are tried for deep water (`SEA2` depth > 0x80); the first one found plays a
//! random `SEA` group of `NRES.DAT [SOUNDS]` there with chance `chance / 32768`. If no
//! water turns up and no `DANGER` sound played in the last 400 frames (timer 0x28c25a8),
//! a random `FIELD` group plays at a random point of the view with its chance.

use std::collections::HashMap;

use acr_audio::VoiceId;
use acr_formats::SoundList;
use acr_formats::normalize_path;
use acr_sim::Rng;
use acr_sim::fog::Fog;

use crate::music::Audio;

pub const SOUND_LIST: &str = "SOUNDLIST.TXT";
/// Holds the ambient sound lines in its `[SOUNDS]` section.
pub const NRES: &str = "NRES.DAT";

/// Frames a `DANGER` sound keeps the land ambience quiet (0x586550).
const DANGER_FRAMES: u32 = 400;

/// An ambient line of `NRES.DAT [SOUNDS]`: `SEA 600 1 SEA` = chance out of 32768 per frame,
/// group count, `SOUNDLIST.TXT` groups (read by 0x4ec6bc and following).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ambient {
    pub chance: i32,
    pub sounds: Vec<String>,
}

/// The `SEA` (deep water) and `FIELD` (land) lines of `NRES.DAT`.
pub fn parse_ambient(data: &[u8]) -> (Ambient, Ambient) {
    let text = String::from_utf8_lossy(data);
    let (mut sea, mut field) = (Ambient::default(), Ambient::default());
    let mut in_sounds = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_sounds = line.eq_ignore_ascii_case("[SOUNDS]");
            continue;
        }
        let mut words = line.split_whitespace();
        let target = match words.next() {
            Some(w) if in_sounds && w.eq_ignore_ascii_case("SEA") => &mut sea,
            Some(w) if in_sounds && w.eq_ignore_ascii_case("FIELD") => &mut field,
            _ => continue,
        };
        let (Some(chance), Some(count)) =
            (words.next().and_then(|w| w.parse().ok()), words.next().and_then(|w| w.parse::<usize>().ok()))
        else {
            continue;
        };
        *target = Ambient { chance, sounds: words.take(count).map(str::to_owned).collect() };
    }
    (sea, field)
}

/// What the ambience needs to know about the running game.
pub struct AmbientView<'a> {
    /// Game frames (simulation ticks) since the last call.
    pub ticks: u32,
    /// `[left, top, width, height]` in original screen pixels, as for [`placed`].
    pub view: [f32; 4],
    /// Map size in 32-pixel cells.
    pub map: [i32; 2],
    /// Water depth per 32-pixel cell (`SEA2`).
    pub depth: Option<&'a acr_formats::m3d::Grid<u8>>,
    /// The local player's fog of war, which mutes sounds (see [`placed`]).
    pub fog: Option<&'a Fog>,
}

/// A sound to play: group name from `SOUNDLIST.TXT`, pan (-1 left .. 1 right), volume 0..=1.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEvent {
    pub name: String,
    pub pan: f32,
    pub volume: f32,
}

pub struct UnitSounds {
    list: Option<SoundList>,
    /// Sample bytes by normalized path; `None` while loading.
    samples: HashMap<String, Option<Vec<u8>>>,
    /// The voice last started per sample, to skip samples that are still playing.
    voices: HashMap<String, VoiceId>,
    rng: Rng,
    requests: Vec<String>,
    sea: Ambient,
    field: Ambient,
    /// Frames left in which the land ambience stays quiet after a `DANGER` sound.
    danger: u32,
    /// ACR's own groups (hit and pain sounds, [`crate::hit_sounds`]): name → game paths.
    own: HashMap<String, Vec<String>>,
}

impl Default for UnitSounds {
    fn default() -> Self {
        Self::new()
    }
}

impl UnitSounds {
    pub fn new() -> Self {
        let own: HashMap<String, Vec<String>> = crate::hit_sounds::GROUPS
            .iter()
            .filter_map(|(g, _)| Some((g.to_string(), crate::hit_sounds::files(g)?)))
            .collect();
        let mut requests = vec![SOUND_LIST.to_owned(), NRES.to_owned()];
        let mut samples = HashMap::new();
        for file in own.values().flatten() {
            samples.insert(normalize_path(file), None);
            requests.push(file.clone());
        }
        Self {
            list: None,
            samples,
            voices: HashMap::new(),
            rng: Rng::new(0x5a1d, 3),
            requests,
            sea: Ambient::default(),
            field: Ambient::default(),
            danger: 0,
            own,
        }
    }

    /// Files to load; deliver them with [`UnitSounds::on_loaded`].
    pub fn take_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.requests)
    }

    /// Takes a loaded file if it is the sound list or one of its samples.
    pub fn on_loaded(&mut self, path: &str, bytes: &[u8]) -> bool {
        if path == NRES {
            (self.sea, self.field) = parse_ambient(bytes);
            // Other parts of the game read NRES.DAT too.
            return false;
        }
        if path == SOUND_LIST {
            let list = SoundList::parse(bytes);
            for sound in &list.sounds {
                for file in &sound.files {
                    if self.samples.insert(normalize_path(file), None).is_none() {
                        self.requests.push(file.clone());
                    }
                }
            }
            self.list = Some(list);
            return true;
        }
        match self.samples.get_mut(&normalize_path(path)) {
            Some(slot @ None) => {
                *slot = Some(bytes.to_vec());
                true
            }
            _ => false,
        }
    }

    /// Plays one variant of each event's group like the original's `PlayEffect`.
    pub fn play(&mut self, events: &[SoundEvent], audio: &mut Audio) {
        for event in events {
            let (files, danger) = match self.list.as_ref().and_then(|l| l.get(&event.name)) {
                Some(sound) => (&sound.files, sound.modifiers.iter().any(|m| m.eq_ignore_ascii_case("DANGER"))),
                None => match self.own.get(&event.name) {
                    Some(files) => (files, false),
                    None => continue,
                },
            };
            let n = files.len();
            if n == 0 {
                continue;
            }
            if danger {
                self.danger = DANGER_FRAMES;
            }
            let start = self.rng.below(n as u32) as usize;
            for k in 0..n {
                let file = &files[(start + k) % n];
                let key = normalize_path(file);
                if self.voices.get(&key).is_some_and(|v| audio.is_playing(*v)) {
                    continue;
                }
                let Some(Some(bytes)) = self.samples.get(&key) else { break };
                if let Some(voice) = audio.play_at(&key, bytes, event.volume, event.pan) {
                    log::debug!("sound {} -> {key} (pan {:.2}, volume {:.2})", event.name, event.pan, event.volume);
                    self.voices.insert(key, voice);
                }
                break;
            }
        }
    }
}

impl UnitSounds {
    /// The ambient sounds of `view.ticks` game frames.
    pub fn ambient(&mut self, view: &AmbientView) -> Vec<SoundEvent> {
        let mut out = Vec::new();
        for _ in 0..view.ticks {
            self.danger = self.danger.saturating_sub(1);
            out.extend(self.ambient_frame(view));
        }
        out
    }

    /// `rand() & 0x7fff` of the original's sound code (0x585d27).
    fn rand(&mut self) -> i32 {
        self.rng.below(32768) as i32
    }

    fn ambient_frame(&mut self, view: &AmbientView) -> Option<SoundEvent> {
        let [left, top, width, height] = view.view;
        // The view in the original's units: 32-pixel columns, 16-pixel rows.
        let (x0, y0) = ((left / 32.0) as i32, (top / 16.0) as i32);
        let (w, h) = ((width / 32.0) as i32, (height / 16.0) as i32);
        for _ in 0..10 {
            let x = x0 + ((self.rand() * w) >> 15);
            let y = y0 + ((self.rand() * h) >> 15);
            if x <= 0 || x >= view.map[0] || y <= 0 || y >= view.map[1] {
                continue;
            }
            let deep = view.depth.and_then(|d| d.get(x as usize, y as usize)).is_some_and(|d| d > 0x80);
            if !deep {
                continue;
            }
            let name = self.pick(true)?;
            return event(name, (x * 32) as f32, (y * 16) as f32, view.view, view.fog);
        }
        if self.danger > 0 {
            return None;
        }
        let name = self.pick(false)?;
        let y = top + ((self.rand() * h) >> 11) as f32;
        let x = left + ((self.rand() * w) >> 10) as f32;
        event(name, x, y, view.view, view.fog)
    }

    /// A random group of the `SEA` or `FIELD` line, if its chance comes up.
    fn pick(&mut self, sea: bool) -> Option<String> {
        let line = if sea { &self.sea } else { &self.field };
        let (chance, n) = (line.chance, line.sounds.len() as i32);
        if self.rand() >= chance || n == 0 {
            return None;
        }
        let i = ((self.rand() * n) >> 15) as usize;
        let line = if sea { &self.sea } else { &self.field };
        line.sounds.get(i).cloned()
    }
}

fn event(name: String, x: f32, y: f32, view: [f32; 4], fog: Option<&Fog>) -> Option<SoundEvent> {
    let (volume, pan) = placed(x, y, view, fog)?;
    Some(SoundEvent { name, pan, volume })
}

/// Original pixels around the view in which world sounds are still heard (0x586807):
/// 1024 left and right, 512 above and below.
const HEAR_MARGIN: [f32; 2] = [1024.0, 512.0];
/// DirectSound pan limit for world sounds (hundredths of a dB the far channel loses).
const MAX_PAN: f32 = 4000.0;
/// DirectSound volume of a sound outside the view but within the margin (-8 dB).
const OFF_SCREEN_VOLUME: f32 = -800.0;

/// Volume (0..=1) and pan (-1..=1) of a sound at `(x, y)` in original screen pixels
/// (`x` = world x, `y` = world y / 2) for the view `[left, top, width, height]`, or
/// `None` if it is too far away to be heard.
///
/// The original (0x5868d3) skips sounds beyond [`HEAR_MARGIN`], plays sounds in the view
/// at full volume and the others at -8 dB, and pans by twice the distance from the view
/// centre, clamped to ±4000. While fog of war is on (`0x775e18`) it also skips sounds
/// where the local player's fog at `(x, 2y)` is below 900 (`GetFogSpotVisibility`'s limit,
/// without the height); `fog` is `None` when there is none.
pub fn placed(x: f32, y: f32, view: [f32; 4], fog: Option<&Fog>) -> Option<(f32, f32)> {
    let [left, top, width, height] = view;
    let (right, bottom) = (left + width, top + height);
    if x < left - HEAR_MARGIN[0]
        || x > right + HEAR_MARGIN[0]
        || y < top - HEAR_MARGIN[1]
        || y > bottom + HEAR_MARGIN[1]
    {
        return None;
    }
    if fog.is_some_and(|f| f.get(x as i32, 2 * y as i32) < acr_sim::fog::VISIBLE) {
        return None;
    }
    let pan = (2.0 * (x - (left + right) / 2.0)).clamp(-MAX_PAN, MAX_PAN);
    let on_screen = (left..right).contains(&x) && (top..bottom).contains(&y);
    let volume = if on_screen { 0.0 } else { OFF_SCREEN_VOLUME };
    Some((gain(volume), (1.0 - gain(-pan.abs())).copysign(pan)))
}

/// Amplitude of a DirectSound attenuation in hundredths of a dB.
fn gain(hundredths_db: f32) -> f32 {
    10f32.powf(hundredths_db / 2000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: [f32; 4] = [1000.0, 500.0, 1024.0, 768.0];

    #[test]
    fn reads_the_ambient_lines_of_nres() {
        let data = b"[CONST]\r\nSEA 1 1 X\r\n[SOUNDS]\r\nALARM  ALARM\r\nSEA    600 1 SEA\r\nFIELD  300 2 FIELD BIRDS\r\n[ORDERICONS]\r\n";
        let (sea, field) = parse_ambient(data);
        assert_eq!(sea, Ambient { chance: 600, sounds: vec!["SEA".into()] });
        assert_eq!(field, Ambient { chance: 300, sounds: vec!["FIELD".into(), "BIRDS".into()] });
    }

    #[test]
    fn ambience_plays_sea_over_water_and_field_on_land() {
        let mut s = UnitSounds::new();
        s.sea = Ambient { chance: 32768, sounds: vec!["SEA".into()] };
        s.field = Ambient { chance: 32768, sounds: vec!["FIELD".into()] };
        let water = acr_formats::m3d::Grid { width: 64, height: 64, cells: vec![200u8; 64 * 64] };
        let land = acr_formats::m3d::Grid { width: 64, height: 64, cells: vec![0u8; 64 * 64] };
        let view = |depth, ticks| AmbientView { ticks, view: VIEW, map: [64, 64], depth: Some(depth), fog: None };
        let sea = s.ambient(&view(&water, 3));
        assert!(sea.len() == 3 && sea.iter().all(|e| e.name == "SEA" && e.volume == 1.0));
        let field = s.ambient(&view(&land, 3));
        assert!(field.len() == 3 && field.iter().all(|e| e.name == "FIELD"));
        // A DANGER sound keeps the land quiet for 400 frames; the sea is not affected.
        s.danger = DANGER_FRAMES;
        assert!(s.ambient(&view(&land, 399)).is_empty());
        assert_eq!(s.ambient(&view(&land, 2)).len(), 2);
        s.danger = DANGER_FRAMES;
        assert_eq!(s.ambient(&view(&water, 1)).len(), 1);
    }

    #[test]
    fn centre_is_full_volume_and_unpanned() {
        assert_eq!(placed(1512.0, 884.0, VIEW, None), Some((1.0, 0.0)));
    }

    #[test]
    fn pans_by_distance_from_the_centre() {
        // 250 px right of the centre: pan 500, the left channel loses 5 dB.
        let (volume, pan) = placed(1762.0, 600.0, VIEW, None).unwrap();
        assert_eq!(volume, 1.0);
        assert!((pan - (1.0 - 10f32.powf(-0.25))).abs() < 1e-6);
        let (_, left) = placed(1262.0, 600.0, VIEW, None).unwrap();
        assert_eq!(left, -pan);
    }

    #[test]
    fn off_screen_sounds_are_quieter_until_out_of_range() {
        // 1488 px right of the centre, off screen: -8 dB, the left channel loses 29.76 dB.
        let (volume, pan) = placed(3000.0, 600.0, VIEW, None).unwrap();
        assert!((volume - 10f32.powf(-0.4)).abs() < 1e-6);
        assert!((pan - (1.0 - 10f32.powf(-1.488))).abs() < 1e-6);
        // A wide view reaches the pan limit: the far channel loses 40 dB.
        let (_, pan) = placed(4600.0, 600.0, [0.0, 0.0, 5120.0, 1440.0], None).unwrap();
        assert!((pan - 0.99).abs() < 1e-6);
        assert!(placed(1000.0 - 1024.0, 500.0 - 512.0, VIEW, None).is_some());
        assert_eq!(placed(1000.0 - 1025.0, 600.0, VIEW, None), None);
        assert_eq!(placed(1500.0, 500.0 + 768.0 + 513.0, VIEW, None), None);
    }

    #[test]
    fn fog_mutes_sounds() {
        // A grid nobody has lit is black everywhere.
        let fog = Fog::new(1);
        assert_eq!(placed(1512.0, 884.0, VIEW, Some(&fog)), None);
    }
}
