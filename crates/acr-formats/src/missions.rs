//! Mission lists: `Missions\Missions.txt`, `Missions\SingleMiss.txt` and
//! `Missions\Campaigns.txt` (loaded by 0x569c27 and following).
//!
//! `Missions.txt` holds records of lines: identifier (`#SINGLE1`), name, script DLL, map,
//! description file, a count `n` and `n` more lines. For single missions the first extra
//! line is the briefing text shown in the menu; campaign missions add a bonus count and a
//! narration (`file MP3:Music\...`). `SingleMiss.txt` is a count followed by identifiers.
//! `Campaigns.txt` is a count followed by campaigns: name, description file, background,
//! mission count and `#id ON|OFF a b c` lines: open at start or not, and the three missions a
//! victory opens. A player's progress is kept in `Players\<name>.txt`, see [`Progress`].

/// One record of `Missions.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mission {
    pub id: String,
    pub name: Vec<u8>,
    pub script: String,
    pub map: String,
    pub description: String,
    pub extra: Vec<String>,
}

impl Mission {
    /// The text the menus show for the mission: the first extra line if it names a
    /// `.txt` file (0x5d6c9d), else the description file.
    pub fn briefing(&self) -> &str {
        match self.extra.first() {
            Some(f) if f.to_ascii_lowercase().contains(".txt") => f.split(' ').next().unwrap_or(f),
            _ => &self.description,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignMission {
    pub id: String,
    /// `ON`: available from the start.
    pub open: bool,
    /// Indices of the campaign's missions a victory in this one opens.
    pub unlocks: [u8; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Campaign {
    pub name: Vec<u8>,
    pub description: String,
    pub background: String,
    pub missions: Vec<CampaignMission>,
}

/// Lines as the original reads them (`0x569b0c`): up to LF, CR removed.
fn lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split(|&b| b == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l))
}

fn text(line: &[u8]) -> String {
    String::from_utf8_lossy(line).trim().to_owned()
}

/// Parses `Missions.txt`; stops at the first incomplete record.
pub fn parse_missions(data: &[u8]) -> Vec<Mission> {
    let mut it = lines(data).filter(|l| !l.iter().all(u8::is_ascii_whitespace));
    let mut out = Vec::new();
    while let (Some(id), Some(name), Some(script), Some(map), Some(description), Some(count)) =
        (it.next(), it.next(), it.next(), it.next(), it.next(), it.next())
    {
        let Ok(n) = text(count).parse::<usize>() else { break };
        let extra: Vec<String> = it.by_ref().take(n).map(text).collect();
        out.push(Mission {
            id: text(id),
            name: name.trim_ascii().to_vec(),
            script: text(script),
            map: text(map),
            description: text(description),
            extra,
        });
    }
    out
}

/// Parses `SingleMiss.txt`: the identifiers of the single missions, in menu order.
pub fn parse_single(data: &[u8]) -> Vec<String> {
    let words: Vec<String> = data.split(u8::is_ascii_whitespace).filter(|w| !w.is_empty()).map(text).collect();
    let n = words.first().and_then(|w| w.parse::<usize>().ok()).unwrap_or(0);
    words.into_iter().skip(1).take(n).collect()
}

/// Parses `Campaigns.txt`.
pub fn parse_campaigns(data: &[u8]) -> Vec<Campaign> {
    let mut it = lines(data).filter(|l| !l.iter().all(u8::is_ascii_whitespace));
    let n = it.next().and_then(|l| text(l).parse::<usize>().ok()).unwrap_or(0);
    let mut out = Vec::new();
    for _ in 0..n {
        let (Some(name), Some(description), Some(background), Some(count)) =
            (it.next(), it.next(), it.next(), it.next())
        else {
            break;
        };
        let Ok(count) = text(count).parse::<usize>() else { break };
        let missions = it
            .by_ref()
            .take(count)
            .map(|l| {
                let line = text(l);
                let mut words = line.split_whitespace();
                let id = words.next().unwrap_or("").to_owned();
                let open = words.next().is_some_and(|w| w.eq_ignore_ascii_case("ON"));
                let mut unlocks = [0; 3];
                for u in &mut unlocks {
                    *u = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                }
                CampaignMission { id, open, unlocks }
            })
            .collect();
        out.push(Campaign {
            name: name.trim_ascii().to_vec(),
            description: text(description),
            background: text(background),
            missions,
        });
    }
    out
}

/// A player's campaign progress, `Players\<name>.txt` (written by 0x56ac4e, read by
/// 0x56ad98; `standart player info.txt` is the template for new players).
///
/// One word per mission, as in the original's campaign table (0x28b7728): bit 0 of the low
/// byte = open, the three higher bytes = the missions a victory opens (from
/// `Campaigns.txt`). The file is the campaign count, then per campaign its mission count and
/// the words, each written as `"%d "`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub campaigns: Vec<Vec<u32>>,
}

impl Progress {
    /// The state of a new player: the `ON` missions open.
    pub fn new(campaigns: &[Campaign]) -> Self {
        let word = |m: &CampaignMission| {
            u32::from(m.open)
                | u32::from(m.unlocks[0]) << 8
                | u32::from(m.unlocks[1]) << 16
                | u32::from(m.unlocks[2]) << 24
        };
        Self { campaigns: campaigns.iter().map(|c| c.missions.iter().map(word).collect()).collect() }
    }

    /// Reads a player file over the state of a new player. Like the original it stops at the
    /// first count that does not fit the campaigns, keeping what it read before.
    pub fn read(data: &[u8], campaigns: &[Campaign]) -> Self {
        let mut progress = Self::new(campaigns);
        let mut words = data
            .split(u8::is_ascii_whitespace)
            .filter(|w| !w.is_empty())
            .map(|w| String::from_utf8_lossy(w).parse::<i64>().map(|v| v as u32).unwrap_or(0));
        if words.next() != Some(progress.campaigns.len() as u32) {
            return progress;
        }
        for missions in &mut progress.campaigns {
            if words.next() != Some(missions.len() as u32) {
                break;
            }
            for (m, w) in missions.iter_mut().zip(words.by_ref()) {
                *m = w;
            }
        }
        progress
    }

    pub fn write(&self) -> Vec<u8> {
        let mut out = format!("{} ", self.campaigns.len());
        for missions in &self.campaigns {
            out += &format!("{} ", missions.len());
            for &m in missions {
                out += &format!("{} ", m as i32);
            }
        }
        out.into_bytes()
    }

    pub fn is_open(&self, campaign: usize, mission: usize) -> bool {
        self.campaigns.get(campaign).and_then(|c| c.get(mission)).is_some_and(|&w| w & 0xff != 0)
    }

    /// How many missions the campaign's list shows: up to the last open one (0x5c5432).
    pub fn shown(&self, campaign: usize) -> usize {
        self.campaigns.get(campaign).map_or(0, |c| c.iter().rposition(|&w| w & 0xff != 0).map_or(0, |i| i + 1))
    }

    /// A victory in a campaign mission opens the missions it names (0x4bcbb1).
    pub fn win(&mut self, campaign: usize, mission: usize) {
        let Some(missions) = self.campaigns.get_mut(campaign) else { return };
        let Some(&word) = missions.get(mission) else { return };
        for shift in [8, 16, 24] {
            let next = (word >> shift & 0xff) as usize;
            if let Some(m) = missions.get_mut(next) {
                *m |= 1;
            }
        }
    }
}

/// Finds a mission by identifier (case-insensitive, as `_stricmp`).
pub fn find<'a>(missions: &'a [Mission], id: &str) -> Option<&'a Mission> {
    missions.iter().find(|m| m.id.eq_ignore_ascii_case(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MISSIONS: &[u8] = b"#SINGLE1\r\nDie R\xfcckkehr.\r\nMissions\\S1\\Mission.dll\r\nMissions\\S1\\S1.m3d\r\n\
Missions\\S1\\S1.txt\r\n1\r\nMissions\\S1\\Sin1.txt\r\n#c1@\r\nKolumbus\r\na.dll\r\na.m3d\r\na.txt\r\n3\r\naT.txt\r\n46\r\n\
a_.txt MP3:Music\\Ger\\a.ogg\r\n";

    #[test]
    fn parses_missions() {
        let m = parse_missions(MISSIONS);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].id, "#SINGLE1");
        assert_eq!(m[0].name, b"Die R\xfcckkehr.");
        assert_eq!(m[0].map, "Missions\\S1\\S1.m3d");
        assert_eq!(m[0].briefing(), "Missions\\S1\\Sin1.txt");
        assert_eq!(m[1].extra, ["aT.txt", "46", "a_.txt MP3:Music\\Ger\\a.ogg"]);
        assert_eq!(m[1].briefing(), "aT.txt");
        assert!(find(&m, "#C1@").is_some());
    }

