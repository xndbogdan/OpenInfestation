use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, ObjectList, ObjectNode, ObjectNodeKind,
    Selector12GrantContext,
};

const REGISTRY_NULL: ActorHandle = ActorHandle::from_raw(0x0801_0000);
const PROCESS_NULL: ActorHandle = ActorHandle::from_raw(0x1357_2468);

fn allocation() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([0; 3]),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &[0; 4096],
        },
    }
}

fn known(flags: u32) -> ActorCaptureSavedFlags {
    ActorCaptureSavedFlags {
        bytes: Some(flags.to_le_bytes().map(Some)),
    }
}

#[derive(Default)]
struct Host {
    phase: crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
    commands: Vec<ActorCapturePlayerCommand>,
    mutate: Option<ObjectId>,
    fail: bool,
}

impl ActorCaptureHost for Host {
    type Error = &'static str;

    fn player_command(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut ActorCaptureContext<'_>,
        request: ActorCapturePlayerCommand,
    ) -> Result<(), Self::Error> {
        assert_eq!((request.code, request.value), (0x42, 0));
        self.commands.push(request);
        if let Some(actor) = self.mutate {
            // Explicit supplied command effects; these are not a port of the
            // unowned 0046C760 body or a source of constructor defaults.
            arena.set_flag0(actor, false).unwrap();
            arena.actor_mut(actor).unwrap().word_6c = 99;
            context.process.process_null_handle = None;
            self.phase.set(Some(77));
            context.saved_flags.bytes = Some([Some(0xab), None, None, None]);
        }
        if self.fail {
            Err("command stopped")
        } else {
            Ok(())
        }
    }
}

struct Fixture {
    arena: ObjectArena,
    actor: ObjectId,
    source: ObjectId,
    peer: ObjectId,
    process: ParticleFactoryProcessState,
    saved: ActorCaptureSavedFlags,
    host: Host,
}

impl Fixture {
    fn new() -> Self {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 16,
            null_handle: REGISTRY_NULL,
        })
        .unwrap();
        let actor = arena.allocate_actor(allocation()).unwrap();
        let source = arena.allocate_actor(allocation()).unwrap();
        let peer = arena.allocate_actor(allocation()).unwrap();
        Self {
            arena,
            actor,
            source,
            peer,
            process: ParticleFactoryProcessState {
                network_flag: 0,
                local_slot: 1,
                host_flag: 0,
                settings: [0; 20],
                process_null_handle: Some(PROCESS_NULL),
            },
            saved: ActorCaptureSavedFlags {
                bytes: Some([None; 4]),
            },
            host: Host::default(),
        }
    }

    fn flags(&mut self, flags: u32, dirty: u16) {
        let base = &mut self.arena.owned_mut(self.actor).unwrap().state.base;
        base.flags = flags;
        base.dirty_word = dirty;
    }

    fn apply(&mut self, operation: u32) -> Result<bool, ActorCaptureError<&'static str>> {
        let source = self.arena.actor(self.source).unwrap().handle;
        self.apply_source(operation, source)
    }

    fn apply_source(
        &mut self,
        operation: u32,
        source: ActorHandle,
    ) -> Result<bool, ActorCaptureError<&'static str>> {
        self.arena.apply_shared_actor_capture(
            ActorCaptureRequest {
                actor: self.actor,
                operation,
                source,
            },
            &mut ActorCaptureContext {
                process: &mut self.process,
                saved_flags: &mut self.saved,
            },
            &mut self.host,
        )
    }
}

