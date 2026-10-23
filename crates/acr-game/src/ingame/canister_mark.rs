//! The canister mark: a red X on the ground where a selected cannon's canister (`KARTECH`)
//! would go, the point `artillery::fire_canister` aims at (256 px ahead along its facing,
//! `NewAttackPoint(x + TCos[dir], y + TSin[dir])`, 0x4dc8fa), or the point of a canister or
//! ground attack it is carrying out. Drawn in world screen pixels with the other overlays.

use acr_sim::World;
use acr_sim::trig::{cos, sin};
use acr_sim::world::{Order, Unit};

use crate::ui::draw::{Draw, Rect};

/// Half the width of the X, world screen pixels.
const ARM: i32 = 7;

/// The ground point (world pixels) a selected gun `u` marks: its ground attack's point, else
/// the canister's aim ahead of it. `None` for units without canister.
pub fn aim(world: &World, u: &Unit) -> Option<(i32, i32)> {
    world.catalog.get(u.kind).sector_weapon?;
    if let Some(Order::AttackPoint { x, y, .. }) = u.orders.last() {
        return Some((*x, *y));
    }
    let (x, y) = u.pos.pixels();
    Some((x + cos(u.dir), y + sin(u.dir)))
}

/// The X at screen point `(sx, sy)` in colour `c`: two diagonals two pixels thick and a
/// dark outline pixel each side, so it reads on grass, sand and snow.
pub fn draw_x(c: [u8; 4], sx: i32, sy: i32, out: &mut Vec<Draw>) {
    let dark = [40, 0, 0, 255];
    for (colour, grow) in [(dark, 1), (c, 0)] {
        for i in -ARM..=ARM {
            for (x, y) in [(sx + i, sy + i / 2), (sx + i, sy - i / 2)] {
                let r = Rect { x0: x - grow, y0: y - grow, x1: x + 2 + grow, y1: y + 2 + grow };
                out.push(Draw::fill(colour, r).unclipped());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_x_is_centred() {
        let mut out = Vec::new();
        draw_x([255, 0, 0, 255], 100, 50, &mut out);
        assert_eq!(out.len(), 4 * (2 * ARM as usize + 1));
    }
}
