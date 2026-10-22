//! Nation files (`*.NDS`) and the nation list (`NATIONS.LST`).
//!
//! An `.NDS` file is a sequence of `[SECTION]`s holding whitespace-separated tokens (see
//! [`super`] for comments). [`NdsFile`] keeps every section; [`NationDef`] interprets
//! the sections of a nation file: its members (unit/building types and their `.MD`
//! file), what each producer builds, the upgrades (tech tree) and formations.
//! `WEAPON.NDS` uses the same container for weapon definitions and is read with
//! [`NdsFile`] only.
//!
//! Object ids carry a nation suffix: `EngPik(EN)` is the `EngPik` definition as a member
//! of the English nation.

use super::{Issue, Token, token_lines};
use crate::text::CodePage;

/// Count prefixes above this are treated as corrupt.
const MAX_COUNT: i32 = 10_000;

/// A line of tokens inside a section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub line: u32,
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Name without brackets; empty for tokens before the first section.
    pub name: String,
    pub line: u32,
    pub lines: Vec<Line>,
}

impl Section {
    /// All tokens of the section with their line numbers.
    pub fn tokens(&self) -> impl Iterator<Item = (u32, &str)> {
        self.lines.iter().flat_map(|l| l.tokens.iter().map(move |t| (l.line, t.as_str())))
    }
}

/// The generic section structure of an `.NDS` file.
#[derive(Debug, Clone, Default)]
pub struct NdsFile {
    pub sections: Vec<Section>,
}

impl NdsFile {
    pub fn parse(data: &[u8]) -> Self {
        let mut sections: Vec<Section> = Vec::new();
        for tokens in token_lines(data) {
            let first = tokens[0];
            let header = first.bytes.len() > 2 && first.bytes[0] == b'[' && first.bytes.ends_with(b"]");
            if header {
                let name = CodePage::Cp1252.decode(&first.bytes[1..first.bytes.len() - 1]);
                sections.push(Section { name, line: first.line, lines: Vec::new() });
                if tokens.len() == 1 {
                    continue;
                }
            } else if sections.is_empty() {
                sections.push(Section { name: String::new(), line: first.line, lines: Vec::new() });
            }
            let line = Line { line: first.line, tokens: tokens[usize::from(header)..].iter().map(text).collect() };
            sections.last_mut().unwrap().lines.push(line);
        }
        NdsFile { sections }
    }

    /// The first section with this name.
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// For `WEAPON.NDS`: the weapon ids defined in `[MEMBERS]`.
    pub fn weapon_ids(&self) -> Vec<&str> {
        let lines = self.section("MEMBERS").map(|s| &s.lines[..]).unwrap_or_default();
        lines.iter().map(|l| l.tokens[0].as_str()).collect()
    }
}

fn text(t: &Token) -> String {
    t.text(CodePage::Cp1252)
}

/// `[MEMBERS]` entry: object id and the `.MD` file defining it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub id: String,
    pub md: String,
}

/// One button of a producer's build menu (`[FIXED_PRODUCE]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProduceItem {
    pub id: String,
    /// Column and row of the button in the command panel.
    pub x: i32,
    pub y: i32,
    pub hotkey: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Producer {
    pub id: String,
    pub items: Vec<ProduceItem>,
}

/// A head followed by a counted list of ids (`HEAD n ID*n`), the shape of most sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountList {
    pub head: String,
    pub items: Vec<String>,
}

/// One upgrade of the tech tree (`[UPGRADE]`).
///
/// ```text
/// ID ICON #POSITION x y #LEVEL n MESSAGE #COST n (RES amount)*n #TIME t
///     EFFECT...
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upgrade {
    pub id: String,
    /// Icon id from `ICONLIST.TXT`.
    pub icon: String,
    /// Button position in the command panel of the building offering it.
    pub position: (i32, i32),
    pub level: i32,
    /// `#STAGELEVEL` instead of `#LEVEL`.
    pub stage_level: bool,
    /// Text id of the hint.
    pub message: String,
    pub cost: Vec<(String, i32)>,
    pub time: i32,
    /// The effect, e.g. `["DAMAGE", "WEAPONKIND", "PIKA", "EngPik(EN)", "+1"]`.
    pub effect: Vec<String>,
    pub line: u32,
}

