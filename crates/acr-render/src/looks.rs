//! Individual looks of soldiers (#283): the Rust port of `faces.choose` of the asset
//! pipeline (`assets/blender/lib/acr_assets/faces.py` on the faces branch, contract in
//! `docs/assets-humans.md`, "Faces and bodies"), and the per-model data the renderer needs
//! to show a look: face and body morph variants baked on the CPU, the hair and beard nodes,
//! the tints.
//!
//! The picker draws from the same 32-bit hash stream (`_mix32` = lowbias32, `Rng`) in the
//! same order as the Python reference, so one seed (the unit id) gives the same look in the
//! game as in the asset tools. The tables are copied from `faces.py` (glTF extras are read
//! into unordered maps, and the draw order of [`Rng::pick`] depends on the table order).

/// Face and body morph targets in `faces.FACE_MORPHS` + `BODY_MORPHS` order, with their
/// weight ranges.
pub const LOOK_MORPHS: [(&str, f32, f32); 12] = [
    ("face_jaw", -1.0, 1.0),
    ("face_chin", -1.0, 1.0),
    ("face_cheekbones", -1.0, 1.0),
    ("face_nose", -1.0, 1.0),
    ("face_nose_bridge", -1.0, 1.0),
    ("face_brow", 0.0, 1.0),
    ("face_eyes", -1.0, 1.0),
    ("face_age", 0.0, 1.0),
    ("face_fat", 0.0, 1.0),
    ("face_thin", 0.0, 1.0),
    ("body_stocky", 0.0, 1.0),
    ("body_thin", 0.0, 1.0),
];

/// Index of a morph target name in [`LOOK_MORPHS`].
pub fn look_morph(name: &str) -> Option<usize> {
    LOOK_MORPHS.iter().position(|(n, _, _)| *n == name)
}

const HEIGHT_M: (f64, f64) = (1.58, 1.80);
const HEIGHT_MEAN_M: f64 = 1.69;
const HEIGHT_SD_M: f64 = 0.055;
/// The height the models are built at: an instance is scaled by `height_m / BUILT_HEIGHT_M`.
pub const BUILT_HEIGHT_M: f32 = 1.73;

/// Hair colours (sRGB), `faces.HAIR_COLOURS`; the last two are the greys.
pub const HAIR_COLOURS: [(&str, [f32; 3]); 8] = [
    ("black", [0.07, 0.06, 0.05]),
    ("dark_brown", [0.16, 0.11, 0.07]),
    ("brown", [0.28, 0.19, 0.11]),
    ("chestnut", [0.38, 0.21, 0.11]),
    ("dark_blond", [0.50, 0.40, 0.24]),
    ("ginger", [0.55, 0.27, 0.11]),
    ("grizzled", [0.36, 0.33, 0.30]),
    ("grey", [0.55, 0.53, 0.50]),
];
const HAIR_ODDS: [f64; 6] = [0.12, 0.30, 0.25, 0.12, 0.15, 0.06];
/// The neutral colour (sRGB) the hair material is painted in: the tint divides it out.
pub const HAIR_NEUTRAL: [f32; 3] = [0.80, 0.78, 0.75];

/// Skin tints (sRGB multipliers), `faces.SKIN_TONES["english"]`.
pub const SKIN_TONES: [(&str, [f32; 3]); 7] = [
    ("fair", [1.05, 1.02, 0.99]),
    ("pale_ruddy", [1.04, 0.95, 0.91]),
    ("ruddy", [1.03, 0.88, 0.80]),
    ("weathered", [0.95, 0.85, 0.76]),
    ("sunburnt", [1.00, 0.82, 0.70]),
    ("tanned", [0.86, 0.73, 0.61]),
    ("olive", [0.88, 0.82, 0.68]),
];

/// Hair styles and the node suffixes they show (`faces.HAIR_VARIANTS`).
pub const HAIR_VARIANTS: [(&str, &[&str]); 3] =
    [("short", &["hair_short"]), ("tied", &["hair_tied", "hair_queue"]), ("loose", &["hair_loose"])];
/// Beards and their node suffixes (`faces.BEARD_VARIANTS`).
pub const BEARD_VARIANTS: [(&str, &[&str]); 5] = [
    ("none", &[]),
    ("moustache", &["beard_moustache"]),
    ("sideburns", &["beard_sideburns"]),
    ("full", &["beard_full", "beard_moustache"]),
    ("moustache_sideburns", &["beard_moustache", "beard_sideburns"]),
];
/// Every variant node suffix: bit `i` of a look's node mask shows suffix `i`.
pub const VARIANT_NODES: [&str; 7] =
    ["hair_short", "hair_tied", "hair_queue", "hair_loose", "beard_full", "beard_moustache", "beard_sideburns"];

