//! Projectile types (`WEAPON.NDS`, loader 0x60b390) with the frame counts of their
//! animations (`WEAPON.ADS`) and the explosion media of the terrain textures
//! (`TEXTURES.LST`). See `projectile.rs` and `docs/simulation.md`.

use std::collections::BTreeMap;

use acr_formats::defs::{NdsFile, UnitDef};

/// Explosion media (`NRES.DAT [MEDIA]` order, lookup 0x4eb020).
pub mod media {
    pub const DEFAULT: i32 = 0;
    pub const WATER: i32 = 1;
    pub const BUILDING: i32 = 9;
    pub const FLASH: i32 = 11;
    pub const NAMES: [&str; 12] = [
        "DEFAULT",
        "WATER",
        "STONE",
        "GRASS",
        "IRON",
        "WOOD",
        "FOOD",
        "WOOD&STONE",
        "BOAT",
        "BUILDING",
        "STENA",
        "FLASH",
    ];

    /// The id of a media name (case-insensitive).
    pub fn id(name: &str) -> Option<i32> {
        NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|i| i as i32)
    }
}

/// How a projectile moves (Weapon+0x57, jump table 0x558d40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Propagation {
    Stand,
    SlightUp,
    Random,
    Fly,
    Immediate,
    /// A ballistic arc at `Speed` degrees elevation.
    Angle,
    /// Like `Random` (but not stopped by buildings).
    Random1,
    /// A bounce off the ground with restitution `Speed/64`.
    Reflect,
}

impl Propagation {
    fn parse(w: &str) -> Self {
        match w.to_ascii_uppercase().as_str() {
            "SLIGHTUP" => Self::SlightUp,
            "RANDOM" => Self::Random,
            "FLY" => Self::Fly,
            "IMMEDIATE" => Self::Immediate,
            "ANGLE" => Self::Angle,
            "RANDOM1" => Self::Random1,
            "REFLECT" => Self::Reflect,
            _ => Self::Stand,
        }
    }
}

/// Children of an explosion: `n = min + ((max − min)·rando() >> 15)` picks from `list`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Children {
    pub list: Vec<u16>,
    /// The file's count (the engine indexes with it even beyond the four stored).
    pub count: i32,
    pub min: i32,
    pub max: i32,
}

/// One weapon (`Weapon`, 0x5d bytes).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Weapon {
    pub name: String,
    /// Frames of its animation (`anim+1`), which time its life.
    pub frames: i32,
    /// `[CHILDWEAPON]` (+0x04..+0x16).
    pub children: Children,
    /// `[CUSTOMEXPLOSION]` per media (+0x18): the last matching record wins.
    pub custom: Vec<(i32, Children)>,
    /// `[TILEWEAPON]`: trail weapons and how many per tick (+0x21, +0x31).
    pub trail: Vec<u16>,
    pub trail_count: i32,
    /// +0x36: the frame at which a `Times`-1 object explodes (255: at creation).
    pub hot_frame: i32,
    /// `[SYNCWEAPON]` (+0x37): created all at once at the explosion.
    pub sync: Vec<u16>,
    /// +0x49 / +0x4b: round damage and its radius in 1/16 px.
    pub damage: i32,
    pub radius: i32,
    /// +0x4d: speed (px per frame), elevation (`Angle`) or restitution (`Reflect`).
    pub speed: i32,
    /// +0x4f: animation cycles (−1 = forever).
    pub times: i32,
    /// `[DAMAGEHEIGHT]` (+0x51): below this height the object hurts every tick.
    pub damage_height: i32,
    /// `[DETONATOR]` (+0x53): corpses fly away from the blast.
    pub detonator: i32,
    /// +0x56: gravity index (NO 0, LO 1, HI 2, HI1 3, HI2 4, HI3 5).
    pub gravity: i32,
    pub propagation: Propagation,
    /// `[FULLPARENT]` (+0x58 bit0): damage and target come from the shooter.
    pub full_parent: bool,
}