impl Upgrade {
    /// Effect category (`DAMAGE`, `BUILD`, `ENABLE`, `UPSTAGE0`, ...).
    pub fn category(&self) -> Option<&str> {
        self.effect.first().map(String::as_str)
    }

    /// The trailing numeric value of the effect and whether it is a percentage.
    pub fn value(&self) -> Option<(i32, bool)> {
        let last = self.effect.last()?;
        let (num, percent) = match last.strip_suffix('%') {
            Some(n) => (n, true),
            None => (last.as_str(), false),
        };
        Some((num.strip_prefix('+').unwrap_or(num).parse().ok()?, percent))
    }

    /// Object ids the effect applies to: the ids after `GRP n`, or the single id before the value.
    pub fn targets(&self) -> Vec<&str> {
        let e: Vec<&str> = self.effect.iter().map(String::as_str).collect();
        if let Some(i) = e.iter().position(|&t| t == "GRP") {
            let n = e.get(i + 1).and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
            return e.iter().skip(i + 2).take(n).copied().filter(|t| t.contains('(')).collect();
        }
        e.iter().copied().filter(|t| t.contains('(')).collect()
    }
}

/// A formation (`[OFFICERS]`): name, its variants by size and the unit types allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formation {
    pub name: String,
    pub variants: Vec<String>,
    pub units: Vec<String>,
}

/// Commanders able to form formations: officer, drummer, optional flag bearer (`NONE`
/// for absent slots), then the formations they lead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficerGroup {
    pub commanders: Vec<String>,
    pub formations: Vec<Formation>,
}

/// A nation file interpreted.
#[derive(Debug, Clone, Default)]
pub struct NationDef {
    pub members: Vec<Member>,
    pub fixed_produce: Vec<Producer>,
    /// Objects available from the start.
    pub enabled: Vec<String>,
    pub upgrades: Vec<Upgrade>,
    /// Which building offers which upgrades.
    pub upgrade_place: Vec<CountList>,
    /// Upgrades unlocked when an upgrade is researched.
    pub upgrade_links: Vec<CountList>,
    /// Upgrades performed at once when an upgrade is researched (`[AUTOUPGRADE]`).
    pub auto_upgrade: Vec<CountList>,
    /// Upgrades available from the start (`[UPGRADEENABLE]`).
    pub upgrade_enable: Vec<String>,
    /// Upgrades shown greyed out (`[DISABLED_UPGRADES]`).
    pub disabled_upgrades: Vec<String>,
    /// Upgrades researched per building rather than per nation (`[PRIVATE]`).
    pub private: Vec<String>,
    /// Upgrades that raise a building's stage (`[UPGRADESTAGE]`).
    pub upgrade_stage: Vec<String>,
    /// Buildings whose rally point can be set (`[CANSETDEST]`).
    pub can_set_dest: Vec<String>,
    /// `[UNITLOCK]` lines `ID n LOCKID`: `ID` is disabled while the player has `n` objects
    /// of `LOCKID`.
    pub unit_lock: Vec<(String, i32, String)>,
    /// Upgrades required before an object or upgrade becomes available.
    pub access_control: Vec<CountList>,
    /// Groups of the nation as listed in the game (`#INFANTRY` etc. are group headers).
    pub country: Vec<CountList>,
    pub officers: Vec<OfficerGroup>,
    /// `id n` pairs of `[STAGEDEPENDENT]`.
    pub stage_dependent: Vec<(String, i32)>,
    pub issues: Vec<Issue>,
}

/// Sections that are plain id lists.
pub const ID_LIST_SECTIONS: &[&str] =
    &["ENABLED", "UPGRADEENABLE", "DISABLED_UPGRADES", "CANSETDEST", "PRIVATE", "UPGRADESTAGE"];
/// Sections that are `HEAD n ID*n` lists.
pub const COUNT_LIST_SECTIONS: &[&str] = &["UPGRADEPLACE", "UPGRADELINKS", "AUTOUPGRADE", "ACCESSCONTROL", "COUNTRY"];

impl NationDef {
    pub fn parse(data: &[u8]) -> Self {
        Self::from_nds(&NdsFile::parse(data))
    }

