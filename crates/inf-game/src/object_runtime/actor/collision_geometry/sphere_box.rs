//!004F1F90, reached by sphere table00544C08+08, and the point table's
//!004F1D20.
use super::{CollisionContact, ContactSphere};

///Native cardinal vector DWORDs, preserving supplied bits without normalization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactAxisNormals {
    pub positive: [[u32; 3]; 3],
    pub negative: [[u32; 3]; 3],
}
impl ContactAxisNormals {
    ///CRT 0048F5D0/0048F630/0048F690 and 0048F6F0/0048F750/0048F7B0
    ///construct these six fixed vectors once.
    ///All other text references read them directly or pass readonly source
    ///arguments to490370/491D10/43B2B0/40D1B0/47D440; no later writer exists.
    ///The byte-authenticated startup receipts cover all six constructors.
    pub const NATIVE_STARTUP: Self = Self {
        positive: [[0x3f800000, 0, 0], [0, 0x3f800000, 0], [0, 0, 0x3f800000]],
        negative: [[0xbf800000, 0, 0], [0, 0xbf800000, 0], [0, 0, 0xbf800000]],
    };
}

pub(super) fn sphere_box(
    contact: &mut CollisionContact,
    request: ContactSphere,
    center: [i32; 3],
    half_extents: [i32; 3],
    normals: ContactAxisNormals,
) {
    nearest_face(
        contact,
        request.center,
        [request.radius; 3],
        center,
        half_extents,
        normals,
    );
}

///004F23B0, the box table 00544C50's +08: the request's box against the
///callback's, as 004F1F90 with its half extents in place of the radius.
pub(super) fn box_box(
    contact: &mut CollisionContact,
    center: [i32; 3],
    half_extents: [i32; 3],
    other_center: [i32; 3],
    other_half_extents: [i32; 3],
    normals: ContactAxisNormals,
) {
    nearest_face(
        contact,
        center,
        half_extents,
        other_center,
        other_half_extents,
        normals,
    );
}

///004F1D20, the point table 00544BF0's +08: a zero-radius request against
///the callback's box. Unlike 004F1F90 it gates on |centre - point| below
///each half extent, and a tie between two faces keeps the low one.
pub(super) fn point_box(
    contact: &mut CollisionContact,
    point: [i32; 3],
    center: [i32; 3],
    half_extents: [i32; 3],
    normals: ContactAxisNormals,
) {
    contact.write_flag(0);
    for i in 0..3 {
        let difference = center[i].wrapping_sub(point[i]);
        let distance = if difference >= 0 {
            difference
        } else {
            point[i].wrapping_sub(center[i])
        };
        if distance >= half_extents[i] {
            return;
        }
    }
    contact.write_flag(1);
    // (distance, high face) per axis: 004F1DA0..004F1E4C.
    let face: [(i32, bool); 3] = std::array::from_fn(|i| {
        let offset = point[i].wrapping_sub(center[i]);
        let mut low = half_extents[i].wrapping_add(offset);
        if low < 0 {
            low = center[i]
                .wrapping_sub(half_extents[i])
                .wrapping_sub(point[i]);
        }
        let mut high = offset.wrapping_sub(half_extents[i]);
        if high < 0 {
            high = half_extents[i]
                .wrapping_sub(point[i])
                .wrapping_add(center[i]);
        }
        if high < low {
            (high, true)
        } else {
            (low, false)
        }
    });
    // 004F1E52..004F1EC4: equal X/Z chooses Z, equal with Y chooses Y.
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
        normals.positive[axis]
    } else {
        normals.negative[axis]
    };
    for (i, bits) in normal.into_iter().enumerate() {
        contact.write_normal(i, f32::from_bits(bits));
    }
    contact.write_displacement(face[axis].0);
}

fn nearest_face(
    contact: &mut CollisionContact,
    request_center: [i32; 3],
    request_extents: [i32; 3],
    center: [i32; 3],
    half_extents: [i32; 3],
    normals: ContactAxisNormals,
) {
    //4F1F98 clears only flag, before any input-dependent rejection.
    contact.write_flag(0);
    for i in 0..3 {
        if request_center[i].wrapping_add(request_extents[i])
            <= center[i].wrapping_sub(half_extents[i])
            || request_center[i].wrapping_sub(request_extents[i])
                >= center[i].wrapping_add(half_extents[i])
        {
            return;
        }
    }
    contact.write_flag(1);
    let face: [(i32, bool); 3] = std::array::from_fn(|i| {
        let positive = request_center[i]
            .wrapping_sub(half_extents[i])
            .wrapping_sub(center[i])
            .wrapping_sub(request_extents[i])
            .wrapping_abs();
        let negative = half_extents[i]
            .wrapping_sub(center[i])
            .wrapping_add(request_center[i])
            .wrapping_add(request_extents[i])
            .wrapping_abs();
        //Equal faces retain the positive normal. MIN remains negative after
        //native wrapped negation, so neither unsigned abs nor clamping agrees.
        if negative < positive {
            (negative, true)
        } else {
            (positive, false)
        }
    });
    //4F212E..21FA: equal X/Z choosesZ, equal withY choosesY.
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
