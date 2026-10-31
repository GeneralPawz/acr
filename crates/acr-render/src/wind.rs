//! The wind field every swaying plant shares: `wind.wgsl` in Rust, for the readers outside
//! the shaders (weather sound, [`crate::weather`]) and the tests. Models (`model.wgsl`) and
//! ground cover (`ground_cover.wgsl`) sway by [`gust`] and [`trunk_bend`] alike, so plants of
//! one kind sway at one frequency, and a plant differs by its phase and stiffness only.

const WIND_LEAN: f32 = 0.06;
const WIND_ROCK: f32 = 0.07;
const WIND_ROCK_RATE: f32 = 1.1;
const FLAG_RIPPLE: f32 = 0.16;
const FLAG_WAVE_K: f32 = 9.0;
const FLAG_RATE: f32 = 5.0;
const FLAG_PACE: f32 = 0.45;
const FLAG_LULL: f32 = 0.4;
const FLAG_TRAIL: f32 = 0.12;

/// The gust strength (0..1) at world point `at` (px) and wind clock `clock` for wind
/// direction `dir` (unit): `gust` of `wind.wgsl`. The fronts travel along `dir`.
pub fn gust(at: [f32; 2], dir: [f32; 2], clock: f32) -> f32 {
    let along = at[0] * dir[0] + at[1] * dir[1];
    let across = -at[0] * dir[1] + at[1] * dir[0];
    let a = (along * 0.0052 - clock * 0.78 + 1.6 * (across * 0.0021 + clock * 0.05).sin()).sin();
    let b = (along * 0.003 - clock * 0.33 + across * 0.0013 + 1.7).sin();
    let g = (0.45 + 0.35 * a + 0.3 * b).clamp(0.0, 1.0);
    g * g * (3.0 - 2.0 * g)
}

/// The downwind lean (before stiffness and the wind's strength) of a plant of `tempo` with
/// per-plant `phase` in gust `g` at `clock`: `trunk_bend` of `wind.wgsl`.
pub fn trunk_bend(g: f32, clock: f32, tempo: f32, phase: f32) -> f32 {
    WIND_LEAN * g + WIND_ROCK * (clock * tempo * WIND_ROCK_RATE + phase).sin() * (0.3 + 0.7 * g)
}

/// The strength (0..1.5) of a flag's ripple in wind `s` (0..1) and gust `g`, for a bearer
/// walking at `pace` m/s: `flag_strength` of `wind.wgsl`.
pub fn flag_strength(s: f32, g: f32, pace: f32) -> f32 {
    (s * (FLAG_LULL + (1.0 - FLAG_LULL) * g) + FLAG_PACE * pace).min(1.5)
}

/// The cloth's displacement (m) along its normal at distance `u` (0 pole .. 1 fly) from the
/// pole, `row` (0..1) down the hoist, wind clock `clock`: `flag_wave` of `wind.wgsl`.
pub fn flag_wave(u: f32, row: f32, clock: f32, strength: f32) -> f32 {
    let envelope = u * (0.35 + 0.65 * u);
    FLAG_RIPPLE * strength * envelope * (FLAG_WAVE_K * u - FLAG_RATE * clock + 1.3 * row).sin()
}