    pub fn from_nds(nds: &NdsFile) -> Self {
        let mut def = NationDef::default();
        for section in &nds.sections {
            match section.name.to_ascii_uppercase().as_str() {
                "MEMBERS" => def.parse_members(section),
                "FIXED_PRODUCE" => def.parse_fixed_produce(section),
                "ENABLED" => def.enabled = section.tokens().map(|(_, t)| t.to_owned()).collect(),
                "UPGRADE" => def.parse_upgrades(section),
                "UPGRADEPLACE" => def.upgrade_place = def.count_lists(section),
                "UPGRADELINKS" => def.upgrade_links = def.count_lists(section),
                "ACCESSCONTROL" => def.access_control = def.count_lists(section),
                "COUNTRY" => def.country = def.count_lists(section),
                "OFFICERS" => def.parse_officers(section),
                "STAGEDEPENDENT" => def.parse_stage_dependent(section),
                "AUTOUPGRADE" => def.auto_upgrade = def.count_lists(section),
                "UPGRADEENABLE" => def.upgrade_enable = id_list(section),
                "DISABLED_UPGRADES" => def.disabled_upgrades = id_list(section),
                "PRIVATE" => def.private = id_list(section),
                "UPGRADESTAGE" => def.upgrade_stage = id_list(section),
                "CANSETDEST" => def.can_set_dest = id_list(section),
                "UNITLOCK" => def.parse_unit_lock(section),
                "SPECIAL_UPGRADE" | "SPECIAL_UNIT" | "END" | "" => {}
                n if ID_LIST_SECTIONS.contains(&n) => {}
                _ => def.issue(section.line, format!("unknown section [{}]", section.name)),
            }
        }
        def
    }

    fn issue(&mut self, line: u32, message: String) {
        self.issues.push(Issue { line, message });
    }

    fn parse_members(&mut self, section: &Section) {
        for l in &section.lines {
            match &l.tokens[..] {
                [id, md] => self.members.push(Member { id: id.clone(), md: md.clone() }),
                _ => self.issue(l.line, format!("[MEMBERS]: expected `ID MDNAME`, found {:?}", l.tokens)),
            }
        }
    }

    fn parse_fixed_produce(&mut self, section: &Section) {
        let mut lines = section.lines.iter();
        while let Some(head) = lines.next() {
            let count = match &head.tokens[..] {
                [_, n] => n.parse::<i32>().ok().filter(|n| (0..=MAX_COUNT).contains(n)),
                _ => None,
            };
            let Some(count) = count else {
                self.issue(head.line, format!("[FIXED_PRODUCE]: expected `PRODUCER n`, found {:?}", head.tokens));
                continue;
            };
            let mut producer = Producer { id: head.tokens[0].clone(), items: Vec::new() };
            for _ in 0..count {
                let Some(l) = lines.next() else {
                    self.issue(head.line, format!("[FIXED_PRODUCE]: {} lists too few items", producer.id));
                    break;
                };
                let t = &l.tokens;
                let (x, y) = (t.get(1).and_then(|v| v.parse().ok()), t.get(2).and_then(|v| v.parse().ok()));
                match (x, y, t.len()) {
                    (Some(x), Some(y), 3 | 4) => {
                        producer.items.push(ProduceItem { id: t[0].clone(), x, y, hotkey: t.get(3).cloned() })
                    }
                    _ => self.issue(l.line, format!("[FIXED_PRODUCE]: expected `ID x y [KEY]`, found {t:?}")),
                }
            }
            self.fixed_produce.push(producer);
        }
    }

    fn parse_upgrades(&mut self, section: &Section) {
        let tokens: Vec<(u32, &str)> = section.tokens().collect();
        let starts: Vec<usize> =
            (0..tokens.len()).filter(|&i| tokens.get(i + 2).is_some_and(|t| t.1 == "#POSITION")).collect();
        if let Some(&(line, _)) = tokens.first().filter(|_| starts.first() != Some(&0)) {
            self.issue(line, "[UPGRADE]: tokens before the first upgrade".into());
        }
        for (k, &start) in starts.iter().enumerate() {
            let end = starts.get(k + 1).copied().unwrap_or(tokens.len());
            let words: Vec<&str> = tokens[start..end].iter().map(|t| t.1).collect();
            match parse_upgrade(&words, tokens[start].0) {
                Ok(u) => self.upgrades.push(u),
                Err(e) => self.issue(tokens[start].0, format!("[UPGRADE] {}: {e}", words[0])),
            }
        }
    }