/// All weapons of `WEAPON.NDS`, by index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Weapons {
    pub list: Vec<Weapon>,
    by_name: BTreeMap<String, u16>,
    /// Explosion media of each terrain texture (`TEXTURES.LST`, `FORSTRELA i # MEDIA`).
    pub texture_media: Vec<i32>,
}

fn gravity(w: &str) -> i32 {
    match w.to_ascii_uppercase().as_str() {
        "LO_GRAVITY" => 1,
        "HI_GRAVITY" => 2,
        "HI_GRAVITY1" => 3,
        "HI_GRAVITY2" => 4,
        "HI_GRAVITY3" => 5,
        _ => 0,
    }
}

impl Weapons {
    /// Reads `WEAPON.NDS` with the animations of `WEAPON.ADS` and `TEXTURES.LST`.
    pub fn parse(nds: &[u8], ads: Option<&[u8]>, textures: Option<&[u8]>) -> Self {
        let frames: BTreeMap<String, i32> = ads
            .map(|a| {
                UnitDef::parse(a)
                    .animations
                    .iter()
                    .map(|an| (an.name.to_ascii_uppercase(), an.frame_refs().len() as i32))
                    .collect()
            })
            .unwrap_or_default();
        let file = NdsFile::parse(nds);
        let mut ws = Weapons::default();
        if let Some(members) = file.section("MEMBERS") {
            for l in &members.lines {
                let t = &l.tokens;
                if t.len() < 9 {
                    continue;
                }
                let int = |i: usize| t[i].parse::<i32>().unwrap_or(0);
                let anim = t[1].to_ascii_uppercase();
                let w = Weapon {
                    name: t[0].clone(),
                    frames: frames.get(&anim).copied().unwrap_or(1),
                    children: Children::default(),
                    custom: Vec::new(),
                    trail: Vec::new(),
                    trail_count: 0,
                    hot_frame: 0,
                    sync: Vec::new(),
                    damage: i32::from(int(2) as u16),
                    radius: i32::from((int(3) << 4) as i16),
                    speed: i32::from(int(7) as i16),
                    times: i32::from(int(8) as i16),
                    damage_height: 0,
                    detonator: 0,
                    gravity: gravity(&t[5]),
                    propagation: Propagation::parse(&t[6]),
                    full_parent: false,
                };
                let id = ws.list.len() as u16;
                ws.by_name.entry(w.name.to_ascii_uppercase()).or_insert(id);
                ws.list.push(w);
            }
        }
        let find = |ws: &Weapons, n: &str| ws.find(n);
        // The sections in file order: later lines overwrite the hot frame.
        for s in &file.sections {
            for l in &s.lines {
                let t = &l.tokens;
                let Some(id) = t.first().and_then(|n| find(&ws, n)) else { continue };
                let int = |i: usize| t.get(i).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
                let list = |from: usize, n: i32| -> Vec<u16> {
                    t.iter().skip(from).take(n.clamp(0, 4) as usize).filter_map(|n| find(&ws, n)).collect()
                };
                match s.name.to_ascii_uppercase().as_str() {
                    "CHILDWEAPON" => {
                        let ch =
                            Children { list: list(5, int(4)), count: int(4).clamp(0, 4), min: int(1), max: int(2) };
                        let w = &mut ws.list[usize::from(id)];
                        w.children = ch;
                        w.hot_frame = int(3) & 0xFF;
                    }
                    "CUSTOMEXPLOSION" => {
                        let m = t.get(1).and_then(|m| media::id(m)).unwrap_or(0);
                        let ch =
                            Children { list: list(6, int(5)), count: int(5).clamp(0, 4), min: int(2), max: int(3) };
                        let w = &mut ws.list[usize::from(id)];
                        w.custom.push((m, ch));
                        w.hot_frame = int(4) & 0xFF;
                    }
                    "TILEWEAPON" => {
                        let tr = list(3, int(2));
                        let w = &mut ws.list[usize::from(id)];
                        (w.trail, w.trail_count) = (tr, i32::from(int(1) as u16));
                    }
                    "SYNCWEAPON" => {
                        let sy = list(2, int(1));
                        let w = &mut ws.list[usize::from(id)];
                        (w.sync, w.hot_frame) = (sy, 0);
                    }
                    "FULLPARENT" => {
                        for n in t {
                            if let Some(k) = find(&ws, n) {
                                ws.list[usize::from(k)].full_parent = true;
                            }
                        }
                    }
                    "DAMAGEHEIGHT" => ws.list[usize::from(id)].damage_height = int(1),
                    "DETONATOR" => ws.list[usize::from(id)].detonator = int(1),
                    _ => {}
                }
            }
        }
        if let Some(tx) = textures {
            for line in String::from_utf8_lossy(tx).lines() {
                let w: Vec<&str> = line.split_whitespace().collect();
                if let [k, i, "#", m, ..] = w.as_slice()
                    && k.eq_ignore_ascii_case("FORSTRELA")
                    && let (Ok(i), Some(m)) = (i.parse::<usize>(), media::id(m))
                    && i < 256
                {
                    if ws.texture_media.len() <= i {
                        ws.texture_media.resize(i + 1, media::DEFAULT);
                    }
                    ws.texture_media[i] = m;
                }
            }
        }
        ws
    }