/// Odds of the line infantry (`regular`) and of civilians and irregulars (`civilian`):
/// `faces.STYLES`, as (index into the variants table, weight) in the table's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Regular,
    Civilian,
}

impl Style {
    fn hair(self) -> &'static [(usize, f64)] {
        match self {
            Self::Regular => &[(1, 0.75), (0, 0.25)],
            Self::Civilian => &[(2, 0.45), (1, 0.25), (0, 0.30)],
        }
    }

    fn beard(self) -> &'static [(usize, f64)] {
        match self {
            Self::Regular => &[(0, 0.78), (2, 0.14), (1, 0.05), (4, 0.03)],
            Self::Civilian => &[(0, 0.40), (3, 0.28), (1, 0.12), (2, 0.12), (4, 0.08)],
        }
    }

    fn stubble(self) -> (f64, f64) {
        match self {
            Self::Regular => (0.15, 1.0),
            Self::Civilian => (0.3, 1.0),
        }
    }
}

/// `faces._mix32`: lowbias32.
pub fn mix32(x: u32) -> u32 {
    let mut x = x;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

/// `faces.Rng`: a deterministic stream from a seed.
pub struct Rng {
    state: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self { state: mix32(seed ^ 0x9E37_79B9) }
    }

    /// Uniform in [0, 1).
    pub fn u(&mut self) -> f64 {
        self.state = mix32(self.state.wrapping_add(0x6D2B_79F5));
        f64::from(self.state) / 4_294_967_296.0
    }

    /// Triangular in (−1, 1).
    pub fn tri(&mut self) -> f64 {
        self.u() - self.u()
    }

    /// An index into `odds` (index, weight) by weight; the table's own index is returned.
    fn pick(&mut self, odds: &[(usize, f64)]) -> usize {
        let mut r = self.u() * odds.iter().map(|o| o.1).sum::<f64>();
        for &(k, w) in odds {
            r -= w;
            if r < 0.0 {
                return k;
            }
        }
        odds.last().map_or(0, |o| o.0)
    }

    fn pick_even(&mut self, n: usize) -> usize {
        let odds: Vec<(usize, f64)> = (0..n).map(|i| (i, 1.0)).collect();
        self.pick(&odds)
    }

    fn normal(&mut self) -> f64 {
        (0..6).map(|_| self.u()).sum::<f64>() - 3.0
    }
}

/// One soldier's look (`faces.choose`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// Weights in [`LOOK_MORPHS`] order.
    pub morphs: [f32; 12],
    /// Index into [`HAIR_VARIANTS`].
    pub hair: usize,
    /// Index into [`BEARD_VARIANTS`].
    pub beard: usize,
    /// Index into [`HAIR_COLOURS`].
    pub hair_colour: usize,
    /// Index into [`SKIN_TONES`].
    pub skin: usize,
    pub stubble: f32,
    pub pocks: f32,
    pub scar: bool,
    pub height_m: f32,
}

fn round3(x: f64) -> f32 {
    ((x * 1000.0).round() / 1000.0) as f32
}

/// `faces.choose(seed, style)`: the look of the soldier `seed` (the unit id).
pub fn choose(seed: u32, style: Style) -> Look {
    let mut r = Rng::new(seed);
    let age = r.u().powf(1.6);
    let mut morphs = [0.0; 12];
    for (k, (_, lo, _)) in LOOK_MORPHS.iter().enumerate().take(10) {
        if *lo < 0.0 {
            morphs[k] = round3((1.3 * r.tri()).clamp(-1.0, 1.0));
        }
    }
    morphs[5] = round3(r.u().powf(1.5));
    morphs[7] = round3(age);
    let fat = r.tri() * 1.2;
    morphs[8] = round3(fat.clamp(0.0, 1.0));
    morphs[9] = round3((-fat).clamp(0.0, 1.0));
    let build = r.tri() * 1.3;
    morphs[10] = round3(build.clamp(0.0, 1.0));
    morphs[11] = round3((-build).clamp(0.0, 1.0));
    let hair = r.pick(style.hair());
    let beard = r.pick(style.beard());
    let odds: Vec<(usize, f64)> = HAIR_ODDS.iter().copied().enumerate().collect();
    let mut hair_colour = r.pick(&odds);
    if r.u() < (age - 0.45).max(0.0) * 1.6 {
        hair_colour = 6 + r.pick_even(2);
    }
    let (lo, hi) = style.stubble();
    let stubble = round3(lo + (hi - lo) * r.u());
    let skin = r.pick_even(SKIN_TONES.len());
    let height = HEIGHT_MEAN_M + HEIGHT_SD_M * r.normal() * std::f64::consts::SQRT_2;
    let height_m = round3(height.clamp(HEIGHT_M.0, HEIGHT_M.1));
    let pocks = if r.u() < 0.2 { round3(0.3 + 0.7 * r.u()) } else { 0.0 };
    let scar = r.u() < 0.15;
    Look { morphs, hair, beard, hair_colour, skin, stubble, pocks, scar, height_m }
}

