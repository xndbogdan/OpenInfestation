//!Auxiliary05 retains the original callback packet while resolving a new H8.
use super::*;

pub(super) struct AuxiliaryChildRequest<'a> {
    pub slot: u16,
    pub orientation: u16,
    pub attachment: i16,
    pub counterpart: AuxiliaryModelSource<'a>,
    pub relative: [i32; 3],
    pub radius: i32,
    pub callbacks: CallbackTable,
    pub values: AuxiliaryValueSource<'a>,
}

impl Run<'_> {
    pub(super) fn execute_child(
        &mut self,
        working: &Working<'_>,
        vertices: &ModelVertices,
        request: AuxiliaryChildRequest<'_>,
        output: &mut CollisionContact,
    ) -> Result<(), AuxiliaryModelError> {
        let AuxiliaryChildRequest {
            slot,
            orientation,
            attachment,
            counterpart,
            relative,
            radius,
            callbacks,
            values,
        } = request;
        //4F2FC3 captures origin before405540;4F30AC rereads the actual slot
        //after model lookup when adjusting the copied query's relative center.
        let origin = vertices.world(attachment)?;
        let current = self.context(working);
        let basis = if orientation & 7 == 6 {
            match working {
                Working::Stack(context) => context.saved_basis,
                Working::ProcessB => self.process.query_bases()[1],
            }
        } else {
            let mut basis = MenuBasis::INITIAL;
            basis.copy_oriented_from(current.basis, orientation);
            basis
        };
        let mut child = StackContext {
            origin,
            basis,
            table: ModelSurfaceTableOrigin::TemplateDefault,
            saved_basis: MenuBasis::INITIAL,
        };
        let residency = self
            .residency
            .ok_or(AuxiliaryModelError::RequiredChildResources { slot })?;
        let binding = crate::object_runtime::resident_model_resource(residency, slot)?
            .ok_or(AuxiliaryModelError::RequiredChildModel { slot })?;
        let qualification = crate::world_furniture_effect::AuxiliaryRawSource {
            registration: binding.registration.clone(),
            allocation: std::sync::Arc::downgrade(binding.allocation),
            vertices_span: binding.model.vertices_span,
        };
        let origin = vertices.world(attachment)?;
        let relative = std::array::from_fn(|i| relative[i].wrapping_sub(origin[i]));
        self.execute(
            Working::Stack(&mut child),
            AuxiliaryModelSource {
                model: binding.model,
                qualification: &qualification,
            },
            counterpart,
            relative,
            radius,
            callbacks,
            values,
            output,
        )?;
        //A stopped child retains reached graphics, scratch and contact state.
        //Only normal recursion return publishes the parent's original context.
        self.publish(&self.context(working))
    }
}
