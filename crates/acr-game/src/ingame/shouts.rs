//! Panic shouts (0x600bc8): every flight leg of a panicking unit adds one of the eight
//! `FEAR_RNDM0..7` lines of `TEXT\MORALE_TEXT.TXT` 90 px above it, while the life bars are on
//! (Tab, 0x90d411) and the type has no `NIKAKIXMAM`. The text list (0x28f2188) takes a line
//! only where it overlaps no other (0x5fffc0); each drawn frame (0x600320) a line rises one
//! pixel and ages by one, fading in over 16 frames and out after 48, and goes after 80. The
//! original picks the line with the C library's `rand()`, outside the synchronised state.

use crate::ui::draw::Draw;
use crate::ui::fonts::FontId;

/// `FontM13` in colour 2 (0x92dcc8): shouts of the player's side.
pub const FRIEND: FontId = FontId("SmallRedFont");
/// `FontM13` in colour 3 (0x90d8ec): shouts of other units.
pub const OTHER: FontId = FontId("SmallYellowFont");
/// Original frames per simulation tick (`FPSTime` is half a tick).
pub const FRAMES_PER_TICK: u32 = 2;

/// A shout waiting for the interface to lay it out: the point above the unit's head
/// (world-screen pixels) and whether the unit is on the player's side (`NMask`, OB+0x9a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pending {
    pub x: i32,
    pub y: i32,
    pub friend: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Shout {
    /// Left edge and top, world-screen pixels.
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    age: u8,
    text: Vec<u8>,
    font: FontId,
}

#[derive(Debug, Default)]
pub struct Shouts {
    pending: Vec<Pending>,
    list: Vec<Shout>,
    frames: u32,
    seed: u32,
}

impl Shouts {
    /// A unit started a flight leg (0x600c20: `RealX>>4`, `(RealY>>5) − height − 90`).
    pub fn push(&mut self, p: Pending) {
        self.pending.push(p);
    }

    /// `ticks` simulation ticks passed.
    pub fn advance(&mut self, ticks: u32) {
        self.frames += ticks * FRAMES_PER_TICK;
    }

    /// The `rand() & 7` of 0x600bc8 (any generator: it never reaches the simulation).
    fn pick(&mut self) -> usize {
        self.seed = self.seed.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.seed >> 16) & 7) as usize
    }

    /// Lays out the waiting shouts, ages the list and returns its draws (world-screen
    /// pixels). `measure` gives a line's width and height in a font.
    pub fn update(&mut self, lines: &[Vec<u8>; 8], measure: impl Fn(FontId, &[u8]) -> (i32, i32)) -> Vec<Draw> {
        for p in std::mem::take(&mut self.pending) {
            let text = lines[self.pick()].clone();
            let font = if p.friend { FRIEND } else { OTHER };
            let (w, h) = measure(font, &text);
            let s = Shout { x: p.x - w / 2, y: p.y, w, h: h + 1, age: 0, text, font };
            if !self.list.iter().any(|o| overlaps(o, &s)) {
                self.list.push(s);
            }
        }
        for _ in 0..std::mem::take(&mut self.frames) {
            for s in &mut self.list {
                s.y -= 1;
                s.age = s.age.saturating_add(1);
            }
            self.list.retain(|s| s.age <= 80);
        }
        self.list.iter().map(|s| Draw::game_text_faded(s.font, &s.text, s.x, s.y, fade(s.age)).unclipped()).collect()
    }
}

/// Queues the shouts of the last step's flight legs: only with the life bars on (Tab,
/// 0x90d411) and not for `NIKAKIXMAM` types (NM+0x172e bit 5, animals); the colour follows
/// the unit's alliance mask for the local player (OB+0x9a). Units of other players in the
/// fog of war stay silent.
pub fn collect(
    world: &acr_sim::World,
    terrain: &acr_render::terrain::TerrainData,
    objects: &acr_render::objects_view::ObjectsView,
    fog: Option<&acr_sim::fog::Fog>,
    player: u8,
    life_bars: bool,
    shouts: &mut Shouts,
) {
    if !life_bars {
        return;
    }
    for u in shouters(world, objects, fog, player) {
        let (x, y) = (u.pos.x >> 4, u.pos.y >> 4);
        let friend = u.nmask >> (player & 7) & 1 != 0;
        shouts.push(Pending { x, y: (y >> 1) - terrain.height_at(x, y) - 90, friend });
    }
}

