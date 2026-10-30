//! Retained current default/copy projection aliases and published model globals.
//! Required lifetime/publication hooks preserve one actual process history.
//! Native544740 slots1C/20 begin NULL.4386FC copies their CURRENT values into
//!546E60.4A014D/158/15B and normal4A1118/11B/12A/12D own later selection.

use crate::{
    frontend_scene::{ModelSurfaceTableOrigin, SceneFade, SceneSetup, SceneView},
    menu::MenuBasis,
    model_context::{ModelDispatch, ModelTransform},
};
use inf_formats::{FrontendExecutable, FrontendExecutableError};
use std::{cell::RefCell, rc::Rc};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedProjectionSlot {
    Local1c,
    World20,
}
impl SelectedProjectionSlot {
    fn index(self) -> usize {
        match self {
            Self::Local1c => 0,
            Self::World20 => 1,
        }
    }
}

/// Pointer identity, not an invocation depth/camera/default mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionEntry {
    NativeNull,
    NearLocal,
    NearWorld,
    FarLocal,
    FarWorld,
}

///Exactly six window DWORDs; loading/menu publishers do not rewrite fade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelProjectionWindow {
    pub focal: [i32; 2],
    pub viewport: [i32; 2],
    pub center: [i32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublishedModelWorld {
    pub position: [i32; 3],
    pub basis: MenuBasis,
    pub transform: ModelTransform,
}

#[derive(Debug, Error)]
pub enum ProcessModelGraphicsError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("model process loan is already borrowed")]
    BorrowConflict,
    #[error("selected renderer table546E60 lacks its reached4386FC copy")]
    RequiredRendererTable,
    #[error("supplied model transform does not match the current published view/shift")]
    PublicationSourceMismatch,
    #[error("current {table:?} selected slot {slot:?} is the native NULL callback")]
    NativeNullProjection {
        table: ModelSurfaceTableOrigin,
        slot: SelectedProjectionSlot,
    },
}

#[derive(Clone, Copy, Debug)]
struct ProjectionFields {
    focal: [i32; 2],
    viewport: [i32; 2],
    center: [i32; 2],
    fade: [i32; 2],
    factor: i32,
}
#[derive(Clone, Copy, Debug)]
struct State {
    default: [ProjectionEntry; 2],
    /// Unconstructed knownness, not a hidden Default fallback.
    renderer: Option<[ProjectionEntry; 2]>,
    view: SceneView,
    shift: u8,
    projection: ProjectionFields,
    world: PublishedModelWorld,
}

/// The published globals as [`ProcessModelGraphics::save`] copied them.
#[derive(Clone, Copy, Debug)]
pub struct ProcessModelGraphicsSave(State);

/// Clone shares ONE process history. No Default/reset/snapshot constructor.
#[derive(Clone)]
pub struct ProcessModelGraphics(Rc<RefCell<State>>);
impl std::fmt::Debug for ProcessModelGraphics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessModelGraphics")
            .field("owner", &Rc::as_ptr(&self.0))
            .finish()
    }
}
impl ProcessModelGraphics {
    /// Authenticate template FILE bytes and genuine loader-zero global ranges
    /// ONCE at process startup. This is not Scene-config adoption or a later
    /// epoch/world/query reset.4386FC remains a distinct reached copy phase.
    pub fn native_startup(
        executable: FrontendExecutable<'_>,
    ) -> Result<Self, ProcessModelGraphicsError> {
        let expected = [
            0x004e_b8e0,
            0x004e_baa0,
            0x004e_b5a0,
            0x004e_b730,
            0x004e_bcd0,
            0x004e_bcf0,
            0,
            0,
            0,
        ];
        for (i, expected) in expected.into_iter().enumerate() {
            if executable.word(0x0054_4740 + i as u32 * 4)? != expected {
                return Err(FrontendExecutableError::Header(
                    "unexpected default model projection table",
                )
                .into());
            }
        }
        //55F190..1C3 view,1C4..1F7 world,1F8..22B composed transform,
        //22C..24F projection/fade and250 coordinate-shift DWORD. All are beyond
        //the original file-backed .data end547000; validate through the parser.
        executable.loader_zero_data(0x0055_f190, 0xc4)?;
        let zero_basis = MenuBasis { words: [0; 10] };
        Ok(Self(Rc::new(RefCell::new(State {
            default: [ProjectionEntry::NativeNull; 2],
            renderer: None,
            view: SceneView {
                eye: [0; 3],
                basis: zero_basis,
            },
            shift: 0,
            world: PublishedModelWorld {
                position: [0; 3],
                basis: zero_basis,
                transform: ModelTransform {
                    view_position: [0; 3],
                    view_basis: zero_basis,
                },
            },
            projection: ProjectionFields {
                focal: [0; 2],
                viewport: [0; 2],
                center: [0; 2],
                fade: [0; 2],
                factor: 0,
            },
        }))))
    }
    pub fn same_owner(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// Port option: the published globals as they are now, which
    /// [`Self::restore`] puts back, so the draws between leave nothing
    /// published (the world draw's view shown, drawn apart from the
    /// game's). The one history goes on.
    pub(crate) fn save(&self) -> Result<ProcessModelGraphicsSave, ProcessModelGraphicsError> {
        let state = self
            .0
            .try_borrow()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?;
        Ok(ProcessModelGraphicsSave(*state))
    }

    /// Put back what [`Self::save`] copied.
    pub(crate) fn restore(
        &self,
        save: ProcessModelGraphicsSave,
    ) -> Result<(), ProcessModelGraphicsError> {
        *self
            .0
            .try_borrow_mut()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)? = save.0;
        Ok(())
    }

