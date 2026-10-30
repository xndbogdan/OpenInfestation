//! Selector 16, the world camera: constructor 0040F590, vtable 005204D0.
//!
//! The world manager constructs it on demand through 00478E00 (H13 argument
//! 1, null position/rotation, placement -1). Its +34 after-load is the
//! 00401310 no-op. The view-mode setter 0040F800 and target replacement
//! 00410E70 are concrete. Deletion retires the two owned targets before the
//! shared actor tail; released custody is not a native NULL-pointer store.

use super::ActorExtension;
use crate::object_runtime::{
    ActorConstructionError, ActorConstructionRequest, NativeHeapRef, ObjectArena, ObjectError,
    ObjectId, WeightedTargetConstructorNull,
};

mod destruction;
mod held;
mod native;
mod parameters;
mod pose;
pub use held::{HeldCameraError, HeldCameraHost};
pub use native::{CameraDrawRequest, NativeCameraHost, NativeCameraInputs, NativeCameraServices};
pub use parameters::{CameraParameterState, CameraSlewError, CameraSlewRequest};
pub(super) use pose::rotation::rotate_world_vector;
pub use pose::{
    CameraDerivedComponent, CameraDerivedTargetMode, CameraDerivedTargetRequest, CameraGlobalState,
    CameraHeightRequest, CameraPose, CameraPoseError, CameraPoseHost, CameraPoseTickRequest,
};

/// Settings DWORDs read through Settings_GetPtr (00463AB0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraSettings {
    /// +68 First Person. 0040F800 receives only its low byte.
    pub first_person: u32,
    /// +6C Camera Style. Only the exact value 1 selects the alternate tables.
    pub camera_style: u32,
}

impl CameraSettings {
    /// Reads the live settings words at +68/+6C.
    pub fn from_words(words: &[u32]) -> Self {
        Self {
            first_person: words[0x68 / 4],
            camera_style: words[0x6c / 4],
        }
    }
}

/// 0040F170's three writes into a parameter block. The table words are
/// static executable identities, never dereferenceable host pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraParameterBlock {
    pub word_18: u32,
    pub table_1c: u32,
    pub table_20: u32,
}

impl CameraParameterBlock {
    /// 00410EC0 clamps the unsigned table selector to one.
    pub fn set_mode(&mut self, mode: u32) {
        self.word_18 = mode.min(1);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector16State {
    pub block_138: CameraParameterBlock,
    pub block_15c: CameraParameterBlock,
    /// +138 and +15C vector words are unwritten until their first targets
    /// and snap tick. Metadata above remains initialized independently.
    pub eye_138: CameraParameterState,
    pub look_15c: CameraParameterState,
    pub byte_184: u8,
    pub byte_185: u8,
    pub byte_186: u8,
    pub byte_187: u8,
    /// 0040F800 writes 3 for view mode zero and 2 otherwise.
    pub mode_188: u32,
    pub target_18c: Option<NativeHeapRef>,
    pub target_190: Option<NativeHeapRef>,
    /// Local WORD offset +194/+196/+198, read only by camera mode one.
    pub offset_194: Option<[i16; 3]>,
    pub dword_19c: u32,
    /// Not written by 0040F590; the camera's +88 (0040F880) clears it.
    pub dword_180: Option<u32>,
    pub byte_1a0: u8,
}

/// The parameter tables +154/+158 (+138's +1C/+20) and +178/+17C (+15C's)
/// for a Camera Style: only the exact value 1 selects the alternate four.
pub(in crate::object_runtime) fn camera_style_tables(camera_style: u32) -> [u32; 4] {
    if camera_style == 1 {
        [0x0054_6990, 0x0054_6980, 0x0054_69d8, 0x0054_69c8]
    } else {
        [0x0054_6960, 0x0054_6970, 0x0054_69a8, 0x0054_69b8]
    }
}

impl Selector16State {
    fn new(settings: CameraSettings) -> Self {
        let [eye_1c, eye_20, look_1c, look_20] = camera_style_tables(settings.camera_style);
        Self {
            block_138: CameraParameterBlock {
                word_18: 0,
                table_1c: eye_1c,
                table_20: eye_20,
            },
            block_15c: CameraParameterBlock {
                word_18: 0,
                table_1c: look_1c,
                table_20: look_20,
            },
            eye_138: CameraParameterState::UNWRITTEN,
            look_15c: CameraParameterState::UNWRITTEN,
            byte_184: 0,
            byte_185: 1,
            byte_186: 0,
            byte_187: 0,
            mode_188: 3,
            target_18c: None,
            target_190: None,
            offset_194: None,
            dword_19c: 0,
            dword_180: None,
            byte_1a0: 0,
        }
    }
}

#[derive(Clone, Copy)]
enum CameraTargetSlot {
    Primary,
    Secondary,
}

impl ObjectArena {
    /// 0040F590 after the shared 0047A7A0 prefix. Its closing 0040F800 call
    /// has no target yet, so only the camera's own pure +80 and +188 run.
    pub fn allocate_selector16(
        &mut self,
        request: ActorConstructionRequest<'_>,
        settings: CameraSettings,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector16(Box::new(Selector16State::new(settings)));
        self.set_camera_view_mode(object, settings.first_person as u8)?;
        Ok(object)
    }

    pub fn selector16(&self, object: ObjectId) -> Result<&Selector16State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector16(state) => Ok(state),
            _ => Err(ObjectError::NotSelector16(object)),
        }
    }