#[test]
fn constructor_holes_are_unknown_across_shared_and_derived_prefixes() {
    let mut f = Fixture::new();
    let request = DerivedActorConstructionRequest {
        actor: allocation(),
        world_mask_limits: [3, 3],
    };
    let derived = f.arena.allocate_derived_actor(request, &mut 0).unwrap();
    let family = f
        .arena
        .allocate_family428650_actor(request, &mut 0)
        .unwrap();
    let grants = inf_formats::GrantTable {
        virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let selector12 = f
        .arena
        .allocate_selector12_actor(
            request,
            &mut 0,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: Some(&[0; 6]),
            },
        )
        .unwrap();
    for actor in [f.actor, derived, family, selector12] {
        let state = f.arena.actor(actor).unwrap();
        assert_eq!(state.capture_handle_70, None);
        assert_eq!(state.capture_flags_d4, None);
        assert_eq!(state.owner_handle, REGISTRY_NULL, "+88 is distinct");
    }
}

#[test]
fn operation_consumes_low_byte_and_eligibility_bypasses_all_dependencies() {
    let mut f = Fixture::new();
    f.flags(0xc0, 0x8808);
    f.saved.bytes = None;
    assert_eq!(
        f.apply(0x100),
        Err(ActorCaptureError::MissingSavedFlagsStorage)
    );
    assert_eq!(f.arena.object(f.actor).unwrap().base.flags, 0xc0);
    f.flags(0x43, 0);
    f.saved.bytes = Some([None; 4]);
    assert_eq!(f.apply(0x101), Ok(true));
    assert_eq!(f.saved, known(0x43));

    for (operation, flags) in [(1, 0), (0x101, 0x80), (u32::MAX, 0xc0), (0, 0x40)] {
        let mut f = Fixture::new();
        f.flags(flags, 0x1234);
        f.arena.actor_handles = None;
        f.arena.add_world_player([0; 6]);
        f.process.process_null_handle = None;
        f.saved.bytes = None;
        let before = f.arena.object(f.actor).unwrap().clone();
        assert_eq!(f.apply(operation), Ok(false));
        assert_eq!(f.arena.object(f.actor).unwrap(), &before);
        assert!(f.host.commands.is_empty());
    }
}

#[test]
fn ordinary_capture_publishes_source_and_saved_flags_then_moves_nodes_in_order() {
    let mut f = Fixture::new();
    let flags = 0x0400_0147;
    f.flags(flags, 0xa000);
    let source = f.arena.actor(f.source).unwrap().handle;
    assert_eq!(f.apply(1), Ok(true));
    assert_eq!(f.saved, known(flags));
    let actor = f.arena.actor(f.actor).unwrap();
    assert_eq!(actor.capture_handle_70, Some(source));
    assert_eq!(actor.capture_flags_d4, Some(flags));
    assert_eq!(actor.owner_handle, REGISTRY_NULL);
    let base = &f.arena.object(f.actor).unwrap().base;
    assert_eq!(base.flags, 0x0400_00c0);
    assert_eq!(base.dirty_word, 0xa00a);
    let parking = f.arena.list(ObjectList::Parking);
    assert_eq!(
        &parking[parking.len() - 2..],
        &[
            ObjectNode {
                object: f.actor,
                kind: ObjectNodeKind::Flag0
            },
            ObjectNode {
                object: f.actor,
                kind: ObjectNodeKind::Flag1
            },
        ]
    );
    assert!(f.host.commands.is_empty());
}

#[test]
fn remote_capture_still_saves_and_disables_membership_without_overwriting_holes() {
    let mut f = Fixture::new();
    let old_handle = ActorHandle::from_raw(0xdead_beef);
    let actor = f.arena.actor_mut(f.actor).unwrap();
    actor.capture_handle_70 = Some(old_handle);
    actor.capture_flags_d4 = Some(0x1122_3344);
    f.flags(0x8000_0143, 0x4000);
    assert_eq!(f.apply(1), Ok(true));
    assert_eq!(f.saved, known(0x8000_0143));
    let actor = f.arena.actor(f.actor).unwrap();
    assert_eq!(actor.capture_handle_70, Some(old_handle));
    assert_eq!(actor.capture_flags_d4, Some(0x1122_3344));
    let base = &f.arena.object(f.actor).unwrap().base;
    assert_eq!(base.flags, 0x8000_0040);
    assert_eq!(base.dirty_word, 0x4002);
}

