//! Unit and building definitions (`*.MD` in the archive root, one object type per file).
//!
//! A file is a sequence of commands terminated by `[END]`. Each command is a keyword
//! followed by a keyword-specific argument list (integers and words), parsed with the same
//! whitespace/comment rules as the original reader (see [`super`]). Argument lists may
//! span several lines; list lengths are given by count prefixes.
//!
//! ```text
//! NAME AnmBiz                         object id, referenced by .NDS [MEMBERS]
//! MESSAGE Копейщик                    developer name, CP866
//! USERLC 0 AnmBizC SHADOW -160 -132   sprite set 0 = ANMBIZC.GP
//! #ATTACK 9 2 0 0 0 1                 animation: 9 directions, 2 frames (sprite set, frame)...
//! @MOTION_L 9 1 0 11                  animation: 9 directions, sprite set 1, frames 0..=11
//! LIFE 400
//! PRICE 2 FOOD 50 GOLD 10             count-prefixed list
//! [END]
//! ```
//!
//! After a command's arguments the reader skips the rest of the line, so surplus tokens
//! (`PRICE 1 FOOD 50 GOLD 10` costs only food) are ignored; they are kept in
//! [`Command::ignored`]. [`UnitDef`] keeps every command in file order
//! ([`UnitDef::commands`]); keywords without a known grammar are kept as
//! [`CommandKind::Unknown`] with the rest of their line.
//! `DESCRIPTION\*.MD` files share the extension but are plain CP1252 prose.

use super::{Issue, Token, Tokenizer, Value};
use crate::text::CodePage;

/// Upper bound for count prefixes; the largest in the shipped data is 1 093.
const MAX_COUNT: i32 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    /// Keyword with a known argument grammar.
    Known,
    /// `#NAME` or `@NAME` animation definition, also stored in [`UnitDef::animations`].
    Animation,
    /// Keyword without a known grammar; `args` holds the rest of its line.
    Unknown,
}

/// One command with its arguments, flattened in file order (count prefixes included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub keyword: String,
    pub line: u32,
    pub args: Vec<Value>,
    pub kind: CommandKind,
    /// Tokens after the arguments on the same line, which the engine skips.
    pub ignored: Vec<Value>,
}

impl Command {
    pub fn int(&self, i: usize) -> Option<i32> {
        self.args.get(i).and_then(Value::as_int)
    }

    pub fn word(&self, i: usize) -> Option<&str> {
        self.args.get(i).and_then(Value::as_word)
    }

    /// Reads a count-prefixed list of `(word, int)` pairs starting at argument `at`.
    pub fn word_int_pairs(&self, at: usize) -> Vec<(&str, i32)> {
        let n = self.int(at).unwrap_or(0).max(0) as usize;
        (0..n).filter_map(|i| Some((self.word(at + 1 + 2 * i)?, self.int(at + 2 + 2 * i)?))).collect()
    }

    /// Reads a count-prefixed list of `(x, y)` points starting at argument `at`.
    pub fn points(&self, at: usize) -> Vec<(i32, i32)> {
        let n = self.int(at).unwrap_or(0).max(0) as usize;
        (0..n).filter_map(|i| Some((self.int(at + 1 + 2 * i)?, self.int(at + 2 + 2 * i)?))).collect()
    }
}

/// A sprite pack used by the object (`USERLC index name shadow dx dy`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteSet {
    /// Index referenced by animation frames.
    pub index: i32,
    /// Sprite pack name without extension; the file is `<NAME>.GP` in the archive root.
    pub name: String,
    /// `SHADOW` (any case) or `NONE`.
    pub shadow: String,
    /// Drawing offset of the sprite relative to the object's position.
    pub offset: (i32, i32),
}

impl SpriteSet {
    /// Archive path of the sprite pack.
    pub fn gp_path(&self) -> String {
        format!("{}.GP", self.name.to_ascii_uppercase())
    }
}

