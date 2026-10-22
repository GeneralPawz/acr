//! Resource sprites that change: fields (`COMPLEX`) and carcasses (`MEAT`). Each sprite type
//! has an `ObjCharacter` (loader 0x5011af) from its `.RSR`: what it supplies (`[SOURCES]`),
//! what it becomes after so many work cycles (`[WORKTRANSFORM]`) or so much time
//! (`[TIMETRANSFORM]`; `NONE` = it disappears). The sprite timer is `TimeReq::Handle`
//! (0x4d4440), work is `Sprite::PerformWork` (0x4d2cb0). Fields are planted by
//! `CreateObject` (0x5155c5) for `SPRITEOBJECT` types, around mills by 0x51522f; carcasses
//! appear when an animal with `TRANSFORMTOMEAT` has died (0x511f4b). See
//! `docs/simulation.md`.

use acr_formats::m3d::SpriteGroup;
use acr_formats::sprite_list::SpriteList;

use crate::catalog::{TypeId, resource_index};
use crate::fixed::Pos;
use crate::trig::norma;
use crate::world::{FRM_DEC, World};

/// A sprite type's behaviour (`ObjCharacter`, 0x49 bytes, defaults at 0x501559).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjChar {
    /// `[SOURCES]` source type (`ResType`, 0xFF none).
    pub res_type: Option<u8>,
    /// Amount per work cycle (`ResPerWork`, u8).
    pub per_work: i32,
    /// Work radius in px (`WorkRadius`, default 32).
    pub radius: i32,
    /// `[WORKTRANSFORM]`: cycles (`WorkAmount`) and the next type (`WNextObj`, default the
    /// type itself; `None` = removed).
    pub work_amount: i32,
    pub work_next: Option<u16>,
    /// `[TIMETRANSFORM]`: frames (`TimeAmount`) and the next type.
    pub time_amount: i32,
    pub time_next: Option<u16>,
    /// `[INTERNAL_SOURCE]` resource (mine deposits); only deposits keep a spacing radius.
    pub deposit: bool,
    /// The list's radius (`.LST`), for the spacing of new sprites.
    pub list_radius: i32,
}

/// The `ObjCharacter`s of one sprite group, by type index, and their names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct SpriteKinds {
    pub chars: Vec<ObjChar>,
    pub names: Vec<String>,
}

impl SpriteKinds {
    pub fn new(list: &SpriteList, rsr: &acr_formats::defs::SpriteResources) -> Self {
        let names: Vec<String> = list.entries.iter().map(|e| e.name.to_ascii_uppercase()).collect();
        let index = |n: &str| -> Option<u16> {
            if n.eq_ignore_ascii_case("NONE") {
                return None;
            }
            names.iter().position(|x| x.eq_ignore_ascii_case(n)).map(|i| i as u16)
        };
        let chars = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let own = Some(i as u16);
                let src = rsr.work_sources.get(n);
                let (work_amount, work_next) =
                    rsr.work_transform.get(n).map_or((0, own), |(next, a)| (*a, index(next)));
                let (time_amount, time_next) =
                    rsr.time_transform.get(n).map_or((0, own), |(next, a)| (*a, index(next)));
                ObjChar {
                    res_type: src.and_then(|s| resource_index(&s.0)).map(|r| r as u8),
                    per_work: src.map_or(0, |s| i32::from(s.1 as u8)),
                    radius: src.map_or(32, |s| i32::from(s.2 as u8)),
                    work_amount,
                    work_next,
                    time_amount: i32::from(time_amount as u16),
                    time_next,
                    deposit: rsr.sources.contains_key(n),
                    list_radius: list.entries[i].radius,
                }
            })
            .collect();
        Self { chars, names }
    }

    pub fn find(&self, name: &str) -> Option<u16> {
        self.names.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|i| i as u16)
    }
}

/// A field or carcass on the map (a sprite of the `COMPLEX` or `MEAT` group).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResSprite {
    pub group: SpriteGroup,
    /// Current type (`SGIndex`).
    pub index: u16,
    /// World pixels.
    pub x: i32,
    pub y: i32,
    /// Gone (`Enabled` cleared).
    pub removed: bool,
    /// Work cycles done on this type (`WorkOver`, u8).
    pub work_over: u8,
    /// Frames since the last time step (`TimePassed`, u16).
    pub time_passed: u16,
}