#[test]
fn resolver_is_lazy_and_null_request_preserves_actual_cache_effects() {
    let mut f = Fixture::new();
    f.flags(0x40, 0);
    let source = f.arena.actor(f.source).unwrap().handle;
    assert_eq!(
        f.arena.resolve_actor_handle(source).unwrap(),
        Some(f.source)
    );
    assert_eq!(f.arena.actor_handles().unwrap().cache().handle, source);
    // The registry NULL is a backed empty-slot identity. Calling the actual
    // resolver caches its null result; an early source==NULL shortcut would
    // incorrectly retain the prior source cache.
    assert_eq!(f.apply_source(1, REGISTRY_NULL), Ok(false));
    assert_eq!(
        f.arena.actor_handles().unwrap().cache().handle,
        REGISTRY_NULL
    );
    assert_eq!(f.arena.actor_handles().unwrap().cache().object, None);
    assert_eq!(f.saved.bytes, Some([None; 4]));
    f.arena.actor_handles = None;
    assert_eq!(
        f.apply(1),
        Err(ActorCaptureError::Construction(
            ActorConstructionError::MissingRegistry
        ))
    );
    assert_eq!(f.arena.object(f.actor).unwrap().base.flags, 0x40);
}

#[test]
fn saved_backing_failure_occurs_after_source_resolution_before_capture_writes() {
    let mut f = Fixture::new();
    f.flags(0x43, 0x5670);
    f.saved.bytes = None;
    let source = f.arena.actor(f.source).unwrap().handle;
    assert_eq!(f.apply(1), Err(ActorCaptureError::MissingSavedFlagsStorage));
    assert_eq!(f.arena.actor_handles().unwrap().cache().handle, source);
    let base = &f.arena.object(f.actor).unwrap().base;
    assert_eq!((base.flags, base.dirty_word), (0x43, 0x5670));
    assert_eq!(f.arena.actor(f.actor).unwrap().capture_handle_70, None);
    assert_eq!(f.arena.actor(f.actor).unwrap().capture_flags_d4, None);
}

#[test]
fn release_partial_saved_dword_retains_memberships_flags_and_dirty_prefix() {
    let mut f = Fixture::new();
    f.arena.set_flag0(f.actor, false).unwrap();
    f.arena.set_flag1(f.actor, false).unwrap();
    f.flags(0x0080_00c0, 0x4008);
    f.saved.bytes = Some([Some(3), None, Some(0x80), Some(0)]);
    f.process.process_null_handle = None;
    assert_eq!(
        f.apply(0),
        Err(ActorCaptureError::UninitializedSavedFlags { offset: 1 })
    );
    let base = &f.arena.object(f.actor).unwrap().base;
    assert_eq!((base.flags, base.dirty_word), (0x0080_0043, 0x4002));
    for list in [ObjectList::Flag0Primary, ObjectList::Flag1Primary] {
        assert_eq!(f.arena.list(list).last().unwrap().object, f.actor);
    }
    assert_eq!(f.arena.actor(f.actor).unwrap().capture_handle_70, None);
    assert!(f.host.commands.is_empty());
}

#[test]
fn release_equality_returns_false_after_reappend_without_null_read() {
    let mut f = Fixture::new();
    f.arena.set_flag1(f.actor, false).unwrap();
    f.flags(0xc1, 8);
    f.saved = known(0x41);
    f.process.process_null_handle = None;
    assert_ne!(
        f.arena
            .list(ObjectList::Flag0Primary)
            .last()
            .unwrap()
            .object,
        f.actor
    );
    assert_eq!(f.apply(0), Ok(false));
    assert_eq!(
        f.arena
            .list(ObjectList::Flag0Primary)
            .last()
            .unwrap()
            .object,
        f.actor
    );
    assert_eq!(f.arena.object(f.actor).unwrap().base.flags, 0x41);
    assert_eq!(f.arena.object(f.actor).unwrap().base.dirty_word, 2);
    assert_eq!(f.arena.actor(f.actor).unwrap().capture_handle_70, None);
    // Repeat with bit0 already true and its node no longer at the tail. The
    // actual setter must reappend even when it does not change the flag bit.
    f.arena.set_flag0(f.peer, true).unwrap();
    f.flags(0xc1, 8);
    assert_eq!(f.apply(0), Ok(false));
    assert_eq!(
        f.arena
            .list(ObjectList::Flag0Primary)
            .last()
            .unwrap()
            .object,
        f.actor
    );
}

