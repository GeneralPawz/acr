//! `defs-check` and `unit`: validate and inspect object definitions.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{self, Write};
use std::path::Path;

use acr_formats::defs::md::CommandKind;
use acr_formats::defs::nds::{NationList, Upgrade};
use acr_formats::defs::{Issue, NationDef, NdsFile, Piece, PieceLinks, SoundList, UnitDef, name_list};
use acr_formats::{GpFile, Vfs, normalize_path};

use crate::Result;

/// Counts occurrences and remembers the examples per key.
#[derive(Default)]
struct Tally(BTreeMap<String, (usize, Vec<String>)>);

/// Examples printed per key unless `--all` is given.
const EXAMPLES: usize = 3;

impl Tally {
    fn add(&mut self, key: impl Into<String>, example: impl FnOnce() -> String) {
        let e = self.0.entry(key.into()).or_default();
        e.0 += 1;
        e.1.push(example());
    }

    fn total(&self) -> usize {
        self.0.values().map(|v| v.0).sum()
    }

    fn print(&self, out: &mut impl Write, title: &str, all: bool) -> io::Result<()> {
        writeln!(out, "{title}: {}", self.total())?;
        for (key, (n, ex)) in &self.0 {
            if all {
                writeln!(
                    out,
                    "  {n:>6}  {key}
          {}",
                    ex.join(
                        "
          "
                    )
                )?;
            } else {
                writeln!(out, "  {n:>6}  {key}  e.g. {}", ex[..ex.len().min(EXAMPLES)].join(", "))?;
            }
        }
        Ok(())
    }
}

fn record_issues(tally: &mut Tally, path: &str, issues: &[Issue]) {
    for i in issues {
        // Group by message shape: drop quoted and numeric details.
        let key: String =
            i.message.split(['"', '{']).next().unwrap_or_default().trim_end_matches(char::is_numeric).to_owned();
        tally.add(key, || format!("{path}:{} {}", i.line, i.message));
    }
}

/// Every path of the VFS whose extension matches, sorted.
fn paths_with_ext(vfs: &Vfs, ext: &str) -> Vec<String> {
    let mut v: Vec<String> =
        vfs.list().map(|(p, _)| p.to_owned()).filter(|p| normalize_path(p).ends_with(ext)).collect();
    v.sort();
    v
}

fn is_root(path: &str) -> bool {
    !path.contains(['\\', '/'])
}

/// Frame counts of sprite packs, loaded on demand.
struct GpFrames(HashMap<String, Option<usize>>);

impl GpFrames {
    fn get(&mut self, vfs: &mut Vfs, path: &str) -> Option<usize> {
        *self
            .0
            .entry(path.to_owned())
            .or_insert_with(|| GpFile::parse(vfs.read(path).ok()?).ok().map(|g| g.frame_count()))
    }
}

/// Upper-cased entries of a plain name list (`ICONLIST.TXT`, `NMLIST.TXT`).
fn name_set(vfs: &mut Vfs, path: &str) -> BTreeSet<String> {
    let data = vfs.read(path).unwrap_or_default();
    name_list(&data).iter().map(|n| n.to_ascii_uppercase()).collect()
}

