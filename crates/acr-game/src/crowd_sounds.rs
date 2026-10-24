//! The din of crowds: `SOUNDLIST.TXT`'s `INGROUP n` and `FORGROUP n min max` modifiers.
//!
//! A sound marked `INGROUP n` counts towards group `n` each time it is asked to play in
//! earshot (0x584f34, from the placed play 0x5864ff, also when every variant is busy). The
//! counts run over windows of 11 game frames (0x585347: the counter 0x28b8950 rolls the
//! table +0x1b84 into +0x1784 when it passes 10). While the last window counted more than
//! `min` sounds of group `n`, every further one starts one of the group's `FORGROUP n`
//! sounds (two random tries for one that is not playing yet; 0x585058) as a silent loop
//! (-10000, `Play` with `DSBPLAY_LOOPING`) at the sound's x, or else pulls the playing one's
//! x towards it by 1/16 (`x = (15 x + new) / 16`, 0x58513d). Each frame (0x585211, 0x585347)
//! a playing crowd sound is panned like any placed sound (`2 (x - centre)`, clamped to
//! ±4000; 0x58526a) and its volume moves by `max(|v| / 5, 10)` hundredths of a dB: up
//! from at most -8000 to 0 while its group's count is above `min`, else down; at -8000 or
//! below it stops. `max` is read but has no effect (0x585012 sets the volume it just read).
//!
//! The shipped list has five crowds: walking Indians (`PUSTIHKAIND`, group 4) above 75 per
//! window bring their war cries (`PUSTIHA`, `indscrm1.wav`), walking horses
//! (`PUSTIHKAKON`, 3) and galloping ones (`LOBEG`, 2) above 75 a herd (`HORSIDUT`,
//! `HORSVINA`), marching European infantry (`PUSTIHKA`, 5) above 254 and death cries
//! (`DEASE`, 1) above 5 an infantry crowd (`DEASEB`, `DEASEA`: `pehota.wav`, Russian "infantry"). So each
//! crowd sound plays at most once at a time, whatever the number of men: thousands of
//! marching units add one looping voice per group, placed where most of them are heard.

use std::collections::HashMap;

use acr_formats::SoundList;
use acr_sim::Rng;

/// Game frames per counting window (0x585501: the counter rolls when it passes 10).
pub const WINDOW: u32 = 11;
/// Volume a crowd sound starts at, silent (hundredths of a dB, 0x5850bc).
const START: i32 = -10000;
/// A crowd sound at or below this volume stops; fading in starts from at most here (0x58542a).
const FLOOR: i32 = -8000;
/// Smallest volume step per frame (0x58540a).
const MIN_STEP: i32 = 10;
/// Pan limit (hundredths of a dB the far side loses), as for every placed sound.
const MAX_PAN: f32 = 4000.0;

/// The crowd sounds of one group (`FORGROUP n min max`).
#[derive(Debug, Clone, Default, PartialEq)]
struct Group {
    sounds: Vec<String>,
    min: u32,
}

/// A crowd sound that plays.
#[derive(Debug, Clone, PartialEq)]
pub struct Crowd {
    /// The `FORGROUP` sound's name in `SOUNDLIST.TXT`.
    pub name: String,
    pub group: u8,
    /// Where it is heard, in original screen pixels (world x).
    pub x: f32,
    /// Volume in hundredths of a dB (0 full, -10000 silent).
    pub level: i32,
}

/// What changed in a frame, for the audio and the log.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Start the crowd sound `name` looping, silent at first.
    Start {
        name: String,
        group: u8,
        count: u32,
    },
    /// Set its volume (0..=1) and pan (-1..=1).
    Set {
        name: String,
        volume: f32,
        pan: f32,
    },
    Stop {
        name: String,
    },
}

#[derive(Debug, Clone)]
pub struct Crowds {
    /// `INGROUP` of each sound name (upper case).
    member: HashMap<String, u8>,
    groups: HashMap<u8, Group>,
    /// Sounds of each group in the window running now, and in the last one.
    counting: HashMap<u8, u32>,
    counted: HashMap<u8, u32>,
    frame: u32,
    /// Game frames run, for the log.
    pub elapsed: u64,
    pub playing: Vec<Crowd>,
    rng: Rng,
    changes: Vec<Change>,
}

impl Crowds {
    /// Reads the groups of the sound list (0x585fea, 0x586052, 0x587190, 0x5871e0).
    pub fn new(list: &SoundList) -> Self {
        let mut member = HashMap::new();
        let mut groups: HashMap<u8, Group> = HashMap::new();
        for sound in &list.sounds {
            for m in &sound.modifiers {
                let mut words = m.split_whitespace();
                let key = words.next().unwrap_or("");
                let n: Vec<u32> = words.filter_map(|w| w.parse().ok()).collect();
                match (key, n.as_slice()) {
                    ("INGROUP", &[g]) if (1..256).contains(&g) => {
                        member.insert(sound.name.to_ascii_uppercase(), g as u8);
                    }
                    ("FORGROUP", &[g, min, _max]) if (1..256).contains(&g) => {
                        let group = groups.entry(g as u8).or_default();
                        // At most 16 crowd sounds per group (0x5871fc).
                        if group.sounds.len() < 16 {
                            group.sounds.push(sound.name.clone());
                        }
                        group.min = min & 0xff;
                    }
                    _ => {}
                }
            }
        }
        Self {
            member,
            groups,
            counting: HashMap::new(),
            counted: HashMap::new(),
            frame: 0,
            elapsed: 0,
            playing: Vec::new(),
            rng: Rng::new(0xc20d, 5),
            changes: Vec::new(),
        }
    }

