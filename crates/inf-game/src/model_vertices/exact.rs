//! Non-native exact twins of the slots, for Vulkan's drawing alone (the
//! plan's item 15). Retail keeps every slot in whole world units: a raw
//! vertex rounds as `00490900` places it, opcode 26's blend moves in steps
//! of two units (`004EC950`), a child's view position rounds (`0048FB40`).
//! At 4K that hops the ACAM's gun pods several pixels against its hull.
//! Beside each slot the store can keep the point the same formulas give
//! without the rounding, and its view-space point, which the tracer then
//! draws from. The game reads none of it: the slots, the behaviour groups
//! and every native byte stay retail's.

use std::rc::Rc;

use crate::fast_hash::FastMap;

use crate::model_context::ModelContext;

/// The twins of one invocation's slots, kept while its frame wants them.
#[derive(Clone, Debug)]
pub(crate) struct ExactTwins {
    /// The model's origin without rounding: its object's position, or the
    /// twin of the parent slot a child stands on.
    pub origin: [f64; 3],
    /// Each slot's world point, where an exact writer set it.
    pub worlds: Vec<Option<[f64; 3]>>,
    /// Each slot's view-space point, as its last projection placed it.
    pub views: Vec<Option<[f64; 3]>>,
    /// The exact native screen point of each projection, by the packed
    /// whole-pixel XY it rounds to: what the raster's corners take.
    pub screens: Rc<FastMap<u32, [f32; 2]>>,
}

impl ExactTwins {
    pub fn new(origin: [f64; 3], slots: usize) -> Self {
        Self {
            origin,
            worlds: vec![None; slots],
            views: vec![None; slots],
            screens: Rc::default(),
        }
    }
}

/// `transform_world` (`00490900`) without rounding, from the exact origin.
pub(crate) fn local_world(context: &ModelContext, origin: [f64; 3], local: [i32; 3]) -> [f64; 3] {
    std::array::from_fn(|component| {
        let coefficient = |first| f64::from(f32::from_bits(context.basis.words[first + component]));
        let [x, y, z] = local.map(f64::from);
        z * coefficient(7) + y * coefficient(4) + x * coefficient(1) + origin[component]
    })
}

/// Opcode 26's blend (`004EC950`) without its two-unit steps.
pub(crate) fn interpolate(left: f64, right: f64, weight: u16) -> f64 {
    left + (right - left) * f64::from(weight) / 65536.0
}

/// A Q31 fraction as a real.
pub(crate) fn q31(value: i32) -> f64 {
    f64::from(value) / 2_147_483_648.0
}

/// `world_stream::cubic` without its Q31 truncations: with `u` twice the
/// fraction, `p0 + c1 u + c2 u^2 + c3 u^3`.
pub(crate) fn cubic([p0, p1, p2, p3]: [f64; 4], t: i32) -> f64 {
    let u = 2.0 * q31(t);
    let c1 = 3.0 * (p1 - p0);
    let c2 = 3.0 * (p0 - 2.0 * p1 + p2);
    let c3 = 3.0 * (p1 - p2) - p0 + p3;
    p0 + u * (c1 + u * (c2 + u * c3))
}

/// The world's wrap at `coordinate_shift`, as `world_coordinates` takes a
/// difference from the eye: its low `32 - shift` bits, signed.
pub(crate) fn wrap(delta: f64, coordinate_shift: u8) -> f64 {
    let shift = u32::from(coordinate_shift & 31);
    if shift == 0 {
        return delta;
    }
    let period = 2f64.powi(32 - shift as i32);
    let half = period / 2.0;
    (delta + half).rem_euclid(period) - half
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deployed (weight 0x5FFF), retail's blend errs up to two units low;
    /// the twin keeps the fraction.
    #[test]
    fn the_blend_keeps_what_retails_two_unit_steps_drop() {
        let native = super::super::interpolate(1000, 1185, 0x5fff);
        let exact = interpolate(1000.0, 1185.0, 0x5fff);
        assert!((exact - (1000.0 + 185.0 * 24575.0 / 65536.0)).abs() < 1e-9);
        assert_eq!(native % 2, 0);
        assert!(exact - f64::from(native) >= 0.0 && exact - f64::from(native) < 2.0);
    }

    /// The cubic agrees with retail's at the ends and keeps its fraction
    /// between.
    #[test]
    fn the_cubic_meets_retails_at_its_ends() {
        let points = [0, 300, 900, 1200];
        let exact = points.map(f64::from);
        for t in [0, 1 << 30] {
            let native = super::super::world_stream::cubic(points, t);
            assert!((cubic(exact, t) - f64::from(native)).abs() < 2.0, "{t}");
        }
        // Halfway (u 0.5): 450 + 225 - 75; retail's truncations, each
        // doubled up to three times, come to 594.
        let half = cubic(exact, 1 << 29);
        assert!((half - 600.0).abs() < 1e-9, "{half}");
        assert_eq!(super::super::world_stream::cubic(points, 1 << 29), 594);
    }

    /// A difference from the eye wraps at 16 bits as the projector's does.
    #[test]
    fn differences_wrap_as_the_projectors_do() {
        assert_eq!(wrap(40000.0, 16), 40000.0 - 65536.0);
        assert_eq!(wrap(-40000.0, 16), -40000.0 + 65536.0);
        assert_eq!(wrap(1234.5, 16), 1234.5);
        assert_eq!(wrap(1.0e9, 0), 1.0e9);
    }
}