    pub(super) fn selector16_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector16State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector16(state) => Ok(state),
            _ => Err(ObjectError::NotSelector16(object)),
        }
    }

    /// Virtual+80 dispatch retains each represented class's original policy.
    ///00427670 changes flag0 membership before storing signed low-byte mode;
    ///004231B0 stores only, while 0047AAE0/004074D0 accept mode0 without writes.
    pub fn apply_actor_view_mode(
        &mut self,
        object: ObjectId,
        mode: u8,
    ) -> Result<bool, ObjectError> {
        let policy = match &self.actor(object)?.extension {
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector30(_)
            | ActorExtension::Selector15(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector39(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector66(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector52(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector93(_) => ActorViewModePolicy::ModeZeroOnly,
            ActorExtension::BehaviorResource(state) => {
                if matches!(state.selector, 25 | 28 | 68) {
                    // 004575E0: a missile a player holds.
                    return self.missile_view_mode(object, mode);
                }
                ActorViewModePolicy::ModeZeroOnly
            }
            ActorExtension::Derived(derived) => match &derived.class {
                super::DerivedActorClass::Selector2(_) => ActorViewModePolicy::StoreByte,
                super::DerivedActorClass::Selector77(_) => ActorViewModePolicy::ModeZeroOnly,
                // Original+80=00427670 for00520E58/00520F34/00521164/
                //00520B84/00520CC8/0051F080/00527058/00521EAC/
                //0052273C/00527EE0/00527DE8, including nested family classes.
                super::DerivedActorClass::Base
                | super::DerivedActorClass::Family428650(_)
                | super::DerivedActorClass::Selector11(_)
                | super::DerivedActorClass::Selector60(_) => {
                    ActorViewModePolicy::Flag0ThenStoreByte
                }
            },
        };
        match policy {
            ActorViewModePolicy::ModeZeroOnly => Ok(mode == 0),
            ActorViewModePolicy::StoreByte => {
                self.actor_mut(object)?.byte_d0 = mode;
                Ok(true)
            }
            ActorViewModePolicy::Flag0ThenStoreByte => {
                // TEST BL; JLE includes128..255 as negative signed bytes.
                //00479E90 always reappends its node, including unchanged flags.
                self.set_flag0(object, (mode as i8) <= 0)?;
                self.actor_mut(object)?.byte_d0 = mode;
                Ok(true)
            }
        }
    }

    /// 0040F800. A target actor refusing the mode is asked again with the
    /// fallback (nonzero -> 0, zero -> 2), whose answer is ignored. The camera's
    /// own +80 is the pure 0047AAE0, and +188 records the final mode.
    pub fn set_camera_view_mode(&mut self, camera: ObjectId, mode: u8) -> Result<(), ObjectError> {
        let mut mode = mode;
        let target = self
            .selector16(camera)?
            .target_18c
            .map(|target| self.actor_follow_target(target).map(|data| data.handle_04))
            .transpose()?;
        // Target +10 = 004827C0 resolves its handle through 0047AD90.
        if let Some(actor) = match target {
            Some(handle) => self
                .resolve_actor_handle(handle)
                .map_err(|_| ObjectError::MissingActorRegistry)?,
            None => None,
        } {
            if !self.apply_actor_view_mode(actor, mode)? {
                mode = if mode != 0 { 0 } else { 2 };
                self.apply_actor_view_mode(actor, mode)?;
            }
        }
        self.selector16_mut(camera)?.mode_188 = if mode != 0 { 2 } else { 3 };
        Ok(())
    }

    /// 0040F740 after its 00478E00 lookup: the camera takes the First Person
    /// setting's low byte as its view mode (0040F800), then the Camera
    /// Style's parameter tables.
    pub(in crate::object_runtime) fn apply_camera_settings(
        &mut self,
        camera: ObjectId,
        settings: CameraSettings,
    ) -> Result<(), ObjectError> {
        self.set_camera_view_mode(camera, settings.first_person as u8)?;
        let [eye_1c, eye_20, look_1c, look_20] = camera_style_tables(settings.camera_style);
        let state = self.selector16_mut(camera)?;
        state.block_138.table_1c = eye_1c;
        state.block_138.table_20 = eye_20;
        state.block_15c.table_1c = look_1c;
        state.block_15c.table_20 = look_20;
        Ok(())
    }

    /// Camera +88 = 0040F880: clear +180 and accept the player.
    pub(in crate::object_runtime) fn attach_camera_actor(
        &mut self,
        camera: ObjectId,
    ) -> Result<(), ObjectError> {
        self.selector16_mut(camera)?.dword_180 = Some(0);
        Ok(())
    }

    /// 00478E00 marks a camera it has just constructed.
    pub(in crate::object_runtime) fn set_camera_byte_187(
        &mut self,
        camera: ObjectId,
    ) -> Result<(), ObjectError> {
        self.selector16_mut(camera)?.byte_187 = 1;
        Ok(())
    }

    /// 00410E70. An existing target first receives view mode 0, is deleted
    /// (00482840 frees without other effects) and +188 becomes 3; the new
    /// target is then installed and the First Person setting reapplied.
    pub fn replace_camera_target(
        &mut self,
        camera: ObjectId,
        target: Option<NativeHeapRef>,
        settings: CameraSettings,
    ) -> Result<(), ObjectError> {
        if self.selector16(camera)?.target_18c.is_some() {
            self.set_camera_view_mode(camera, 0)?;
            let old = self.selector16(camera)?.target_18c.unwrap();
            self.release_actor_service(old)?;
            let state = self.selector16_mut(camera)?;
            state.target_18c = None;
            state.mode_188 = 3;
        }
        let state = self.selector16_mut(camera)?;
        state.target_18c = target;
        self.set_camera_view_mode(camera, settings.first_person as u8)
    }

    /// Complete attract suffix 00487AD1..00487AE6. A replacement error retains
    /// its reached target/actor effects and does not reach the literal byte or
    /// parameter stores. No vector, secondary target or basis is initialized.
    pub(in crate::object_runtime) fn complete_attract_camera_relation(
        &mut self,
        camera: ObjectId,
        target: Option<NativeHeapRef>,
        settings: CameraSettings,
    ) -> Result<(), ObjectError> {
        self.replace_camera_target(camera, target, settings)?;
        self.selector16_mut(camera)?.byte_184 = 0;
        self.set_camera_parameter_mode(camera, 0)
    }

    /// 0040F710 deletes the previous secondary target and stores the supplied
    /// one. It does not change the primary target or view mode.
    pub fn replace_camera_secondary_target(
        &mut self,
        camera: ObjectId,
        target: Option<NativeHeapRef>,
    ) -> Result<(), ObjectError> {
        if let Some(old) = self.selector16(camera)?.target_190 {
            self.release_actor_service(old)?;
        }
        self.selector16_mut(camera)?.target_190 = target;
        Ok(())
    }

    /// 004828D0 clones a follow target through 004825D0(handle, &focus_14).
    /// The explicit-point constructor leaves refreshed +08 and +1A unwritten.
    pub fn copy_camera_primary_to_secondary(
        &mut self,
        camera: ObjectId,
    ) -> Result<(), ObjectError> {
        let target = match self.selector16(camera)?.target_18c {
            Some(reference) => self
                .clone_native_target(reference, WeightedTargetConstructorNull::ActorRegistry)
                .map_err(|error| match error {
                    crate::object_runtime::NativeTargetError::Object(error) => error,
                    crate::object_runtime::NativeTargetError::Heap(error) => error.into(),
                    crate::object_runtime::NativeTargetError::MissingProcessNull => {
                        unreachable!("follow clone does not readprocessNULL")
                    }
                })?,
            None => None,
        };
        self.replace_camera_secondary_target(camera, target)
    }

    /// 00410EE0 applies 00410EC0 to both smoothing blocks in order.
    pub fn set_camera_parameter_mode(
        &mut self,
        camera: ObjectId,
        mode: u32,
    ) -> Result<(), ObjectError> {
        let state = self.selector16_mut(camera)?;
        state.block_138.set_mode(mode);
        state.block_15c.set_mode(mode);
        Ok(())
    }
}

/// Stable virtual policies, selected by actual class/vtable provenance.
enum ActorViewModePolicy {
    ModeZeroOnly,
    StoreByte,
    Flag0ThenStoreByte,
}

#[cfg(test)]
#[path = "selector16/view_mode_tests.rs"]
mod view_mode_tests;
