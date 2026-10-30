//! Ordinary mode-zero model background, 0040E7CB..0040E970.
//!
//! This phase borrows the actual loading scalar owner and decoded sine table.
//! Its host owns the captured renderer, the canonical incoming-context renderer
//! alias, projection publication and the existing H8 interpreter. It queues
//! model117; the enclosing Tick owns the subsequent finish and flush. Native
//! pointer aliases, individual scalar faults and complete primitive acceptance
//! are outside this complete-known-field domain.

use inf_formats::{ByteSpan, FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use crate::{
    frontend_camera::AnimationClock,
    frontend_scene::{
        ModelCallbackPolicy, ModelFailurePolicy, ModelPrimitiveContext, ModelSurfaceTableOrigin,
        SceneCommand, SceneFade, SceneModelSubmission, SceneView,
    },
    loading_runtime::LoadingProcessState,
    menu::MenuBasis,
    menu_basis::{MenuBasisMath, RotationPlane},
};

#[derive(Debug, PartialEq, Eq, Error)]
pub enum LoadingModelStartupError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("loading model template requires the supported loader-zero .data profile")]
    UnsupportedLoaderProfile,
    #[error("loading model initializer operand at {address:#010x} is unsupported")]
    Initializer { address: u32 },
}

/// Retained successful startup template, not the caller's mutable renderer view.
/// The basis constructors and successful 00439880 tail establish these fields;
/// the loader supplies the never-written eye/far-color/mode zero backing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadingModelTemplate {
    view: SceneView,
    model_basis: MenuBasis,
    fade: SceneFade,
    far_color: u32,
    mode: u32,
}

impl LoadingModelTemplate {
    /// Validate only the consumed startup profile/operands. This does not hash
    /// the executable, execute its constructors, map BSS as file data, or prove
    /// arbitrary later computed writers. Focal and dimensions are live inputs.
    pub fn native_startup(executable: &[u8]) -> Result<Self, LoadingModelStartupError> {
        let mapped = FrontendExecutable::parse(executable)?;
        if mapped.data().virtual_address != 0x0052_e000
            || mapped.data().bytes.len() != 0x19000
            || mapped.file_span() != ByteSpan::new(0x12e000, 0x147000)
        {
            return Err(LoadingModelStartupError::UnsupportedLoaderProfile);
        }
        validate_loader_tail(executable)?;
        // Counter stores of 00439630/00439760 (the CRT jumps 00439620 and
        // 00439750). NativeBasis::INITIAL is the
        // existing owner of their 0/4096 and 4096/4096 identity construction.
        require_code(
            executable,
            0x0043_9637,
            &[0xc7, 0x05, 0x90, 0x75, 0x54, 0x00, 0, 0, 0, 0],
        )?;
        require_code(
            executable,
            0x0043_9767,
            &[0xc7, 0x05, 0xf8, 0x74, 0x54, 0x00, 0, 0, 0, 0],
        )?;
        // Successful startup stores the same fade DWORD twice, then installs
        // the genuine template surface table. No renderer pointer zero is used.
        require_code(executable, 0x0043_9939, &[0xb8, 0, 0, 0x10, 0])?;
        require_code(executable, 0x0043_9940, &[0xa3, 0x4c, 0x75, 0x54, 0])?;
        require_code(executable, 0x0043_9945, &[0xa3, 0x50, 0x75, 0x54, 0])?;
        require_code(
            executable,
            0x0043_994b,
            &[0xc7, 0x05, 0x38, 0x75, 0x54, 0, 0x40, 0x47, 0x54, 0],
        )?;
        if mapped.word(0x0052_0494)? != 0x0040_e0d0 || mapped.word(0x0052_0498)? != 0x0043_a2d0 {
            return Err(LoadingModelStartupError::Initializer {
                address: 0x0052_0494,
            });
        }
        Ok(Self {
            view: SceneView {
                eye: [0; 3],
                basis: MenuBasis::INITIAL,
            },
            model_basis: MenuBasis::INITIAL,
            fade: SceneFade::new([0x10_0000; 2]),
            far_color: 0,
            mode: 0,
        })
    }

    pub fn view(&self) -> SceneView {
        self.view
    }
    pub fn fade(&self) -> SceneFade {
        self.fade
    }
    pub fn far_color(&self) -> u32 {
        self.far_color
    }
    pub fn mode(&self) -> u32 {
        self.mode
    }
}

/// Exact six DWORD publication at 0040E84B..0040E865. Fade is a later phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadingModelProjection {
    pub focal: [i32; 2],
    pub viewport: [i32; 2],
    pub center: [i32; 2],
}

pub struct LoadingModelRequest<'a> {
    pub state: &'a LoadingProcessState,
    pub template: &'a LoadingModelTemplate,
    /// Actual current 00546F20/24 source, not doubled and not a viewer scale.
    pub focal: [i32; 2],
    pub math: &'a MenuBasisMath,
}

