//! Arithmetic asymmetries of sphere-pair 004F1420 and group 004F2B30.

use super::{CollisionContact, ContactAxisNormals};
use crate::model_context::fistp_low32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactSphere {
    /// Native integer world coordinates, full signed DWORDs before subtraction.
    pub center: [i32; 3],
    /// Native scalar DWORD, not a header WORD, absolute value or bounded radius.
    pub radius: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactGeometryRequest {
    /// 004F1420: request +10/+14/+18/+20 versus callback vertex/scalar.
    SpherePair {
        request: ContactSphere,
        candidate: ContactSphere,
    },
    ///004F1F90: strict axis gates and nearest box face, including wrapped math.
    SphereBox {
        request: ContactSphere,
        center: [i32; 3],
        half_extents: [i32; 3],
        normals: ContactAxisNormals,
    },
    /// 004F1D20: a zero-radius request's point against a callback box.
    PointBox {
        point: [i32; 3],
        center: [i32; 3],
        half_extents: [i32; 3],
        normals: ContactAxisNormals,
    },
    /// 004F2B30 ignores vertex arguments and always publishes flag/score one.
    GroupDirection { relative_xyz: [i32; 3] },
    /// 004F15F0: the request's box against a callback sphere, with
    /// 004F1F90's gates and its faces mirrored.
    BoxSphere {
        center: [i32; 3],
        half_extents: [i32; 3],
        sphere: ContactSphere,
        normals: ContactAxisNormals,
    },
    /// 004F23B0: the request's box against a callback box.
    BoxBox {
        center: [i32; 3],
        half_extents: [i32; 3],
        other_center: [i32; 3],
        other_half_extents: [i32; 3],
        normals: ContactAxisNormals,
    },
}

/// Execute only the supplied primitive. The original callback returns void;
/// the caller's existing result fields remain authoritative on rejection.
/// Floating arithmetic follows supported CRT FPCW 027F (53-bit, nearest-even).
pub fn apply_contact_geometry(contact: &mut CollisionContact, request: ContactGeometryRequest) {
    match request {
        ContactGeometryRequest::SpherePair { request, candidate } => {
            sphere_pair(contact, request, candidate);
        }
        ContactGeometryRequest::GroupDirection { relative_xyz } => {
            group_direction(contact, relative_xyz);
        }
        ContactGeometryRequest::SphereBox {
            request,
            center,
            half_extents,
            normals,
        } => {
            super::sphere_box::sphere_box(contact, request, center, half_extents, normals);
        }
        ContactGeometryRequest::PointBox {
            point,
            center,
            half_extents,
            normals,
        } => super::sphere_box::point_box(contact, point, center, half_extents, normals),
        ContactGeometryRequest::BoxSphere {
            center,
            half_extents,
            sphere,
            normals,
        } => super::box_sphere::box_sphere(contact, center, half_extents, sphere, normals),
        ContactGeometryRequest::BoxBox {
            center,
            half_extents,
            other_center,
            other_half_extents,
            normals,
        } => super::sphere_box::box_box(
            contact,
            center,
            half_extents,
            other_center,
            other_half_extents,
            normals,
        ),
    }
}

fn sphere_pair(contact: &mut CollisionContact, request: ContactSphere, candidate: ContactSphere) {
    let delta: [i32; 3] =
        std::array::from_fn(|i| request.center[i].wrapping_sub(candidate.center[i]));
    let radius = request.radius.wrapping_add(candidate.radius);
    let diameter = radius.wrapping_mul(2) as u32;
    // 4F144B..69 uses unsigned inclusive comparisons of these wrapped DWORDs.
    if delta
        .iter()
        .any(|delta| radius.wrapping_add(*delta) as u32 > diameter)
    {
        return;
    }
    // 4F146F..89: low DWORD Z² + Y² + X², compared unsigned with low R².
    let square = |value: i32| value.wrapping_mul(value) as u32;
    let squared = square(delta[2])
        .wrapping_add(square(delta[1]))
        .wrapping_add(square(delta[0]));
    if squared > square(radius) {
        return;
    }
    // 4F1495, before the first floating result store.
    contact.write_flag(1);
    // FILD QWORD has a zero high DWORD: the wrapped squared sum is unsigned.
    // FSQRT remains in x87 53-bit precision; there is no f32 denominator spill.
    let distance = f64::from(squared).sqrt();
    if distance == 0.0 {
        contact.write_normal(0, 1.0);
        contact.write_normal(1, 0.0);
        contact.write_normal(2, 0.0);
    } else {
        for (component, delta) in delta.into_iter().enumerate() {
            // The gates use full DWORD deltas; numerators use signed LOWWORDs.
            contact.write_normal(component, (f64::from(delta as i16) / distance) as f32);
        }
    }
    // 4F14ED FISTP QWORD uses nearest-even, then only its low DWORD is loaded.
    contact.write_displacement(radius.wrapping_sub(fistp_low32(distance)));
}

fn group_direction(contact: &mut CollisionContact, relative_xyz: [i32; 3]) {
    // 4F2B37/3D precede every coordinate read and normal write.
    contact.write_flag(1);
    contact.write_displacement(1);
    let [x, y, z] = relative_xyz.map(|value| f64::from(value as i16));
    // All i16 inputs are exactly representable in f32. Y/Z spills therefore
    // retain them, but 4F2B86's FSQRT spill changes the divisor used below.
    let distance = ((z * z + y * y) + x * x).sqrt() as f32;
    if distance == 0.0 {
        contact.write_normal(0, 1.0);
        contact.write_normal(1, 0.0);
        contact.write_normal(2, 0.0);
    } else {
        for (component, value) in [x, y, z].into_iter().enumerate() {
            contact.write_normal(component, (value / f64::from(distance)) as f32);
        }
    }
}
