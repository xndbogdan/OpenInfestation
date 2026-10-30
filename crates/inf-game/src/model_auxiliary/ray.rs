//! The ray table `00544C20` that the sight test's model hits run (`0047D080`
//! for an actor, `0043A7E0` for furniture).
//!
//! The request holds the model's origin in the ray's frame (`+10..+18`), the
//! ray's basis (`+34`) and its length (`+38`); the ray leaves the origin
//! along +Z. A hit records the length left past its entry (`+10`), so the
//! nearest hit scores highest. Every test is integer arithmetic but the box
//! test's direction, which `FISTP` rounds from the basis.

use inf_formats::native_basis::{isqrt_48efc0, NativeBasis};

/// The ray's `+34` basis and `+38` length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RayRequest {
    pub basis: NativeBasis,
    pub length: i32,
}

/// `00520A50`: the scale of the box test's direction.
const DIRECTION_SCALE: f64 = 1024.0;

fn add(relative: [i32; 3], vertex: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|axis| relative[axis].wrapping_add(vertex[axis]))
}

/// `004F37F0`, `+00` (opcode 8E): the sphere of `radius` at `vertex`.
/// Returns the hit's `+10`.
pub(super) fn sphere(
    ray: RayRequest,
    relative: [i32; 3],
    vertex: [i32; 3],
    radius: i32,
) -> Option<i32> {
    let [x, y, z] = add(relative, vertex);
    if z.wrapping_add(radius) < 0 || z.wrapping_sub(radius) > ray.length {
        return None;
    }
    let lateral = (y.wrapping_mul(y) as u32).wrapping_add(x.wrapping_mul(x) as u32);
    let squared = radius.wrapping_mul(radius) as u32;
    if lateral > squared {
        return None;
    }
    // MOVSX AX: the root's low WORD, sign-extended.
    let root = isqrt_48efc0(squared.wrapping_sub(lateral) as i32) as u16 as i16;
    let entry = z.wrapping_sub(i32::from(root));
    if entry > ray.length {
        return None;
    }
    Some(ray.length.wrapping_sub(entry.max(0)))
}

/// `004F3880`, `+08` (opcode 8F): the world-aligned box of `half` extents
/// around `vertex`. Its bounding sphere must reach the ray; the centre then
/// returns to world axes through the ray's basis (`004909E0`) and the ray's
/// direction is its forward row scaled by 1024. The z, x and y faces follow,
/// each through the face the ray meets first.
pub(super) fn box_hit(
    ray: RayRequest,
    relative: [i32; 3],
    vertex: [i32; 3],
    half: [i32; 3],
) -> Option<i32> {
    let [x, y, z] = add(relative, vertex);
    let bound = half.iter().fold(0i32, |sum, &extent| {
        sum.wrapping_add(extent.wrapping_mul(extent))
    });
    let lateral = (y.wrapping_mul(y) as u32).wrapping_add(x.wrapping_mul(x) as u32);
    if lateral > bound as u32 {
        return None;
    }
    let radius = isqrt_48efc0(bound) as i32;
    if radius.wrapping_add(z) < 0 || z.wrapping_sub(radius) > ray.length {
        return None;
    }
    let local = ray
        .basis
        .transform_words([x as i16, y as i16, z as i16])
        .map(|word| i32::from(word as i16));
    let direction: [i32; 3] = std::array::from_fn(|axis| {
        let product = f64::from(f32::from_bits(ray.basis.words[7 + axis])) * DIRECTION_SCALE;
        let rounded = product.round_ties_even();
        if rounded.is_finite()
            && (-9.223_372_036_854_776e18..9.223_372_036_854_776e18).contains(&rounded)
        {
            i32::from(rounded as i64 as i16)
        } else {
            0
        }
    });
    let [dx, dy, dz] = direction;
    let [lx, ly, lz] = local;
    let distance = |a: i32, b: i32, c: i32| -> Option<i32> {
        let squared = a
            .wrapping_mul(a)
            .wrapping_add(b.wrapping_mul(b))
            .wrapping_add(c.wrapping_mul(c));
        let distance = isqrt_48efc0(squared) as i32;
        (distance < ray.length).then(|| ray.length - distance)
    };
    let inside = |value: i32, extent: i32| value.wrapping_abs() < extent;
    // The z faces (004F3967).
    if dz != 0 {
        let mut depth = if dz > 0 {
            lz.wrapping_sub(half[2])
        } else {
            lz.wrapping_add(half[2])
        };
        let (mut across_x, mut across_y) = (0, 0);
        let reaches = if dz > 0 { depth >= 0 } else { depth <= 0 };
        if reaches {
            across_x = dx.wrapping_mul(depth) / dz;
            across_y = dy.wrapping_mul(depth) / dz;
        } else {
            depth = 0;
        }
        let across_x = across_x.wrapping_sub(lx);
        let across_y = across_y.wrapping_sub(ly);
        if inside(across_x, half[0]) && inside(across_y, half[1]) {
            return distance(depth, across_y, across_x);
        }
    }
    // The x faces (004F39E7): a negative direction takes the far face's
    // depth as it is.
    if dx != 0 {
        let mut depth = if dx > 0 {
            lx.wrapping_sub(half[0])
        } else {
            lx.wrapping_add(half[0])
        };
        let (mut across_z, mut across_y) = (0, 0);
        if dx > 0 && depth < 0 {
            depth = 0;
        } else {
            across_z = dz.wrapping_mul(depth) / dx;
            across_y = dy.wrapping_mul(depth) / dx;
        }
        let across_z = across_z.wrapping_sub(lz);
        let across_y = across_y.wrapping_sub(ly);
        if inside(across_z, half[2]) && inside(across_y, half[1]) {
            return distance(across_z, across_y, depth);
        }
    }
    // The y faces (004F3A7C).
    if dy != 0 {
        let mut depth = if dy > 0 {
            ly.wrapping_sub(half[1])
        } else {
            ly.wrapping_add(half[1])
        };
        let (mut across_z, mut across_x) = (0, 0);
        if dy > 0 && depth < 0 {
            depth = 0;
        } else {
            across_z = dz.wrapping_mul(depth) / dy;
            across_x = dx.wrapping_mul(depth) / dy;
        }
        let across_z = across_z.wrapping_sub(lz);
        let across_x = across_x.wrapping_sub(lx);
        if inside(across_z, half[2]) && inside(across_x, half[0]) {
            return distance(across_z, depth, across_x);
        }
    }
    None
}

