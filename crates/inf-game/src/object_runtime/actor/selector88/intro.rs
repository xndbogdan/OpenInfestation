//! Selector88's retained native intro script and read/update adapter.
//! Keeps original shared H14 custody and post-load handle rewriting intact.

use thiserror::Error;

use super::{selector88_row, RetainedInstanceCommand};
use crate::intro_sequence::{IntroActorList, IntroAllocationWords, IntroRoute, IntroSequence};
use crate::object_runtime::{ActorExtension, ObjectArena, ObjectError, ObjectId, WorldLoadError};

#[derive(Clone, Debug)]
pub struct Selector88IntroScript {
    retained: RetainedInstanceCommand,
}

#[derive(Debug, Error)]
pub enum Selector88IntroError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Script(#[from] WorldLoadError),
    #[error("selector88 {object:?} has no retained intro script")]
    MissingScript { object: ObjectId },
    #[error("selector88 {object:?} intro reads unwritten allocation DWORD +{offset:X}")]
    UnwrittenAllocation { object: ObjectId, offset: u32 },
    #[error("selector88 intro allocation words have already been supplied")]
    AllocationAlreadySupplied,
    #[error("fresh intro deadline approximation requires an untouched newly attached controller")]
    NotFreshAttachment,
    #[error("selector88 intro state belongs to a different actor")]
    WrongController,
}

impl Selector88IntroScript {
    /// Shares the exact command allocation retained by command16.
    pub fn retained(&self) -> &RetainedInstanceCommand {
        &self.retained
    }

    fn offset(&self, relative: u32) -> Result<u32, WorldLoadError> {
        self.retained
            .allocation_offset
            .checked_add(relative)
            .ok_or(WorldLoadError::OffsetOverflow)
    }

    /// The four route IDs stay authored one-based integers. 00404F00 compares
    /// placement +2A to this ID minus one; these are not rewritten as handles.
    pub fn route_reference(&self, route: IntroRoute) -> Result<i32, WorldLoadError> {
        let relative = match route {
            IntroRoute::Primary00 => 0,
            IntroRoute::Secondary04 => 4,
            IntroRoute::Advance08 => 8,
            IntroRoute::Exit0c => 12,
        };
        Ok(self
            .retained
            .allocation()
            .runtime_allocation()
            .dword(self.offset(relative)?)? as i32)
    }

    pub fn main_reference_handle(&self) -> Result<u32, WorldLoadError> {
        Ok(self
            .retained
            .allocation()
            .runtime_allocation()
            .dword(self.offset(0x10)?)?)
    }

    /// Signed counts skip arrays when nonpositive. Reads always use the shared
    /// runtime image, so changes through aliases remain visible.
    pub fn reference_count(&self, list: IntroActorList) -> Result<i32, WorldLoadError> {
        let (count, _) = pair(list);
        Ok(self
            .retained
            .allocation()
            .runtime_allocation()
            .dword(self.offset(count)?)? as i32)
    }

    /// Reads a rewritten handle through the exact relocation, retaining source
    /// identity. This intentionally permits the unconditional Exit2c row0 read
    /// in state9 even when the separately stored count is zero (retail does so).
    pub fn reference_handle(
        &self,
        list: IntroActorList,
        index: u32,
    ) -> Result<u32, WorldLoadError> {
        let (_, pointer) = pair(list);
        let row = selector88_row(self.retained.allocation(), self.offset(pointer)?, index)?;
        Ok(self.retained.allocation().runtime_allocation().dword(row)?)
    }
}

fn pair(list: IntroActorList) -> (u32, u32) {
    match list {
        IntroActorList::Primary14 => (0x14, 0x18),
        IntroActorList::Sequential1c => (0x1c, 0x20),
        IntroActorList::Secondary24 => (0x24, 0x28),
        IntroActorList::Exit2c => (0x2c, 0x30),
    }
}

impl ObjectArena {
    /// FreshIntroDeadlineApproximation: choose zero for allocator-owned +140
    /// only at first attachment. Both retail allocation paths can reuse dirty
    /// payload, so this is a replaceable allocation policy, not a native writer.
    /// +138 stays unknown: ordinary stage-zero entry overwrites it before any
    /// host query. +150 remains unknown until native stage eight writes it.
    pub fn apply_fresh_intro_deadline_approximation(
        &mut self,
        object: ObjectId,
    ) -> Result<(), Selector88IntroError> {
        let state = self.selector88(object)?;
        if state.word_138.is_some()
            || state.word_140.is_some()
            || state.word_150.is_some()
            || state.word_144 != 0
            || state.word_148 != 0
            || state.word_14c != 0
            || state.actor_154.is_some()
            || state.actor_158.is_some()
        {
            return Err(Selector88IntroError::NotFreshAttachment);
        }
        let ActorExtension::Selector88(state) = &mut self.actor_mut(object)?.extension else {
            unreachable!()
        };
        state.word_140 = Some(0);
        Ok(())
    }

