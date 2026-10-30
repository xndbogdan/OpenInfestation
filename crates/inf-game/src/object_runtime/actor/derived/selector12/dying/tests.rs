use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, DerivedActorConstructionRequest, Selector12GrantContext,
};
use std::cell::Cell;

pub(super) fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x0801_0000),
    })
    .unwrap();
    let grants = inf_formats::GrantTable {
        virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let actor = arena
        .allocate_selector12_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([100, 200, 300]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &[0; 4096],
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut 0,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap();
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.word_28 = 400;
    base.flags = 0x64;
    base.dirty_word = 0;
    arena.family428650_actor_mut(actor).unwrap().words_188_19c[0] = Some(0);
    (arena, actor)
}

#[derive(Default)]
struct Host {
    stop: Option<&'static str>,
    remove_position: bool,
    mutate_refresh: bool,
    null: Option<ActorHandle>,
    null_reads: Cell<u32>,
    events: Vec<&'static str>,
    bursts: Vec<ParticleBurst>,
    states: Vec<ActorStateChangeRequest>,
}

impl TargetNullHandleSource for Host {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.null_reads.set(self.null_reads.get() + 1);
        self.null
    }
}

impl Selector12DyingHost for Host {
    type Error = &'static str;

    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push("refresh");
        assert_eq!(arena.actor(actor).unwrap().rotating.visual.model_word, 248);
        assert_eq!(arena.object(actor).unwrap().base.dirty_word & 0x100, 0x100);
        assert_eq!(arena.derived_actor(actor).unwrap().word_16c, None);
        assert_eq!(arena.derived_actor(actor).unwrap().word_170, None);
        if self.mutate_refresh {
            arena.owned_mut(actor).unwrap().state.base.position_words = Some([7, 8, 9]);
            arena.derived_actor_mut(actor).unwrap().word_16c = Some(1);
            arena.derived_actor_mut(actor).unwrap().word_170 = Some(2);
            *rng = 0x8765_4321;
        }
        if self.remove_position {
            arena.owned_mut(actor).unwrap().state.base.position_words = None;
        }
        if self.stop == Some("refresh") {
            Err("refresh")
        } else {
            Ok(())
        }
    }

    fn birth_burst(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        burst: ParticleBurst,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push("burst");
        self.bursts.push(burst);
        assert_eq!(arena.derived_actor(actor).unwrap().word_16c, Some(0x8000));
        assert_eq!(arena.derived_actor(actor).unwrap().word_170, Some(0x8000));
        if self.mutate_refresh {
            assert_eq!(*rng, 0x8765_4321);
        }
        // A failed lower service may have already published its own prefix.
        arena.actor_mut(actor).unwrap().word_74 = 123;
        if self.stop == Some("burst") {
            Err("burst")
        } else {
            Ok(())
        }
    }

    fn change_state(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorStateChangeRequest,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push("state");
        self.states.push(request);
        arena.actor_mut(actor).unwrap().word_c4 = 1;
        if self.stop == Some("state") {
            Err("state")
        } else {
            Ok(())
        }
    }
}

#[test]
fn false_low_byte_bypasses_receiver_rng_and_services() {
    let (mut arena, _) = fixture();
    let actor = ObjectArena::new().allocate(crate::object_runtime::ObjectClassKind::Base);
    let mut host = Host::default();
    let mut rng = 0x1234;
    for entering in [0, 0x100, 0xffff_ff00] {
        arena
            .run_selector12_dying(
                actor,
                Selector12DyingRequest::Change { entering },
                &mut host,
                &mut rng,
            )
            .unwrap();
    }
    assert_eq!(rng, 0x1234);
    assert!(host.events.is_empty());
    assert_eq!(host.null_reads.get(), 0);
}

