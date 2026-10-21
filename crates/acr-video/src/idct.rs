//! The 8x8 integer inverse DCT of Bink video.
//!
//! A separable butterfly with 12-bit fixed-point rotation constants: columns first without
//! rounding, then rows with `(x + 127) >> 8`. Results are stored with 8-bit wrap-around
//! (no clamping), which is what the bitstream is encoded against.

const A1: i32 = 2896; // cos(pi/4) << 12
const A2: i32 = 2217;
const A3: i32 = 3784;
const A4: i32 = -5352;

#[inline(always)]
fn mul(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b) >> 11
}

/// One 8-point transform over `s` (input) into `d` (output); `k` maps output values.
#[inline(always)]
fn butterfly(s: [i32; 8], k: impl Fn(i32) -> i32) -> [i32; 8] {
    let a0 = s[0].wrapping_add(s[4]);
    let a1 = s[0].wrapping_sub(s[4]);
    let a2 = s[2].wrapping_add(s[6]);
    let a3 = mul(A1, s[2].wrapping_sub(s[6]));
    let a4 = s[5].wrapping_add(s[3]);
    let a5 = s[5].wrapping_sub(s[3]);
    let a6 = s[1].wrapping_add(s[7]);
    let a7 = s[1].wrapping_sub(s[7]);
    let b0 = a4.wrapping_add(a6);
    let b1 = mul(A3, a5.wrapping_add(a7));
    let b2 = mul(A4, a5).wrapping_sub(b0).wrapping_add(b1);
    let b3 = mul(A1, a6.wrapping_sub(a4)).wrapping_sub(b2);
    let b4 = mul(A2, a7).wrapping_add(b3).wrapping_sub(b1);
    let even0 = a0.wrapping_add(a2);
    let even3 = a0.wrapping_sub(a2);
    let even1 = a1.wrapping_add(a3).wrapping_sub(a2);
    let even2 = a1.wrapping_sub(a3).wrapping_add(a2);
    [
        k(even0.wrapping_add(b0)),
        k(even1.wrapping_add(b2)),
        k(even2.wrapping_add(b3)),
        k(even3.wrapping_sub(b4)),
        k(even3.wrapping_add(b4)),
        k(even2.wrapping_sub(b3)),
        k(even1.wrapping_sub(b2)),
        k(even0.wrapping_sub(b0)),
    ]
}

/// Transforms `block` (row-major coefficients) into pixel offsets, row-major.
pub fn idct(block: &[i32; 64]) -> [i32; 64] {
    let mut temp = [0i32; 64];
    for x in 0..8 {
        let col: [i32; 8] = std::array::from_fn(|y| block[y * 8 + x]);
        let out = if col[1..].iter().all(|&c| c == 0) { [col[0]; 8] } else { butterfly(col, |v| v) };
        for (y, v) in out.into_iter().enumerate() {
            temp[y * 8 + x] = v;
        }
    }
    let mut out = [0i32; 64];
    for y in 0..8 {
        let row: [i32; 8] = std::array::from_fn(|x| temp[y * 8 + x]);
        out[y * 8..y * 8 + 8].copy_from_slice(&butterfly(row, |v| v.wrapping_add(0x7F) >> 8));
    }
    out
}

/// Writes the transform of `block` into an 8x8 area of `dst` with row stride `stride`.
pub fn idct_put(dst: &mut [u8], stride: usize, block: &[i32; 64]) {
    let pixels = idct(block);
    for (row, values) in dst.chunks_mut(stride).zip(pixels.as_chunks::<8>().0.iter()) {
        for (d, &v) in row[..8].iter_mut().zip(values) {
            *d = v as u8;
        }
    }
}

/// Adds the transform of `block` to an 8x8 area of `dst`.
pub fn idct_add(dst: &mut [u8], stride: usize, block: &[i32; 64]) {
    let pixels = idct(block);
    for (row, values) in dst.chunks_mut(stride).zip(pixels.as_chunks::<8>().0.iter()) {
        for (d, &v) in row[..8].iter_mut().zip(values) {
            *d = (i32::from(*d).wrapping_add(v)) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_only_block_is_flat() {
        let mut block = [0; 64];
        block[0] = 100 << 3;
        let out = idct(&block);
        assert!(out.iter().all(|&v| v == out[0]), "{out:?}");
        // a0 + a2 + b0 on a row of [800, 0, ..] is 800, then (800 + 127) >> 8 = 3.
        assert_eq!(out[0], (800 + 127) >> 8);
    }

    #[test]
    fn horizontal_frequencies_give_identical_rows() {
        let mut block = [0; 64];
        block[1] = 900;
        block[5] = -300;
        let out = idct(&block);
        for y in 1..8 {
            assert_eq!(out[y * 8..y * 8 + 8], out[..8]);
        }
        // An odd horizontal frequency is antisymmetric around the centre.
        block[5] = 0;
        let out = idct(&block);
        for x in 0..4 {
            assert!((out[x] + out[7 - x]).abs() <= 1, "{:?}", &out[..8]);
        }
        assert!(out[0] > out[3] && out[3] > 0);
    }

    #[test]
    fn put_wraps_instead_of_clamping() {
        let mut block = [0; 64];
        block[0] = 300 << 8;
        let mut dst = [0u8; 64];
        idct_put(&mut dst, 8, &block);
        assert_eq!(dst[0], 44);
        let mut dst = [250u8; 64];
        block[0] = 10 << 8;
        idct_add(&mut dst, 8, &block);
        assert_eq!(dst[63], 4);
    }
}
