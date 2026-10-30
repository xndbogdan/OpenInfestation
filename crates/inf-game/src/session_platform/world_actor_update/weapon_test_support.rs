//! The world's owners and the vehicle's weapon emission the weapons' world
//! tests share.

use crate::frontend_camera::AnimationClock;
use crate::object_runtime::{
    ActorServiceAllocationRequest, ActorServiceResourceKind, CameraSettings, NativeWeaponFireNode,
    ObjectArena, ObjectId, ProcessActorBehaviorTables, WeaponEmissionRequest, WeaponFiringContext,
    WeaponFiringEmitter, WeaponFiringRuntime,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::process_particles::ProcessParticleRuntime;
use crate::process_sound::ProcessSoundRuntime;
use crate::process_tasks::ProcessTaskRuntime;
use crate::session::SessionState;
use crate::session_platform::world_actor_factory::{RequiredListenerEffects, WorldActorChildHost};
use crate::world_furniture_effect::FurnitureEffectQueue;
use inf_formats::native_basis::NativeBasisMath;
use inf_formats::FrontendExecutable;

/// The world's owners a grenade's update borrows.
pub(super) struct Owners {
    pub tasks: ProcessTaskRuntime,
    pub particles: ProcessParticleRuntime,
    pub sounds: ProcessSoundRuntime,
    pub furniture: FurnitureEffectQueue,
    pub process: ParticleFactoryProcessState,
    pub tables: ProcessActorBehaviorTables,
    pub clock: AnimationClock,
    pub state: SessionState,
    /// G+1B4's jitter, lent to the grenade's messages.
    pub map: crate::world_map::WorldMapState,
}

impl Owners {
    pub(super) fn new(executable: FrontendExecutable<'_>) -> Self {
        // The session's tasks, retained at boot (0048EE20).
        let process = ParticleFactoryProcessState::native_startup();
        let mut tasks = ProcessTaskRuntime::default();
        tasks.retain(process.process_null_handle.unwrap()).unwrap();
        Self {
            tasks,
            particles: ProcessParticleRuntime::default(),
            sounds: ProcessSoundRuntime::new(executable).unwrap(),
            furniture: FurnitureEffectQueue::default(),
            process,
            tables: ProcessActorBehaviorTables::from_executable(executable).unwrap(),
            clock: AnimationClock::default(),
            state: SessionState::default(),
            map: crate::world_map::WorldMapState::new(),
        }
    }
}

/// Fire the vehicle's weapon row `row` along +X at `position`, through the
/// world's child factory.
pub(super) fn fire(
    world: &mut crate::world_backend::WorldSession,
    resident: &crate::ResidentRuntime,
    executable: FrontendExecutable<'_>,
    owners: &mut Owners,
    sine: &[u16; 4096],
    row: u32,
    position: [u16; 3],
    rng: &mut u32,
) {
    let node = NativeWeaponFireNode {
        direction_bits: [0x3f80_0000, 0, 0],
        compensation: 0,
        group: 0,
        selection: row,
        speed_scale: 0,
        next: None,
    };
    emit(
        world, resident, executable, owners, sine, node, position, rng,
    );
}

/// Emit `node` from the vehicle at `position` (0048C890).
pub(super) fn emit(
    world: &mut crate::world_backend::WorldSession,
    resident: &crate::ResidentRuntime,
    executable: FrontendExecutable<'_>,
    owners: &mut Owners,
    sine: &[u16; 4096],
    node: NativeWeaponFireNode,
    position: [u16; 3],
    rng: &mut u32,
) {
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let math = NativeBasisMath::new(*sine);
    let reference = world
        .arena
        .actor_service_heap_mut()
        .allocate(ActorServiceAllocationRequest {
            kind: ActorServiceResourceKind::WeaponFireNode,
        })
        .unwrap()
        .unwrap();
    world
        .arena
        .actor_service_heap_mut()
        .construct_weapon_fire_node(reference, node)
        .unwrap();
    let node = reference;
    let mut effects = RequiredListenerEffects;
    let mut children = WorldActorChildHost {
        residency: resident,
        executable,
        process: &owners.process,
        terrain: &world.terrain,
        world_flags: world.world_flags,
        clock: 0,
        sine,
        math: &math,
        camera_settings: Some(CameraSettings {
            first_person: 0,
            camera_style: 0,
        }),
        world_start: Some(&mut world.world_start),
        listeners: &mut world.listeners,
        listener_effects: &mut effects,
    };
    let mut pulse = crate::session_platform::DrawPulseHost {
        flash: &mut world.flash,
        map: Some(&mut owners.map),
        residency: resident,
    };
    WeaponFiringRuntime {
        executable,
        factory_process: &owners.process,
        particles: &mut owners.particles,
        statistics: None,
        sounds: &mut owners.sounds,
        residency: resident,
        phase: Some(4),
        children: Some(&mut children),
        pulse: Some(&mut pulse),
    }
    .emit(
        &mut world.arena,
        &mut WeaponFiringContext {
            tables: &mut owners.tables,
            rng,
        },
        WeaponEmissionRequest {
            actor: vehicle,
            node,
            position,
        },
    )
    .unwrap();
}

pub(super) fn disposed(arena: &ObjectArena, object: ObjectId) -> bool {
    arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|node| node.object == object)
}
