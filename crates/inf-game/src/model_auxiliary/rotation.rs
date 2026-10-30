//!Auxiliary30: the two mode tests intentionally have opposite senses.
use super::*;
use inf_formats::ModelAuxiliaryRotation;

#[cfg(test)]
#[path = "rotation_tests.rs"]
mod tests;

impl Run<'_> {
    pub(super) fn rotate_saved(
        &mut self,
        working: &mut Working<'_>,
        first: &ModelAuxiliaryRotation,
        second: Option<&ModelAuxiliaryRotation>,
        address: usize,
        values: AuxiliaryValueSource<'_>,
    ) -> Result<(), AuxiliaryModelError> {
        self.rotate_one(working, first, first.mode_raw != 0, address, values)?;
        if let Some(second) = second {
            self.rotate_one(working, second, second.mode_raw == 0, address, values)?;
        }
        Ok(())
    }

    fn rotate_one(
        &mut self,
        working: &mut Working<'_>,
        rotation: &ModelAuxiliaryRotation,
        copy_orientation: bool,
        address: usize,
        values: AuxiliaryValueSource<'_>,
    ) -> Result<(), AuxiliaryModelError> {
        let current = self.context(working).basis;
        let mut saved = match working {
            Working::Stack(context) => context.saved_basis,
            Working::ProcessB => self.process.query_bases()[1],
        };
        if copy_orientation {
            //MOVSX BYTE then SAR2, before evaluating the optional packed WORD.
            saved.copy_oriented_from(current, (i16::from(rotation.orientation_raw) >> 2) as u16);
            self.publish_saved_basis(working, saved);
        }
        if !copy_orientation || rotation.packed_angle != 0 {
            // 004A0E10 (called at 004F2EFF and 004F2F41): the packed angle,
            // then the saved basis turns in the orientation's plane.
            let angle = values.evaluate(rotation.packed_angle, address)?;
            let plane = match rotation.orientation_raw as u8 & 3 {
                1 => crate::menu_basis::RotationPlane::Xz,
                2 => crate::menu_basis::RotationPlane::Xy,
                _ => crate::menu_basis::RotationPlane::Yz,
            };
            self.basis_math
                .rotate_in_plane(&mut saved, i32::from(angle), plane);
            self.publish_saved_basis(working, saved);
        }
        Ok(())
    }

    fn publish_saved_basis(&mut self, working: &mut Working<'_>, basis: MenuBasis) {
        match working {
            Working::Stack(context) => context.saved_basis = basis,
            Working::ProcessB => self.process.publish_b_saved_basis(basis),
        }
    }
}
