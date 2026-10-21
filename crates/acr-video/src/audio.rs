//! Bink audio, RDFT flavour (revisions `c`..`i`).
//!
//! Each audio packet starts with the number of decoded bytes (`u32`), followed by blocks
//! aligned to 32 bits. A block codes one transform frame of interleaved samples (stereo is
//! interleaved *before* the transform, so the transform is 2x longer instead of running
//! per channel): two float coefficients, one 8-bit quantizer per critical band, then
//! groups of coefficients with a shared bit width. An inverse real FFT gives the samples;
//! the first 1/16 of each frame is cross-faded with the end of the previous one.

use crate::BinkError;
use crate::bits::BitReader;
use crate::container::AudioTrack;

/// Upper band edges in Hz (the critical bands also used by WMA).
const CRITICAL_FREQS: [u32; 25] = [
    100, 200, 300, 400, 510, 630, 770, 920, 1080, 1270, 1480, 1720, 2000, 2320, 2700, 3150, 3700, 4400, 5300, 6400,
    7700, 9500, 12000, 15500, 24500,
];

/// Lengths (in groups of 8 coefficients) of a run that shares one bit width.
const RUN_LENGTHS: [usize; 16] = [2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 32, 64];

pub struct AudioDecoder {
    rate: u32,
    channels: u16,
    frame_len: usize,
    overlap: usize,
    /// Start coefficient of each band, plus `frame_len` as the end.
    bands: Vec<usize>,
    quant_table: [f32; 96],
    root: f32,
    previous: Vec<f32>,
    first: bool,
    fft: RealInverseFft,
    coeffs: Vec<f32>,
    samples: Vec<f32>,
}