/// The units that shout this step and may be shown to `player`.
fn shouters<'a>(
    world: &'a acr_sim::World,
    objects: &'a acr_render::objects_view::ObjectsView,
    fog: Option<&'a acr_sim::fog::Fog>,
    player: u8,
) -> impl Iterator<Item = &'a acr_sim::Unit> {
    world.shouts.iter().filter_map(|&id| world.unit(id)).filter(move |u| {
        let kind = &world.catalog.get(u.kind).id;
        !objects.objects().definition(kind).is_some_and(|d| d.has("NIKAKIXMAM"))
            && !super::interface::hidden_in_fog(fog, world, u, player)
    })
}

/// 0x5fffc0: the boxes' centres are closer than half their summed sizes on both axes.
fn overlaps(a: &Shout, b: &Shout) -> bool {
    let (ax, ay) = (2 * a.x + a.w, 2 * a.y + a.h);
    let (bx, by) = (2 * b.x + b.w, 2 * b.y + b.h);
    (ax - bx).abs() < a.w + b.w && (ay - by).abs() < a.h + b.h
}

/// The fade of 0x6003c5 in colour-table rows: in over the first 16 frames, out after 48.
pub fn fade(age: u8) -> u8 {
    match age {
        0..16 => 2 * (16 - age),
        16..=48 => 0,
        _ => (age - 48).min(31),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines() -> [Vec<u8>; 8] {
        std::array::from_fn(|i| format!("line {i}").into_bytes())
    }

    fn measure(_: FontId, t: &[u8]) -> (i32, i32) {
        (t.len() as i32 * 6, 12)
    }

    #[test]
    fn shouts_rise_fade_and_go() {
        let mut s = Shouts::default();
        s.push(Pending { x: 500, y: 300, friend: true });
        let d = s.update(&lines(), measure);
        assert_eq!((d.len(), d[0].y), (1, 300));
        s.advance(10);
        let d = s.update(&lines(), measure);
        assert_eq!(d[0].y, 280, "one pixel per frame, two frames per tick");
        s.advance(31);
        assert!(s.update(&lines(), measure).is_empty(), "gone after 80 frames");
    }

    #[test]
    fn overlapping_shouts_are_dropped() {
        let mut s = Shouts::default();
        s.push(Pending { x: 500, y: 300, friend: true });
        s.push(Pending { x: 510, y: 305, friend: false });
        s.push(Pending { x: 900, y: 300, friend: false });
        assert_eq!(s.update(&lines(), measure).len(), 2);
    }

    #[test]
    fn shouts_of_strangers_in_the_fog_are_dropped() {
        use acr_sim::Pos;
        use acr_sim::catalog::{Catalog, ObjectType};
        use acr_sim::fog::Fog;
        use acr_sim::formation::Formations;
        use acr_sim::map::MapGrid;
        use std::sync::Arc;
        let mut c = Catalog::new();
        let t = c.add(ObjectType::from_def(
            "Mus(EN)",
            &acr_formats::defs::UnitDef::parse(b"NAME X\r\nLIFE 100\r\nGEOMETRY 1 10 40\r\n[END]\r\n"),
        ));
        let mut w = acr_sim::World::new(Arc::new(MapGrid::open(1)), Arc::new(c), Arc::new(Formations::default()));
        let mine = w.spawn(t, 0, Pos::from_pixels(900, 900), 0);
        let foe = w.spawn(t, 1, Pos::from_pixels(900, 900), 0);
        w.shouts = vec![mine, foe];
        let objects = acr_render::objects_view::ObjectsView::new();
        let dark = Fog::new(w.map.shift);
        let mut open = Fog::new(w.map.shift);
        open.open();
        let owners = |fog| shouters(&w, &objects, fog, 0).map(|u| u.owner).collect::<Vec<_>>();
        assert_eq!(owners(Some(&dark)), [0], "the stranger is in the dark");
        assert_eq!(owners(Some(&open)), [0, 1]);
        assert_eq!(owners(None), [0, 1]);
    }

    #[test]
    fn fades_in_and_out() {
        assert_eq!([0, 8, 16, 48, 60, 80].map(fade), [32, 16, 0, 0, 12, 31]);
    }
}