#[test]
fn release_unequal_subset_still_requires_null_and_preserves_prior_writes() {
    let mut f = Fixture::new();
    f.flags(0x1040_00c0, 0x8008);
    f.saved = known(0x40);
    let old = ActorHandle::from_raw(0xfeed_1234);
    f.arena.actor_mut(f.actor).unwrap().capture_handle_70 = Some(old);
    f.process.process_null_handle = None;
    assert_eq!(f.apply(0), Err(ActorCaptureError::MissingProcessNullHandle));
    let base = &f.arena.object(f.actor).unwrap().base;
    assert_eq!((base.flags, base.dirty_word), (0x1040_0040, 0x8002));
    assert_eq!(f.arena.actor(f.actor).unwrap().capture_handle_70, Some(old));
    assert_eq!(f.arena.actor(f.actor).unwrap().capture_flags_d4, None);
    f.process.process_null_handle = Some(PROCESS_NULL);
    f.flags(0x1040_00c0, 8);
    assert_eq!(f.apply(0), Ok(true));
    assert_eq!(
        f.arena.actor(f.actor).unwrap().capture_handle_70,
        Some(PROCESS_NULL)
    );
    assert_ne!(PROCESS_NULL, REGISTRY_NULL);
}

#[test]
fn unknown_saved_low_byte_stops_before_flags_or_list_changes() {
    let mut f = Fixture::new();
    f.flags(0xc0, 0x8888);
    f.saved.bytes = Some([None, Some(0), Some(0), Some(0)]);
    let before = f.arena.object(f.actor).unwrap().clone();
    let order = f.arena.list(ObjectList::Parking).to_vec();
    assert_eq!(
        f.apply(0),
        Err(ActorCaptureError::UninitializedSavedFlags { offset: 0 })
    );
    assert_eq!(f.arena.object(f.actor).unwrap(), &before);
    assert_eq!(f.arena.list(ObjectList::Parking), order);
}

#[test]
fn mismatched_identity_requires_actual_player_command_and_retains_its_effects() {
    for fail in [false, true] {
        let mut f = Fixture::new();
        f.flags(0x43, 0);
        f.arena.actor_mut(f.source).unwrap().word_6c = 1;
        assert_eq!(f.apply(1), Ok(false));
        assert!(
            f.host.commands.is_empty(),
            "empty player list bypasses command"
        );
        let player = f.arena.add_world_player([0; 6]);
        f.arena.add_world_player([0; 6]);
        f.host.mutate = Some(f.actor);
        f.host.fail = fail;
        let result = f.apply(1);
        if fail {
            assert_eq!(
                result,
                Err(ActorCaptureError::Host {
                    address: 0x46c760,
                    error: "command stopped"
                })
            );
        } else {
            assert_eq!(result, Ok(false));
        }
        assert_eq!(
            f.host.commands,
            vec![ActorCapturePlayerCommand {
                player,
                code: 0x42,
                value: 0
            }]
        );
        assert_eq!(f.arena.actor(f.actor).unwrap().word_6c, 99);
        assert_eq!(f.arena.object(f.actor).unwrap().base.flags, 0x42);
        assert_eq!(f.process.process_null_handle, None);
        assert_eq!(f.host.phase.snapshot(), Some(77));
        assert_eq!(f.saved.bytes, Some([Some(0xab), None, None, None]));
        assert_eq!(f.arena.actor(f.actor).unwrap().capture_handle_70, None);
        assert_eq!(f.arena.actor(f.actor).unwrap().capture_flags_d4, None);
    }
}
