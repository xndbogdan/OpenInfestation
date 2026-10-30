//! The path tracer's random numbers (the full path tracing plan's stage 6):
//! a spatiotemporal blue-noise texture, 64 frames of 64 by 64 pairs of
//! numbers in [0, 1), each frame's pairs blue over the screen and each
//! texel's pairs blue over the frames (Wolfe, Morrical, Akenine-Möller and
//! Ramamoorthi, "Spatiotemporal Blue Noise Masks", EGSR 2022). Made here, not
//! taken from NVIDIA's (whose licence binds it to their SDK): by Georgiev and
//! Fajardo's swaps ("Blue-noise Dithered Sampling", SIGGRAPH 2016 talk), each
//! frame holding a jittered stratum of every one of 64 by 64 cells of the
//! square, under STBN's energy, which weighs a texel against the others of
//! its frame by their distance on screen and against its own texel in the
//! other frames by their distance in time. A pixel's numbers then spread
//! over the square frame by frame, and its neighbours' differ from its own:
//! the denoiser's histories and filters gather even samples faster than
//! white ones, and faster than the interleaved gradient noise turned by the
//! golden ratios it replaces.

/// Two bytes a texel (the pair's first number, then its second, each as
/// (byte + 0.5) / 256), frame by frame and row by row: what
/// `tests::writes_the_blue_noise` makes.
pub(super) static BLUE_NOISE: &[u8] = include_bytes!("../../data/blue_noise.bin");

#[cfg(test)]
mod tests {
    use super::BLUE_NOISE;

    /// The texture's side and frames (pt_trace.comp's even_numbers).
    const SIDE: usize = 64;
    const FRAMES: usize = 64;
    /// The energy's reach on screen (GF16's sigma_i) and in frames, and how
    /// many texels it reaches each way (at six its weight is under a
    /// three-thousandth).
    const SIGMA_SPACE: f32 = 2.1;
    const SIGMA_TIME: f32 = 2.1;
    const REACH: i32 = 6;
    /// Frames far enough apart that their swaps touch none of each other's
    /// energy, swapped at once.
    const APART: usize = 2 * REACH as usize + 1;
    const SWEEPS: usize = 192;

    struct Rng(u64);

    impl Rng {
        // splitmix64
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    fn value(texel: [u8; 2]) -> [f32; 2] {
        texel.map(|byte| (f32::from(byte) + 0.5) / 256.0)
    }

    /// The energy a texel at `at` of frame `t` would hold with value `v`:
    /// against the others of its frame (`own`, as they stand) by their
    /// distance on screen, and against its own texel in the frames round it
    /// (`all`, as they stood before this round) by theirs in time, each by
    /// how near the values lie (GF16's sample term, its distance to the power
    /// of half the two dimensions).
    fn energy(
        own: &[[u8; 2]],
        all: &[[u8; 2]],
        t: usize,
        at: usize,
        v: [f32; 2],
        space: &[f32],
        time: &[f32],
    ) -> f32 {
        let side = SIDE as i32;
        let (x, y) = ((at % SIDE) as i32, (at / SIDE) as i32);
        let term = |other: [u8; 2]| {
            let o = value(other);
            (-((v[0] - o[0]).powi(2) + (v[1] - o[1]).powi(2)).sqrt()).exp()
        };
        let mut sum = 0.0;
        let span = 2 * REACH + 1;
        for dy in -REACH..=REACH {
            for dx in -REACH..=REACH {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let w = space[((dy + REACH) * span + dx + REACH) as usize];
                let there = ((y + dy).rem_euclid(side) * side + (x + dx).rem_euclid(side)) as usize;
                sum += w * term(own[there]);
            }
        }
        for dt in -REACH..=REACH {
            if dt == 0 {
                continue;
            }
            let frame = (t as i32 + dt).rem_euclid(FRAMES as i32) as usize;
            sum += time[(dt + REACH) as usize] * term(all[frame * SIDE * SIDE + at]);
        }
        sum
    }