    /// Parses `HEAD n ID*n` lists from the token stream of a section.
    fn count_lists(&mut self, section: &Section) -> Vec<CountList> {
        let tokens: Vec<(u32, &str)> = section.tokens().collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let (line, head) = tokens[i];
            let count = tokens.get(i + 1).and_then(|t| t.1.parse::<i32>().ok()).filter(|n| (0..=MAX_COUNT).contains(n));
            let Some(count) = count else {
                self.issue(line, format!("[{}]: expected a count after {head}", section.name));
                i += 1;
                continue;
            };
            let items: Vec<String> = tokens.iter().skip(i + 2).take(count as usize).map(|t| t.1.to_owned()).collect();
            if items.len() < count as usize {
                self.issue(line, format!("[{}]: {head} lists too few items", section.name));
            }
            i += 2 + items.len();
            out.push(CountList { head: head.to_owned(), items });
        }
        out
    }

    fn parse_officers(&mut self, section: &Section) {
        let tokens: Vec<(u32, &str)> = section.tokens().collect();
        let mut i = 0;
        let int_at = |i: usize| tokens.get(i).and_then(|t| t.1.parse::<usize>().ok());
        while i < tokens.len() {
            let line = tokens[i].0;
            let mut group = OfficerGroup { commanders: Vec::new(), formations: Vec::new() };
            while i < tokens.len() && int_at(i).is_none() {
                group.commanders.push(tokens[i].1.to_owned());
                i += 1;
            }
            let n = int_at(i).unwrap_or(0);
            i += 1;
            for _ in 0..n {
                let Some(&(_, name)) = tokens.get(i) else { break };
                let variants = int_at(i + 1).unwrap_or(0);
                let units_at = i + 2 + variants;
                let units = int_at(units_at).unwrap_or(0);
                let words =
                    |a: usize, n: usize| tokens.iter().skip(a).take(n).map(|t| t.1.to_owned()).collect::<Vec<_>>();
                let f = Formation {
                    name: name.to_owned(),
                    variants: words(i + 2, variants),
                    units: words(units_at + 1, units),
                };
                if f.variants.len() < variants || f.units.len() < units || int_at(i + 1).is_none() {
                    self.issue(line, format!("[OFFICERS]: formation {name} is truncated"));
                }
                i = units_at + 1 + units;
                group.formations.push(f);
            }
            if group.commanders.is_empty() {
                self.issue(line, "[OFFICERS]: group without commanders".into());
            }
            self.officers.push(group);
        }
    }

    fn parse_unit_lock(&mut self, section: &Section) {
        for l in &section.lines {
            match &l.tokens[..] {
                [id, n, lock] if n.parse::<i32>().is_ok() => {
                    self.unit_lock.push((id.clone(), n.parse().unwrap(), lock.clone()))
                }
                _ => self.issue(l.line, format!("[UNITLOCK]: expected `ID n LOCKID`, found {:?}", l.tokens)),
            }
        }
    }

    fn parse_stage_dependent(&mut self, section: &Section) {
        for l in &section.lines {
            match &l.tokens[..] {
                [id, n] if n.parse::<i32>().is_ok() => self.stage_dependent.push((id.clone(), n.parse().unwrap())),
                _ => self.issue(l.line, format!("[STAGEDEPENDENT]: expected `ID n`, found {:?}", l.tokens)),
            }
        }
    }

    /// Looks up a member by id (`EngPik(EN)`).
    pub fn member(&self, id: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.id.eq_ignore_ascii_case(id))
    }

    pub fn upgrade(&self, id: &str) -> Option<&Upgrade> {
        self.upgrades.iter().find(|u| u.id.eq_ignore_ascii_case(id))
    }
}

/// The first token of every line (the engine reads one id per line and skips the rest).
fn id_list(section: &Section) -> Vec<String> {
    section.lines.iter().map(|l| l.tokens[0].clone()).collect()
}

