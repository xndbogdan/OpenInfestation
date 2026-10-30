//! 0044EC90 behind terrain table 00537090+00 (via 0044EC70): one sphere
//! against the prepared H12 grid. Helpers are 0044FBF0 (triangle normal),
//! 0044F9A0 (segment distance) and 0044F750 (nearest triangle edge).
//! Floating arithmetic follows supported CRT FPCW 027F (53-bit,
//! nearest-even); every `__ftol` truncates and keeps its low DWORD.

use super::super::derived::native_trunc_low32 as ftol;
use super::CollisionContact;
use crate::terrain_runtime::TerrainSpacing;
use inf_formats::{TerrainCell, TERRAIN_GRID_SIDE};
use thiserror::Error;

/// Live globals 005372F0/F4/F8. Native reads each one separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainSphereGrid {
    /// 005372F0: cell spacing and walk step.
    pub spacing: TerrainSpacing,
    /// 005372F4: fraction mask for the walk bounds and grid indices.
    pub mask: TerrainSpacing,
    /// 005372F8: signed height shift. x86 SHL uses its low five bits.
    pub height_shift: u32,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TerrainSphereError {
    #[error("H12 grid has {cells} cells; 0044EC90 indexes a 512x512 array")]
    GridShape { cells: usize },
    /// Native spins until a wrapped cursor meets its end, or forever. This
    /// port-only bound refuses walks longer than two full 16-bit worlds.
    #[error("0044EC90 walk {start:#x}..{end:#x} by {step:#x} is unbounded")]
    UnboundedWalk { start: u32, end: u32, step: u32 },
}

const MAX_WALK: u32 = 1024;

/// One cell's corners in native stack order: (x,z), (x+1,z), (x+1,z+1), (x,z+1).
#[derive(Clone, Copy)]
struct Corners {
    h00: i32,
    h10: i32,
    h11: i32,
    h01: i32,
}

#[derive(Clone, Copy)]
struct Sphere {
    center: [i32; 3],
    radius: i32,
    radius_squared: i32,
}

/// The current cell and its corner heights.
#[derive(Clone, Copy)]
struct Cell {
    origin: [i32; 2],
    size: i32,
    corners: Corners,
}

/// Plane-projection region of one triangle in cell-local unsigned (u, v).
#[derive(Clone, Copy)]
enum Region {
    /// Default diagonal, first triangle: u <= v.
    OnOrBelowDiagonal,
    /// Default diagonal, second triangle: u > v.
    AboveDiagonal,
    /// Alternate diagonal, first triangle: u <= size - v.
    NearOrigin,
    /// Alternate diagonal, second triangle: u > size - v.
    FarCorner,
}

impl Region {
    fn contains(self, u: u32, v: u32, size: u32) -> bool {
        match self {
            Self::OnOrBelowDiagonal => u <= v,
            Self::AboveDiagonal => u > v,
            Self::NearOrigin => u <= size - v,
            Self::FarCorner => u > size - v,
        }
    }
}

/// 0044F750 stack arguments 9..11.
#[derive(Clone, Copy)]
struct EdgeQuery {
    alternate: bool,
    second: bool,
    diagonal: bool,
}

