//! The plane test behind opcodes 89 (`+0C`) and 8A (`+10`) of the sphere and
//! point tables. The x87 runs at the retail CRT's 53-bit precision, so `f64`
//! arithmetic in the native order reproduces each rounding; an `FSTP` to a
//! DWORD rounds to `f32`. The ray table's tests are in `ray.rs`; the
//! composite's `004F2B30` is the contact geometry's group direction.

use crate::model_context::fistp_low32;

/// What `004F2930` and `004F2730` leave in the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlaneHit {
    /// The sphere does not reach the plane: the flag clears, nothing else.
    Miss,
    /// The flag sets; a smaller penetration (or a negative `+10`) also
    /// takes the normal and `+10`.
    Hit { penetration: i32, normal: [u32; 3] },
}

/// `004F2930` (the sphere table, `+0C` through `004F2720`, and `+10`) and
/// `004F2730` (the point table, radius 0): the plane through three vertices
/// against the request's sphere at `center` (`+10..+18`) of `radius`
/// (`+20`). The edges are WORD differences from the first vertex; the
/// cross product's three DWORDs, its length and two of the unit normal's
/// components pass through `f32`, the first component stays in the
/// register for the distance, which `FISTP` rounds.
pub(super) fn plane(vertices: [[i32; 3]; 3], center: [i32; 3], radius: i32) -> PlaneHit {
    let [v0, v1, v2] = vertices;
    let edge = |vertex: [i32; 3], axis: usize| {
        f64::from((vertex[axis] as u16).wrapping_sub(v0[axis] as u16) as i16)
    };
    let (e1x, e1y, e1z) = (edge(v1, 0), edge(v1, 1), edge(v1, 2));
    let (e2x, e2y, e2z) = (edge(v2, 0), edge(v2, 1), edge(v2, 2));
    let a = f64::from((e2z * e1y - e2y * e1z) as f32);
    let b = f64::from((e2x * e1z - e2z * e1x) as f32);
    let c = f64::from((e2y * e1x - e2x * e1y) as f32);
    let length = (c * c + b * b + a * a).sqrt();
    let (na, nb, nc) = if length == 0.0 {
        (1.0f64, 0.0f32, 0.0f32)
    } else {
        let length = f64::from(length as f32);
        (a / length, (b / length) as f32, (c / length) as f32)
    };
    let delta = |axis: usize| f64::from(center[axis].wrapping_sub(v0[axis]));
    let distance = delta(2) * f64::from(nc) + delta(1) * f64::from(nb) + delta(0) * na;
    let distance = fistp_low32(distance);
    let penetration = radius.wrapping_sub(distance);
    if penetration <= 0 {
        return PlaneHit::Miss;
    }
    PlaneHit::Hit {
        penetration,
        normal: [(na as f32).to_bits(), nb.to_bits(), nc.to_bits()],
    }
}

#[cfg(test)]
#[path = "group_tests.rs"]
mod tests;