fn parse_upgrade(w: &[&str], line: u32) -> Result<Upgrade, String> {
    let int = |i: usize| -> Result<i32, String> {
        let t = w.get(i).ok_or("truncated header")?;
        t.parse().map_err(|_| format!("expected integer, found {t}"))
    };
    let expect = |i: usize, s: &str| -> Result<(), String> {
        match w.get(i) {
            Some(t) if *t == s => Ok(()),
            t => Err(format!("expected {s}, found {t:?}")),
        }
    };
    let stage_level = match w.get(5) {
        Some(&"#LEVEL") => false,
        Some(&"#STAGELEVEL") => true,
        t => return Err(format!("expected #LEVEL or #STAGELEVEL, found {t:?}")),
    };
    expect(8, "#COST")?;
    let n = int(9)?;
    if !(0..=16).contains(&n) {
        return Err(format!("invalid cost count {n}"));
    }
    let mut cost = Vec::new();
    for k in 0..n as usize {
        cost.push((w.get(10 + 2 * k).ok_or("truncated cost")?.to_string(), int(11 + 2 * k)?));
    }
    let t = 10 + 2 * n as usize;
    expect(t, "#TIME")?;
    Ok(Upgrade {
        id: w[0].to_owned(),
        icon: w[1].to_owned(),
        position: (int(3)?, int(4)?),
        level: int(6)?,
        stage_level,
        message: w.get(7).ok_or("truncated header")?.to_string(),
        cost,
        time: int(t + 1)?,
        effect: w[t + 2..].iter().map(|s| s.to_string()).collect(),
        line,
    })
}

/// One entry of `NATIONS.LST`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NationEntry {
    /// Nation key, also used by `[COUNTRY]`.
    pub id: String,
    /// English name.
    pub name: String,
    /// Nation file (`MAY.NDS`).
    pub file: String,
    /// `#TX a b`, meaning open.
    pub tx: Option<(i32, i32)>,
    /// Remaining tokens: the nation suffixes, e.g. `(MA) (MA)`.
    pub suffixes: Vec<String>,
    pub line: u32,
}

/// `NATIONS.LST`: blocks of `count` followed by `count` nation lines.
///
/// The shipped file has two blocks: the 15 American Conquest nations and a leftover
/// block of 19 *Cossacks* nations whose files do not exist.
#[derive(Debug, Clone, Default)]
pub struct NationList {
    pub blocks: Vec<Vec<NationEntry>>,
    pub issues: Vec<Issue>,
}

impl NationList {
    pub fn parse(data: &[u8]) -> Self {
        let mut list = NationList::default();
        let mut lines = token_lines(data).into_iter();
        while let Some(head) = lines.next() {
            let count = match &head[..] {
                [n] => n.int().filter(|n| (0..=MAX_COUNT).contains(n)),
                _ => None,
            };
            let Some(count) = count else {
                list.issues.push(Issue { line: head[0].line, message: "expected a nation count".into() });
                continue;
            };
            let mut block = Vec::new();
            for _ in 0..count {
                let Some(l) = lines.next() else {
                    list.issues.push(Issue { line: head[0].line, message: "nation list truncated".into() });
                    break;
                };
                match parse_nation(&l) {
                    Some(e) => block.push(e),
                    None => {
                        list.issues.push(Issue { line: l[0].line, message: "expected `ID Name FILE.NDS ...`".into() })
                    }
                }
            }
            list.blocks.push(block);
        }
        list
    }
}