/// The two group tests' shared bounds (`004F3B20`, `004F3C60`): the ray's
/// axis must pass between the vertices in x and in y (an OR of the values
/// above zero, or all of them negative, misses) and they must neither all
/// lie behind the origin nor all past the length.
fn group_bounds(length: i32, points: &[[i32; 3]]) -> bool {
    for axis in 0..2 {
        let or = points.iter().fold(0i32, |or, point| or | point[axis]);
        if or > 0 || points.iter().all(|point| point[axis] < 0) {
            return false;
        }
    }
    !points.iter().all(|point| point[2] < 0) && !points.iter().all(|point| point[2] > length)
}

/// `004F3B20`, `+0C` (opcode 89): four vertices. Within the bounds, their
/// mean depth (the wrapped sum, `SAR 2`) must fall short of the length.
/// Returns the hit's `+10`, the length less that mean.
pub(super) fn group4(length: i32, relative: [i32; 3], vertices: [[i32; 3]; 4]) -> Option<i32> {
    let points = vertices.map(|vertex| add(relative, vertex));
    if !group_bounds(length, &points) {
        return None;
    }
    let sum = points[3][2]
        .wrapping_add(points[2][2])
        .wrapping_add(points[1][2])
        .wrapping_add(points[0][2]);
    let mean = sum >> 2;
    (mean < length).then(|| length.wrapping_sub(mean))
}

/// `004F3C60`, `+10` (opcode 8A): three vertices; the mean is the wrapped
/// sum over three through `IMUL 55555556`, truncated toward zero.
pub(super) fn group3(length: i32, relative: [i32; 3], vertices: [[i32; 3]; 3]) -> Option<i32> {
    let points = vertices.map(|vertex| add(relative, vertex));
    if !group_bounds(length, &points) {
        return None;
    }
    let sum = points[2][2]
        .wrapping_add(points[1][2])
        .wrapping_add(points[0][2]);
    let high = ((i64::from(sum) * 0x5555_5556) >> 32) as i32;
    let mean = high.wrapping_add((high as u32 >> 31) as i32);
    (mean < length).then(|| length.wrapping_sub(mean))
}

#[cfg(test)]
#[path = "ray_tests.rs"]
mod tests;
