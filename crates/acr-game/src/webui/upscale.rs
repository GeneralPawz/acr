//! A pixel-art scaler for the touch skin's portraits (`scale/<n>/..` assets; docs/webui.md
//! "Assets"): the original's 40 pixel icons, enlarged by the browser, are blocks; scaled here
//! first and then smoothed by the browser they keep their edges.
//!
//! This is Scale2x (AdvMAME2x) with a colour tolerance, run once per doubling: a pixel splits
//! into four, each taking a neighbour's colour where two neighbours agree and the opposite pair
//! does not, so diagonal edges are followed instead of stepped. The palette art is dithered
//! and anti-aliased by hand, so "agree" means "close", not "equal". It is a few operations
//! per pixel, no tables and no state: an icon takes microseconds on a phone, which is why it
//! runs in Rust when the asset is made and the page only fetches the result (and the browser
//! caches it).

use acr_formats::raster::Image;

/// The largest factor: 40 pixel icons become 160, about what a phone shows them at.
pub const MAX_SCALE: u32 = 4;

/// Colours of this summed channel distance (and alpha both on or off) count as equal.
const TOLERANCE: i32 = 48;

fn same(a: [u8; 4], b: [u8; 4]) -> bool {
    match (a[3] >= 128, b[3] >= 128) {
        (false, false) => true,
        (true, true) => (0..3).map(|i| (i32::from(a[i]) - i32::from(b[i])).abs()).sum::<i32>() <= TOLERANCE,
        _ => false,
    }
}

fn double(src: &Image) -> Image {
    let (w, h) = (src.width as i32, src.height as i32);
    let mut out = Image::new(src.width * 2, src.height * 2);
    let at = |x: i32, y: i32, fallback: [u8; 4]| -> [u8; 4] {
        if x < 0 || y < 0 || x >= w || y >= h {
            return fallback;
        }
        let i = ((y * w + x) * 4) as usize;
        [src.rgba[i], src.rgba[i + 1], src.rgba[i + 2], src.rgba[i + 3]]
    };
    for y in 0..h {
        for x in 0..w {
            let e = at(x, y, [0; 4]);
            let (b, d, f, hh) = (at(x, y - 1, e), at(x - 1, y, e), at(x + 1, y, e), at(x, y + 1, e));
            let corners = if !same(b, hh) && !same(d, f) {
                [
                    if same(d, b) { d } else { e },
                    if same(b, f) { f } else { e },
                    if same(d, hh) { d } else { e },
                    if same(hh, f) { f } else { e },
                ]
            } else {
                [e; 4]
            };
            for (k, c) in corners.iter().enumerate() {
                let (ox, oy) = (x * 2 + (k as i32 & 1), y * 2 + (k as i32 >> 1));
                let i = ((oy * w * 2 + ox) * 4) as usize;
                out.rgba[i..i + 4].copy_from_slice(c);
            }
        }
    }
    out
}

/// `src` enlarged `scale` times; `scale` is 1, 2 or [`MAX_SCALE`]. `None` for any other factor.
pub fn upscale(src: &Image, scale: u32) -> Option<Image> {
    match scale {
        1 => Some(src.clone()),
        2 => Some(double(src)),
        4 => Some(double(&double(src))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32, pixels: &[[u8; 4]]) -> Image {
        Image { width: w, height: h, rgba: pixels.iter().flatten().copied().collect() }
    }

    const R: [u8; 4] = [200, 0, 0, 255];
    const B: [u8; 4] = [0, 0, 200, 255];
    const T: [u8; 4] = [0, 0, 0, 0];

    fn px(img: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2], img.rgba[i + 3]]
    }

    #[test]
    fn sizes_follow_the_factor() {
        let src = image(3, 2, &[R; 6]);
        assert_eq!((upscale(&src, 1).unwrap().width, upscale(&src, 2).unwrap().height), (3, 4));
        let four = upscale(&src, 4).unwrap();
        assert_eq!((four.width, four.height, four.rgba.len()), (12, 8, 12 * 8 * 4));
        assert!(upscale(&src, 3).is_none() && upscale(&src, 0).is_none() && upscale(&src, 8).is_none());
    }

    #[test]
    fn flat_areas_stay_flat() {
        let big = upscale(&image(2, 2, &[R; 4]), 4).unwrap();
        assert!(big.rgba.chunks(4).all(|p| p == R));
    }

    /// The red centre has blue above and to the left and red right and below: its upper-left
    /// quarter follows the blue edge, its lower-right one stays red.
    #[test]
    fn a_diagonal_edge_is_cut_not_stepped() {
        let src = image(3, 3, &[B, B, R, B, R, R, R, R, R]);
        let big = upscale(&src, 2).unwrap();
        assert_eq!(px(&big, 2, 2), B);
        assert_eq!(px(&big, 3, 3), R);
    }

    #[test]
    fn transparency_is_kept_apart_from_colour() {
        let big = upscale(&image(2, 1, &[T, R]), 2).unwrap();
        assert_eq!(px(&big, 0, 0), T);
        assert_eq!(px(&big, 3, 1), R);
    }

    #[test]
    fn near_colours_count_as_equal() {
        assert!(same([100, 100, 100, 255], [110, 100, 105, 255]));
        assert!(!same([100, 100, 100, 255], [160, 100, 100, 255]));
        assert!(same([1, 2, 3, 0], [9, 9, 9, 10]));
        assert!(!same(T, R));
    }
}
