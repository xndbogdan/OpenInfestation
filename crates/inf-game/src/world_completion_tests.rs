use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
};
use crate::world_mission::WorldMissionInputs;
use inf_formats::MissionSaveState;

const SINE: [u16; 4096] = [0; 4096];
const PROCESS: ParticleFactoryProcessState = ParticleFactoryProcessState {
    network_flag: 0,
    local_slot: 1,
    host_flag: 0,
    settings: [0; 20],
    process_null_handle: Some(ActorHandle::from_raw(0)),
};

fn inputs(network_active: bool) -> WorldCompletionInputs<'static> {
    WorldCompletionInputs {
        network_active,
        process: None,
    }
}

fn actor_inputs() -> WorldCompletionInputs<'static> {
    WorldCompletionInputs {
        network_active: false,
        process: Some(&PROCESS),
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn mission() -> WorldMission {
    WorldMission::new(
        MissionSaveState::new_game(),
        WorldMissionInputs {
            current_world_a0: 22,
            dword_1c: 0,
            byte_13: None,
            record_0c: None,
        },
    )
}

fn actor(arena: &mut ObjectArena) -> ObjectId {
    arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        })
        .unwrap()
}

#[derive(Default)]
struct Host {
    calls: u32,
    fail: bool,
}
impl WorldCompletionHost for Host {
    type Error = &'static str;
    fn flush_link_records(&mut self) -> Result<(), Self::Error> {
        self.calls += 1;
        if self.fail {
            Err("cleanup")
        } else {
            Ok(())
        }
    }
}

#[test]
fn intro_completion_marks_current_world_and_selects_level_one_without_changing_current() {
    let mut mission = mission();
    mission.save.owner_3c[22] = 0x102;
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    let mut host = Host::default();
    let outcome = complete_world(
        &mut mission,
        &mut arena,
        INTRO_COMPLETION,
        inputs(false),
        &mut host,
    )
    .unwrap();
    assert_eq!(
        outcome,
        WorldCompletionOutcome::Accepted {
            request_phase: SessionPhase::Phase8
        }
    );
    assert_eq!(host.calls, 1);
    assert_eq!(mission.current_world_a0, 22);
    assert_eq!(mission.save.owner_3c[22], 0x103);
    assert_eq!(mission.save.mission_selection, 18);
    assert_eq!(mission.save.owner_a4, 0);
    assert_eq!(arena.world_player(player).unwrap().byte_ed, 0);
    assert_eq!(arena.world_player(player).unwrap().dword_f0, 0);
}

#[test]
fn network_and_cleanup_failure_do_not_mutate_the_mission() {
    let mut mission = mission();
    let before = mission.clone();
    let mut arena = arena();
    let mut host = Host {
        calls: 0,
        fail: true,
    };
    assert_eq!(
        complete_world(
            &mut mission,
            &mut arena,
            INTRO_COMPLETION,
            inputs(true),
            &mut host
        )
        .unwrap(),
        WorldCompletionOutcome::NetworkIgnored
    );
    assert_eq!(host.calls, 0);
    assert_eq!(mission, before);
    assert!(matches!(
        complete_world(
            &mut mission,
            &mut arena,
            INTRO_COMPLETION,
            inputs(false),
            &mut host
        ),
        Err(WorldCompletionError::Cleanup("cleanup"))
    ));
    assert_eq!(host.calls, 1);
    assert_eq!(mission, before);
}

#[test]
fn rejected_actor_is_disposed_after_cleanup_before_mission_writes() {
    let mut mission = mission();
    let before = mission.clone();
    let mut arena = arena();
    arena.add_world_player([0; 6]);
    let actor = actor(&mut arena);
    let mut host = Host::default();
    let request = WorldCompletionRequest {
        actor: Some(actor),
        ..INTRO_COMPLETION
    };
    assert_eq!(
        complete_world(&mut mission, &mut arena, request, actor_inputs(), &mut host).unwrap(),
        WorldCompletionOutcome::ActorRejected
    );
    assert_eq!(host.calls, 1);
    // 00479FA0 queues disposal; it does not synchronously destroy the actor.
    assert!(arena.actor(actor).is_ok());
    assert!(arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|node| node.object == actor));
    assert_eq!(mission, before);
}