/// One animation frame: a frame of a [`SpriteSet`], per direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRef {
    pub sprite: i32,
    pub frame: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimFrames {
    /// `#NAME dirs n (sprite frame)*n`
    List(Vec<FrameRef>),
    /// `@NAME dirs sprite first last`; `last < first` plays backwards.
    Range { sprite: i32, first: i32, last: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    /// Canonical name with `#` prefix, as referenced by other commands (`#MOTION_L`).
    pub name: String,
    /// Number of stored directions (1 for buildings, 9 for most units, 16, 65 for ships).
    pub directions: i32,
    pub frames: AnimFrames,
    pub line: u32,
}

impl Animation {
    /// The frames in playback order.
    pub fn frame_refs(&self) -> Vec<FrameRef> {
        match &self.frames {
            AnimFrames::List(v) => v.clone(),
            &AnimFrames::Range { sprite, first, last } => {
                let frames: Vec<i32> =
                    if first <= last { (first..=last).collect() } else { (last..=first).rev().collect() };
                frames.into_iter().map(|frame| FrameRef { sprite, frame }).collect()
            }
        }
    }

    /// Whether the name is one of the animation slots the engine knows.
    pub fn is_known_name(&self) -> bool {
        ANIMATION_NAMES.contains(&self.name.as_str()) || numbered_slot(&self.name)
    }
}

/// `SOUND frame #ANIMATION SOUNDNAME`: plays a `SOUNDLIST.TXT` entry during an animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimSound {
    pub frame: i32,
    pub animation: String,
    pub sound: String,
}

/// A parsed `.MD` file.
#[derive(Debug, Clone, Default)]
pub struct UnitDef {
    /// Every command in file order, including animations and unknown keywords.
    pub commands: Vec<Command>,
    pub sprites: Vec<SpriteSet>,
    pub animations: Vec<Animation>,
    pub sounds: Vec<AnimSound>,
    /// Whether the terminating `[END]` was found.
    pub ended: bool,
    pub issues: Vec<Issue>,
}

impl UnitDef {
    /// Parses a definition. Never fails; problems are collected in [`UnitDef::issues`].
    pub fn parse(data: &[u8]) -> Self {
        let mut def = UnitDef::default();
        let mut tk = Tokenizer::new(data);
        while let Some(t) = tk.next() {
            if t.is("[END]") {
                def.ended = true;
                let rest = std::iter::from_fn(|| tk.next()).count();
                if rest > 0 {
                    def.issue(t.line, format!("{rest} tokens after [END] ignored"));
                }
                break;
            }
            let keyword = t.text(CodePage::Cp866);
            if let Some(&prefix @ (b'#' | b'@')) = t.bytes.first() {
                def.parse_animation(&mut tk, keyword, prefix == b'@', t.line);
                continue;
            }
            let mut cmd =
                Command { keyword, line: t.line, args: Vec::new(), kind: CommandKind::Known, ignored: Vec::new() };
            let result = match grammar(&cmd.keyword) {
                Some(Grammar::Spec(spec)) => run_spec(&mut tk, spec.as_bytes(), &mut 0, &mut cmd.args),
                Some(Grammar::LineSort) => parse_linesort(&mut tk, &mut cmd.args),
                Some(Grammar::ActivePoint(per_dir)) => def.parse_active_point(&mut tk, per_dir, &mut cmd.args),
                None => {
                    cmd.kind = CommandKind::Unknown;
                    cmd.args = tk.rest_of_line().iter().map(value).collect();
                    Ok(())
                }
            };
            if let Err(e) = result {
                def.issue(t.line, format!("{}: {e}", cmd.keyword));
            }
            cmd.ignored = tk.rest_of_line().iter().map(value).collect();
            def.extract(&cmd);
            def.commands.push(cmd);
        }
        if !def.ended {
            def.issue(tk.line(), "missing [END]".into());
        }
        def
    }

    fn issue(&mut self, line: u32, message: String) {
        self.issues.push(Issue { line, message });
    }