    /// Whether the list has any crowd.
    pub fn any(&self) -> bool {
        !self.groups.is_empty()
    }

    /// The sound `name` was asked to play in earshot at `x` (original screen pixels).
    pub fn heard(&mut self, name: &str, x: f32) {
        let Some(&g) = self.member.get(&name.to_ascii_uppercase()) else { return };
        *self.counting.entry(g).or_default() += 1;
        let Some(group) = self.groups.get(&g) else { return };
        let count = self.counted.get(&g).copied().unwrap_or(0);
        if count <= group.min || group.sounds.is_empty() {
            return;
        }
        let mut last = None;
        for _ in 0..2 {
            let pick = &group.sounds[self.rng.below(group.sounds.len() as u32) as usize];
            match self.playing.iter().position(|c| &c.name == pick) {
                Some(i) => last = Some(i),
                None => {
                    self.changes.push(Change::Start { name: pick.clone(), group: g, count });
                    self.playing.push(Crowd { name: pick.clone(), group: g, x, level: START });
                    return;
                }
            }
        }
        if let Some(i) = last {
            let c = &mut self.playing[i];
            c.x = (c.x * 15.0 + x) / 16.0;
        }
    }

    /// One game frame with the view's centre at `centre_x`: pans and fades the crowd sounds
    /// and rolls the counting window. Returns the changes since the last call.
    pub fn frame(&mut self, centre_x: f32) -> Vec<Change> {
        let (groups, counted) = (&self.groups, &self.counted);
        let mut changes = std::mem::take(&mut self.changes);
        self.playing.retain_mut(|c| {
            let busy = groups.get(&c.group).is_some_and(|g| counted.get(&c.group).copied().unwrap_or(0) > g.min);
            c.level = fade(c.level, busy);
            if c.level <= FLOOR {
                changes.push(Change::Stop { name: c.name.clone() });
                return false;
            }
            let pan = (2.0 * (c.x - centre_x)).clamp(-MAX_PAN, MAX_PAN);
            let pan = (1.0 - gain(-pan.abs())).copysign(pan);
            changes.push(Change::Set { name: c.name.clone(), volume: gain(c.level as f32), pan });
            true
        });
        self.frame += 1;
        self.elapsed += 1;
        if self.frame >= WINDOW {
            self.frame = 0;
            let before = std::mem::replace(&mut self.counted, std::mem::take(&mut self.counting));
            for (&g, group) in &self.groups {
                let (was, now) = (before.get(&g).copied().unwrap_or(0), self.counted.get(&g).copied().unwrap_or(0));
                if (was > group.min) != (now > group.min) {
                    let state = if now > group.min { "above" } else { "below" };
                    log::info!(
                        "crowd group {g} at frame {}: {now} sounds in {WINDOW} frames, {state} {} ({:?})",
                        self.elapsed,
                        group.min,
                        group.sounds
                    );
                } else if now > 0 {
                    log::debug!("crowd group {g}: {now} sounds in {WINDOW} frames (min {})", group.min);
                }
            }
        }
        changes
    }

    /// The sounds of each group counted in the last window.
    #[cfg(test)]
    pub fn counted(&self, group: u8) -> u32 {
        self.counted.get(&group).copied().unwrap_or(0)
    }
}

/// One frame's volume step (0x5853f0..0x58548b): towards 0 while the crowd is heard, else
/// away; the effects volume is the mixer's.
fn fade(level: i32, busy: bool) -> i32 {
    let step = (level.abs() / 5).max(MIN_STEP);
    let level = if busy {
        let v = level.max(FLOOR);
        if v < -step { v + step } else { 0 }
    } else if level > START {
        level - step
    } else {
        level
    };
    level.min(0)
}