#[test]
fn supplied_actor_lookup_and_low_word_ordinal_wrap_follow_native_policy() {
    let mut mission = mission();
    let mut arena = arena();
    arena.add_world_player([0; 6]);
    let second = arena.add_world_player([1; 6]);
    let actor = actor(&mut arena);
    arena
        .bind_player_vehicle(second, arena.actor(actor).unwrap().handle)
        .unwrap();
    let request = WorldCompletionRequest {
        next_selection: 3,
        start_ordinal: 0,
        actor: Some(actor),
    };
    let mut host = Host::default();
    assert_eq!(
        complete_world(&mut mission, &mut arena, request, actor_inputs(), &mut host).unwrap(),
        WorldCompletionOutcome::Accepted {
            request_phase: SessionPhase::Phase8
        }
    );
    assert!(arena.actor(actor).is_ok());
    assert_eq!(mission.save.owner_a4, u16::MAX);
    assert_eq!(mission.save.mission_selection, 3);
}

#[test]
fn missing_first_player_is_reached_after_native_mission_writes() {
    let mut mission = mission();
    let mut arena = arena();
    let mut host = Host::default();
    assert!(matches!(
        complete_world(
            &mut mission,
            &mut arena,
            INTRO_COMPLETION,
            inputs(false),
            &mut host
        ),
        Err(WorldCompletionError::MissingPlayer)
    ));
    assert_eq!(host.calls, 1);
    assert_eq!(mission.save.owner_3c[22], 1);
    assert_eq!(mission.save.mission_selection, 18);
    assert_eq!(mission.save.owner_a4, 0);
}

#[test]
fn missing_process_null_retains_cleanup_before_actor_disposal_or_mission_writes() {
    let mut mission = mission();
    let before = mission.clone();
    let mut arena = arena();
    let actor = actor(&mut arena);
    let before_actor = arena.object(actor).unwrap().clone();
    let before_disposal = arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .to_vec();
    let request = WorldCompletionRequest {
        actor: Some(actor),
        ..INTRO_COMPLETION
    };
    let unavailable = ParticleFactoryProcessState {
        process_null_handle: None,
        ..PROCESS
    };
    let mut host = Host::default();
    for process in [None, Some(&unavailable)] {
        assert!(matches!(
            complete_world(
                &mut mission,
                &mut arena,
                request,
                WorldCompletionInputs {
                    network_active: false,
                    process
                },
                &mut host,
            ),
            Err(WorldCompletionError::PlayerLookup(
                PlayerActorLookupError::MissingProcessNullHandle
            ))
        ));
        assert_eq!(mission, before);
        assert_eq!(arena.object(actor).unwrap(), &before_actor);
        assert_eq!(
            arena.list(crate::object_runtime::ObjectList::Disposal),
            before_disposal
        );
    }
    assert_eq!(host.calls, 2);
}

#[test]
fn actor_lookup_unavailability_does_not_preempt_network_or_cleanup_failure() {
    let mut mission = mission();
    let before = mission.clone();
    let mut arena = arena();
    let actor = actor(&mut arena);
    let request = WorldCompletionRequest {
        actor: Some(actor),
        ..INTRO_COMPLETION
    };
    let mut host = Host {
        calls: 0,
        fail: true,
    };
    assert_eq!(
        complete_world(&mut mission, &mut arena, request, inputs(true), &mut host).unwrap(),
        WorldCompletionOutcome::NetworkIgnored
    );
    assert_eq!(host.calls, 0);
    assert!(matches!(
        complete_world(&mut mission, &mut arena, request, inputs(false), &mut host),
        Err(WorldCompletionError::Cleanup("cleanup"))
    ));
    assert_eq!(host.calls, 1);
    assert_eq!(mission, before);
    assert!(arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .is_empty());
}

#[test]
fn actor_completion_uses_process_null_instead_of_registry_constructor_null() {
    let mut mission = mission();
    let before = mission.clone();
    let mut arena = arena();
    let player = arena.add_world_player([0; 6]);
    let actor = actor(&mut arena);
    arena
        .bind_player_vehicle(player, arena.actor(actor).unwrap().handle)
        .unwrap();
    let request = WorldCompletionRequest {
        actor: Some(actor),
        ..INTRO_COMPLETION
    };
    let process = ParticleFactoryProcessState {
        process_null_handle: Some(ActorHandle::from_raw(0x8000_0000)),
        ..PROCESS
    };
    let mut host = Host::default();
    // The constructed raw D8=registry NULL is nonnull for this actual live
    // process value, so it is authoritative and does not fall back to D4.
    assert_eq!(
        complete_world(
            &mut mission,
            &mut arena,
            request,
            WorldCompletionInputs {
                network_active: false,
                process: Some(&process)
            },
            &mut host,
        )
        .unwrap(),
        WorldCompletionOutcome::ActorRejected
    );
    assert_eq!(host.calls, 1);
    assert_eq!(mission, before);
    assert!(arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|node| node.object == actor));
}
