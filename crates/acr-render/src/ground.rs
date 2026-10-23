//! The detailed ground of 3D mode (docs/ground.md, phase 1): each original ground texture
//! (`TILES3.BMP`) belongs to a material class with a tileable CC0 texture set (albedo +
//! height, normal + roughness). The terrain mesh is drawn once more over the original ground
//! (`ground.wgsl`): the classes of a triangle's three vertices blend by height, tinted by the
//! mean colour of each vertex's original texture and shaded by the original light levels, so
//! the map keeps its colours from afar and gets real soil up close. Paving, logs and water
//! keep the original (class [`KEEP`]). Render only.

/// Material classes; class `i` draws texture layer `i` (`assets/ground/materials.py`).
pub const CLASSES: [&str; 15] = [
    "grass", "meadow", "dirt", "mud", "sand", "rock", "forest", "snow", "field", "cracked", "red", "gravel", "desert",
    "steppe", "snowy",
];

/// Texture layers: one per class in class order, then the second sets ([`SECOND`]).
pub const LAYERS: [&str; 20] = [
    "grass", "meadow", "dirt", "mud", "sand", "rock", "forest", "snow", "field", "cracked", "red", "gravel", "desert",
    "steppe", "snowy", "grass2", "meadow2", "dirt2", "desert2", "rock2",
];

/// Metres one repeat of each layer's texture covers.
pub const REPEAT_METRES: [f32; 20] = [
    2.0, 2.0, 3.0, 2.5, 3.0, 3.0, 2.5, 3.0, 3.0, 3.0, 2.5, 2.0, 3.0, 2.5, 3.0, // classes
    2.5, 3.0, 3.0, 3.0, 4.0, // second sets
];

/// Per class the layer of its second set, mixed in by low-frequency noise against tiling (its
/// own layer: the same set at a larger, turned repeat).
pub const SECOND: [u8; 15] = [15, 16, 17, 3, 4, 19, 6, 7, 8, 9, 10, 11, 18, 13, 14];

/// The class of textures drawn as in the original (paving, logs, water).
pub const KEEP: u8 = 255;

const G: u8 = 0;
const M: u8 = 1;
const D: u8 = 2;
const U: u8 = 3;
const S: u8 = 4;
const R: u8 = 5;
const F: u8 = 6;
const W: u8 = 7;
const L: u8 = 8;
const C: u8 = 9;
const E: u8 = 10;
const V: u8 = 11;
const T: u8 = 12;
const P: u8 = 13;
const N: u8 = 14;
const K: u8 = KEEP;

/// The class of each `TILES3.BMP` texture, from its contact sheet (`ACR-analysis/ground/
/// tiles3_sheet.png`); textures past the table keep the original.
#[rustfmt::skip]
pub const TILE_CLASS: [u8; 136] = [
    G, M, M, M, G, G, M, M, R, R, R, R, // 0
    G, M, V, D, G, M, G, M, G, M, G, M, // 12
    K, K, V, L, F, U, U, R, L, R, D, N, // 24
    K, K, R, G, K, K, C, C, K, K, R, R, // 36
    K, U, U, U, C, U, U, U, R, R, F, F, // 48
    K, K, F, M, M, M, E, D, M, M, E, D, // 60
    S, L, E, S, K, K, E, S, R, R, R, R, // 72
    D, L, R, V, K, K, R, V, L, P, P, W, // 84
    M, V, R, W, S, S, S, W, S, S, S, W, // 96
    S, S, W, L, E, L, S, K, P, P, P, P, // 108
    T, T, T, T, T, T, T, T, T, T, T, T, // 120
    T, T, T, T,                         // 132
];

/// The material class of original texture `tile`.
pub fn class_of(tile: u8) -> u8 {
    TILE_CLASS.get(usize::from(tile)).copied().unwrap_or(KEEP)
}

/// Game paths of the packed textures: per layer `<layer>_a.png` (albedo, height) and
/// `<layer>_n.png` (normal, roughness), in layer order, a before n.
pub fn texture_files() -> Vec<String> {
    LAYERS
        .iter()
        .flat_map(|c| [format!("ACR/Models/ground/{c}_a.png"), format!("ACR/Models/ground/{c}_n.png")])
        .collect()
}

/// One class's mip chains: albedo + height, normal + roughness (level 0 first).
pub type MaterialSet = (Vec<Vec<u8>>, Vec<Vec<u8>>);