    fn parse_animation(&mut self, tk: &mut Tokenizer, keyword: String, range: bool, line: u32) {
        let name = format!("#{}", &keyword[1..]);
        let mut args = Vec::new();
        let spec: &[u8] = if range { b"iiii" } else { b"i#(ii)" };
        let result = run_spec(tk, spec, &mut 0, &mut args);
        let ints: Vec<i32> = args.iter().filter_map(Value::as_int).collect();
        match result {
            Ok(()) => {
                let frames = if range {
                    AnimFrames::Range { sprite: ints[1], first: ints[2], last: ints[3] }
                } else {
                    AnimFrames::List(ints[2..].chunks(2).map(|c| FrameRef { sprite: c[0], frame: c[1] }).collect())
                };
                self.animations.push(Animation { name, directions: ints[0], frames, line });
            }
            Err(e) => self.issue(line, format!("{keyword}: {e}")),
        }
        let ignored = tk.rest_of_line().iter().map(value).collect();
        self.commands.push(Command { keyword, line, args, kind: CommandKind::Animation, ignored });
    }

    /// `SETACTIVEPOINT #ANIM frame (x y)*dirs`, `SETACTIVEPOINT2` has two points per direction.
    fn parse_active_point(&self, tk: &mut Tokenizer, per_dir: usize, args: &mut Vec<Value>) -> Result<(), String> {
        run_spec(tk, b"si", &mut 0, args)?;
        let anim = args[0].as_word().unwrap_or_default();
        let dirs = self.animation(anim).ok_or_else(|| format!("unknown animation {anim}"))?.directions;
        for _ in 0..dirs.max(0) as usize * per_dir {
            run_spec(tk, b"ii", &mut 0, args)?;
        }
        Ok(())
    }

    fn extract(&mut self, cmd: &Command) {
        match cmd.keyword.as_str() {
            "USERLC" if cmd.args.len() == 5 => self.sprites.push(SpriteSet {
                index: cmd.int(0).unwrap_or_default(),
                name: cmd.word(1).unwrap_or_default().to_owned(),
                shadow: cmd.word(2).unwrap_or_default().to_owned(),
                offset: (cmd.int(3).unwrap_or_default(), cmd.int(4).unwrap_or_default()),
            }),
            "SOUND" if cmd.args.len() == 3 => self.sounds.push(AnimSound {
                frame: cmd.int(0).unwrap_or_default(),
                animation: cmd.word(1).unwrap_or_default().to_owned(),
                sound: cmd.word(2).unwrap_or_default().to_owned(),
            }),
            _ => {}
        }
    }

    /// The last command with this keyword (later commands override earlier ones).
    pub fn get(&self, keyword: &str) -> Option<&Command> {
        self.commands.iter().rev().find(|c| c.keyword == keyword)
    }

    /// All commands with this keyword, in file order.
    pub fn all<'a>(&'a self, keyword: &'a str) -> impl Iterator<Item = &'a Command> + 'a {
        self.commands.iter().filter(move |c| c.keyword == keyword)
    }

    /// Whether a (flag) keyword is present.
    pub fn has(&self, keyword: &str) -> bool {
        self.commands.iter().any(|c| c.keyword == keyword)
    }

    fn word_of(&self, keyword: &str) -> Option<&str> {
        self.get(keyword).and_then(|c| c.word(0))
    }

    fn int_of(&self, keyword: &str) -> Option<i32> {
        self.get(keyword).and_then(|c| c.int(0))
    }

    /// Object id (`NAME`), referenced by nation files.
    pub fn name(&self) -> Option<&str> {
        self.word_of("NAME")
    }

    /// Developer name (`MESSAGE`), usually Russian.
    pub fn message(&self) -> Option<&str> {
        self.word_of("MESSAGE")
    }

    /// Icon id from `ICONLIST.TXT`.
    pub fn icon(&self) -> Option<&str> {
        self.word_of("ICON")
    }

    /// Role from the engine's fixed list (`PEASANT`, `LIGHTINFANTRY`, `CENTER`, ...).
    pub fn usage(&self) -> Option<&str> {
        self.word_of("USAGE")
    }

    pub fn is_building(&self) -> bool {
        self.has("BUILDING")
    }

    /// Hit points.
    pub fn life(&self) -> Option<i32> {
        self.int_of("LIFE")
    }

    /// Cost as `(resource, amount)`; resources are WOOD, GOLD, STONE, FOOD, IRON, COAL.
    pub fn price(&self) -> Vec<(&str, i32)> {
        self.get("PRICE").map(|c| c.word_int_pairs(0)).unwrap_or_default()
    }

    /// Weapons per attack index (`WEAPON idx WEAPONID`, ids from `WEAPON.NDS`).
    pub fn weapons(&self) -> Vec<(i32, &str)> {
        self.all("WEAPON").filter_map(|c| Some((c.int(0)?, c.word(1)?))).collect()
    }

    /// Map piece (`PIECE BUILDINGS\Name.smp`) placed with the building in the editor.
    pub fn piece(&self) -> Option<&str> {
        self.word_of("PIECE")
    }

    pub fn sprite(&self, index: i32) -> Option<&SpriteSet> {
        self.sprites.iter().rev().find(|s| s.index == index)
    }

    /// Looks up an animation by name (`#ATTACK`; a leading `@` is accepted too).
    pub fn animation(&self, name: &str) -> Option<&Animation> {
        let name = name.strip_prefix(['#', '@']).unwrap_or(name);
        self.animations.iter().rev().find(|a| a.name[1..].eq_ignore_ascii_case(name))
    }

    /// Commands whose keyword has no known grammar.
    pub fn unknown(&self) -> impl Iterator<Item = &Command> {
        self.commands.iter().filter(|c| c.kind == CommandKind::Unknown)
    }
}