pub fn defs_check(dir: &Path, show_keywords: bool, all: bool) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let mut out = io::stdout().lock();
    let mut issues = Tally::default();
    let mut refs = Tally::default();

    let sounds = SoundList::parse(&vfs.read("SOUNDLIST.TXT")?);
    record_issues(&mut issues, "SOUNDLIST.TXT", &sounds.issues);
    let sound_names: BTreeSet<String> = sounds.sounds.iter().map(|s| s.name.to_ascii_uppercase()).collect();
    let missing_wavs = sounds.sounds.iter().flat_map(|s| &s.files).filter(|f| !vfs.contains(f)).count();
    let wavs: usize = sounds.sounds.iter().map(|s| s.files.len()).sum();
    let icons = name_set(&mut vfs, "ICONLIST.TXT");
    let loaded = name_set(&mut vfs, "NMLIST.TXT");
    let weapons = NdsFile::parse(&vfs.read("WEAPON.NDS")?);
    let weapon_ids: BTreeSet<String> = weapons.weapon_ids().iter().map(|w| w.to_ascii_uppercase()).collect();

    // Unit and building definitions.
    let md_paths = paths_with_ext(&vfs, ".MD");
    let (defs, descriptions): (Vec<_>, Vec<_>) = md_paths.iter().partition(|p| is_root(p));
    let mut keywords: BTreeMap<String, (usize, BTreeSet<String>)> = BTreeMap::new();
    let mut unknown = Tally::default();
    let mut ignored = Tally::default();
    let mut gp = GpFrames(HashMap::new());
    let (mut commands, mut animations, mut frame_refs, mut bad_frames, mut clean) = (0, 0, 0, 0, 0);
    let mut md_names = BTreeSet::new();
    let mut listed_count = 0;
    for path in &defs {
        let def = UnitDef::parse(&vfs.read(path)?);
        let stem = path.trim_end_matches(|c| c != '.').trim_end_matches('.');
        let listed = loaded.contains(&stem.to_ascii_uppercase());
        listed_count += usize::from(listed);
        let tag = if listed { "" } else { " (file not in NMLIST.TXT)" };
        if let Some(name) = def.name().filter(|_| listed) {
            md_names.insert(name.to_ascii_uppercase());
        }
        record_issues(&mut issues, path, &def.issues);
        clean += usize::from(def.issues.is_empty());
        commands += def.commands.len();
        animations += def.animations.len();
        for c in &def.commands {
            let key =
                if c.kind == CommandKind::Animation { format!("{}anim", &c.keyword[..1]) } else { c.keyword.clone() };
            let e = keywords.entry(key).or_default();
            e.0 += 1;
            e.1.insert(stem.to_owned());
            if c.kind == CommandKind::Unknown {
                unknown.add(&c.keyword, || format!("{path}:{}", c.line));
            }
            if !c.ignored.is_empty() {
                let key = if c.kind == CommandKind::Animation { "animation" } else { c.keyword.as_str() };
                ignored.add(key, || format!("{path}:{}", c.line));
            }
        }
        if def.name().is_none_or(|n| !n.eq_ignore_ascii_case(stem)) {
            refs.add(format!("NAME differs from file name{tag}"), || format!("{path}: {:?}", def.name()));
        }
        for a in &def.animations {
            if !a.is_known_name() {
                refs.add(format!("animation name unknown to the engine{tag}"), || format!("{path}: {}", a.name));
            }
            for f in a.frame_refs() {
                frame_refs += 1;
                let Some(set) = def.sprite(f.sprite) else {
                    bad_frames += 1;
                    refs.add(format!("animation uses an undefined USERLC index{tag}"), || {
                        format!("{path}: {} #{}", a.name, f.sprite)
                    });
                    continue;
                };
                let Some(frames) = gp.get(&mut vfs, &set.gp_path()) else { continue };
                let per_dir = frames / a.directions.max(1) as usize;
                if f.frame < 0 || f.frame as usize >= per_dir {
                    bad_frames += 1;
                    refs.add(format!("animation frame beyond the sprite pack{tag}"), || {
                        format!("{path}: {} {} frame {} of {frames}/{}", a.name, set.name, f.frame, a.directions)
                    });
                }
            }
        }
        for s in &def.sprites {
            if gp.get(&mut vfs, &s.gp_path()).is_none() {
                refs.add(format!("USERLC sprite pack missing{tag}"), || format!("{path}: {}", s.name));
            }
        }
        for c in def.commands.iter().filter(|c| c.keyword.ends_with("_SOUND")) {
            if c.word(0).is_some_and(|s| !sound_names.contains(&s.to_ascii_uppercase())) {
                refs.add(format!("sound not in SOUNDLIST.TXT{tag}"), || format!("{path}: {:?}", c.word(0)));
            }
        }
        for s in &def.sounds {
            if !sound_names.contains(&s.sound.to_ascii_uppercase()) {
                refs.add(format!("sound not in SOUNDLIST.TXT{tag}"), || format!("{path}: {}", s.sound));
            }
            if def.animation(&s.animation).is_none() {
                refs.add(format!("SOUND for an undefined animation{tag}"), || format!("{path}: {}", s.animation));
            }
        }
        if def.icon().is_some_and(|i| !icons.contains(&i.to_ascii_uppercase())) {
            refs.add(format!("icon not in ICONLIST.TXT{tag}"), || format!("{path}: {:?}", def.icon()));
        }
        for (_, w) in def.weapons() {
            if !weapon_ids.contains(&w.to_ascii_uppercase()) {
                refs.add(format!("weapon not in WEAPON.NDS{tag}"), || format!("{path}: {w}"));
            }
        }
        if def.piece().is_some_and(|p| !vfs.contains(p)) {
            refs.add(format!("PIECE file missing{tag}"), || format!("{path}: {:?}", def.piece()));
        }
    }
    let unknown_cmds = unknown.total();
    writeln!(
        out,
        "MD   {} definitions: {commands} commands, {} keywords, {unknown_cmds} unknown",
        defs.len(),
        keywords.len()
    )?;
    writeln!(out, "     {animations} animations, {frame_refs} frame references, {bad_frames} unresolved")?;
    writeln!(out, "     {listed_count} listed in NMLIST.TXT, {} descriptions (CP1252 text)", descriptions.len())?;

    // Nations.
    let nds_paths = paths_with_ext(&vfs, ".NDS");
    let mut members = BTreeSet::new();
    let (mut upgrades, mut nations) = (0, 0);
    let mut categories: BTreeMap<String, usize> = BTreeMap::new();
    for path in &nds_paths {
        let nds = NdsFile::parse(&vfs.read(path)?);
        if path.eq_ignore_ascii_case("WEAPON.NDS") {
            continue;
        }
        nations += 1;
        let def = NationDef::from_nds(&nds);
        record_issues(&mut issues, path, &def.issues);
        upgrades += def.upgrades.len();
        let ids: BTreeSet<String> = def.members.iter().map(|m| m.id.to_ascii_uppercase()).collect();
        let upgrade_ids: BTreeSet<String> = def.upgrades.iter().map(|u| u.id.to_ascii_uppercase()).collect();
        for m in &def.members {
            members.insert(m.id.to_ascii_uppercase());
            if !md_names.contains(&m.md.to_ascii_uppercase()) {
                refs.add("member NAME not defined by a listed .MD", || format!("{path}: {} -> {}", m.id, m.md));
            }
        }
        let known = |id: &str| ids.contains(&id.to_ascii_uppercase()) || upgrade_ids.contains(&id.to_ascii_uppercase());
        for p in &def.fixed_produce {
            for id in std::iter::once(&p.id).chain(p.items.iter().map(|i| &i.id)) {
                if !known(id) {
                    refs.add("[FIXED_PRODUCE] id not a member", || format!("{path}: {id}"));
                }
            }
        }
        for u in &def.upgrades {
            *categories.entry(u.category().unwrap_or("(none)").to_owned()).or_default() += 1;
            check_upgrade(&mut refs, path, u, &known, &icons);
        }
        for l in &def.upgrade_place {
            for id in &l.items {
                if !upgrade_ids.contains(&id.to_ascii_uppercase()) {
                    refs.add("[UPGRADEPLACE] upgrade not defined", || format!("{path}: {id}"));
                }
            }
        }
    }
    let lst = NationList::parse(&vfs.read("NATIONS.LST")?);
    record_issues(&mut issues, "NATIONS.LST", &lst.issues);
    for e in lst.blocks.iter().flatten() {
        if !vfs.contains(&e.file) {
            refs.add("NATIONS.LST file missing", || e.file.clone());
        }
    }
    writeln!(
        out,
        "NDS  {nations} nation files: {} members, {upgrades} upgrades; WEAPON.NDS: {} weapons",
        members.len(),
        weapon_ids.len()
    )?;
    let blocks: Vec<String> = lst.blocks.iter().map(|b| b.len().to_string()).collect();
    writeln!(out, "     NATIONS.LST blocks of {} nations", blocks.join(" + "))?;
    writeln!(out, "SND  {} sounds, {wavs} files, {missing_wavs} missing", sounds.sounds.len())?;

    // Map pieces.
    let loose: BTreeSet<String> = vfs.list().filter(|(_, s)| *s == "<loose>").map(|(p, _)| p.to_owned()).collect();
    let (mut pieces, mut loose_pieces, mut objects, mut bugged) = (0, 0, 0, 0);
    for path in paths_with_ext(&vfs, ".SMP") {
        let p = Piece::parse(&vfs.read(&path)?);
        record_issues(&mut issues, &path, &p.issues);
        pieces += 1;
        loose_pieces += usize::from(loose.contains(&path));
        bugged += usize::from(p.lock_size_bug);
        for o in &p.objects {
            objects += 1;
            if !members.contains(&o.id.to_ascii_uppercase()) {
                refs.add("SMP object not a nation member", || format!("{path}: {}", o.id));
            }
        }
    }
    let lnk = paths_with_ext(&vfs, ".SMP.LNK");
    for path in &lnk {
        record_issues(&mut issues, path, &PieceLinks::parse(&vfs.read(path)?).issues);
    }
    let loose_lnk = lnk.iter().filter(|p| loose.contains(*p)).count();
    writeln!(
        out,
        "SMP  {pieces} pieces ({loose_pieces} loose), {objects} placed objects, {bugged} with the LOCK size bug"
    )?;
    writeln!(out, "LNK  {} piece link files ({loose_lnk} loose)", lnk.len())?;
    writeln!(out, "MD   {clean} of {} definitions without parse issues", defs.len())?;

    writeln!(out)?;
    issues.print(&mut out, "parse issues", all)?;
    unknown.print(&mut out, "unknown MD keywords", all)?;
    ignored.print(&mut out, "MD commands with surplus tokens (skipped like the engine does)", all)?;
    refs.print(&mut out, "unresolved references", all)?;
    if show_keywords {
        writeln!(out, "\n| keyword | commands | files |\n|---|---:|---:|")?;
        for (k, (n, files)) in &keywords {
            writeln!(out, "| `{k}` | {n} | {} |", files.len())?;
        }
        writeln!(out, "\n| upgrade effect | upgrades |\n|---|---:|")?;
        for (k, n) in &categories {
            writeln!(out, "| `{k}` | {n} |")?;
        }
    }
    Ok(())
}