/// Where the cloth's free edge streams (m, world xy) before the weight `u²`: downwind with
/// the wind `s` along `dir`, opposite to the bearer's `pace` along the unit `heading`:
/// `flag_drift` of `wind.wgsl`.
pub fn flag_drift(dir: [f32; 2], s: f32, heading: [f32; 2], pace: f32) -> [f32; 2] {
    let back = FLAG_PACE * pace * 2.0;
    [(dir[0] * s - heading[0] * back) * FLAG_TRAIL, (dir[1] * s - heading[1] * back) * FLAG_TRAIL]
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: [f32; 2] = [0.94, 0.34];

    /// The rocking's dominant angular frequency (rad/s) of a plant in a steady gust, found
    /// by the spacing of the zero crossings of the bend about its mean.
    fn rock_frequency(tempo: f32, phase: f32, stiffness: f32) -> f32 {
        let dt = 0.002;
        let v: Vec<f32> = (0..20_000).map(|i| stiffness * trunk_bend(1.0, i as f32 * dt, tempo, phase)).collect();
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        let ups: Vec<f32> =
            v.windows(2).enumerate().filter(|(_, w)| w[0] < mean && w[1] >= mean).map(|(i, _)| i as f32 * dt).collect();
        std::f32::consts::TAU * (ups.len() - 1) as f32 / (ups[ups.len() - 1] - ups[0])
    }

    #[test]
    fn one_kind_rocks_at_one_frequency_whatever_phase_and_stiffness() {
        let base = rock_frequency(1.0, 0.0, 1.0);
        assert!((base - WIND_ROCK_RATE).abs() < 0.02, "{base}");
        for (phase, stiffness) in [(1.3, 0.8), (4.0, 1.2), (5.9, 1.0)] {
            let f = rock_frequency(1.0, phase, stiffness);
            assert!((f - base).abs() < 0.02, "{phase} {stiffness}: {f}");
        }
        // Only the kind's tempo changes the frequency.
        assert!((rock_frequency(2.0, 0.0, 1.0) - 2.0 * base).abs() < 0.04);
    }

    #[test]
    fn gust_phase_depends_on_the_position_along_the_wind() {
        // The same clock, 300 px apart along the wind: the gusts differ.
        let at = [3000.0, 2000.0];
        let ahead = [at[0] + DIR[0] * 300.0, at[1] + DIR[1] * 300.0];
        assert!((0..50).any(|i| (gust(at, DIR, i as f32 * 0.7) - gust(ahead, DIR, i as f32 * 0.7)).abs() > 0.05));
        // Positions with the same distance along the wind (and nearby across) agree.
        let side = [at[0] - DIR[1] * 10.0, at[1] + DIR[0] * 10.0];
        for i in 0..50 {
            let t = i as f32 * 0.7;
            assert!((gust(at, DIR, t) - gust(side, DIR, t)).abs() < 0.1);
        }
        // The front travels downwind: what `at` sees now, a point ahead sees later.
        let (speed, mut best) = (0.78 / 0.0052, (f32::MAX, 0.0));
        for i in 0..2000 {
            let late = i as f32 * 0.01;
            let d = (gust(at, DIR, 1.0) - gust(ahead, DIR, 1.0 + late)).abs();
            if d < best.0 {
                best = (d, late);
            }
        }
        assert!(best.1 > 0.0 && best.1 < 300.0 / speed * 3.0, "{best:?}");
        for i in 0..200 {
            assert!((0.0..=1.0).contains(&gust(at, DIR, i as f32 * 0.37)));
        }
    }

    /// The largest displacement of the cloth at `u` over a few seconds.
    fn peak(u: f32, strength: f32) -> f32 {
        (0..600).map(|i| flag_wave(u, 0.3, i as f32 * 0.01, strength).abs()).fold(0.0, f32::max)
    }

    #[test]
    fn the_flag_stays_put_at_the_pole() {
        for i in 0..100 {
            for row in [0.0, 0.5, 1.0] {
                assert_eq!(flag_wave(0.0, row, i as f32 * 0.37, 1.5), 0.0);
            }
        }
        // And moves away from it, more towards the fly.
        assert!(peak(0.3, 1.0) > 0.0 && peak(1.0, 1.0) > peak(0.3, 1.0));
    }

    #[test]
    fn the_ripple_travels_from_the_pole_to_the_fly() {
        // The first node (the wave's sign changes from + to -) past u = 0.5 moves outwards
        // with the clock, at the wave's phase speed.
        let node = |t: f32| {
            (300..1000)
                .map(|i| i as f32 * 0.001)
                .find(|&u| flag_wave(u, 0.0, t, 1.0) > 0.0 && flag_wave(u + 0.001, 0.0, t, 1.0) <= 0.0)
        };
        let (t0, t1) = (0.2, 0.3);
        // A node leaves through the fly and a new one comes in at the pole, so follow the
        // nearest to where the first one went.
        let (a, b) = (node(t0).unwrap(), node(t1));
        let moved = b.unwrap_or(1.0) - a;
        assert!(moved > 0.0, "{a} {b:?}");
        let speed = FLAG_RATE / FLAG_WAVE_K;
        assert!((moved / (t1 - t0) - speed).abs() < 0.05 * speed + 0.02, "{moved} vs {}", speed * (t1 - t0));
    }

    #[test]
    fn the_ripple_grows_with_the_wind_and_the_bearers_pace() {
        let calm = flag_strength(0.0, 0.5, 0.0);
        assert_eq!(calm, 0.0);
        let mut last = 0.0;
        for s in [0.2, 0.5, 1.0] {
            let p = peak(0.8, flag_strength(s, 0.5, 0.0));
            assert!(p > last, "{s}: {p}");
            last = p;
        }
        let mut last = 0.0;
        for pace in [0.0, 0.6, 1.2, 2.4] {
            let p = peak(0.8, flag_strength(0.3, 0.5, pace));
            assert!(p > last, "{pace}: {p}");
            last = p;
        }
        // A bearer walking in a dead calm still makes cloth move; a gust adds to the wind.
        assert!(peak(0.8, flag_strength(0.0, 0.0, 1.2)) > 0.02);
        assert!(flag_strength(0.5, 1.0, 0.0) > flag_strength(0.5, 0.0, 0.0));
    }

    #[test]
    fn the_free_edge_trails_downwind_and_opposite_to_the_bearer() {
        let heading = [0.0, 1.0];
        let walking = flag_drift([0.0, 0.0], 0.0, heading, 1.2);
        assert!(walking[1] < 0.0 && walking[0].abs() < 1e-6, "{walking:?}");
        let windy = flag_drift(DIR, 1.0, heading, 0.0);
        assert!(windy[0] > 0.0 && windy[1] > 0.0);
        // Walking and the wind add up when they push the same way.
        let both = flag_drift([0.0, -1.0], 1.0, heading, 1.2);
        assert!(both[1] < flag_drift([0.0, -1.0], 1.0, heading, 0.0)[1]);
    }
}
