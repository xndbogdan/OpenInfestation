//! Model transforms at 0049FEF0/004A0AE0/004A0ED0 and publication 0048FB40.
//!
//! World state and its published view transform are separate. Child origins
//! are actual runtime-slot world coordinates, never raw local coordinates.
//! Allocation, vertex/shade preprocessing, resource lookup, callback custody,
//! and execution of the selected dispatch table belong to the program owner.

use inf_formats::ModelHeader;

use crate::{
    frontend_scene::SceneView,
    menu::MenuBasis,
    menu_basis::{MenuBasisMath, RotationPlane},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelContext {
    /// Context +08..+10; published unchanged at 0055F1C4..0055F1CC.
    pub position: [i32; 3],
    /// Context +14..+38; counter then three world-space basis vectors.
    pub basis: MenuBasis,
    /// Context +A0. Its constructor runs even before model culling.
    saved_basis: MenuBasis,
}

impl ModelContext {
    /// 0049FEF0 copies the caller's position/basis and initializes +A0 through
    /// 0040D6A0. Registers are owned separately and are not initialized here.
    pub const fn new(position: [i32; 3], basis: MenuBasis) -> Self {
        Self {
            position,
            basis,
            saved_basis: MenuBasis::INITIAL,
        }
    }

    pub const fn saved_basis(&self) -> &MenuBasis {
        &self.saved_basis
    }

    /// Opcode0F (004A0B60) copies the nine global axis words into +A4..+C4,
    /// leaving +A0's counter intact. CRT 0048F5D0/630/690 establish those
    /// read-only axes at0055F290/280/270 as X/Y/Z unit vectors.
    pub fn save_identity_basis(&mut self) {
        self.saved_basis.words[1..].copy_from_slice(&MenuBasis::INITIAL.words[1..]);
    }

    /// Opcode10 (004A0BE0) copies all ten +A0 words into +14..+38. The
    /// executor then republishes 0048FB40; this does not reinitialize vertices.
    pub fn restore_saved_basis(&mut self) {
        self.basis = self.saved_basis;
    }

    /// First phase of opcode03 (004A0AE0): 00490520 overwrites the nine basis
    /// floats but leaves the saved basis's counter untouched. The program
    /// owner must do this BEFORE evaluating the optional packed angle.
    /// Here low-three-bit6 means identity permutation, not saved-basis access.
    pub fn save_oriented_basis(&mut self, orientation: u16) {
        self.saved_basis.copy_oriented_from(self.basis, orientation);
    }

    /// Non-native, the port fix `hovercraft_fans`: the saved basis's X axis
    /// negated, undoing a mirror ([`crate::model_execution::hovercraft_fans_unmirrored`]).
    pub fn unmirror_saved_basis(&mut self) {
        for word in &mut self.saved_basis.words[1..4] {
            *word ^= 0x8000_0000;
        }
    }

    /// Opcode04's complete 004A0940 phase. Full DWORD differences wrap before
    /// FILD; Y spills before squaring, while retained Z multiplies its f32
    /// spill. X remains retained; outputs Y/Z and the denominator use f32
    /// spills. 00490370 replaces all ten saved words, with rebuild-Y mode0.
    pub(crate) fn save_basis_from_world_points(
        &mut self,
        origin: [i32; 3],
        first: [i32; 3],
        second: [i32; 3],
    ) {
        let direction = |point: [i32; 3]| {
            normalize_world_difference(std::array::from_fn(|i| point[i].wrapping_sub(origin[i])))
        };
        inf_formats::native_basis::orient_from_forward(
            &mut self.saved_basis,
            direction(first),
            direction(second),
            false,
        );
    }

    /// Second phase of opcode03. The program owner only calls this when the
    /// authored packed-angle WORD is nonzero, even if it evaluates to zero.
    /// Signed axis1 selects491530, axis2 selects4917D0, all others491290.
    pub fn rotate_saved_basis(&mut self, axis: i16, angle: u16, math: &MenuBasisMath) {
        let plane = match axis {
            1 => RotationPlane::Xz,
            2 => RotationPlane::Xy,
            _ => RotationPlane::Yz,
        };
        math.rotate_in_plane(&mut self.saved_basis, i32::from(angle), plane);
    }

    /// 004A0ED0 takes first three DWORDs of the signed-indexed runtime slot.
    /// They are already world coordinates: do not transform/add them again.
    /// Low-three-bit6 copies all ten saved words and ignores sign bits. Other
    /// orientations start with counter0 and apply 00490520 to the world basis.
    /// The child's own saved basis starts at identity, not the parent's saved
    /// value. Four inherited value registers are the separate value owner's job.
    pub fn child(&self, position: [i32; 3], orientation: u16) -> Self {
        let basis = if orientation & 7 == 6 {
            self.saved_basis
        } else {
            let mut basis = MenuBasis::INITIAL;
            basis.copy_oriented_from(self.basis, orientation);
            basis
        };
        Self::new(position, basis)
    }

    /// Concrete 0048FB40 output, computed before the outer visibility gate.
    /// Native top entry publishes once, child entry twice, parent restoration
    /// once. These calls do not mutate the source basis/counter; the enclosing
    /// owner publishes this result at each of those actual boundaries.
    pub fn publish(&self, view: SceneView, coordinate_shift: u8) -> ModelTransform {
        let view_vectors = vectors(view.basis);
        // 0048FB40 inlines 00490DC0: each model axis through 00490710 into
        // the view's frame, the counter plus one, and past 100 the same
        // normalization as the menu rotation (0048FC06..0048FD7B).
        let view_basis = view.basis.compose(self.basis);

        let shift = u32::from(coordinate_shift & 31);
        let delta: [i32; 3] = std::array::from_fn(|axis| {
            self.position[axis]
                .wrapping_sub(view.eye[axis])
                .wrapping_shl(shift)
                >> shift
        });
        let [x, y, z] = delta.map(f64::from);
        // Unlike basis composition, 0048FDD8..0048FE45 adds X, Y, then Z.
        // FILD retains signed32 exactly, FISTP QWORD rounds nearest-even,
        // and only the low DWORD is published (no saturating i32 conversion).
        let view_position = view_vectors.map(|v| fistp_low32((x * v[0] + y * v[1]) + z * v[2]));
        ModelTransform {
            view_position,
            view_basis,
        }
    }
}

fn normalize_world_difference([x, y, z]: [i32; 3]) -> [f32; 3] {
    let x = f64::from(x);
    let y = f64::from(y as f32);
    let retained_z = f64::from(z);
    let z = f64::from(z as f32);
    let length = ((retained_z * z + y * y) + x * x).sqrt();
    if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        let spilled = f64::from(length as f32);
        [
            (x / spilled) as f32,
            (y / spilled) as f32,
            (z / spilled) as f32,
        ]
    }
}

