//! Mission lists: `Missions\Missions.txt`, `Missions\SingleMiss.txt` and
//! `Missions\Campaigns.txt` (loaded by 0x569c27 and following).
//!
//! `Missions.txt` holds records of lines: identifier (`#SINGLE1`), name, script DLL, map,
//! description file, a count `n` and `n` more lines. For single missions the first extra
//! line is the briefing text shown in the menu; campaign missions add a bonus count and a
//! narration (`file MP3:Music\...`). `SingleMiss.txt` is a count followed by identifiers.
//! `Campaigns.txt` is a count followed by campaigns: name, description file, background,
//! mission count and `#id ON|OFF a b c` lines (open at start or not).

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
                CampaignMission {
                    id: words.next().unwrap_or("").to_owned(),
                    open: words.next().is_some_and(|w| w.eq_ignore_ascii_case("ON")),
                }
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
            [CampaignMission { id: "#c1@".into(), open: true }, CampaignMission { id: "#c2@".into(), open: false }]
        );
    }
}
