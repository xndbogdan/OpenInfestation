//! Complete attract actor setup 004879CA..00487AE6, after the outer guards.
//!
//! H1/0 is captured before the genuine direct factory. Publication and a
//! fresh resolver precede late H12 acquisition and the shared XYZ-only copy.
//! Exclusive publication and immutable residency/process loans exclude
//! synchronous aliases of those owners. A terrain host may mutate the arena;
//! reached errors retain all earlier effects. Target allocation then precedes
//! fresh publication/handle reads and the captured camera relation. Timer and
//! native phase admission remain separate; no enclosing game-constructor state
//! or playback is implied.

use thiserror::Error;

use super::{
    construct_selector93_direct, ActorConstructionError, ActorHandle, ActorTerrainHost,
    CameraSettings, FollowTargetRequest, NativeTargetClass, ObjectArena, ObjectError, ObjectId,
    Selector93FactoryError, Selector93FactoryRequest,
};
use crate::word_resources::{resolve_h1_word, H1WordError};

/// Supplied retained game publication fields, independent of actor ownership.
/// There is no implicit native constructor or initial NULL value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttractActorSetupState {
    /// Semantic object publication at game +5E0.
    pub object_5e0: Option<ObjectId>,
    /// Actual packed actor handle publication at game +5DC. None represents
    /// unwritten backing, distinct from a supplied actual NULL handle.
    pub handle_5dc: Option<ActorHandle>,
}

pub struct AttractActorSetupRequest<'a> {
    pub publication: &'a mut AttractActorSetupState,
    pub factory: Selector93FactoryRequest<'a>,
    /// Actual camera captured by the enclosing caller before the outer guards.
    pub camera: ObjectId,
    /// Current immutable settings for the reached replacement phase.
    pub settings: CameraSettings,
}

#[derive(Debug, Error)]
pub enum AttractActorSetupError<T> {
    #[error(transparent)]
    Position(#[from] H1WordError),
    #[error(transparent)]
    Factory(#[from] Selector93FactoryError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Resolve(#[from] ActorConstructionError),
    #[error("attract placement requires current H12/0: {0}")]
    Terrain(T),
    #[error("attract setup requires target allocation: {0}")]
    Allocation(crate::object_runtime::ActorServiceHeapError),
}

/// Complete setup in the constructor-backed, registered source domain.
/// Required failures retain constructor, callback and publication effects.
pub fn setup_attract_actor<H: ActorTerrainHost>(
    arena: &mut ObjectArena,
    request: AttractActorSetupRequest<'_>,
    terrain: &mut H,
) -> Result<ObjectId, AttractActorSetupError<H::Error>> {
    let object = place_actor::<H>(arena, request.publication, request.factory, terrain)?;
    attach_attract_camera::<H::Error>(
        arena,
        AttractCameraRelationRequest {
            publication: request.publication,
            camera: request.camera,
            settings: request.settings,
        },
    )?;
    Ok(object)
}

/// The complete placement phase; no camera input is consumed on its errors.
fn place_actor<T: ActorTerrainHost>(
    arena: &mut ObjectArena,
    publication: &mut AttractActorSetupState,
    factory: Selector93FactoryRequest<'_>,
    terrain: &mut T,
) -> Result<ObjectId, AttractActorSetupError<T::Error>> {
    // 004879E3 consumes the first raw DWORD, before even factory preflight.
    // No WorldStart pair, override, heading or count read belongs to native
    // placement. Shared source validation is the typed admission boundary.
    let packed_xz = resolve_h1_word(factory.runtime, 0)?;
    let xz = [packed_xz as u16, (packed_xz >> 16) as u16];
    let object = construct_selector93_direct(arena, factory)?;

    // 004879EE stores the object before the fresh actor+84 read004879F4.
    publication.object_5e0 = Some(object);
    let handle = arena.actor(object)?.handle;
    publication.handle_5dc = Some(handle); // 004879FB

    // 00487A01 ignores the returned object, but retains real cache effects.
    let _ = arena.resolve_actor_handle(handle)?;

    let current = terrain
        .terrain(arena, object)
        .map_err(AttractActorSetupError::Terrain)?;
    let raw_height = current.raw_height_at(xz);
    let plane = (current.plane_base() >> (current.plane_shift() & 31)) as i16;
    // 00487A68 compares DX/AX as signed WORDs, then00487A75 adds512.
    let y = (raw_height.max(plane) as u16).wrapping_add(512);
    // The exclusive state loan keeps this publication unchanged during the
    // service. The arena receiver is still freshly validated by the real copy.
    let published = publication.object_5e0.expect("reached publication");
    arena.copy_actor_position(published, [xz[0], y, xz[1]])?;
    Ok(published)
}

/// One consumed acquisition phase. The complete entry guarantees publication;
/// the false allocation arm does not read it or construct/resolve a new target.
struct AttractCameraRelationRequest<'a> {
    publication: &'a AttractActorSetupState,
    camera: ObjectId,
    settings: CameraSettings,
}

fn attach_attract_camera<T>(
    arena: &mut ObjectArena,
    request: AttractCameraRelationRequest<'_>,
) -> Result<(), AttractActorSetupError<T>> {
    // 00487A90 malloc(20h) precedes any camera validation or new referent read.
    let target = arena
        .allocate_actor_service(super::ActorServiceAllocationRequest {
            kind: super::ActorServiceResourceKind::Target(NativeTargetClass::ActorFollow),
        })
        .map_err(AttractActorSetupError::Allocation)?;
    if let Some(reference) = target {
        let object = request.publication.object_5e0.expect("reached publication");
        let handle = arena.actor(object)?.handle;
        let data = arena.follow_target(FollowTargetRequest::resolve_actor(handle))?;
        arena.construct_native_target(reference, super::NativeTarget::ActorFollow(data))?;
    }
    arena.complete_attract_camera_relation(request.camera, target, request.settings)?;
    Ok(())
}

#[cfg(test)]
#[path = "attract_placement_tests.rs"]
mod tests;