    #[test]
    fn parses_lists() {
        assert_eq!(parse_single(b"2\r\n#SINGLE1\r\n#HUNT"), ["#SINGLE1", "#HUNT"]);
        let c = parse_campaigns(
            b"1\r\nKolumbus\r\nMissions\\C\\.txt\r\nbg.bmp\r\n2\r\n#c1@ ON 1 1 1\r\n#c2@ OFF  2 2 2\r\n",
        );
        assert_eq!(c.len(), 1);
        assert_eq!(
            c[0].missions,
            [
                CampaignMission { id: "#c1@".into(), open: true, unlocks: [1, 1, 1] },
                CampaignMission { id: "#c2@".into(), open: false, unlocks: [2, 2, 2] }
            ]
        );
    }

    /// `standart player info.txt` of the German 1.46 installation.
    const STANDARD: &[u8] = b"8 5 16843009 33686016 50529024 67372032 16843008 7 16843009 33686016 50529024 67372032 84215040 101058048 16843008 5 16843009 33686016 50529024 67372032 16843008 5 16843009 33686016 50529024 67372032 16843008 5 16843009 33686016 50529024 67372032 16843008 5 16843009 33686016 50529024 67372032 16843008 5 16843009 33686016 50529024 67372032 16843008 5 16843009 33686016 50529024 67372032 16843008 ";