    pub fn selector88_intro_script(
        &self,
        object: ObjectId,
    ) -> Result<Selector88IntroScript, Selector88IntroError> {
        let retained = self
            .selector88(object)?
            .retained_13c
            .clone()
            .ok_or(Selector88IntroError::MissingScript { object })?;
        Ok(Selector88IntroScript { retained })
    }

    /// Allocation history is separate from constructor writes. Supply these
    /// only when known or under an explicitly recorded approximation policy.
    pub fn supply_selector88_intro_allocation(
        &mut self,
        object: ObjectId,
        allocation: IntroAllocationWords,
    ) -> Result<(), Selector88IntroError> {
        let current = self.selector88(object)?;
        if current.word_138.is_some() || current.word_140.is_some() || current.word_150.is_some() {
            return Err(Selector88IntroError::AllocationAlreadySupplied);
        }
        let ActorExtension::Selector88(state) = &mut self.actor_mut(object)?.extension else {
            unreachable!()
        };
        state.word_138 = Some(allocation.elapsed_138 as u32);
        state.word_140 = Some(allocation.deadline_140 as u32);
        state.word_150 = allocation.sequential_timer_150.map(|word| word as u32);
        Ok(())
    }

    /// Extract the native +24 callback state before borrowing the arena into a
    /// concrete host. Publish it afterward, including on reached host failure.
    pub fn selector88_intro_sequence(
        &self,
        object: ObjectId,
    ) -> Result<IntroSequence<ObjectId>, Selector88IntroError> {
        let state = self.selector88(object)?;
        let known = |value: Option<u32>, offset| {
            value
                .map(|word| word as i32)
                .ok_or(Selector88IntroError::UnwrittenAllocation { object, offset })
        };
        Ok(IntroSequence {
            controller: object,
            // 00444930 stage zero/timer zero reaches 00443B10's helper entry
            // on its first tick, which writes +138=0 before the first host
            // query. Its preceding add has no consumer on this path. Keep
            // allocator storage unknown until that reached native write.
            elapsed_138: if state.word_138.is_none()
                && state.word_140.is_some()
                && state.word_148 == 0
                && state.word_144 == 0
                && state.actor_154.is_none()
                && state.actor_158.is_none()
            {
                0
            } else {
                known(state.word_138, 0x138)?
            },
            deadline_140: known(state.word_140, 0x140)?,
            timer_144: state.word_144 as i32,
            stage_148: state.word_148,
            sequential_index_14c: state.word_14c as i32,
            sequential_timer_150: state.word_150.map(|word| word as i32),
            primary_helper_154: state.actor_154,
            secondary_helper_158: state.actor_158,
        })
    }

    pub fn publish_selector88_intro_sequence(
        &mut self,
        object: ObjectId,
        sequence: &IntroSequence<ObjectId>,
    ) -> Result<(), Selector88IntroError> {
        if sequence.controller != object {
            return Err(Selector88IntroError::WrongController);
        }
        self.selector88(object)?;
        let ActorExtension::Selector88(state) = &mut self.actor_mut(object)?.extension else {
            unreachable!()
        };
        state.word_138 = Some(sequence.elapsed_138 as u32);
        state.word_140 = Some(sequence.deadline_140 as u32);
        state.word_144 = sequence.timer_144 as u32;
        state.word_148 = sequence.stage_148;
        state.word_14c = sequence.sequential_index_14c as u32;
        state.word_150 = sequence.sequential_timer_150.map(|word| word as u32);
        state.actor_154 = sequence.primary_helper_154;
        state.actor_158 = sequence.secondary_helper_158;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_map::{MapLoadContext, MapLoadState, ProcessWorldMapRuntime};
    use crate::{frontend_assets::FrontendAssets, Installation, ResidencyRequest, ResidentRuntime};
    use inf_formats::FrontendExecutable;

    #[test]
    fn fresh_deadline_policy_only_owns_initial_140_and_rejects_resume() {
        let sine = [0; 4096];
        let mut arena =
            ObjectArena::with_actor_registry(crate::object_runtime::ActorRegistryConfig {
                owner_count: 2,
                initial_capacity: 8,
                null_handle: crate::object_runtime::ActorHandle::from_raw(0),
            })
            .unwrap();
        let object = arena
            .allocate_selector88(crate::object_runtime::ActorConstructionRequest {
                constructor_null:
                    crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: None,
                owner: 1,
                terrain: crate::object_runtime::ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: None,
                    sine_quarter: &sine,
                },
            })
            .unwrap();
        arena
            .apply_fresh_intro_deadline_approximation(object)
            .unwrap();
        let stored = arena.selector88(object).unwrap();
        assert_eq!(
            (stored.word_138, stored.word_140, stored.word_150),
            (None, Some(0), None)
        );
        let mut sequence = arena.selector88_intro_sequence(object).unwrap();
        assert_eq!((sequence.elapsed_138, sequence.deadline_140), (0, 0));
        assert!(matches!(
            arena.apply_fresh_intro_deadline_approximation(object),
            Err(Selector88IntroError::NotFreshAttachment)
        ));
        sequence.stage_148 = 1;
        arena
            .publish_selector88_intro_sequence(object, &sequence)
            .unwrap();
        assert!(matches!(
            arena.apply_fresh_intro_deadline_approximation(object),
            Err(Selector88IntroError::NotFreshAttachment)
        ));
    }