    /// A weapon by name (case-insensitive).
    pub fn find(&self, name: &str) -> Option<u16> {
        self.by_name.get(&name.to_ascii_uppercase()).copied()
    }

    pub fn get(&self, id: u16) -> &Weapon {
        &self.list[usize::from(id)]
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NDS: &[u8] = b"[MEMBERS]\r\nSHOTFOG #SHOTFOG 0 0 NONE NO_GRAVITY STAND 1 1\r\n\
        ENDSHOT #POINT 0 10 NONE NO_GRAVITY FLY 1500 -1\r\nPPOINTT #POINT 0 0 NONE NO_GRAVITY SLIGHTUP 0 1\r\n\
        NUCL2P #NUCL1 340 35 NONE HI_GRAVITY ANGLE 10 -1\r\nSKOKAT #NUCL1 500 16 NONE HI_GRAVITY REFLECT 32 -1\r\n\
        [CHILDWEAPON]\r\nSHOTFOG 1 1 255 1 ENDSHOT\r\nNUCL2P 1 1 0 2 SKOKAT SKOKAT\r\n[FULLPARENT]\r\nENDSHOT\r\nSHOTFOG\r\n\
        [CUSTOMEXPLOSION]\r\nSKOKAT WATER 1 1 0 1 ENDSHOT\r\n[SYNCWEAPON]\r\nPPOINTT 1 NUCL2P\r\n[DAMAGEHEIGHT]\r\nSKOKAT 95\r\n[END]\r\n";

    #[test]
    fn parses_weapons_and_their_relations() {
        let ads = b"#SHOTFOG 1 8 0 0 0 1 0 2 0 3 0 4 0 5 0 6 0 7\r\n";
        let ws = Weapons::parse(NDS, Some(ads), Some(b"FORSTRELA 3 # FLASH\r\n"));
        let shot = ws.get(ws.find("shotfog").unwrap());
        assert_eq!((shot.frames, shot.hot_frame, shot.full_parent), (8, 255, true));
        assert_eq!(shot.children.list, [ws.find("ENDSHOT").unwrap()]);
        let n = ws.get(ws.find("NUCL2P").unwrap());
        assert_eq!((n.damage, n.radius, n.gravity, n.propagation, n.speed), (340, 35 * 16, 2, Propagation::Angle, 10));
        let sk = ws.get(ws.find("SKOKAT").unwrap());
        assert_eq!((sk.damage_height, sk.custom[0].0), (95, media::WATER));
        assert_eq!(ws.get(ws.find("PPOINTT").unwrap()).sync, [ws.find("NUCL2P").unwrap()]);
        assert_eq!(ws.texture_media, [0, 0, 0, media::FLASH]);
    }
}