/// Execute 0044EC90 against `cells` (native x*512+z order). The contact is
/// written at the first touching cell, then only by a deeper one; a call
/// without contact leaves every caller-owned DWORD untouched. The 0044ECB2
/// lazy H12 load belongs to the caller: these cells are already resident.
pub fn terrain_sphere_contact(
    contact: &mut CollisionContact,
    cells: &[TerrainCell],
    grid: TerrainSphereGrid,
    center: [i32; 3],
    radius: i32,
) -> Result<(), TerrainSphereError> {
    if cells.len() != TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE {
        return Err(TerrainSphereError::GridShape { cells: cells.len() });
    }
    let size = i32::from(grid.spacing.units());
    let keep = !u32::from(grid.mask.units() - 1);
    let [x, y, z] = center;
    let x_start = x.wrapping_sub(radius) as u32 & keep;
    let x_end = x.wrapping_add(size).wrapping_add(radius) as u32 & keep;
    let z_start = z.wrapping_sub(radius) as u32 & keep;
    let z_end = z.wrapping_add(size).wrapping_add(radius) as u32 & keep;
    let floor = y.wrapping_sub(radius);
    let rows = walk_count(z_start, z_end, size as u32)?;
    if rows == 0 {
        return Ok(());
    }
    let columns = walk_count(x_start, x_end, size as u32)?;
    let sphere = Sphere {
        center,
        radius,
        radius_squared: radius.wrapping_mul(radius),
    };
    let step = if size != 0x80 { 2 } else { 1 };
    // 0044ED78..85: low WORD >> 7, masked by the arithmetic ~mask >> 7.
    let index_keep = ((keep as i32) >> 7) as u32;
    let x_index_start = u32::from(x_start as u16 >> 7) & index_keep;
    let mut z_index = u32::from(z_start as u16 >> 7) & index_keep;
    let shift = grid.height_shift & 31;
    let index = |x: u32, z: u32| (((x & 0xffff) << 9) + z) as usize;
    let height =
        |x: u32, z: u32| (i32::from(cells[index(x, z)].height_i8()).wrapping_shl(shift)) as i16;
    let mut deepest = None;
    let mut z_cursor = z_start;
    for _ in 0..rows {
        let z_near = z_index & 0xffff;
        let z_far = (z_index + step) & 0x1ff;
        let mut x_index = x_index_start;
        let mut near = i32::from(height(x_index, z_near));
        let mut far = i32::from(height(x_index, z_far));
        let mut previous_top = if near > far { near } else { far };
        let mut x_cursor = x_start;
        for _ in 0..columns {
            let x_next = (x_index + step) & 0x1ff;
            let corners = Corners {
                h00: near,
                h01: far,
                h10: i32::from(height(x_next, z_near)),
                h11: i32::from(height(x_next, z_far)),
            };
            let column_top = if corners.h10 <= corners.h11 {
                corners.h11
            } else {
                corners.h10
            };
            let top = if previous_top > column_top {
                previous_top
            } else {
                column_top
            };
            previous_top = column_top;
            if top > floor {
                let cell = Cell {
                    origin: [x_cursor as i32, z_cursor as i32],
                    size,
                    corners,
                };
                let alternate = cells[index(x_index, z_near)].stored_alternate_diagonal();
                if let Some((normal, distance)) = cell_contact(sphere, cell, alternate) {
                    let displacement = radius.wrapping_sub(distance);
                    // 0044F69C..A3: the first contact always lands; later
                    // ones only when strictly deeper than the stored +10.
                    if deepest.is_none_or(|deepest| deepest < displacement) {
                        deepest = Some(displacement);
                        contact.write_flag(1);
                        for (component, value) in normal.into_iter().enumerate() {
                            contact.write_normal(component, value);
                        }
                        contact.write_displacement(displacement);
                    }
                }
            }
            x_cursor = x_cursor.wrapping_add(size as u32);
            x_index = x_next;
            near = corners.h10;
            far = corners.h11;
        }
        z_cursor = z_cursor.wrapping_add(size as u32);
        z_index = z_far;
    }
    Ok(())
}

fn walk_count(start: u32, end: u32, step: u32) -> Result<u32, TerrainSphereError> {
    let span = end.wrapping_sub(start);
    if span % step != 0 || span / step > MAX_WALK {
        return Err(TerrainSphereError::UnboundedWalk { start, end, step });
    }
    Ok(span / step)
}