impl Look {
    /// Bit `i` set: the variant node [`VARIANT_NODES`]`[i]` is shown.
    pub fn node_mask(&self) -> u32 {
        HAIR_VARIANTS[self.hair]
            .1
            .iter()
            .chain(BEARD_VARIANTS[self.beard].1)
            .filter_map(|s| VARIANT_NODES.iter().position(|n| n == s))
            .fold(0, |m, i| m | 1 << i)
    }
}

/// The most head tiles a face texture holds (#306, `faces.PORTRAIT_TILES` in the build; a
/// 3 x 3 grid at most).
pub const FACE_TILES_MAX: u8 = 9;

/// Tiles per side of a face texture holding `n` head tiles (`faces.tile_grid`).
pub fn tile_grid(n: u8) -> u8 {
    (1..=3).find(|g| g * g >= n).unwrap_or(3)
}

/// The face tile (#306) soldier `seed` shows of a texture holding `n`: its own hash of the
/// seed, so the portrait varies independently of the face shape drawn by [`choose`].
pub fn face_tile(seed: u32, n: u8) -> u8 {
    if n <= 1 {
        return 0;
    }
    (mix32(seed ^ 0x5F0C_E7A1) % u32::from(n)) as u8
}

/// Number of face and body variants baked per model ([`crate::model::ModelData::looks`]).
pub const VARIANTS: usize = 16;

/// The morph weights of baked variant `k`: the look of a fixed seed per variant, so the
/// variants spread like the soldiers they stand for.
pub fn variant_weights(k: usize) -> [f32; 12] {
    choose(0x5EED_0000 + k as u32, Style::Regular).morphs
}