    fn campaigns(counts: &[usize]) -> Vec<Campaign> {
        counts
            .iter()
            .map(|&n| Campaign {
                name: Vec::new(),
                description: String::new(),
                background: String::new(),
                missions: (0..n)
                    .map(|i| CampaignMission {
                        id: format!("#m{i}"),
                        open: i == 0,
                        unlocks: if i == 0 { [1; 3] } else { [(i + 1).min(n - 1) as u8, 0, 0].map(|u| u.max(1)) },
                    })
                    .collect(),
            })
            .collect()
    }

    #[test]
    fn progress_round_trips_the_original_file() {
        let p = Progress::read(STANDARD, &campaigns(&[5, 7, 5, 5, 5, 5, 5, 5]));
        assert_eq!(p.campaigns[0], [0x0101_0101, 0x0202_0200, 0x0303_0300, 0x0404_0400, 0x0101_0100]);
        assert_eq!(p.write(), STANDARD);
        assert_eq!(p.shown(0), 1);
    }

    #[test]
    fn a_victory_opens_the_next_missions() {
        let mut p = Progress::read(STANDARD, &campaigns(&[5, 7, 5, 5, 5, 5, 5, 5]));
        p.win(0, 0);
        assert!(p.is_open(0, 1) && !p.is_open(0, 2));
        assert_eq!(p.shown(0), 2);
        p.win(0, 1);
        assert_eq!(p.shown(0), 3);
        assert_eq!(p.write()[..20], b"8 5 16843009 3368601"[..]);
    }

    #[test]
    fn mismatched_files_keep_the_defaults() {
        let c = campaigns(&[2]);
        assert_eq!(Progress::read(b"3 2 1 1 ", &c), Progress::new(&c));
        assert_eq!(Progress::read(b"1 3 1 1 1 ", &c), Progress::new(&c));
        assert_eq!(Progress::read(b"1 2 1 1 ", &c).campaigns[0], [1, 1]);
    }
}