pub trait LoadingModelHost {
    type Error;
    /// Operations are executed by the same captured renderer/short-resource
    /// loan. Model retains queued work until the enclosing common flush.
    fn submit_loading_scene(&mut self, command: SceneCommand) -> Result<(), Self::Error>;
    fn surface_width(&mut self) -> Result<i32, Self::Error>;
    fn surface_height(&mut self) -> Result<i32, Self::Error>;
    /// Update focal/viewport/center only; existing fade/view remain until their
    /// own reached commands. Atomic typed publication is not native fault parity.
    fn publish_loading_projection(
        &mut self,
        projection: LoadingModelProjection,
    ) -> Result<(), Self::Error>;
}

pub fn draw_loading_model_40e7cb<H: LoadingModelHost>(
    request: LoadingModelRequest<'_>,
    host: &mut H,
) -> Result<(), H::Error> {
    // 0043B5D0: directional publication precedes the ambient scalar stores.
    host.submit_loading_scene(SceneCommand::DirectionalLight {
        direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
        rgb: [128; 3],
    })?;
    host.submit_loading_scene(SceneCommand::AmbientLight([32; 3]))?;

    // 0043B4A0 reads the captured renderer's dimensions; 0040E7F9 rereads
    // height for the shifted background center. Do not reuse the initial Tick
    // dimensions or the caller context's cached viewport. Context+0 at40E7EA
    // names this actual renderer under the explicitly admitted alias domain.
    let width = host.surface_width()?;
    let height = host.surface_height()?;
    let fresh_height = host.surface_height()?;
    let center_y = fresh_height.wrapping_mul(5).wrapping_shl(4) >> 8;
    let age_div_200 = (request.state.age_micros as i32) / 200;
    let selector = request.state.selector;
    let pulse = request.state.pulse as u16;
    host.publish_loading_projection(LoadingModelProjection {
        focal: request.focal,
        viewport: [width, height],
        center: [width / 2, center_y],
    })?;

    // Local object basis starts from the retained process unit basis every
    // time. 0040E8B2 rereads the full signed age DWORD for signed /32 yaw.
    let mut basis = request.template.model_basis;
    request.math.rotate_in_plane(
        &mut basis,
        (request.state.age_micros as i32) / 32,
        RotationPlane::Xz,
    );
    host.submit_loading_scene(SceneCommand::PublishFade(request.template.fade))?;
    host.submit_loading_scene(SceneCommand::PublishView(request.template.view))?;
    host.submit_loading_scene(SceneCommand::Model(SceneModelSubmission {
        model_slot: 117,
        position: [0, 0, 500],
        basis,
        // The shared submission shape carries a clock, but 0040E0D0 never
        // reads it: its selector0 is the captured signed age quotient below.
        animation: AnimationClock::default(),
        surface_table: ModelSurfaceTableOrigin::TemplateDefault,
        primitive_context: ModelPrimitiveContext::Caller {
            far_color: request.template.far_color,
            mode: request.template.mode,
        },
        failure_policy: ModelFailurePolicy::LoadingModel117StackHistoryApproximation,
        policy: ModelCallbackPolicy::Loading {
            age_div_200,
            selector,
            pulse,
        },
    }))
}

fn validate_loader_tail(bytes: &[u8]) -> Result<(), LoadingModelStartupError> {
    let pe = word_at(bytes, 0x3c)? as usize;
    let count = half_at(bytes, pe + 6)? as usize;
    let first = pe + 24 + half_at(bytes, pe + 20)? as usize;
    let mut text_profile = false;
    let mut zero_tail = false;
    for index in 0..count {
        let at = first + index * 40;
        match bytes.get(at..at + 8) {
            Some(name) if name == b".text\0\0\0" => {
                text_profile = word_at(bytes, at + 12)? == 0x1000
                    && word_at(bytes, at + 16)? == 0x11e000
                    && word_at(bytes, at + 20)? == 0x1000;
            }
            Some(name) if name == b".data\0\0\0" => {
                // Only validate genuine loader-zero backing; FrontendExecutable
                // and all authored resource parsers keep their raw-only domain.
                zero_tail =
                    word_at(bytes, at + 8)? == 0x132c38 && word_at(bytes, at + 36)? == 0xc0000040;
            }
            _ => {}
        }
    }
    if text_profile && zero_tail {
        Ok(())
    } else {
        Err(LoadingModelStartupError::UnsupportedLoaderProfile)
    }
}

fn require_code(
    bytes: &[u8],
    address: u32,
    expected: &[u8],
) -> Result<(), LoadingModelStartupError> {
    let offset = (address - 0x0040_0000) as usize;
    if bytes.get(offset..offset + expected.len()) == Some(expected) {
        Ok(())
    } else {
        Err(LoadingModelStartupError::Initializer { address })
    }
}

fn word_at(bytes: &[u8], offset: usize) -> Result<u32, FrontendExecutableError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(FrontendExecutableError::Truncated { offset, bytes: 4 })?;
    Ok(u32::from_le_bytes(value.try_into().expect("DWORD")))
}
fn half_at(bytes: &[u8], offset: usize) -> Result<u16, FrontendExecutableError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(FrontendExecutableError::Truncated { offset, bytes: 2 })?;
    Ok(u16::from_le_bytes(value.try_into().expect("WORD")))
}

#[cfg(test)]
mod tests;