/// Amplitude of a DirectSound attenuation in hundredths of a dB.
fn gain(hundredths_db: f32) -> f32 {
    10f32.powf(hundredths_db / 2000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &[u8] = b"PUSTIHKA 2\r\nINGROUP 5\r\nSound\\new\\CLEAR3S.wav\r\nSound\\new\\CLEAR3S.wav\r\n\r\n\
        PUSTIHKAIND 1\r\nINGROUP 4\r\nSound\\new\\CLEAR3S.wav\r\n\r\n\
        DEASEB 1\r\nFORGROUP 5 254 255\r\nSound\\new\\pehota.wav\r\n\r\n\
        PUSTIHA 1\r\nFORGROUP 4 75 150\r\nSound\\new\\indscrm1.wav\r\n\r\nFIRE1 1\r\nSound\\X.wav\r\n";

    fn crowds() -> Crowds {
        Crowds::new(&SoundList::parse(LIST))
    }

    /// `n` sounds of `name` at `x` in each of `windows` windows, frame by frame.
    fn march(c: &mut Crowds, name: &str, n: u32, x: f32, windows: u32) -> Vec<Change> {
        let mut out = Vec::new();
        for f in 0..WINDOW * windows {
            // Spread the window's sounds over its frames.
            let k = n * (f % WINDOW + 1) / WINDOW - n * (f % WINDOW) / WINDOW;
            for _ in 0..k {
                c.heard(name, x);
            }
            out.extend(c.frame(1000.0));
        }
        out
    }

    #[test]
    fn reads_the_groups_of_the_list() {
        let c = crowds();
        assert_eq!(c.member.get("PUSTIHKAIND"), Some(&4));
        assert_eq!(c.member.get("PUSTIHKA"), Some(&5));
        assert_eq!(c.groups[&4], Group { sounds: vec!["PUSTIHA".into()], min: 75 });
        assert_eq!(c.groups[&5].min, 254);
        assert!(c.any() && !c.member.contains_key("FIRE1"));
    }

    #[test]
    fn a_crowd_starts_only_above_min_in_the_last_window() {
        let mut c = crowds();
        // 75 Indian steps a window are not enough (the count must exceed `min`).
        assert!(march(&mut c, "PUSTIHKAIND", 75, 1000.0, 3).is_empty());
        assert_eq!(c.counted(4), 75);
        // 76 are: the next sound after the window rolls starts the war cries.
        let changes = march(&mut c, "PUSTIHKAIND", 76, 1000.0, 2);
        let starts: Vec<_> = changes.iter().filter(|c| matches!(c, Change::Start { .. })).collect();
        assert_eq!(starts, [&Change::Start { name: "PUSTIHA".into(), group: 4, count: 76 }]);
        assert_eq!(c.playing.len(), 1);
        // Unknown and ungrouped sounds count nowhere.
        c.heard("FIRE1", 0.0);
        c.heard("NONE", 0.0);
        assert_eq!(c.counting.values().sum::<u32>(), c.counting.get(&4).copied().unwrap_or(0));
    }

    #[test]
    fn one_voice_per_crowd_sound_however_many_march() {
        let mut c = crowds();
        // Three thousand marching men: one infantry crowd, no more.
        let changes = march(&mut c, "PUSTIHKA", 3000, 1000.0, 4);
        assert_eq!(changes.iter().filter(|c| matches!(c, Change::Start { .. })).count(), 1);
        assert_eq!(c.playing.len(), 1);
        assert_eq!(c.playing[0].name, "DEASEB");
        // 254 a window do not start it.
        let mut c = crowds();
        assert!(march(&mut c, "PUSTIHKA", 254, 1000.0, 4).is_empty());
    }

    #[test]
    fn fades_in_to_full_then_out_and_stops() {
        let mut c = crowds();
        let changes = march(&mut c, "PUSTIHKAIND", 100, 1000.0, 5);
        let Some(Change::Set { volume, pan, .. }) = changes.last() else { panic!("{changes:?}") };
        // Centred at full volume after the fade in (20-odd frames from -80 dB).
        assert_eq!((*volume, *pan), (1.0, 0.0));
        // The first step: from -10000 by 2000 after clamping to -8000.
        let first = changes.iter().find_map(|c| match c {
            Change::Set { volume, .. } => Some(*volume),
            _ => None,
        });
        assert_eq!(first, Some(gain(-6000.0)));
        // The marching stops: the crowd fades out and stops within a window and 35 frames.
        let quiet = march(&mut c, "PUSTIHKAIND", 0, 1000.0, 5);
        let stop = quiet.iter().position(|c| matches!(c, Change::Stop { .. })).expect("stops");
        assert!((WINDOW as usize..WINDOW as usize * 2 + 35).contains(&stop), "{stop}");
        assert!(c.playing.is_empty());
    }

    #[test]
    fn follows_where_most_are_heard() {
        let mut c = crowds();
        march(&mut c, "PUSTIHKAIND", 100, 1000.0, 3);
        // Heard from the right now: the crowd moves by 1/16 per sound and pans right.
        let changes = march(&mut c, "PUSTIHKAIND", 100, 1800.0, 2);
        assert!((c.playing[0].x - 1800.0).abs() < 1.0);
        let Some(Change::Set { pan, .. }) = changes.last() else { panic!() };
        assert!((pan - (1.0 - gain(-1600.0))).abs() < 1e-3, "{pan}");
    }

    #[test]
    fn volume_steps_match_the_original() {
        assert_eq!(fade(-10000, true), -6000);
        assert_eq!(fade(-6000, true), -4800);
        assert_eq!(fade(-40, true), -30);
        assert_eq!(fade(-10, true), 0);
        assert_eq!(fade(0, true), 0);
        assert_eq!(fade(0, false), -10);
        assert_eq!(fade(-100, false), -120);
        assert_eq!(fade(-10000, false), -10000);
    }
}
