//! Colour lookup tables (`*.GRD`) of the original 8-bit renderer.
//!
//! A table is a list of 256-byte rows; the renderer replaces a palette index `c` by
//! `table[row * 256 + c]`. `AGEW_TR4.GRD` and `AGEW_TR.GRD` have 256 rows (one per source
//! colour, i.e. a two-colour blend), `AGEW_1D.GRD` has 32 rows of darkness levels. See
//! `docs/formats/gp.md` for which table each sprite encoding uses.

use crate::pal::Palette;

#[derive(Clone, PartialEq, Eq)]
pub struct BlendTable(Vec<u8>);

impl BlendTable {
    /// Accepts any whole number of 256-byte rows.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        (!bytes.is_empty() && bytes.len().is_multiple_of(256)).then(|| Self(bytes.to_vec()))
    }

    pub fn rows(&self) -> usize {
        self.0.len() / 256
    }

    pub fn lookup(&self, row: u8, color: u8) -> u8 {
        self.0[usize::from(row) * 256 + usize::from(color)]
    }

    /// Least-squares weight `w` of the row colour for a 256-row table, so that
    /// `rgb(table[a][b]) ≈ w * rgb(a) + (1 - w) * rgb(b)`.
    pub fn mix_weight(&self, palette: &Palette) -> Option<f64> {
        if self.rows() != 256 {
            return None;
        }
        let (mut num, mut den) = (0.0, 0.0);
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                let (ca, cb, ct) = (palette.rgb(a), palette.rgb(b), palette.rgb(self.lookup(a, b)));
                for k in 0..3 {
                    let x = f64::from(ca[k]) - f64::from(cb[k]);
                    num += x * (f64::from(ct[k]) - f64::from(cb[k]));
                    den += x * x;
                }
            }
        }
        (den > 0.0).then(|| num / den)
    }

    /// Per row, the least-squares opacity `a` of black so that
    /// `rgb(table[row][c]) ≈ (1 - a) * rgb(c)`.
    pub fn darkness(&self, palette: &Palette) -> Vec<f64> {
        (0..self.rows())
            .map(|row| {
                let (mut num, mut den) = (0.0, 0.0);
                for c in 0..=255u8 {
                    let (src, dst) = (palette.rgb(c), palette.rgb(self.lookup(row as u8, c)));
                    for k in 0..3 {
                        num += f64::from(src[k]) * f64::from(dst[k]);
                        den += f64::from(src[k]) * f64::from(src[k]);
                    }
                }
                if den > 0.0 { 1.0 - num / den } else { 0.0 }
            })
            .collect()
    }
}

impl std::fmt::Debug for BlendTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlendTable({} rows)", self.rows())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grey ramp: index i has brightness i.
    fn grey() -> Palette {
        Palette(std::array::from_fn(|i| [i as u8; 3]))
    }

    #[test]
    fn rejects_partial_rows() {
        assert!(BlendTable::parse(&[0; 255]).is_none());
        assert_eq!(BlendTable::parse(&[0; 512]).unwrap().rows(), 2);
    }

    #[test]
    fn fits_mix_weight() {
        let table: Vec<u8> = (0..65536u32).map(|i| ((3 * (i >> 8) + (i & 255)) / 4) as u8).collect();
        let w = BlendTable::parse(&table).unwrap().mix_weight(&grey()).unwrap();
        assert!((w - 0.75).abs() < 0.01, "{w}");
    }

    #[test]
    fn fits_darkness_per_row() {
        // Row r scales brightness by 1 - r/4.
        let table: Vec<u8> = (0..1024u32).map(|i| ((i & 255) * (4 - (i >> 8)) / 4) as u8).collect();
        let d = BlendTable::parse(&table).unwrap().darkness(&grey());
        for (row, expected) in [0.0, 0.25, 0.5, 0.75].into_iter().enumerate() {
            assert!((d[row] - expected).abs() < 0.01, "row {row}: {}", d[row]);
        }
    }
}