impl AudioDecoder {
    pub fn new(track: &AudioTrack) -> Result<Self, BinkError> {
        if track.uses_dct() {
            return Err(BinkError("DCT audio is not supported"));
        }
        let channels = track.channels();
        let rate = track.sample_rate;
        if rate == 0 {
            return Err(BinkError("invalid sample rate"));
        }
        let mut bits = match rate {
            0..22050 => 9,
            22050..44100 => 10,
            _ => 11,
        };
        if channels == 2 {
            bits += 1;
        }
        let frame_len = 1usize << bits;
        let overlap = frame_len / 16;
        let interleaved_rate = rate * u32::from(channels);
        let half_rate = interleaved_rate.div_ceil(2) as usize;
        let root = 2.0 / ((frame_len as f32).sqrt() * 32768.0);
        // 0.0664 / log10(e): quantizer steps of about 0.664 dB.
        let quant_table = std::array::from_fn(|i| (i as f32 * 0.152_891_65).exp() * root);
        let band_count = (1..25).find(|&n| half_rate <= CRITICAL_FREQS[n - 1] as usize).unwrap_or(25);
        let mut bands = vec![2];
        bands.extend((1..band_count).map(|i| (CRITICAL_FREQS[i - 1] as usize * frame_len / half_rate) & !1));
        bands.push(frame_len);
        Ok(Self {
            rate,
            channels,
            frame_len,
            overlap,
            bands,
            quant_table,
            root,
            previous: vec![0.0; overlap],
            first: true,
            fft: RealInverseFft::new(frame_len),
            coeffs: vec![0.0; frame_len],
            samples: vec![0.0; frame_len],
        })
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Decodes one audio packet, appending interleaved samples in -1..1 to `out`.
    pub fn decode(&mut self, packet: &[u8], out: &mut Vec<f32>) -> Result<(), BinkError> {
        if packet.len() < 4 {
            return Ok(());
        }
        let mut r = BitReader::new(packet);
        r.skip(32); // decoded size in bytes
        // A block needs at least two floats (2 x 29 bits).
        while r.left() >= 58 {
            self.decode_block(&mut r)?;
            r.align32();
            out.extend_from_slice(&self.samples[..self.frame_len - self.overlap]);
        }
        Ok(())
    }

    fn read_float(r: &mut BitReader) -> f32 {
        let power = r.read(5) as i32;
        let mantissa = r.read(23) as f32;
        let f = mantissa * 2f32.powi(power - 23);
        if r.bit() { -f } else { f }
    }

    fn decode_block(&mut self, r: &mut BitReader) -> Result<(), BinkError> {
        let c = &mut self.coeffs;
        c[0] = Self::read_float(r) * self.root;
        c[1] = Self::read_float(r) * self.root;
        let band_count = self.bands.len() - 1;
        let mut quant = [0f32; 25];
        for q in &mut quant[..band_count] {
            *q = self.quant_table[(r.read(8) as usize).min(95)];
        }
        let mut band = 0;
        let mut q = quant[0];
        let mut i = 2;
        while i < self.frame_len {
            let end = if r.bit() { i + RUN_LENGTHS[r.read(4) as usize] * 8 } else { i + 8 };
            let end = end.min(self.frame_len);
            let width = r.read(4);
            if width == 0 {
                c[i..end].fill(0.0);
                i = end;
                while self.bands[band] < i {
                    q = quant[band];
                    band += 1;
                }
            } else {
                while i < end {
                    if self.bands[band] == i {
                        q = quant[band];
                        band += 1;
                    }
                    let v = r.read(width);
                    c[i] = match v {
                        0 => 0.0,
                        _ if r.bit() => -q * v as f32,
                        _ => q * v as f32,
                    };
                    i += 1;
                }
            }
        }
        if r.overrun() {
            return Err(BinkError("audio block truncated"));
        }
        self.fft.run(&self.coeffs, &mut self.samples);

        let n = self.overlap;
        if !self.first {
            for (i, (s, &p)) in self.samples[..n].iter_mut().zip(&self.previous).enumerate() {
                *s = (p * (n - i) as f32 + *s * i as f32) / n as f32;
            }
        }
        self.previous.copy_from_slice(&self.samples[self.frame_len - n..]);
        self.first = false;
        Ok(())
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Complex {
    re: f32,
    im: f32,
}

impl Complex {
    fn mul(self, o: Self) -> Self {
        Self { re: self.re * o.re - self.im * o.im, im: self.re * o.im + self.im * o.re }
    }
    fn add(self, o: Self) -> Self {
        Self { re: self.re + o.re, im: self.im + o.im }
    }
    fn sub(self, o: Self) -> Self {
        Self { re: self.re - o.re, im: self.im - o.im }
    }
    fn conj(self) -> Self {
        Self { re: self.re, im: -self.im }
    }
    fn expi(angle: f64) -> Self {
        Self { re: angle.cos() as f32, im: angle.sin() as f32 }
    }
}

/// Real-output inverse FFT of length `n` through a complex FFT of length `n / 2`.
///
/// Input layout (Bink's): `c[0]` = DC, `c[1]` = Nyquist, `c[2k], c[2k+1]` = cosine and
/// sine amplitude of frequency `k`. Output:
/// `y[t] = c[0]/2 + c[1]/2 * (-1)^t + sum_k (c[2k] cos(2 pi k t / n) + c[2k+1] sin(2 pi k t / n))`.
struct RealInverseFft {
    n: usize,
    /// `e^(2 pi i k / n)` for `k < n/2` (splitting twiddles).
    split: Vec<Complex>,
    /// `e^(2 pi i k / (n/2))` for `k < n/4` (butterfly twiddles).
    twiddles: Vec<Complex>,
    bit_reverse: Vec<u32>,
    buf: Vec<Complex>,
}

impl RealInverseFft {
    fn new(n: usize) -> Self {
        assert!(n.is_power_of_two() && n >= 4);
        let m = n / 2;
        let tau = std::f64::consts::TAU;
        let split = (0..m).map(|k| Complex::expi(tau * k as f64 / n as f64)).collect();
        let twiddles = (0..m / 2).map(|k| Complex::expi(tau * k as f64 / m as f64)).collect();
        let bits = m.trailing_zeros();
        let bit_reverse = (0..m as u32).map(|i| if bits == 0 { 0 } else { i.reverse_bits() >> (32 - bits) }).collect();
        Self { n, split, twiddles, bit_reverse, buf: vec![Complex::default(); m] }
    }

    fn run(&mut self, c: &[f32], out: &mut [f32]) {
        let m = self.n / 2;
        // Spectrum Y[k] = a_k - i b_k, Hermitian; Y[0] = c[0], Y[m] = c[1].
        let y = |k: usize| -> Complex {
            match k {
                0 => Complex { re: c[0], im: 0.0 },
                k if k == m => Complex { re: c[1], im: 0.0 },
                k => Complex { re: c[2 * k], im: -c[2 * k + 1] },
            }
        };
        for k in 0..m {
            let a = y(k);
            // Y[k + m] = conj(Y[m - k])
            let b = if k == 0 { y(m) } else { y(m - k).conj() };
            let even = a.add(b);
            let odd = a.sub(b).mul(self.split[k]);
            // Z = E + i O
            let z = Complex { re: even.re - odd.im, im: even.im + odd.re };
            self.buf[self.bit_reverse[k] as usize] = z;
        }
        // In-place iterative radix-2 inverse FFT (positive exponent).
        let mut size = 2;
        while size <= m {
            let half = size / 2;
            let step = m / size;
            for start in (0..m).step_by(size) {
                for j in 0..half {
                    let w = self.twiddles[j * step];
                    let t = self.buf[start + j + half].mul(w);
                    let u = self.buf[start + j];
                    self.buf[start + j] = u.add(t);
                    self.buf[start + j + half] = u.sub(t);
                }
            }
            size *= 2;
        }
        for (pair, z) in out.as_chunks_mut::<2>().0.iter_mut().zip(&self.buf) {
            pair[0] = 0.5 * z.re;
            pair[1] = 0.5 * z.im;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct(c: &[f32]) -> Vec<f32> {
        let n = c.len();
        (0..n)
            .map(|t| {
                let mut s = f64::from(c[0]) / 2.0 + f64::from(c[1]) / 2.0 * if t % 2 == 0 { 1.0 } else { -1.0 };
                for k in 1..n / 2 {
                    let a = std::f64::consts::TAU * (k * t) as f64 / n as f64;
                    s += f64::from(c[2 * k]) * a.cos() + f64::from(c[2 * k + 1]) * a.sin();
                }
                s as f32
            })
            .collect()
    }

    #[test]
    fn fft_matches_direct_sum() {
        for n in [4usize, 8, 64, 512] {
            let c: Vec<f32> = (0..n).map(|i| ((i * 7919 % 113) as f32 - 56.0) / 17.0).collect();
            let mut out = vec![0.0; n];
            RealInverseFft::new(n).run(&c, &mut out);
            for (t, (got, want)) in out.iter().zip(direct(&c)).enumerate() {
                assert!((got - want).abs() < 1e-3 * (n as f32), "n={n} t={t}: {got} vs {want}");
            }
        }
    }

    #[test]
    fn bands_for_stereo_44k() {
        let d = AudioDecoder::new(&AudioTrack { sample_rate: 44100, flags: 0xE000, id: 0 }).unwrap();
        assert_eq!((d.frame_len, d.overlap), (4096, 256));
        assert_eq!(d.bands.len(), 26);
        assert_eq!(d.bands[..4], [2, 8, 18, 26]);
        assert_eq!(*d.bands.last().unwrap(), 4096);
    }

    #[test]
    fn rejects_dct_tracks() {
        assert!(AudioDecoder::new(&AudioTrack { sample_rate: 22050, flags: 0x1000, id: 0 }).is_err());
    }

    #[test]
    fn reads_bink_floats() {
        // power 24, mantissa 3, sign 1  ->  -3 * 2^1 = -6
        let mut bits = 24u64 | (3 << 5) | (1 << 28);
        let bytes: Vec<u8> = (0..8)
            .map(|_| {
                let b = bits as u8;
                bits >>= 8;
                b
            })
            .collect();
        assert_eq!(AudioDecoder::read_float(&mut BitReader::new(&bytes)), -6.0);
    }
}