    #[test]
    fn mission22_intro_retained_script_and_native_state_share_the_loaded_allocation() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        runtime
            .apply(ResidencyRequest::Activate { level: 47 })
            .unwrap();
        let assets = FrontendAssets::open(&root).unwrap();
        let grants = inf_formats::parse_grant_table(
            FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        let offline = crate::offline_prepare::offline_game(&grants).unwrap();
        let mut maps = ProcessWorldMapRuntime::native_startup();
        let mut map_load = MapLoadState::native_mission_constructor();
        let mut rng = 0;
        let mut load_owner = crate::world_backend::WorldLoadOwner::import_saved(
            crate::world_backend::OfflineWorldLoadRequest {
                mission: offline.mission,
                player: offline.player,
                previous_world: 1,
                visibility: None,
            },
        )
        .unwrap();
        let mut load_listener_effects = crate::world_backend::EmptyListenerEffects;
        //Explicit startup-proven fixture process owner, shared by retries below.
        let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
            inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        let load_metadata = crate::world_backend::load_offline_world(
            &mut runtime,
            crate::world_backend::OfflineWorldLoadInputs {
                terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(crate::terrain_runtime::TerrainSpacing::Units256), sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                furniture_radius: &mut furniture_radius,
                        reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
                owner: &mut load_owner,
                listener_effects: &mut load_listener_effects,
                policy: crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
                particles: None,
                phase: None,
            assets: &assets,
            process: Some(&crate::process_particles::factories::ParticleFactoryProcessState::native_startup()),
            map: MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            furniture: None,
                settings_84: 0,
                ultra: None,
        },
        )
        .unwrap();
        let mut world = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
        let controller = world.arena.find_actor_by_placement(118).unwrap();
        let script = world.arena.selector88_intro_script(controller).unwrap();
        let alias = script.clone();
        assert!(std::sync::Arc::ptr_eq(
            script.retained().allocation(),
            alias.retained().allocation()
        ));
        assert_eq!(script.retained().allocation_offset, 6524);
        let routes = [
            IntroRoute::Primary00,
            IntroRoute::Secondary04,
            IntroRoute::Advance08,
            IntroRoute::Exit0c,
        ]
        .map(|route| script.route_reference(route).unwrap());
        let lists = [
            IntroActorList::Primary14,
            IntroActorList::Sequential1c,
            IntroActorList::Secondary24,
            IntroActorList::Exit2c,
        ];
        let counts = lists.map(|list| script.reference_count(list).unwrap());
        assert_eq!(routes, [59, 65, 70, 108]);
        assert_eq!(counts, [6, 2, 5, 2]);
        for route in routes {
            assert!(world.arena.find_route_node(route as u32).is_some());
        }
        let authored: [&[i32]; 4] = [
            &[48, 49, 50, 51, 83, 84],
            &[30, 31],
            &[68, 69, 52, 79, 80],
            &[97, 98],
        ];
        for (list, references) in lists.into_iter().zip(authored) {
            for (row, &reference) in references.iter().enumerate() {
                let handle = script.reference_handle(list, row as u32).unwrap();
                let resolved = world
                    .arena
                    .resolve_actor_handle(crate::object_runtime::ActorHandle::from_raw(handle))
                    .unwrap();
                assert_eq!(resolved, world.arena.find_actor_by_placement(reference));
                assert!(resolved.is_some());
            }
        }
        assert!(matches!(
            world.arena.selector88_intro_sequence(controller),
            Err(Selector88IntroError::UnwrittenAllocation { offset: 0x138, .. })
        ));
        world
            .arena
            .supply_selector88_intro_allocation(
                controller,
                IntroAllocationWords {
                    elapsed_138: -7,
                    deadline_140: 1234,
                    sequential_timer_150: None,
                },
            )
            .unwrap();
        let mut sequence = world.arena.selector88_intro_sequence(controller).unwrap();
        assert_eq!(
            (
                sequence.elapsed_138,
                sequence.deadline_140,
                sequence.sequential_timer_150
            ),
            (-7, 1234, None)
        );
        assert!(matches!(
            world.arena.supply_selector88_intro_allocation(
                controller,
                IntroAllocationWords {
                    elapsed_138: 0,
                    deadline_140: 0,
                    sequential_timer_150: None,
                }
            ),
            Err(Selector88IntroError::AllocationAlreadySupplied)
        ));
        sequence.stage_148 = 9;
        sequence.timer_144 = -1;
        world
            .arena
            .publish_selector88_intro_sequence(controller, &sequence)
            .unwrap();
        assert_eq!(
            world.arena.selector88_intro_sequence(controller).unwrap(),
            sequence
        );
        assert!(std::sync::Arc::ptr_eq(
            script.retained().allocation(),
            world
                .arena
                .selector88(controller)
                .unwrap()
                .retained_13c
                .as_ref()
                .unwrap()
                .allocation()
        ));
    }
}