    fn generate() -> Vec<u8> {
        let n = SIDE * SIDE;
        let mut rng = Rng(0x5354_424e);
        let mut texels = vec![[0_u8; 2]; FRAMES * n];
        for frame in texels.chunks_mut(n) {
            for (cell, texel) in frame.iter_mut().enumerate() {
                *texel = [
                    (cell % SIDE * 4 + rng.below(4)) as u8,
                    (cell / SIDE * 4 + rng.below(4)) as u8,
                ];
            }
            for i in (1..n).rev() {
                frame.swap(i, rng.below(i + 1));
            }
        }
        let span = 2 * REACH + 1;
        let space: Vec<f32> = (0..span * span)
            .map(|i| {
                let (dx, dy) = (i % span - REACH, i / span - REACH);
                (-((dx * dx + dy * dy) as f32) / (SIGMA_SPACE * SIGMA_SPACE)).exp()
            })
            .collect();
        let time: Vec<f32> = (-REACH..=REACH)
            .map(|dt| (-((dt * dt) as f32) / (SIGMA_TIME * SIGMA_TIME)).exp())
            .collect();
        for sweep in 0..SWEEPS {
            for phase in 0..APART {
                let all = texels.clone();
                std::thread::scope(|scope| {
                    for (t, own) in texels.chunks_mut(n).enumerate() {
                        if t % APART != phase {
                            continue;
                        }
                        let (all, space, time) = (&all, &space, &time);
                        scope.spawn(move || {
                            let mut rng = Rng((sweep * FRAMES + t) as u64 ^ 0x5354_424e_0000);
                            for _ in 0..n {
                                let a = rng.below(n);
                                // Half the swaps anywhere in the frame, half near.
                                let b = if rng.next() & 1 == 0 {
                                    rng.below(n)
                                } else {
                                    let x = (a % SIDE + rng.below(17) + SIDE - 8) % SIDE;
                                    let y = (a / SIDE + rng.below(17) + SIDE - 8) % SIDE;
                                    y * SIDE + x
                                };
                                if a == b {
                                    continue;
                                }
                                let (va, vb) = (value(own[a]), value(own[b]));
                                let before = energy(own, all, t, a, va, space, time)
                                    + energy(own, all, t, b, vb, space, time);
                                let after = energy(own, all, t, a, vb, space, time)
                                    + energy(own, all, t, b, va, space, time);
                                if after < before {
                                    own.swap(a, b);
                                }
                            }
                        });
                    }
                });
            }
        }
        texels.into_iter().flatten().collect()
    }

    /// Writes the texture into the crate's data (about three minutes in a release
    /// build): run explicitly after changing the generator.
    #[test]
    #[ignore = "writes data/blue_noise.bin; run explicitly"]
    fn writes_the_blue_noise() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/data/blue_noise.bin");
        std::fs::write(path, generate()).unwrap();
    }

    /// Each frame of the texture holds one jittered stratum of every cell of
    /// the square, and neighbours on screen and in time lie further apart in
    /// value than random pairs do (the energy's work).
    #[test]
    fn blue_noise_frames_are_stratified_and_spread() {
        let n = SIDE * SIDE;
        assert_eq!(BLUE_NOISE.len(), FRAMES * n * 2);
        let texel = |t: usize, x: usize, y: usize| {
            let at = ((t % FRAMES) * n + (y % SIDE) * SIDE + x % SIDE) * 2;
            [BLUE_NOISE[at], BLUE_NOISE[at + 1]]
        };
        for t in 0..FRAMES {
            let mut seen = vec![false; n];
            for i in 0..n {
                let [a, b] = texel(t, i % SIDE, i / SIDE);
                let cell = usize::from(b / 4) * SIDE + usize::from(a / 4);
                assert!(!seen[cell], "frame {t} holds cell {cell} twice");
                seen[cell] = true;
            }
        }
        let apart = |p: [u8; 2], q: [u8; 2]| {
            let (p, q) = (value(p), value(q));
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
        };
        let (mut space, mut time) = (0.0, 0.0);
        for t in 0..FRAMES {
            for y in 0..SIDE {
                for x in 0..SIDE {
                    space += apart(texel(t, x, y), texel(t, x + 1, y));
                    time += apart(texel(t, x, y), texel(t + 1, x, y));
                }
            }
        }
        let count = (FRAMES * n) as f32;
        // Two numbers even in the unit square lie 0.52 apart on average; the
        // texture's neighbours 0.59 on screen and 0.66 in time.
        assert!(
            space / count > 0.57,
            "neighbours on screen {}",
            space / count
        );
        assert!(time / count > 0.62, "neighbours in time {}", time / count);
    }
}