fn check_upgrade(refs: &mut Tally, path: &str, u: &Upgrade, known: &dyn Fn(&str) -> bool, icons: &BTreeSet<String>) {
    for t in u.targets() {
        if !known(t) {
            refs.add("upgrade target not a member", || format!("{path}: {} -> {t}", u.id));
        }
    }
    if !icons.contains(&u.icon.to_ascii_uppercase()) {
        refs.add("upgrade icon not in ICONLIST.TXT", || format!("{path}: {} {}", u.id, u.icon));
    }
}

/// Pretty-prints one unit or building definition with its resolved sprite packs.
pub fn unit(dir: &Path, name: &str) -> Result<()> {
    let mut vfs = Vfs::open_game_dir(dir)?;
    let name = name.trim_end_matches(".MD").trim_end_matches(".md");
    let (file, def) = find_unit(&mut vfs, name)?;
    let mut out = io::stdout().lock();
    let opt = |s: Option<&str>| s.unwrap_or("-").to_owned();
    writeln!(out, "{file}: {} ({})", opt(def.name()), opt(def.message()))?;
    writeln!(out, "  icon {}  usage {}  life {}", opt(def.icon()), opt(def.usage()), def.life().unwrap_or(0))?;
    let price: Vec<String> = def.price().iter().map(|(r, n)| format!("{r} {n}")).collect();
    let price = if price.is_empty() { "-".to_owned() } else { price.join(", ") };
    writeln!(out, "  {}  price: {price}", if def.is_building() { "building" } else { "unit" })?;
    let weapons: Vec<String> = def.weapons().iter().map(|(i, w)| format!("{i}:{w}")).collect();
    if !weapons.is_empty() {
        writeln!(out, "  weapons: {}", weapons.join(", "))?;
    }
    if let Some(p) = def.piece() {
        writeln!(out, "  piece: {p}{}", if vfs.contains(p) { "" } else { " (missing)" })?;
    }

    let mut nations = Vec::new();
    for path in paths_with_ext(&vfs, ".NDS") {
        let nation = NationDef::parse(&vfs.read(&path)?);
        for m in nation.members.iter().filter(|m| def.name().is_some_and(|n| m.md.eq_ignore_ascii_case(n))) {
            nations.push(format!("{} ({path})", m.id));
        }
    }
    writeln!(out, "  member of: {}", if nations.is_empty() { "-".to_owned() } else { nations.join(", ") })?;

    writeln!(out, "\nsprite sets")?;
    let mut gp = GpFrames(HashMap::new());
    for s in &def.sprites {
        let frames = gp.get(&mut vfs, &s.gp_path()).map_or("missing".to_owned(), |n| format!("{n} frames"));
        writeln!(out, "  {:>2}  {:<16} {:<14} {} offset {:?}", s.index, s.name, s.gp_path(), frames, s.offset)?;
        if s.shadow != "SHADOW" {
            writeln!(out, "      shadow mode {}", s.shadow)?;
        }
    }
    writeln!(out, "\nanimations (directions x frames, sprite set: frames)")?;
    for a in &def.animations {
        let refs = a.frame_refs();
        let mut by_set: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
        for f in &refs {
            by_set.entry(f.sprite).or_default().push(f.frame);
        }
        let sets: Vec<String> = by_set
            .iter()
            .map(|(set, frames)| {
                let name = def.sprite(*set).map_or("?", |s| s.name.as_str());
                format!("{name}: {}", frame_ranges(frames))
            })
            .collect();
        writeln!(out, "  {:<14} {:>2} x {:<3} {}", a.name, a.directions, refs.len(), sets.join("; "))?;
    }
    if !def.sounds.is_empty() {
        writeln!(out, "\nsounds")?;
        for s in &def.sounds {
            writeln!(out, "  {} frame {}: {}", s.animation, s.frame, s.sound)?;
        }
    }
    writeln!(out, "\nother commands")?;
    let skip = ["NAME", "MESSAGE", "ICON", "USERLC", "SOUND", "PRICE", "LIFE", "USAGE", "WEAPON", "PIECE", "BUILDING"];
    for c in def.commands.iter().filter(|c| c.kind != CommandKind::Animation && !skip.contains(&c.keyword.as_str())) {
        let args: Vec<String> = c.args.iter().take(12).map(|a| a.to_string()).collect();
        let more = if c.args.len() > 12 { format!(" ... ({} values)", c.args.len()) } else { String::new() };
        let tag = if c.kind == CommandKind::Unknown { " [unknown]" } else { "" };
        writeln!(out, "  {}{tag} {}{more}", c.keyword, args.join(" ").trim_end())?;
    }
    for c in def.commands.iter().filter(|c| !c.ignored.is_empty()) {
        let ignored: Vec<String> = c.ignored.iter().map(|a| a.to_string()).collect();
        writeln!(out, "note: line {} {}: surplus tokens ignored: {}", c.line, c.keyword, ignored.join(" "))?;
    }
    for i in &def.issues {
        writeln!(out, "issue: {i}")?;
    }
    Ok(())
}