#[test]
fn refresh_failure_retains_three_draws_model_and_earlier_motion() {
    let (mut arena, actor) = fixture();
    let mut host = Host {
        stop: Some("refresh"),
        ..Host::default()
    };
    let mut rng = 0;
    let mut expected_rng = rng;
    let random = std::array::from_fn::<_, 3, _>(|_| name_entry_rng_next(&mut expected_rng));
    let result = arena.run_selector12_dying(
        actor,
        Selector12DyingRequest::Change { entering: 0x101 },
        &mut host,
        &mut rng,
    );
    assert_eq!(result, Err(Selector12DyingError::Refresh("refresh")));
    assert_eq!(rng, expected_rng);
    assert_eq!(host.events, ["refresh"]);
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([100, 600, 300])
    );
    assert_eq!(arena.object(actor).unwrap().base.flags, 0x24);
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0x102);
    let motion = arena.actor(actor).unwrap();
    assert_eq!(
        motion.words_8e_92,
        [
            (200 - ((random[0].wrapping_mul(400) as i32) >> 16)) as u16,
            400,
            (200 - ((random[1].wrapping_mul(400) as i32) >> 16)) as u16,
        ]
    );
    assert_eq!(motion.words_94_9c[1], 0x8000u32.wrapping_sub(random[2]));
    assert_eq!(arena.derived_actor(actor).unwrap().word_16c, None);
    assert_eq!(arena.derived_actor(actor).unwrap().word_170, None);
}

#[test]
fn burst_observes_fresh_refresh_position_rng_and_published_scales() {
    let (mut arena, actor) = fixture();
    let mut host = Host {
        stop: Some("burst"),
        mutate_refresh: true,
        ..Host::default()
    };
    let mut rng = 0;
    assert_eq!(
        arena.run_selector12_dying(
            actor,
            Selector12DyingRequest::Change { entering: 1 },
            &mut host,
            &mut rng
        ),
        Err(Selector12DyingError::Burst("burst"))
    );
    assert_eq!(host.events, ["refresh", "burst"]);
    assert_eq!(
        host.bursts,
        [ParticleBurst {
            debris_count: 16,
            position: [7, 8, 9],
            placement_radius: 20,
            central_type: 4,
            debris_types: [5, 5],
            speed: 8,
            flags: 0,
        }]
    );
    assert_eq!(rng, 0x8765_4321);
    assert_eq!(arena.actor(actor).unwrap().word_74, 123);
    assert_eq!(arena.derived_actor(actor).unwrap().word_16c, Some(0x8000));
}

#[test]
fn missing_postrefresh_position_stops_before_scale_and_burst_writes() {
    let (mut arena, actor) = fixture();
    let mut host = Host {
        remove_position: true,
        ..Host::default()
    };
    assert!(matches!(
        arena.run_selector12_dying(
            actor,
            Selector12DyingRequest::Change { entering: 1 },
            &mut host,
            &mut 0
        ),
        Err(Selector12DyingError::RequiredField {
            offset: 0x32,
            address: 0x0046_7951,
            ..
        })
    ));
    assert_eq!(host.events, ["refresh"]);
    assert_eq!(arena.derived_actor(actor).unwrap().word_16c, None);
}

#[test]
fn update_reads_null_only_after_signed_timer_and_flag_gates() {
    for (elapsed, flags) in [
        (0, 0),
        (10_000_000, 0),
        (0xffff_ffff, 0),
        (0x8000_0000, 0),
        (10_000_001, 0x0200_0000),
    ] {
        let (mut arena, actor) = fixture();
        arena.family428650_actor_mut(actor).unwrap().words_188_19c[0] = Some(elapsed);
        arena.owned_mut(actor).unwrap().state.base.flags = flags;
        let mut host = Host::default();
        let mut rng = 7;
        let before = arena.object(actor).unwrap().clone();
        arena
            .run_selector12_dying(actor, Selector12DyingRequest::Update, &mut host, &mut rng)
            .unwrap();
        assert_eq!(arena.object(actor).unwrap(), &before);
        assert_eq!(host.null_reads.get(), 0);
        assert!(host.events.is_empty());
        assert_eq!(rng, 7);
    }
    let (mut arena, actor) = fixture();
    arena.family428650_actor_mut(actor).unwrap().words_188_19c[0] = Some(10_000_001);
    let mut host = Host::default();
    assert_eq!(
        arena.run_selector12_dying(actor, Selector12DyingRequest::Update, &mut host, &mut 0),
        Err(Selector12DyingError::MissingProcessNull)
    );
    assert_eq!(host.null_reads.get(), 1);
    assert!(host.events.is_empty());
    host.null = Some(ActorHandle::from_raw(0x7654_3210));
    host.stop = Some("state");
    assert_eq!(
        arena.run_selector12_dying(actor, Selector12DyingRequest::Update, &mut host, &mut 0),
        Err(Selector12DyingError::StateChange("state"))
    );
    assert_eq!(
        host.states,
        [ActorStateChangeRequest {
            target_state: 0,
            argument: 0,
            source: host.null.unwrap()
        }]
    );
    assert_eq!(arena.actor(actor).unwrap().word_c4, 1);
}