/// The decoded texture sets: per class the albedo + height and the normal + roughness
/// images, RGBA8, all `size` × `size`, with their mipmaps.
pub struct MaterialImages {
    pub size: u32,
    /// Per layer: (albedo mips, normal mips), level 0 first.
    pub layers: Vec<MaterialSet>,
    /// Mean albedo per layer (linear RGB), what the tint replaces.
    pub means: Vec<[f32; 3]>,
}

fn decode_png(bytes: &[u8]) -> Result<(u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("png too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    if info.width != info.height || !info.width.is_power_of_two() {
        return Err(format!("{} x {}: not a square power of two", info.width, info.height));
    }
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
        other => return Err(format!("{other:?}: expected RGB(A)")),
    };
    Ok((info.width, rgba))
}

/// Level 0 and its box-filtered mipmaps down to 1 × 1.
fn mips(size: u32, level0: Vec<u8>) -> Vec<Vec<u8>> {
    let mut out = vec![level0];
    let mut s = size as usize;
    while s > 1 {
        let prev = out.last().expect("level 0");
        let n = s / 2;
        let mut next = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                for c in 0..4 {
                    let at = |dx: usize, dy: usize| u32::from(prev[((2 * y + dy) * s + 2 * x + dx) * 4 + c]);
                    next[(y * n + x) * 4 + c] = ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8;
                }
            }
        }
        out.push(next);
        s = n;
    }
    out
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

impl MaterialImages {
    /// Decodes the files of [`texture_files`] (same order). `skip` drops that many top
    /// levels (the browser takes half the resolution).
    pub fn decode(files: &[Vec<u8>], skip: usize) -> Result<Self, String> {
        if files.len() != LAYERS.len() * 2 {
            return Err(format!("{} ground textures, expected {}", files.len(), LAYERS.len() * 2));
        }
        let mut size = 0;
        let mut layers = Vec::new();
        let mut means = Vec::new();
        for (i, pair) in files.as_chunks::<2>().0.iter().enumerate() {
            let (sa, a) = decode_png(&pair[0]).map_err(|e| format!("{}_a.png: {e}", LAYERS[i]))?;
            let (sn, n) = decode_png(&pair[1]).map_err(|e| format!("{}_n.png: {e}", LAYERS[i]))?;
            if sa != sn || (size != 0 && sa != size) {
                return Err(format!("{}: sizes differ", LAYERS[i]));
            }
            size = sa;
            let mut a = mips(sa, a);
            let mut n = mips(sn, n);
            let skip = skip.min(a.len() - 1);
            a.drain(..skip);
            n.drain(..skip);
            let last = a.last().expect("1 x 1 level");
            means.push(std::array::from_fn(|c| srgb_to_linear(f32::from(last[c]) / 255.0)));
            layers.push((a, n));
        }
        Ok(Self { size: size >> skip.min(size.trailing_zeros() as usize), layers, means })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_field_textures_have_classes() {
        // The blank field's grass, dirt and snow (blank_map.rs).
        assert_eq!(class_of(0), 0);
        assert_eq!(CLASSES[usize::from(class_of(71))], "dirt");
        assert_eq!(CLASSES[usize::from(class_of(103))], "snow");
        // Water and paving keep the original.
        assert_eq!(class_of(89), KEEP);
        assert_eq!(class_of(200), KEEP);
    }

    #[test]
    fn layers_match_classes() {
        assert_eq!(LAYERS[..CLASSES.len()], CLASSES);
        assert!(SECOND.iter().all(|&l| usize::from(l) < LAYERS.len()));
        assert!(TILE_CLASS.iter().all(|&c| c == KEEP || usize::from(c) < CLASSES.len()));
        // The new splits: desert soil, red soil, cracked earth, snow on rock, roads.
        assert_eq!(CLASSES[usize::from(class_of(125))], "desert");
        assert_eq!(CLASSES[usize::from(class_of(70))], "red");
        assert_eq!(CLASSES[usize::from(class_of(52))], "cracked");
        assert_eq!(CLASSES[usize::from(class_of(35))], "snowy");
        assert_eq!(CLASSES[usize::from(class_of(14))], "gravel");
    }

    #[test]
    fn mips_reach_one_texel() {
        let m = mips(4, vec![100; 4 * 4 * 4]);
        assert_eq!(m.len(), 3);
        assert_eq!(m[2], vec![100; 4]);
    }
}