/// Finds a definition by file name or, failing that, by its `NAME`.
fn find_unit(vfs: &mut Vfs, name: &str) -> Result<(String, UnitDef)> {
    let file = format!("{}.MD", name.to_ascii_uppercase());
    if vfs.contains(&file) {
        return Ok((file.clone(), UnitDef::parse(&vfs.read(&file)?)));
    }
    for path in paths_with_ext(vfs, ".MD").into_iter().filter(|p| is_root(p)) {
        let def = UnitDef::parse(&vfs.read(&path)?);
        if def.name().is_some_and(|n| n.eq_ignore_ascii_case(name)) {
            return Ok((path, def));
        }
    }
    Err(format!("no definition with file name or NAME {name}").into())
}

/// Formats frame numbers compactly: `0-11, 14, 12-0`.
fn frame_ranges(frames: &[i32]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < frames.len() {
        let step = frames.get(i + 1).map(|n| n - frames[i]).filter(|d| d.abs() == 1);
        let mut j = i;
        while let Some(d) = step {
            if frames.get(j + 1) != Some(&(frames[j] + d)) {
                break;
            }
            j += 1;
        }
        parts.push(if j > i { format!("{}-{}", frames[i], frames[j]) } else { frames[i].to_string() });
        i = j + 1;
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    #[test]
    fn frame_ranges_compacts_runs() {
        assert_eq!(super::frame_ranges(&[0, 1, 2, 5, 4, 3, 9]), "0-2, 5-3, 9");
    }
}