/// Both triangles of one cell, then 0044F662's choice: the second triangle
/// wins ties, and its normal is published with the distance.
fn cell_contact(sphere: Sphere, cell: Cell, alternate: bool) -> Option<([f32; 3], i32)> {
    let [x, y, z] = sphere.center;
    let Corners { h00, h10, h11, h01 } = cell.corners;
    let size = cell.size as i16;
    let x_relative = x.wrapping_sub(cell.origin[0]);
    let z_relative = f64::from(z.wrapping_sub(cell.origin[1]) as f32);
    let word = |value: i32| value as i16;
    let (first, second) = if !alternate {
        // 0044EF8B..C9 spills Z, X and Y through f32 before the dot product.
        let x_relative = f64::from(x_relative as f32);
        let y_relative = f64::from(y.wrapping_sub(h00) as f32);
        let plane = triangle_normal(
            [0, word(h01.wrapping_sub(h00)), size],
            [size, word(h11.wrapping_sub(h01)), 0],
        );
        let distance = ftol(
            (z_relative * f64::from(plane[2]) + y_relative * f64::from(plane[1]))
                + x_relative * f64::from(plane[0]),
        );
        let query = EdgeQuery {
            alternate,
            second: false,
            diagonal: true,
        };
        let (first, edge_reached) = triangle_contact(
            sphere,
            cell,
            plane,
            distance,
            Region::OnOrBelowDiagonal,
            query,
        );
        let plane = triangle_normal(
            [0, word(h11.wrapping_sub(h10)), size],
            [size, word(h10.wrapping_sub(h00)), 0],
        );
        let distance = ftol(
            (z_relative * f64::from(plane[2]) + y_relative * f64::from(plane[1]))
                + x_relative * f64::from(plane[0]),
        );
        let query = EdgeQuery {
            alternate,
            second: true,
            diagonal: !edge_reached,
        };
        let (second, _) =
            triangle_contact(sphere, cell, plane, distance, Region::AboveDiagonal, query);
        (first, second)
    } else {
        // 0044F304..32: Y and X stay in x87; only Z was spilled.
        let plane = triangle_normal(
            [0, word(h01.wrapping_sub(h00)), size],
            [size, word(h10.wrapping_sub(h00)), 0],
        );
        let distance = ftol(
            (f64::from(y.wrapping_sub(h00)) * f64::from(plane[1])
                + f64::from(x_relative) * f64::from(plane[0]))
                + z_relative * f64::from(plane[2]),
        );
        let query = EdgeQuery {
            alternate,
            second: false,
            diagonal: true,
        };
        let (first, edge_reached) =
            triangle_contact(sphere, cell, plane, distance, Region::NearOrigin, query);
        // The far triangle measures from corner (x+1, z), not (x, z).
        let plane = triangle_normal(
            [0, word(h11.wrapping_sub(h10)), size],
            [size, word(h11.wrapping_sub(h01)), 0],
        );
        let distance = ftol(
            (f64::from(x_relative.wrapping_sub(cell.size)) * f64::from(plane[0])
                + f64::from(y.wrapping_sub(h10)) * f64::from(plane[1]))
                + z_relative * f64::from(plane[2]),
        );
        let query = EdgeQuery {
            alternate,
            second: true,
            diagonal: !edge_reached,
        };
        let (second, _) = triangle_contact(sphere, cell, plane, distance, Region::FarCorner, query);
        (first, second)
    };
    match (first, second) {
        (Some(first), Some(second)) if first.1 >= second.1 => Some(second),
        (Some(first), _) => Some(first),
        (None, second) => second,
    }
}

/// One triangle after its plane distance. Returns its contact and whether
/// 0044F750 ran, which clears the next call's diagonal argument.
fn triangle_contact(
    sphere: Sphere,
    cell: Cell,
    plane: [f32; 3],
    distance: i32,
    region: Region,
    query: EdgeQuery,
) -> (Option<([f32; 3], i32)>, bool) {
    if distance >= sphere.radius {
        return (None, false);
    }
    // The signed low WORD of the distance projects the center onto the plane.
    let along = f64::from(distance as i16);
    let x_offset = ftol(along * f64::from(plane[0])) as i16;
    let z_offset = ftol(f64::from(plane[2]) * along) as i16;
    let [x, _, z] = sphere.center;
    let u = x
        .wrapping_sub(i32::from(x_offset))
        .wrapping_sub(cell.origin[0]) as u32;
    let v = z
        .wrapping_sub(i32::from(z_offset))
        .wrapping_sub(cell.origin[1]) as u32;
    let size = cell.size as u32;
    if u <= size && v <= size && region.contains(u, v, size) {
        return (Some((plane, distance)), false);
    }
    let (vector, squared) = nearest_edge(sphere.center, cell, query);
    if squared >= sphere.radius_squared {
        return (None, true);
    }
    // A zero squared distance keeps the plane normal and distance zero.
    if squared == 0 {
        return (Some((plane, 0)), true);
    }
    // FILD QWORD with a zero high DWORD reads the squared sum unsigned.
    let length = f64::from(squared as u32).sqrt();
    let normal = if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        vector.map(|value| (f64::from(value) / length) as f32)
    };
    (Some((normal, ftol(length))), true)
}

/// 0044FBF0: normalized cross product. X and Y spill through f32; Z stays
/// in x87 for both the length and its own division.
fn triangle_normal(a: [i16; 3], b: [i16; 3]) -> [f32; 3] {
    let [a0, a1, a2] = a.map(f64::from);
    let [b0, b1, b2] = b.map(f64::from);
    let x = (b2 * a1 - b1 * a2) as f32;
    let y = (b0 * a2 - b2 * a0) as f32;
    let z = a0 * b1 - b0 * a1;
    let (x, y) = (f64::from(x), f64::from(y));
    let length = ((z * z + y * y) + x * x).sqrt();
    if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        [
            (x / length) as f32,
            (y / length) as f32,
            (z / length) as f32,
        ]
    }
}