fn value(t: &Token) -> Value {
    match t.int() {
        Some(v) => Value::Int(v),
        None => Value::Word(t.text(CodePage::Cp866)),
    }
}

enum Grammar {
    /// `i` integer, `s` word, `#(...)` count followed by that many groups.
    Spec(&'static str),
    LineSort,
    ActivePoint(usize),
}

/// Interprets a grammar spec, appending the arguments to `out`.
fn run_spec(tk: &mut Tokenizer, spec: &[u8], at: &mut usize, out: &mut Vec<Value>) -> Result<(), String> {
    while let Some(&c) = spec.get(*at) {
        *at += 1;
        match c {
            b'i' => {
                let t = tk.peek().ok_or("unexpected end of file")?;
                let v = t.int().ok_or_else(|| format!("expected integer, found {:?}", t.text(CodePage::Cp866)))?;
                tk.next();
                out.push(Value::Int(v));
            }
            b's' => {
                let t = tk.next().ok_or("unexpected end of file")?;
                out.push(Value::Word(t.text(CodePage::Cp866)));
            }
            b'#' => {
                run_spec(tk, b"i", &mut 0, out)?;
                let n = out.last().and_then(Value::as_int).unwrap_or_default();
                if !(0..=MAX_COUNT).contains(&n) {
                    return Err(format!("invalid count {n}"));
                }
                let group_start = *at + 1;
                let group_end = group_start + spec[group_start..].iter().position(|&b| b == b')').ok_or("bad spec")?;
                for _ in 0..n {
                    run_spec(tk, &spec[group_start..group_end], &mut 0, out)?;
                }
                *at = group_end + 1;
            }
            _ => return Err("bad spec".into()),
        }
    }
    Ok(())
}

/// `LINESORT #ANIM` followed by any of `GROUND`, `TOP`, `LINE x1 y1 x2 y2`, `POINT x y`.
fn parse_linesort(tk: &mut Tokenizer, out: &mut Vec<Value>) -> Result<(), String> {
    run_spec(tk, b"s", &mut 0, out)?;
    while let Some(t) = tk.peek() {
        let spec: &[u8] = match t.bytes {
            b"GROUND" | b"TOP" => b"s",
            b"LINE" => b"siiii",
            b"POINT" => b"sii",
            _ => break,
        };
        run_spec(tk, spec, &mut 0, out)?;
    }
    Ok(())
}

/// Argument grammars, from the `sscanf` formats in `dmcr.exe` and checked against the data.
fn grammar(keyword: &str) -> Option<Grammar> {
    let spec = match keyword {
        "ANIMAL"
        | "ARCHER"
        | "ARMATTACK"
        | "ARTPODGOTOVKA"
        | "BARABAN"
        | "BREFLECT"
        | "BUILDING"
        | "CANBECAPTUREDWHENFREE"
        | "CANBEKILLEDINSIDE"
        | "CANSHOOT"
        | "CANSTOPBUILD"
        | "CANSTORM"
        | "CANTCAPTURE"
        | "CAPTURE"
        | "COMMANDCENTER"
        | "EVROUNIT"
        | "FARM"
        | "FORMLIKESHOOTERS"
        | "FORWARD"
        | "FRIENDLYFIRE"
        | "FULLROTATION"
        | "HIGHUNIT"
        | "HIGHUNITCANTENTER"
        | "NAEMNIK"
        | "NIKAKIXMAM"
        | "NO25"
        | "NO_HUNGRY"
        | "OFFICER"
        | "PEASANT"
        | "PEASANTCONVERTOR"
        | "PRIEST"
        | "RESSUBST"
        | "SHAMAN"
        | "SHOWDELAY"
        | "SHOWIDLE"
        | "SHOWIDLEPEASANTS"
        | "SLOWDEATH"
        | "SLOWRECHARGE"
        | "SPRITEOBJECT"
        | "STANDGROUND"
        | "TORG"
        | "TURNOFF" => "",
        "ADDSHOTRADIUS"
        | "BUILDSTAGES"
        | "COSTPERCENT"
        | "EVROSHOT"
        | "EXITPAUSE"
        | "FIRELIMIT"
        | "FORCE"
        | "LIFE"
        | "LINEARSORT"
        | "LOWCOSTRADIUS"
        | "MFARM"
        | "NOPAUSEDATTACK"
        | "PEASANTABSORBER"
        | "PROMAX"
        | "PROMAXCTG"
        | "PSIXOZ"
        | "RAZBROS"
        | "ROTATE"
        | "SEARCH_ENEMY_RADIUS"
        | "SEARCH_ENEMY_RADIUS_SHIFT"
        | "SHIELD"
        | "SKILLDAMAGEBONUS"
        | "SKILLDAMAGEMASK"
        | "STOPDISTANCE"
        | "STORMFORCE"
        | "TRANSFORMTOMEAT"
        | "TRANSPORT"
        | "UNITABSORBER"
        | "UNITRADIUS"
        | "USETRANSX"
        | "VES"
        | "VISION"
        | "ZALP" => "i",
        "ADD_ATTACK_RADIUS" | "ATTACK_PAUSE" | "DAMAGE" | "DAMAGEDEC" | "DIRECTTRANS" | "FEARFACTOR" | "FEARRADIUS"
        | "FEARTYPE" | "FISHING" | "INFO" | "MAXINDEPO" | "OPPOSIT" | "PROMAX2" | "RATE" | "WATERCHECKDIST"
        | "ZPOINTS" => "ii",
        "ATTACK_ANGLES" | "ATTACK_RADIUS" | "DET_RADIUS" | "EXDAMAGE" | "GEOMETRY" => "iii",
        "ARTDEPO" | "BUILDBAR" | "IDLE" | "LOCATION" | "RECTANGLE" | "SETANMPARAM" => "iiii",
        "RANGE" => "iiiiii",
        "WEAPONINSECTOR" => "iiiiiii",
        "ADDSECTION" => "iiiiiiiiiiiiiii",
        "BORN_SOUND" | "BREAKANIMATION" | "CATEGORY" | "ICON" | "MEDIA" | "MESSAGE" | "MOTIONSTYLE" | "NAME"
        | "ORDER_SOUND" | "PAGE1" | "PAGE1HINT" | "PAGE2" | "PAGE2HINT" | "PAGE3" | "PAGE3HINT" | "PIECE"
        | "SELECT_SOUND" | "USAGE" => "s",
        "EXPLMEDIA" | "RESCONSUMER" | "TWOPARTS" => "si",
        "AISHIP" | "PORT" => "sii",
        "TAKERESSTAGES" => "siii",
        "EXSPRITES" => "siiii",
        "WEAPON" | "WEAPONKIND" => "is",
        "SOUND" => "iss",
        "USERLC" => "issii",
        "CANKILL" | "MATHERIAL" | "RESOURCEBASE" => "#(s)",
        "ATTMASK" | "DESTRUCT" => "i#(s)",
        "PRICE" | "PORTION" | "PROTECTION" => "#(si)",
        "PRODUCER" => "#(sii)",
        "RASTRATA_NA_VISTREL" => "i#(si)",
        "RASTRATA_NA_VISTREL2" => "ii#(si)",
        "BORNPOINTS" | "BUILDLOCKPOINTS" | "BUILDPOINTS" | "CHECKPOINTS" | "CONCENTRATOR" | "CRAFTPOINTS"
        | "DAMPOINTS" | "FIRES" | "LOCKPOINTS" | "SMOKE" => "#(ii)",
        "EXPLPOINTS" | "STAGELOCKPOINTS" | "STAGE_FIRES" | "STAGE_SMOKE" => "i#(ii)",
        "DOOR" => "#(iiii)",
        "FLAGS" => "i#(iiii)",
        "MULTIWEAPON" | "WAVES" => "iii#(ii)",
        "3DBARS" => "#(iiiii)",
        "EX_SHOTS" => "#(iiiiiiiii)",
        "EX_SHOTS2" => "#(iiiiiiiiii)",
        "LINESORT" => return Some(Grammar::LineSort),
        "SETACTIVEPOINT" => return Some(Grammar::ActivePoint(1)),
        "SETACTIVEPOINT2" => return Some(Grammar::ActivePoint(2)),
        _ => return None,
    };
    Some(Grammar::Spec(spec))
}

/// Animation slots listed in `dmcr.exe` (numbered variants are matched separately).
const ANIMATION_NAMES: &[&str] = &[
    "#SHAR",
    "#SHARBUILD",
    "#MOTION_L",
    "#MOTION_R",
    "#MOTION_LB",
    "#MOTION_RB",
    "#MINISTEP_L",
    "#MINISTEP_R",
    "#MINISTEP_LB",
    "#MINISTEP_RB",
    "#ATTACK",
    "#PATTACK",
    "#UATTACK",
    "#DEATH",
    "#DEATHLIE1",
    "#DEATHLIE2",
    "#DEATHLIE3",
    "#STAND",
    "#PSTAND",
    "#WORK",
    "#FIST",
    "#STANDHI",
    "#STANDLO",
    "#BUILDHI",
    "#BUILD",
    "#DAMAGE",
    "#TRANS01",
    "#TRANS10",
    "#REST",
    "#REST1",
    "#ROTATE_R",
    "#ROTATE_L",
    "#WORKTREE",
    "#WORKSTONE",
    "#WORKFIELD",
    "#BUILDLO_0",
    "#BUILDLO_1",
    "#BUILDLO_2",
    "#BUILDLO_3",
    "#BUILDHI_0",
    "#BUILDHI_1",
    "#BUILDHI_2",
    "#BUILDHI_3",
    "#HIANIMATION",
    "#TRANSX3",
    "#TRANS3X",
];

/// `#ATTACK0`..`#ATTACK10` and the same for the other per-attack-index slots.
fn numbered_slot(name: &str) -> bool {
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let n = &name[base.len()..];
    const BASES: &[&str] = &[
        "#PATTACK",
        "#UATTACK",
        "#ATTACK",
        "#PSTAND",
        "#MOTION_L",
        "#MOTION_R",
        "#MOTION_LB",
        "#MOTION_RB",
        "#MINISTEP_L",
        "#MINISTEP_R",
        "#MINISTEP_LB",
        "#MINISTEP_RB",
    ];
    !n.is_empty() && n.parse::<u32>().is_ok_and(|v| v <= 10) && BASES.contains(&base)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"NAME AnmBiz\r\nMESSAGE \x8a\xae\xaf\xa5\xa9\xe9\xa8\xaa\r\nICON U_Anm\r\n\
USERLC 0 AnmBizC  SHADOW -160 -132 //16 \xf1\xf2\r\nUSERLC 1 AnmBizG shadow 0 0\r\n\
#ATTACK 9 2 0 0 0 1 0 2\r\n/13 0 0 0 1\r\n@MOTION_L  9 1 0 3\r\n@UATTACK 9 1 2 0\r\n\
SETACTIVEPOINT #ATTACK 30 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18\r\n\
PRICE 2 FOOD 50\r\n GOLD 10\r\nLINESORT #ATTACK GROUND LINE 1 2 3 4 POINT 5 6\r\nLIFE 400\r\nBUILDING\r\n\
SOUND 3 #ATTACK HIT\r\nNEWTHING a 1\r\n[END]\r\n";

    #[test]
    fn parses_typed_fields() {
        let def = UnitDef::parse(SAMPLE);
        assert!(def.issues.is_empty(), "{:?}", def.issues);
        assert!(def.ended);
        assert_eq!(def.name(), Some("AnmBiz"));
        assert_eq!(def.message(), Some("Копейщик"));
        assert_eq!(def.icon(), Some("U_Anm"));
        assert_eq!(def.life(), Some(400));
        assert!(def.is_building());
        assert_eq!(def.price(), [("FOOD", 50), ("GOLD", 10)]);
        assert_eq!(def.sprites.len(), 2);
        assert_eq!(def.sprites[0].gp_path(), "ANMBIZC.GP");
        assert_eq!(def.sprites[0].offset, (-160, -132));
        assert_eq!(def.sprite(1).unwrap().shadow, "shadow");
        assert_eq!(def.sounds, [AnimSound { frame: 3, animation: "#ATTACK".into(), sound: "HIT".into() }]);
    }

    #[test]
    fn parses_animations() {
        let def = UnitDef::parse(SAMPLE);
        let attack = def.animation("#ATTACK").unwrap();
        assert_eq!(attack.directions, 9);
        assert_eq!(attack.frame_refs(), [FrameRef { sprite: 0, frame: 0 }, FrameRef { sprite: 0, frame: 1 }]);
        assert_eq!(def.animation("MOTION_L").unwrap().frame_refs().len(), 4);
        let back: Vec<i32> = def.animation("@UATTACK").unwrap().frame_refs().iter().map(|f| f.frame).collect();
        assert_eq!(back, [2, 1, 0]);
        assert!(attack.is_known_name());
        assert_eq!(def.get("SETACTIVEPOINT").unwrap().args.len(), 2 + 18);
        assert_eq!(def.get("LINESORT").unwrap().args.len(), 1 + 1 + 5 + 3);
    }

    #[test]
    fn keeps_unknown_keywords() {
        let def = UnitDef::parse(SAMPLE);
        let unknown: Vec<_> = def.unknown().collect();
        assert_eq!(unknown.len(), 1);
        assert_eq!(unknown[0].keyword, "NEWTHING");
        assert_eq!(unknown[0].args, [Value::Word("a".into()), Value::Int(1)]);
    }

    #[test]
    fn skips_the_rest_of_the_line() {
        let def = UnitDef::parse(SAMPLE);
        let attack = def.commands.iter().find(|c| c.keyword == "#ATTACK").unwrap();
        assert_eq!(attack.ignored, [Value::Int(0), Value::Int(2)]);
        assert_eq!(def.animation("#ATTACK").unwrap().frame_refs().len(), 2);
        let def = UnitDef::parse(b"PRICE 1 FOOD 50 GOLD 10\nBUILDING x\nLIFE 3\n[END]");
        assert_eq!(def.price(), [("FOOD", 50)]);
        assert_eq!(def.get("BUILDING").unwrap().ignored, [Value::Word("x".into())]);
        assert_eq!(def.life(), Some(3));
    }

    #[test]
    fn reports_bad_data_without_panicking() {
        let def = UnitDef::parse(b"LIFE x\nPRICE 99999999 GOLD\n#STAND 9\nSETACTIVEPOINT #NOPE 1 2\n");
        assert_eq!(def.issues.len(), 5, "{:?}", def.issues);
        assert!(!def.ended);
        for len in 0..SAMPLE.len() {
            UnitDef::parse(&SAMPLE[..len]);
        }
    }
}
