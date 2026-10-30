//! 004F15F0, reached by box table 00544C50+00: the box the composite's 8F
//! recursion carries (request center +10/+14/+18, half extents
//! +28/+2C/+30) against an 8E sphere of the counterpart's program.
use super::{CollisionContact, ContactAxisNormals, ContactSphere};

pub(super) fn box_sphere(
    contact: &mut CollisionContact,
    center: [i32; 3],
    half_extents: [i32; 3],
    sphere: ContactSphere,
    normals: ContactAxisNormals,
) {
    //4F160B clears only the flag, before any input-dependent rejection.
    contact.write_flag(0);
    for i in 0..3 {
        if center[i].wrapping_add(half_extents[i]) <= sphere.center[i].wrapping_sub(sphere.radius)
            || center[i].wrapping_sub(half_extents[i])
                >= sphere.center[i].wrapping_add(sphere.radius)
        {
            return;
        }
    }
    contact.write_flag(1);
    // The faces mirror 004F1F90's: the box is the request here, so the
    // first distance pairs with the positive normal and a strictly smaller
    // second takes the negative one. Wrapped INT_MIN stays negative.
    let face: [(i32, bool); 3] = std::array::from_fn(|i| {
        let positive = center[i]
            .wrapping_sub(sphere.center[i])
            .wrapping_sub(half_extents[i])
            .wrapping_sub(sphere.radius)
            .wrapping_abs();
        let negative = center[i]
            .wrapping_sub(sphere.center[i])
            .wrapping_add(half_extents[i])
            .wrapping_add(sphere.radius)
            .wrapping_abs();
        if negative < positive {
            (negative, true)
        } else {
            (positive, false)
        }
    });
    //4F173B..: X only below both Z and Y, then Z below Y, else Y.
    let axis = if face[0].0 < face[2].0 {
        if face[0].0 < face[1].0 {
            0
        } else {
            1
        }
    } else if face[2].0 < face[1].0 {
        2
    } else {
        1
    };
    let normal = if face[axis].1 {
        normals.negative[axis]
    } else {
        normals.positive[axis]
    };
    for (i, bits) in normal.into_iter().enumerate() {
        contact.write_normal(i, f32::from_bits(bits));
    }
    contact.write_displacement(face[axis].0);
}
