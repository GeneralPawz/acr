//! Pasting map pieces (`*.SMP`) into the map.
//!
//! Two pasters exist: the direct one (0x45AF97) used for pieces around the start points
//! adds heights; the buffered one (0x467E99) used for chains of linked pieces keeps the
//! maximum height (or minimum for `LOW_` pieces) and keeps only the sprites that stand on
//! the piece's own ground.

use acr_formats::m3d::SpriteGroup;

use super::Gen;
use super::territory::norm;

fn group(code: [u8; 2]) -> Option<SpriteGroup> {
    match &code {
        b"AG" => Some(SpriteGroup::Trees),
        b"ST" => Some(SpriteGroup::Stones),
        b"HO" => Some(SpriteGroup::Holes),
        b"CO" => Some(SpriteGroup::Complex),
        _ => None,
    }
}

/// Height rule of the buffered paster.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    Max,
    Min,
}

impl Gen<'_> {
    /// Direct paste at a world position (0x45AF97).
    pub fn paste(&mut self, path: &str, wx: i32, wy: i32) {
        let Some(file) = self.piece(path) else { return };
        let p = &file.piece;
        // VERT (0x45869C): additive.
        let (x0, y0) = ((wx >> 6) * 2, (wy >> 6) * 2);
        for v in &p.vertices {
            let (x, y) = (i32::from(v.x) + x0, i32::from(v.y) + y0);
            if x <= 0 || y <= 0 || x >= self.vw as i32 - 1 || y >= self.vh as i32 - 1 {
                continue;
            }
            let i = y as usize * self.vw + x as usize;
            self.heights[i] = self.heights[i].wrapping_add(v.value);
            if v.texture != 0 && usize::from(v.texture) < self.texture_count {
                self.tiles[i] = v.texture.wrapping_add(0x80);
            }
        }
        // SPRT (0x459165).
        let (sx, sy) = (wx & !63, wy & !63);
        let lim = (self.sea << 5) - 32;
        for o in &p.sprites {
            let (x, y) = (o.x + sx, o.y + sy);
            if x > 0
                && y > 0
                && x < lim
                && y < lim
                && let Some(g) = group(o.group)
            {
                self.add_sprite(g, x, y, o.sprite);
            }
        }
        self.paste_locks(p, wx, wy);
        self.paste_objects(p, wx & !63, wy & !63);
        let (px, py) = (wx & !63, (wy >> 6) << 5);
        let base = (self.height_at(px, py) + 16) >> 5;
        self.paint(p, px >> 5, py >> 5, base);
    }

    /// Ground paint of a piece's `NRG1` chunk (0x5F1866): cells nearer to the viewer
    /// (larger depth) than what is painted already replace the `UORG` values.
    fn paint(&mut self, p: &acr_formats::Piece, x: i32, y: i32, base: i32) {
        let h = p.nrg_header.map(|v| i32::from(v as i16));
        let (cols, rows) = (h[0], h[1]);
        if cols <= 0 || rows <= 0 || p.nrg.len() < (cols * rows * 5) as usize {
            return;
        }
        let (dx, dy) = (h[4] - x, h[5] - y - base);
        let w = 0x100i32 << self.s;
        let (zw, zh, zs) = (0x200i32 << self.s, 0x100i32 << self.s, self.s + 9);
        let zat = |cx: i32, cy: i32| ((cy.clamp(0, zh - 1) << zs) + cx.clamp(0, zw - 1)) as usize;
        let cells: Vec<(i32, i32, i8, i32, i32)> = (0..cols)
            .flat_map(|i| (0..rows).map(move |j| (i, j)))
            .enumerate()
            .map(|(k, (i, j))| {
                let e = &p.nrg[k * 5..k * 5 + 5];
                let t = i32::from(i16::from_le_bytes([e[1], e[2]]));
                let z = i32::from(i16::from_le_bytes([e[3], e[4]])) + (y << 6);
                let v = ((t >> 6) << 6) + (((t & 7) + dx) & 7) + (((((t & 0x3F) >> 3) + dy) & 7) << 3);
                (x + i - h[2], y + j - h[3], e[0] as i8, v, z)
            })
            .collect();
        for &(cx, cy, b, v, z) in &cells {
            if z - self.paint_depth[zat(cx, cy)] > 0 {
                let idx = (cy - base) * w + cx;
                if idx >= 0 && (idx as usize) < self.ground8.len() && b > 0 {
                    self.ground8[idx as usize] = b as u8;
                    self.ground16[idx as usize] = v as u16;
                }
            }
        }
        for &(cx, cy, _, _, z) in &cells {
            let i = zat(cx, cy);
            if z - self.paint_depth[i] > 0 {
                self.paint_depth[i] = z;
            }
        }
    }

    fn paste_locks(&mut self, p: &acr_formats::Piece, wx: i32, wy: i32) {
        let (cx, cy) = (wx >> 6, wy >> 6);
        for &(x, y) in &p.lock[0] {
            self.locked.push(((i32::from(x) + cx) as u16, (i32::from(y) + cy) as u16));
        }
        for &(x, y) in &p.lock[1] {
            self.unlocked.push(((i32::from(x) + cx) as u16, (i32::from(y) + cy) as u16));
        }
    }

    fn paste_objects(&mut self, p: &acr_formats::Piece, bx: i32, by: i32) {
        let owner = self.owner;
        for o in &p.objects {
            self.add_unit(owner, bx + o.x, by + o.y, &o.id);
        }
    }

    /// Buffered paste (0x467E99, 0x4675F3).
    pub fn paste_blend(&mut self, path: &str, wx: i32, wy: i32, blend: Blend) {
        let Some(file) = self.piece(path) else { return };
        let p = &file.piece;
        let (x0, y0) = ((wx >> 6) * 2, (wy >> 6) * 2);
        let mut touched = Vec::new();
        for v in &p.vertices {
            let (x, y) = (i32::from(v.x) + x0, i32::from(v.y) + y0);
            if x <= 0 || y <= 0 || x >= self.vw as i32 - 1 || y >= self.vh as i32 - 1 {
                continue;
            }
            let i = y as usize * self.vw + x as usize;
            let cur = self.heights[i];
            let take = match blend {
                Blend::Max => cur <= v.value,
                Blend::Min => cur >= v.value,
            };
            if !take {
                continue;
            }
            self.heights[i] = v.value;
            if v.texture != 0 && usize::from(v.texture) < self.texture_count {
                self.tiles[i] = v.texture.wrapping_add(0x80);
            }
            if touched.len() < 0x2000 {
                touched.push(i);
            }
        }
        // SPRT (0x4679C2): only where this piece's ground won, marked by a 0x800 lift.
        for &i in &touched {
            self.heights[i] = self.heights[i].wrapping_add(0x800);
        }
        let (sx, sy) = (wx & !63, wy & !63);
        let lim = (self.sea << 5) - 32;
        for o in &p.sprites {
            let (x, y) = (o.x + sx, o.y + sy);
            if x > 0
                && y > 0
                && x < lim
                && y < lim
                && self.height_at(x, y) > 0x400
                && let Some(g) = group(o.group)
            {
                self.add_sprite(g, x, y, o.sprite);
            }
        }
        for &i in &touched {
            self.heights[i] = self.heights[i].wrapping_sub(0x800);
        }
        self.paste_locks(p, wx, wy);
        self.paste_objects(p, wx & !63, wy & !63);
        self.paint(p, (wx & !63) >> 5, wy >> 6, 2);
    }

    /// Places a piece in its owner's territory, distance band `lo..=hi`, trying `tries`
    /// random cells (0x45EDDE). The piece's vertices must all be free (texture bit 7 clear).
    pub fn place_near(&mut self, path: &str, lo: u8, hi: u8, tries: i32) -> bool {
        let Some(file) = self.piece(path) else { return false };
        let foot: Vec<(i32, i32)> = file.piece.vertices.iter().map(|v| (i32::from(v.x), i32::from(v.y))).collect();
        if foot.is_empty() {
            return false;
        }
        let owner = usize::from(lo >> 4);
        let (cx, cy) = self.start_cell.get(owner).copied().unwrap_or((0, 0));
        let lim = self.sea - 3;
        for _ in 0..tries {
            let Some((x, y)) = self.pick_cell(cx, cy, lo, hi) else { continue };
            let (x, y) = (x * 4, y * 4);
            let free = foot.iter().all(|&(dx, dy)| {
                let (vx, vy) = (x + dx, y + dy);
                vx > 2 && vx < lim && vy > 2 && vy < lim && self.tiles[vy as usize * self.vw + vx as usize] <= 0x7F
            });
            if free {
                self.paste(path, x << 5, y << 5);
                return true;
            }
        }
        false
    }

    /// 0x45EFCF: one band `lo..=lo` after the other up to `hi` (exclusive), or any unowned
    /// land for `lo = 0xFF`.
    pub fn place_band(&mut self, path: &str, lo: u8, hi: u8, tries: i32) -> bool {
        if lo == 0xFF {
            return self.place_near(path, 0xFF, 0xFF, tries);
        }
        (lo..hi).any(|v| self.place_near(path, v, v, tries))
    }

    /// Keeps pieces out of a start area: free vertices within `r` pixels become 0xFF
    /// (0x45F39F).
    pub fn reserve(&mut self, cx: i32, cy: i32, r: i32) {
        let pad = (r >> 5) + 3;
        let x0 = ((cx >> 5) - pad).clamp(0, self.vw as i32 - 1);
        let x1 = ((cx >> 5) + pad).clamp(0, self.vw as i32 - 1);
        let y0 = ((cy >> 5) - pad).clamp(0, self.vh as i32 - 1);
        let y1 = ((cy >> 5) + pad).clamp(0, self.vh as i32 - 1);
        for x in x0..=x1 {
            for y in y0..=y1 {
                let v = y as usize * self.vw + x as usize;
                if self.tiles[v] == 0 {
                    let (px, py) = self.vertex_pos(v);
                    if norm(cx - px, cy - py) < r {
                        self.tiles[v] = 0xFF;
                    }
                }
            }
        }
    }
}