/// Exact shared projection globals used by the model-entry consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelProjection {
    /// 0055F22C/30; these are the already-published focal values.
    pub focal: [i32; 2],
    /// 0055F234/38.
    pub viewport: [i32; 2],
    /// 0055F23C/40.
    pub center: [i32; 2],
    /// 0055F248/4C: near-table threshold and far rejection threshold.
    pub fade: [i32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelDispatch {
    /// 00540DC0, context+60=0.
    Near,
    /// 005411C0, context+60=1.
    Far,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelTransform {
    /// 0055F1F8/FC/200.
    pub view_position: [i32; 3],
    /// 0055F204..228: counter followed by composed basis vectors.
    pub view_basis: MenuBasis,
}

impl ModelTransform {
    /// Shared outer frustum gate in 0049FEF0 and 004A0ED0. Extent is stored as
    /// a DWORD but consumed as signed32. Lower edges are inclusive; upper
    /// edges are exclusive. Force-near never bypasses this gate.
    pub fn in_frustum(&self, extent: u32, projection: &ModelProjection) -> bool {
        let extent = extent as i32;
        let [x, y, z] = self.view_position;
        let depth_extent = z.wrapping_add(extent);
        if depth_extent < 64 {
            return false;
        }
        // FILD extent remains exact for the X-radius multiply despite FST
        // saving a rounded copy. The Y-radius multiply uses that f32 copy.
        let extent_float = extent as f32;
        let depth_float = f64::from(depth_extent as f32);
        let [focal_x, focal_y] = projection.focal.map(f64::from);
        let radius_x = f64::from((f64::from(extent) * focal_x) as f32);
        let projected_x = f64::from(projection.center[0]) * depth_float + f64::from(x) * focal_x;
        if projected_x < -radius_x
            || projected_x >= f64::from(projection.viewport[0]) * depth_float + radius_x
        {
            return false;
        }
        let radius_y = f64::from((f64::from(extent_float) * focal_y) as f32);
        let projected_y = f64::from(projection.center[1]) * depth_float - f64::from(y) * focal_y;
        projected_y >= -radius_y
            && projected_y < f64::from(projection.viewport[1]) * depth_float + radius_y
    }

    /// 004A00E0's separate depth policy; invoke only after publication and
    /// outer visibility. Header flag20 forces Near, bypassing far rejection.
    pub fn dispatch(
        &self,
        header: ModelHeader,
        projection: &ModelProjection,
    ) -> Option<ModelDispatch> {
        if header.flags.forces_near_table() {
            return Some(ModelDispatch::Near);
        }
        let z = self.view_position[2];
        let extent = header.visibility_extent as i32;
        if z.wrapping_sub(extent) >= projection.fade[1] {
            None
        } else if z.wrapping_add(extent) >= projection.fade[0] {
            Some(ModelDispatch::Far)
        } else {
            Some(ModelDispatch::Near)
        }
    }

    /// Port option: this transform with its view depth replaced by its
    /// distance from the eye, for [`Self::dispatch`] to choose the table
    /// and the far cut-off of a model outside the view as the eye turned
    /// toward it would.
    pub fn at_distance(&self) -> Self {
        let distance = self
            .view_position
            .map(|axis| f64::from(axis).powi(2))
            .iter()
            .sum::<f64>()
            .sqrt();
        let mut turned = *self;
        turned.view_position[2] = distance.min(f64::from(i32::MAX)) as i32;
        turned
    }

    pub fn admission(
        &self,
        header: ModelHeader,
        projection: &ModelProjection,
    ) -> Option<ModelDispatch> {
        self.in_frustum(header.visibility_extent, projection)
            .then(|| self.dispatch(header, projection))
            .flatten()
    }
}

fn vectors(basis: MenuBasis) -> [[f64; 3]; 3] {
    std::array::from_fn(|axis| {
        std::array::from_fn(|component| {
            f64::from(f32::from_bits(basis.words[1 + axis * 3 + component]))
        })
    })
}

/// Direct FISTP QWORD under the native nearest-even control word, followed by
/// a low-DWORD load. Shared only by consumers with this exact conversion.
pub(crate) fn fistp_low32(value: f64) -> i32 {
    // CVTSD2SI rounds by MXCSR (nearest even; Rust never changes it) and
    // gives 0x8000_0000_0000_0000 for NaN and out of range: low DWORD 0.
    #[cfg(target_arch = "x86_64")]
    #[allow(unused_unsafe)]
    // SAFETY: SSE2 is part of the x86-64 baseline.
    return unsafe {
        use std::arch::x86_64::{_mm_cvtsd_si64, _mm_set_sd};
        _mm_cvtsd_si64(_mm_set_sd(value)) as i32
    };
    #[cfg(not(target_arch = "x86_64"))]
    fistp_low32_portable(value)
}

/// [`fistp_low32`] without SSE2.
#[cfg_attr(target_arch = "x86_64", allow(dead_code))]
fn fistp_low32_portable(value: f64) -> i32 {
    let integral = value.round_ties_even();
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    if integral.is_finite() && (-BOUND..BOUND).contains(&integral) {
        integral as i64 as i32
    } else {
        // x87 masked invalid conversion's integer-indefinite low DWORD.
        0
    }
}

#[cfg(test)]
#[path = "model_context_tests.rs"]
mod tests;