impl World {
    /// The `ObjCharacter` of a resource sprite.
    pub fn sprite_char(&self, s: &ResSprite) -> Option<&ObjChar> {
        let kinds = match s.group {
            SpriteGroup::Complex => &self.sprite_kinds.0,
            SpriteGroup::Meat => &self.sprite_kinds.1,
            _ => return None,
        };
        kinds.chars.get(usize::from(s.index))
    }

    /// Adds a resource sprite and returns its index in [`World::sprites`].
    pub fn add_sprite(&mut self, group: SpriteGroup, index: u16, x: i32, y: i32) -> usize {
        self.sprites.push(ResSprite { group, index, x, y, removed: false, work_over: 0, time_passed: 0 });
        self.sprites.len() - 1
    }
}

/// `TimeReq::Handle` (0x4d4440), every tick: each sprite's time advances by `FrmDec`; at its
/// type's `TimeAmount` it becomes the next type (a new type starts with no work done) or
/// disappears.
pub fn time_step(w: &mut World) {
    for k in 0..w.sprites.len() {
        let s = w.sprites[k];
        if s.removed {
            continue;
        }
        let Some(oc) = w.sprite_char(&s).cloned() else { continue };
        let s = &mut w.sprites[k];
        s.time_passed = s.time_passed.wrapping_add(FRM_DEC as u16);
        if i32::from(s.time_passed) >= oc.time_amount {
            match oc.time_next {
                None => s.removed = true,
                Some(next) => {
                    if s.index != next {
                        s.work_over = 0;
                        s.index = next;
                    }
                }
            }
            s.time_passed = 0;
        }
    }
}

/// `Sprite::PerformWork` (0x4d2cb0): one work cycle worth `amount` percent; returns the
/// amount gathered (the `ResPerWork` of the type before any change). After `WorkAmount ·
/// amount / 100` cycles the sprite becomes its next type or disappears.
pub fn perform_work(w: &mut World, k: usize, amount: i32) -> i32 {
    let s = w.sprites[k];
    let Some(oc) = w.sprite_char(&s).cloned() else { return 0 };
    let s = &mut w.sprites[k];
    s.work_over = s.work_over.wrapping_add(1);
    if oc.work_amount * (amount & 0xFFFF) / 100 <= i32::from(s.work_over) {
        match oc.work_next {
            Some(next) => {
                s.index = next;
                s.work_over = 0;
                s.time_passed = 0;
            }
            None => s.removed = true,
        }
    }
    oc.per_work
}

/// The spacing check of a new sprite (0x4d1b80 / 0x4d1aa0): no sprite of the map or of the
/// fields and carcasses may lie closer than `r` plus its own spacing radius (only deposits
/// have one) in the 128 px cells around.
fn spaced(w: &World, x: i32, y: i32, r: i32) -> bool {
    let rx = (r >> 7) + 1;
    let (cx, cy) = (x >> 7, y >> 7);
    let near = |sx: i32, sy: i32| ((sx >> 7) - cx).abs() <= rx && ((sy >> 7) - cy).abs() <= rx;
    for &(ox, oy, r2) in w.spacing.iter() {
        if near(ox, oy) && norma(ox - x, oy - y) < r2 + r {
            return false;
        }
    }
    w.sprites.iter().filter(|s| !s.removed && near(s.x, s.y)).all(|s| {
        let r2 = w.sprite_char(s).filter(|c| c.deposit).map_or(0, |c| c.list_radius);
        norma(s.x - x, s.y - y) >= r2 + r
    })
}

/// The diamond snap of a sprite position to a grid of `g` px (0x514fea).
fn snap(x: i32, y: i32, g: i32) -> (i32, i32) {
    if g == 0 {
        return (x, y);
    }
    let x1 = x - g;
    let (a, b) = ((x1 + y) >> 1, (x1 - y) >> 1);
    let a = (a + (g >> 1)) / g * g;
    let b = (b + (g >> 1)) / g * g;
    (a + b, a - b)
}