    ///4386FC..709 only, NOT SoftwareScene/resource-epoch construction by itself.
    ///Copy selected entries from the mutable current default, not static bytes.
    pub(crate) fn publish_renderer_constructor_copy(
        &self,
    ) -> Result<(), ProcessModelGraphicsError> {
        let mut state = self
            .0
            .try_borrow_mut()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?;
        state.renderer = Some(state.default);
        Ok(())
    }
    ///Existing model host hook. No source callback lies between native1C/20 stores.
    pub(crate) fn select_projection(
        &self,
        table: ModelSurfaceTableOrigin,
        dispatch: ModelDispatch,
    ) -> Result<(), ProcessModelGraphicsError> {
        let mut state = self
            .0
            .try_borrow_mut()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?;
        let selected = match dispatch {
            ModelDispatch::Near => [ProjectionEntry::NearLocal, ProjectionEntry::NearWorld],
            ModelDispatch::Far => [ProjectionEntry::FarLocal, ProjectionEntry::FarWorld],
        };
        match table {
            ModelSurfaceTableOrigin::TemplateDefault => state.default = selected,
            ModelSurfaceTableOrigin::InheritedRenderer => {
                *state
                    .renderer
                    .as_mut()
                    .ok_or(ProcessModelGraphicsError::RequiredRendererTable)? = selected;
            }
        }
        Ok(())
    }
    ///Fresh selected pointer; known NULL remains a reached callback boundary.
    pub(crate) fn selected(
        &self,
        table: ModelSurfaceTableOrigin,
        slot: SelectedProjectionSlot,
    ) -> Result<ProjectionEntry, ProcessModelGraphicsError> {
        let state = self
            .0
            .try_borrow()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?;
        let pair = match table {
            ModelSurfaceTableOrigin::TemplateDefault => state.default,
            ModelSurfaceTableOrigin::InheritedRenderer => state
                .renderer
                .ok_or(ProcessModelGraphicsError::RequiredRendererTable)?,
        };
        Ok(pair[slot.index()])
    }
    pub(crate) fn publish_view(&self, view: SceneView) -> Result<(), ProcessModelGraphicsError> {
        self.0
            .try_borrow_mut()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?
            .view = view;
        Ok(())
    }
    pub(crate) fn publish_shift(&self, shift: u8) -> Result<(), ProcessModelGraphicsError> {
        self.0
            .try_borrow_mut()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?
            .shift = shift;
        Ok(())
    }
    pub(crate) fn publish_setup(&self, setup: SceneSetup) -> Result<(), ProcessModelGraphicsError> {
        self.0
            .try_borrow_mut()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?
            .projection = ProjectionFields {
            focal: setup.focal,
            viewport: setup.viewport,
            center: setup.center,
            fade: setup.fade,
            factor: setup.fade_factor,
        };
        Ok(())
    }
    ///00452C3C/menu and loading projection publication: preserve fade/factor.
    pub(crate) fn publish_window(
        &self,
        window: ModelProjectionWindow,
    ) -> Result<(), ProcessModelGraphicsError> {
        let mut state = self
            .0
            .try_borrow_mut()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?;
        state.projection.focal = window.focal;
        state.projection.viewport = window.viewport;
        state.projection.center = window.center;
        Ok(())
    }
    pub(crate) fn publish_fade(&self, fade: SceneFade) -> Result<(), ProcessModelGraphicsError> {
        let mut state = self
            .0
            .try_borrow_mut()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?;
        state.projection.fade = fade.bounds;
        state.projection.factor = fade.factor;
        Ok(())
    }
    ///Relevant fields only: saved-basis/register constructors are not invoked.
    pub(crate) fn publish_model(
        &self,
        world: PublishedModelWorld,
    ) -> Result<(), ProcessModelGraphicsError> {
        self.0
            .try_borrow_mut()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?
            .world = world;
        Ok(())
    }
    pub(crate) fn view(&self) -> Result<SceneView, ProcessModelGraphicsError> {
        Ok(self
            .0
            .try_borrow()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?
            .view)
    }
    pub(crate) fn shift(&self) -> Result<u8, ProcessModelGraphicsError> {
        Ok(self
            .0
            .try_borrow()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?
            .shift)
    }
    pub(crate) fn world(&self) -> Result<PublishedModelWorld, ProcessModelGraphicsError> {
        Ok(self
            .0
            .try_borrow()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?
            .world)
    }
    ///Read fields separately at the native consumer's demanded phase. For
    ///example depth<64 never demands focal/center/viewport or Far fog fields.
    pub(crate) fn focal(&self) -> Result<[i32; 2], ProcessModelGraphicsError> {
        self.read_projection(|p| p.focal)
    }
    pub(crate) fn viewport(&self) -> Result<[i32; 2], ProcessModelGraphicsError> {
        self.read_projection(|p| p.viewport)
    }
    pub(crate) fn center(&self) -> Result<[i32; 2], ProcessModelGraphicsError> {
        self.read_projection(|p| p.center)
    }
    pub(crate) fn fade(&self) -> Result<[i32; 2], ProcessModelGraphicsError> {
        self.read_projection(|p| p.fade)
    }
    pub(crate) fn fog_factor(&self) -> Result<i32, ProcessModelGraphicsError> {
        self.read_projection(|p| p.factor)
    }
    fn read_projection<T>(
        &self,
        get: impl FnOnce(&ProjectionFields) -> T,
    ) -> Result<T, ProcessModelGraphicsError> {
        let state = self
            .0
            .try_borrow()
            .map_err(|_| ProcessModelGraphicsError::BorrowConflict)?;
        Ok(get(&state.projection))
    }
}

#[cfg(test)]
mod tests;