/// 0044F750: nearest edge of one triangle, the shared diagonal only when
/// argument 11's low byte is set. Returns the WORD vector and its i32 square.
fn nearest_edge(center: [i32; 3], cell: Cell, query: EdgeQuery) -> ([i16; 3], i32) {
    let [x, y, z] = center;
    let [x0, z0] = cell.origin;
    let Corners { h00, h10, h11, h01 } = cell.corners;
    let size = cell.size as i16;
    let word = |value: i32| value as i16;
    let x_word = word(x.wrapping_sub(x0));
    let (diagonal, first, second) = if !query.alternate {
        let p = [x_word, word(y.wrapping_sub(h00)), word(z.wrapping_sub(z0))];
        let diagonal = query
            .diagonal
            .then(|| segment_distance(p, [size, word(h11.wrapping_sub(h00)), size]));
        if query.second {
            let first = segment_distance(p, [size, word(h10.wrapping_sub(h00)), 0]);
            let p = [p[0].wrapping_sub(size), word(y.wrapping_sub(h10)), p[2]];
            let second = segment_distance(p, [0, word(h11.wrapping_sub(h10)), size]);
            (diagonal, first, second)
        } else {
            let first = segment_distance(p, [0, word(h01.wrapping_sub(h00)), size]);
            let p = [p[0], word(y.wrapping_sub(h01)), p[2].wrapping_sub(size)];
            let second = segment_distance(p, [size, word(h11.wrapping_sub(h01)), 0]);
            (diagonal, first, second)
        }
    } else {
        let p = [
            x_word,
            word(y.wrapping_sub(h01)),
            word(z.wrapping_sub(cell.size).wrapping_sub(z0)),
        ];
        let diagonal = query
            .diagonal
            .then(|| segment_distance(p, [size, word(h10.wrapping_sub(h01)), size.wrapping_neg()]));
        if query.second {
            let first = segment_distance(p, [size, word(h11.wrapping_sub(h01)), 0]);
            let p = [p[0].wrapping_sub(size), word(y.wrapping_sub(h11)), p[2]];
            let second = segment_distance(p, [0, word(h10.wrapping_sub(h11)), size.wrapping_neg()]);
            (diagonal, first, second)
        } else {
            let first = segment_distance(p, [0, word(h00.wrapping_sub(h01)), size.wrapping_neg()]);
            let p = [p[0], word(y.wrapping_sub(h00)), p[2].wrapping_add(size)];
            let second = segment_distance(p, [size, word(h10.wrapping_sub(h00)), 0]);
            (diagonal, first, second)
        }
    };
    let nearer = if first.1 < second.1 { first } else { second };
    // A skipped diagonal is 0x7FFFFFFF. It could only tie a wrapped squared
    // sum equal to 2^31-1, which is never a sum of three squares.
    match diagonal {
        Some(diagonal) if diagonal.1 <= nearer.1 => diagonal,
        _ => nearer,
    }
}

/// 0044F9A0: point `p` against the segment from the origin along `s`.
fn segment_distance(p: [i16; 3], s: [i16; 3]) -> ([i16; 3], i32) {
    let [s0, s1, s2] = s.map(f64::from);
    let length = ((s2 * s2 + s1 * s1) + s0 * s0).sqrt();
    // The f32 length spill is the divisor and the truncated bound.
    let spilled = f64::from(length as f32);
    let unit = if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        [s0 / spilled, s1 / spilled, s2 / spilled]
    };
    let [p0, p1, p2] = p.map(f64::from);
    let along = ftol((p2 * unit[2] + p0 * unit[0]) + p1 * unit[1]);
    let out = if along < 0 {
        p
    } else if along > ftol(spilled) {
        std::array::from_fn(|i| p[i].wrapping_sub(s[i]))
    } else {
        let along = f64::from(along as i16);
        std::array::from_fn(|i| p[i].wrapping_sub(ftol(along * unit[i]) as i16))
    };
    let squared = out.iter().fold(0i32, |sum, &value| {
        sum.wrapping_add(i32::from(value).wrapping_mul(i32::from(value)))
    });
    (out, squared)
}