fn parse_nation(t: &[Token]) -> Option<NationEntry> {
    let mut rest: Vec<String> = t.get(3..)?.iter().map(text).collect();
    let tx = match &rest[..] {
        [tx, a, b, ..] if tx == "#TX" => Some((a.parse().ok()?, b.parse().ok()?)),
        _ => None,
    };
    if tx.is_some() {
        rest.drain(..3);
    }
    Some(NationEntry { id: text(&t[0]), name: text(&t[1]), file: text(&t[2]), tx, suffixes: rest, line: t[0].line })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NATION: &[u8] = b"[MEMBERS]\r\nEngPik(EN) EngPik\r\nEngCen(EN) EngCen\r\n\r\n[FIXED_PRODUCE]\r\n//c\r\n\
EngCen(EN) 2\r\nEngPik(EN) 1 0 Q\r\nEngKri(EN) 0 1\r\n[ENABLED]\r\nEngCen(EN)\r\n/EngPik(EN)\r\n[UPGRADE]\r\n\
UP1 ICO #POSITION 2 3 #LEVEL 1 MSG #COST 2 GOLD 25 WOOD 250 #TIME 200\r\n\tDAMAGE WEAPONKIND PIKA GRP 1 EngPik(EN) +10%\r\n\
UP2 ICO #POSITION 0 0 #STAGELEVEL 2 MSG #COST 0 #TIME 5\r\n / BUILD x\r\n\
[UPGRADEPLACE]\r\nEngCen(EN) 2 UP1\r\n UP2\r\n[COUNTRY]\r\nENGLAND 3\r\n#INFANTRY\r\nEngPik(EN)\r\nEngCen(EN)\r\n\
[OFFICERS]\r\nOf(EN) NONE 1\r\nLINE 2 #L15 #L36 1 EngPik(EN)\r\n[STAGEDEPENDENT]\r\nUP1 1\r\n\
[UPGRADEENABLE]\r\nUP1\r\n/UP2\r\n[AUTOUPGRADE]\r\nUP1 1 UP2\r\n[UNITLOCK]\r\nEngPik(EN) 3 EngPik(EN)\r\n[END]\r\n";

    #[test]
    fn parses_sections() {
        let nds = NdsFile::parse(NATION);
        let names: Vec<_> = nds.sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "MEMBERS",
                "FIXED_PRODUCE",
                "ENABLED",
                "UPGRADE",
                "UPGRADEPLACE",
                "COUNTRY",
                "OFFICERS",
                "STAGEDEPENDENT",
                "UPGRADEENABLE",
                "AUTOUPGRADE",
                "UNITLOCK",
                "END"
            ]
        );
        assert_eq!(nds.section("enabled").unwrap().lines.len(), 1);
    }

    #[test]
    fn parses_nation() {
        let def = NationDef::parse(NATION);
        assert!(def.issues.is_empty(), "{:?}", def.issues);
        assert_eq!(def.member("engpik(en)").unwrap().md, "EngPik");
        assert_eq!(
            def.fixed_produce[0].items[0],
            ProduceItem { id: "EngPik(EN)".into(), x: 1, y: 0, hotkey: Some("Q".into()) }
        );
        assert_eq!(def.fixed_produce[0].items[1].hotkey, None);
        assert_eq!(def.enabled, ["EngCen(EN)"]);
        let up = def.upgrade("UP1").unwrap();
        assert_eq!((up.position, up.level, up.time), ((2, 3), 1, 200));
        assert_eq!(up.cost, [("GOLD".to_string(), 25), ("WOOD".to_string(), 250)]);
        assert_eq!(up.category(), Some("DAMAGE"));
        assert_eq!(up.value(), Some((10, true)));
        assert_eq!(up.targets(), ["EngPik(EN)"]);
        let up2 = def.upgrade("UP2").unwrap();
        assert!(up2.stage_level && up2.effect.is_empty());
        assert_eq!(def.upgrade_place[0].items, ["UP1", "UP2"]);
        assert_eq!(def.country[0].items.len(), 3);
        assert_eq!(def.officers[0].commanders, ["Of(EN)", "NONE"]);
        assert_eq!(def.officers[0].formations[0].variants, ["#L15", "#L36"]);
        assert_eq!(def.officers[0].formations[0].units, ["EngPik(EN)"]);
        assert_eq!(def.stage_dependent, [("UP1".to_string(), 1)]);
        assert_eq!(def.upgrade_enable, ["UP1"]);
        assert_eq!(def.auto_upgrade[0].items, ["UP2"]);
        assert_eq!(def.unit_lock, [("EngPik(EN)".to_string(), 3, "EngPik(EN)".to_string())]);
    }

    #[test]
    fn reports_bad_nation_data() {
        let def = NationDef::parse(
            b"[MEMBERS]\nA\n[FIXED_PRODUCE]\nX 5\nY 1 1\n[UPGRADE]\nU I #POSITION 1 1 #LEVEL x\n[BOGUS]\n",
        );
        assert_eq!(def.issues.len(), 4, "{:?}", def.issues);
        for len in 0..NATION.len() {
            NationDef::parse(&NATION[..len]);
        }
    }

    #[test]
    fn parses_nation_list() {
        let list = NationList::parse(
            b"2\r\nMAY  Maya  MAY.NDS  #TX 143 137  (MA) (MA)\r\nUNIQ Originals UNIQ.nds (un) (UN)\r\n1\r\nX\r\n",
        );
        assert_eq!(list.blocks[0].len(), 2);
        assert_eq!(list.blocks[0][0].tx, Some((143, 137)));
        assert_eq!(list.blocks[0][1].suffixes, ["(un)", "(UN)"]);
        assert_eq!(list.issues.len(), 1);
    }
}