/// The baked variant nearest to `morphs` (squared distance over the weights).
pub fn nearest_variant(variants: &[[f32; 12]], morphs: &[f32; 12]) -> usize {
    let d = |v: &[f32; 12]| v.iter().zip(morphs).map(|(a, b)| (a - b) * (a - b)).sum::<f32>();
    (0..variants.len()).min_by(|&a, &b| d(&variants[a]).total_cmp(&d(&variants[b]))).unwrap_or(0)
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// The linear multipliers of a look: skin (an sRGB multiplier, so raised to the gamma) and
/// hair (the chosen colour over the painted neutral one).
pub fn tints(look: &Look) -> ([f32; 3], [f32; 3]) {
    let skin = SKIN_TONES[look.skin].1.map(|m| m.powf(2.2));
    let c = HAIR_COLOURS[look.hair_colour].1;
    let hair = std::array::from_fn(|i| srgb_to_linear(c[i]) / srgb_to_linear(HAIR_NEUTRAL[i]));
    (skin, hair)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name<T>(table: &[(&'static str, T)], i: usize) -> &'static str {
        table[i].0
    }

    #[test]
    fn the_hash_matches_python() {
        assert_eq!(mix32(0), 0);
        assert_eq!(mix32(1), 1_753_845_952);
        assert_eq!(mix32(0xDEAD_BEEF), 3_861_431_939);
    }

    // Values from `faces.choose` (origin/m2-mh 4f45ace).
    #[test]
    fn choose_matches_python() {
        struct Case {
            seed: u32,
            style: Style,
            morphs: [f32; 12],
            hair: &'static str,
            beard: &'static str,
            colour: &'static str,
            skin: &'static str,
            stubble: f32,
            pocks: f32,
            scar: bool,
            height: f32,
        }
        let cases = [
            Case {
                seed: 0,
                style: Style::Regular,
                morphs: [-0.623, -0.034, -0.514, -0.081, 0.223, 0.162, -0.112, 0.045, 0.0, 0.358, 0.192, 0.0],
                hair: "tied",
                beard: "none",
                colour: "ginger",
                skin: "tanned",
                stubble: 0.99,
                pocks: 0.0,
                scar: false,
                height: 1.707,
            },
            Case {
                seed: 1,
                style: Style::Civilian,
                morphs: [-0.391, 0.705, 0.441, -0.449, -0.154, 0.102, 0.144, 0.004, 0.0, 0.266, 0.0, 0.386],
                hair: "tied",
                beard: "full",
                colour: "dark_blond",
                skin: "ruddy",
                stubble: 0.31,
                pocks: 0.0,
                scar: false,
                height: 1.709,
            },
            Case {
                seed: 42,
                style: Style::Regular,
                morphs: [-0.207, 0.046, 0.046, 0.43, 0.778, 0.86, 0.287, 0.871, 0.68, 0.0, 0.11, 0.0],
                hair: "tied",
                beard: "none",
                colour: "black",
                skin: "pale_ruddy",
                stubble: 0.553,
                pocks: 0.0,
                scar: false,
                height: 1.768,
            },
            Case {
                seed: 1776,
                style: Style::Regular,
                morphs: [0.321, -0.881, 0.558, -0.338, -0.238, 0.384, -0.486, 0.85, 0.0, 0.783, 0.027, 0.0],
                hair: "short",
                beard: "sideburns",
                colour: "grizzled",
                skin: "weathered",
                stubble: 0.805,
                pocks: 0.672,
                scar: true,
                height: 1.655,
            },
            Case {
                seed: u32::MAX,
                style: Style::Civilian,
                morphs: [-0.378, -0.195, 0.498, 0.013, -0.377, 0.529, 0.219, 0.212, 0.227, 0.0, 0.196, 0.0],
                hair: "loose",
                beard: "moustache",
                colour: "brown",
                skin: "sunburnt",
                stubble: 0.825,
                pocks: 0.0,
                scar: false,
                height: 1.702,
            },
        ];
        for c in cases {
            let l = choose(c.seed, c.style);
            for (k, (a, b)) in l.morphs.iter().zip(c.morphs).enumerate() {
                assert!((a - b).abs() < 1.5e-3, "seed {} {}: {a} vs {b}", c.seed, LOOK_MORPHS[k].0);
            }
            assert_eq!(name(&HAIR_VARIANTS, l.hair), c.hair, "seed {}", c.seed);
            assert_eq!(name(&BEARD_VARIANTS, l.beard), c.beard, "seed {}", c.seed);
            assert_eq!(name(&HAIR_COLOURS, l.hair_colour), c.colour, "seed {}", c.seed);
            assert_eq!(name(&SKIN_TONES, l.skin), c.skin, "seed {}", c.seed);
            assert!((l.stubble - c.stubble).abs() < 1.5e-3, "seed {}", c.seed);
            assert!((l.pocks - c.pocks).abs() < 1.5e-3, "seed {}", c.seed);
            assert_eq!(l.scar, c.scar, "seed {}", c.seed);
            assert!((l.height_m - c.height).abs() < 1.5e-3, "seed {}", c.seed);
        }
    }

    #[test]
    fn node_masks_follow_the_variants() {
        let mut l = choose(0, Style::Regular);
        l.hair = 1; // tied: hair_tied + hair_queue
        l.beard = 3; // full: beard_full + beard_moustache
        assert_eq!(l.node_mask(), 0b011_0110);
    }

    #[test]
    fn variants_spread_and_the_nearest_is_found() {
        let v: Vec<[f32; 12]> = (0..VARIANTS).map(variant_weights).collect();
        for (k, w) in v.iter().enumerate() {
            assert_eq!(nearest_variant(&v, w), k);
        }
    }

    #[test]
    fn face_tiles_spread_over_a_unit() {
        assert_eq!([1, 2, 4, 5, 9].map(tile_grid), [1, 2, 2, 3, 3]);
        assert_eq!(face_tile(7, 1), 0);
        let mut seen = [0; 4];
        for seed in 0..12 * 50 {
            seen[usize::from(face_tile(seed, 4))] += 1;
        }
        // Every tile about a quarter of the soldiers.
        assert!(seen.iter().all(|&n| n > 100 && n < 200), "{seen:?}");
    }
}
