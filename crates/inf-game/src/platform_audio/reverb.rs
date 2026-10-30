//! The ambience's shared reverb (step 5 of
//! [the plan](../../../../docs/rt-plans/atmosphere.md)): the echo and tail
//! of a place, which every placed sound sends a share to, so its cost does
//! not grow with the number of sounds. A feedback delay network: eight
//! delay lines, 25 to 65 ms (primes, so their echoes never line up), fed
//! back through a Householder matrix (each line gives back itself less a
//! quarter of their sum), each line losing what its length's share of the
//! decay time takes and its highs through a one-pole low-pass. The send goes
//! in with alternating signs; left and right take the lines with two
//! different sign patterns, so the tail spreads wide.

/// The lines' lengths in frames at 44.1 kHz.
const LENGTHS: [usize; 8] = [1117, 1361, 1559, 1777, 2003, 2273, 2551, 2887];
const SIGNS: [f32; 8] = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
const LEFT: [f32; 8] = [1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0];
const RIGHT: [f32; 8] = [1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0];

/// A place's reverb: its decay time to -60 dB (seconds), how dark its tail
/// is (0 bright to near 1 dull), and how loud the tail is against the dry
/// sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReverbShape {
    pub seconds: f32,
    pub damping: f32,
    pub wet: f32,
}

#[derive(Debug)]
pub struct Reverb {
    lines: [Vec<f32>; 8],
    at: [usize; 8],
    low: [f32; 8],
    gains: [f32; 8],
    damping: f32,
    wet: f32,
    shape: Option<ReverbShape>,
}

impl Default for Reverb {
    fn default() -> Self {
        Self {
            lines: LENGTHS.map(|length| vec![0.0; length]),
            at: [0; 8],
            low: [0.0; 8],
            gains: [0.0; 8],
            damping: 0.0,
            wet: 0.0,
            shape: None,
        }
    }
}

impl Reverb {
    /// Take on `shape`: each line keeps 10^(-3 length / (decay rate)).
    pub fn shape(&mut self, shape: ReverbShape) {
        if self.shape == Some(shape) {
            return;
        }
        self.shape = Some(shape);
        let frames = shape.seconds.max(0.05) * crate::platform_audio::OUTPUT_RATE as f32;
        self.gains = LENGTHS.map(|length| 10_f32.powf(-3.0 * length as f32 / frames));
        self.damping = shape.damping.clamp(0.0, 0.95);
        self.wet = shape.wet.max(0.0);
    }

    /// The tail for one frame of `send` (the voices' mono share), left and
    /// right.
    pub fn next(&mut self, send: f32) -> [f32; 2] {
        let outputs: [f32; 8] = std::array::from_fn(|line| self.lines[line][self.at[line]]);
        let sum: f32 = outputs.iter().sum();
        let mut out = [0.0_f32; 2];
        for line in 0..8 {
            // Householder: each line gets back itself less a quarter of all.
            let back = outputs[line] - 0.25 * sum;
            self.low[line] += (back - self.low[line]) * (1.0 - self.damping);
            let fed = self.low[line] * self.gains[line] + send * SIGNS[line] * 0.5;
            self.lines[line][self.at[line]] = fed;
            self.at[line] = (self.at[line] + 1) % LENGTHS[line];
            out[0] += outputs[line] * LEFT[line];
            out[1] += outputs[line] * RIGHT[line];
        }
        // The eight lines' sum, brought back to the send's scale.
        out.map(|channel| channel * self.wet * 0.35)
    }

    /// Clear the tail (a world's start).
    pub fn clear(&mut self) {
        for line in &mut self.lines {
            line.fill(0.0);
        }
        self.low = [0.0; 8];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaped(seconds: f32) -> Reverb {
        let mut reverb = Reverb::default();
        reverb.shape(ReverbShape {
            seconds,
            damping: 0.2,
            wet: 1.0,
        });
        reverb
    }

    /// Energy over `frames` frames of the tail after an impulse.
    fn energy(reverb: &mut Reverb, frames: usize) -> f32 {
        (0..frames)
            .map(|_| reverb.next(0.0))
            .map(|[left, right]| left * left + right * right)
            .sum()
    }

    /// Silence in, silence out; an impulse rings out, longer for a longer
    /// decay, and the tail fades rather than growing.
    #[test]
    fn an_impulse_rings_out_and_fades() {
        let mut reverb = shaped(1.0);
        assert_eq!(reverb.next(0.0), [0.0, 0.0]);
        reverb.next(1.0);
        let early = energy(&mut reverb, 11_025);
        let late = energy(&mut reverb, 11_025);
        assert!(early > 0.0 && late < early, "{early} then {late}");
        let mut long = shaped(3.0);
        long.next(1.0);
        energy(&mut long, 11_025);
        assert!(energy(&mut long, 11_025) > late);
    }

    /// About -60 dB a decay time on: the energy of the second half second
    /// after a second against the first half second's.
    #[test]
    fn the_decay_time_holds() {
        let mut reverb = shaped(1.0);
        reverb.next(1.0);
        let first = energy(&mut reverb, 22_050);
        energy(&mut reverb, 22_050);
        let after = energy(&mut reverb, 22_050);
        let drop = 10.0 * (after / first).log10();
        assert!((-75.0..-45.0).contains(&drop), "{drop} dB");
    }

    /// Left and right differ: the tail is wide.
    #[test]
    fn the_tail_is_wide() {
        let mut reverb = shaped(1.5);
        reverb.next(1.0);
        let frames: Vec<[f32; 2]> = (0..5_000).map(|_| reverb.next(0.0)).collect();
        assert!(frames
            .iter()
            .any(|[left, right]| (left - right).abs() > 1.0e-4));
    }
}