/// `CreateObject` (0x5155c5) for a `SPRITEOBJECT` type (a field) at `(x16, y16)` (1/16
/// px): the position snaps to the sprite's grid; the ground there must be free of locks
/// (8 × 15 cells around) and flat (the heights 64 px left, right, above and below differ
/// from their mean by less than 40, 32, 32, 32); a random variant of the first sprite type
/// is planted if no other sprite is too close, and the price is paid (unless `anyway`).
/// The random draws follow the engine's order. True if planted.
pub fn create_sprite_object(w: &mut World, player: u8, kind: TypeId, x16: i32, y16: i32, anyway: bool) -> bool {
    let limit = ((240 << w.map.shift) + 2) << 9;
    if x16 < 1024 || y16 < 1024 || x16 > limit || y16 > limit {
        return false;
    }
    w.random.rando();
    let cost = crate::economy::unit_cost(w, player, kind, Some((x16 >> 4, y16 >> 4)));
    if !anyway {
        // The territory permission (0x45f767) only applies with territories set up.
        if w.players[usize::from(player)].resources.iter().zip(&cost).any(|(s, c)| s < c) {
            return false;
        }
        w.random.rando();
    }
    w.random.rando();
    let Some(ex) = w.catalog.get(kind).economy.ex_sprites.clone() else { return false };
    w.random.rando();
    let Some(first) = w.sprite_kinds.0.find(&ex.name) else { return false };
    let oc = &w.sprite_kinds.0.chars[usize::from(first)];
    // `[DISCRET]` grid, 1 by default (no shipped sprite sets another).
    let _ = oc;
    let (px, py) = snap(x16 >> 4, y16 >> 4, 1);
    let (cx, cy) = (px >> 4, py >> 4);
    if w.map.land.check_bar(cx - 4, cy - 3, 8, 15) {
        return false;
    }
    let h = |x, y| w.map.heights.at(x, y);
    let hs = [h(px - 64, py), h(px + 64, py), h(px, py - 64), h(px, py + 64)];
    let mean = hs.iter().sum::<i32>() >> 2;
    if (hs[0] - mean).abs() >= 40 || hs[1..].iter().any(|&v| (v - mean).abs() >= 32) {
        return false;
    }
    let mut index = first;
    if ex.variants != 0 {
        index += (w.random.rando() % ex.variants) as u16;
    }
    let r = w.sprite_kinds.0.chars.get(usize::from(index)).map_or(0, |c| c.list_radius);
    if !spaced(w, px, py, r) {
        return false;
    }
    w.add_sprite(SpriteGroup::Complex, index, px, py);
    if !anyway {
        let stock = &mut w.players[usize::from(player)].resources;
        for (s, c) in stock.iter_mut().zip(&cost) {
            *s -= c;
        }
    }
    true
}

/// 0x51522f: plants fields of `kind` around `(x16, y16)` on a diamond grid of `(2n+1)²`
/// points `step` px apart, each moved by up to `spread` px at random (`EXSPRITES name
/// variants n step spread`; without `EXSPRITES` a 7 × 7 grid 64 px apart). Each field is
/// paid. Returns the number planted.
pub fn plant_fields(w: &mut World, player: u8, x16: i32, y16: i32, kind: TypeId) -> usize {
    let mut n = 0;
    match w.catalog.get(kind).economy.ex_sprites.clone() {
        Some(ex) => {
            for i in -ex.n..=ex.n {
                for j in -ex.n..=ex.n {
                    let x = x16 + (((i + j) * ex.step) << 4) + ((w.random.rando() * ex.spread) >> 11);
                    let y = y16 + (((i - j) * ex.step) << 4) + ((w.random.rando() * ex.spread) >> 11);
                    n += usize::from(create_sprite_object(w, player, kind, x, y, false));
                }
            }
        }
        None => {
            for i in -3..=3 {
                for j in -3..=3 {
                    let (x, y) = (x16 + ((i + j) << 10), y16 + ((i - j) << 10));
                    n += usize::from(create_sprite_object(w, player, kind, x, y, false));
                }
            }
        }
    }
    n
}

/// The carcass of an animal whose death animation has ended (0x511f4b): a `MEAT` sprite of
/// its `TRANSFORMTOMEAT` type at its position.
pub fn leave_carcass(w: &mut World, i: usize) -> bool {
    let u = &w.units[i];
    let Some(meat) = w.catalog.get(u.kind).economy.meat else { return false };
    if usize::from(meat) >= w.sprite_kinds.1.chars.len() {
        return false;
    }
    let (x, y) = u.pos.pixels();
    w.add_sprite(SpriteGroup::Meat, u16::from(meat), x, y);
    true
}

/// The world position of a sprite as a [`Pos`].
pub fn sprite_pos(s: &ResSprite) -> Pos {
    Pos::from_pixels(s.x, s.y)
}
