//! One mandatory process carrier. Cloning shares both owners, never bytes or
//! graphics history. Selected renderer construction is a separate actual phase.
use crate::{
    model_scratch::{ModelScratchError, ProcessModelScratch},
    process_model_graphics::{ProcessModelGraphics, ProcessModelGraphicsError},
};
use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;
#[derive(Clone, Debug)]
pub struct ModelProcessOwners {
    pub scratch: ProcessModelScratch,
    pub graphics: ProcessModelGraphics,
    ///Immutable original0053DF00 table, shared by auxiliary basis rotations.
    pub(crate) basis_math: std::sync::Arc<crate::menu_basis::MenuBasisMath>,
}
#[derive(Debug, Error)]
pub enum ModelProcessOwnerError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error(transparent)]
    Scratch(#[from] ModelScratchError),
    #[error(transparent)]
    Graphics(#[from] ProcessModelGraphicsError),
}
impl ModelProcessOwners {
    ///Native process creation only. No world/screen/resource replacement resets
    ///either pool. The default544740 table starts with its file-backed NULL pair.
    pub fn native_startup(
        executable: FrontendExecutable<'_>,
    ) -> Result<Self, ModelProcessOwnerError> {
        let proof = executable.loader_zero_data(0x0056_4728, 0xfa004)?;
        let sine = executable.mapped(0x0053_df00, 8192)?.bytes;
        Ok(Self {
            scratch: ProcessModelScratch::native_loader_zero(&proof)?,
            graphics: ProcessModelGraphics::native_startup(executable)?,
            basis_math: std::sync::Arc::new(crate::menu_basis::MenuBasisMath::new(
                std::array::from_fn(|i| u16::from_le_bytes([sine[i * 2], sine[i * 2 + 1]])),
            )),
        })
    }
    ///Actual admitted initial/installed renderer constructor only. Candidate
    ///validation, software scene loans and residency epochs do not call this.
    pub(crate) fn selected_renderer_constructed(&self) -> Result<(), ProcessModelGraphicsError> {
        self.graphics.publish_renderer_constructor_copy()
    }
    ///Isolated supplied-state test domain. Its environment is an explicit
    ///fixture publication, never production startup or renderer reconstruction.
    #[cfg(test)]
    pub(crate) fn supplied_scene_fixture(
        scratch: ProcessModelScratch,
        environment: crate::model_execution::ModelEnvironment,
    ) -> Self {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
        )
        .expect("retail fixture executable required");
        let executable = FrontendExecutable::parse(&bytes).expect("supported fixture profile");
        let mut owners =
            Self::native_startup(executable).expect("authenticated fixture process globals");
        owners.scratch = scratch;
        owners
            .selected_renderer_constructed()
            .expect("supplied fixture renderer");
        owners.graphics.publish_view(environment.view).unwrap();
        owners
            .graphics
            .publish_shift(environment.coordinate_shift)
            .unwrap();
        owners
            .graphics
            .publish_setup(crate::frontend_scene::SceneSetup {
                focal: environment.projection.focal,
                viewport: environment.projection.viewport,
                center: environment.projection.center,
                fade: environment.projection.fade,
                fade_factor: environment.fog_factor,
            })
            .unwrap();
        owners
    }
}
