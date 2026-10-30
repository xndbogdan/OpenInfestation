use super::*;
use crate::object_runtime::{ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs};

const SOURCE: ActorHandle = ActorHandle::from_raw(0x0800_0007);
const NULL: ActorHandle = ActorHandle::from_raw(0xffff_ffff);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Query(ActorStateChangeQuery, u32, u16),
    Attribute(ObjectId, ActorHandle),
    Effect(ActorStateChangeEffect<u32>, u32, u16, u32),
}

#[derive(Default)]
struct Host {
    events: Vec<Event>,
    actor: Option<ObjectId>,
    exclude: bool,
    derived: bool,
    attribution_b4: Option<u32>,
    one_state: Option<u32>,
    fail_derived: bool,
    fail_one: bool,
}

impl ActorStateChangeHost for Host {
    type Error = &'static str;
    type AttributionOwner = u32;

    fn query(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, Self::Error> {
        let state = arena.actor(actor).unwrap();
        self.events
            .push(Event::Query(query, state.word_c4, state.health));
        match query {
            ActorStateChangeQuery::ExcludesAttribution68 => Ok(self.exclude),
            ActorStateChangeQuery::HasDerived58 if self.fail_derived => Err("derived query"),
            ActorStateChangeQuery::HasDerived58 => Ok(self.derived),
        }
    }

    fn attribute(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        source: ActorHandle,
    ) -> Result<u32, Self::Error> {
        self.events.push(Event::Attribute(actor, source));
        if let Some(selector) = self.attribution_b4 {
            arena.actor_mut(actor).unwrap().word_b4 = selector;
        }
        Ok(42)
    }

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        effect: ActorStateChangeEffect<u32>,
    ) -> Result<(), Self::Error> {
        let actor = self.actor.unwrap();
        let state = arena.actor(actor).unwrap();
        self.events.push(Event::Effect(
            effect,
            state.word_c4,
            state.health,
            arena.object(actor).unwrap().base.flags,
        ));
        if matches!(effect, ActorStateChangeEffect::StateOne { .. }) {
            if self.fail_one {
                return Err("state one");
            }
            if let Some(state) = self.one_state {
                arena.actor_mut(actor).unwrap().word_c4 = state;
            }
        }
        Ok(())
    }
}

fn fixture() -> (ObjectArena, ObjectId, Host) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: NULL,
    })
    .unwrap();
    let actor = arena
        .allocate_actor(ActorConstructionRequest {
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
        })
        .unwrap();
    arena.actor_mut(actor).unwrap().health = 500;
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.flags = 0;
    base.dirty_word = 0;
    let host = Host {
        actor: Some(actor),
        ..Host::default()
    };
    (arena, actor, host)
}

fn request(target_state: i32) -> ActorStateChangeRequest {
    ActorStateChangeRequest {
        target_state,
        argument: 0x1234,
        source: SOURCE,
    }
}

#[test]
fn descending_state_publishes_each_write_before_ordered_callbacks() {
    let (mut arena, actor, mut host) = fixture();
    arena.actor_mut(actor).unwrap().word_b4 = 18;
    arena.actor_mut(actor).unwrap().byte_78 = 0xfe;
    host.derived = true;
    arena
        .change_actor_state(actor, request(0), &mut host)
        .unwrap();
    assert_eq!(
        host.events,
        vec![
            Event::Query(ActorStateChangeQuery::ExcludesAttribution68, 2, 500),
            Event::Attribute(actor, SOURCE),
            Event::Effect(
                ActorStateChangeEffect::IncrementSelector18Statistic { owner: 42 },
                2,
                500,
                0
            ),
            Event::Query(ActorStateChangeQuery::HasDerived58, 2, 500),
            Event::Effect(
                ActorStateChangeEffect::RecordPlacementDeath { index: -2 },
                2,
                500,
                0
            ),
            Event::Effect(
                ActorStateChangeEffect::StateOne {
                    actor,
                    source: SOURCE
                },
                1,
                0,
                0x8000
            ),
            Event::Effect(
                ActorStateChangeEffect::StateZero {
                    actor,
                    source: SOURCE,
                    argument: 0x1234
                },
                0,
                0,
                0x8000
            ),
        ]
    );
    assert_eq!(arena.actor(actor).unwrap().word_c4, 0);
    // 0x8000 is outside the low-nine-bit dirty mask.
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0);
}

#[test]
fn statistic_uses_captured_owner_and_selector_after_attribution() {
    let (mut arena, actor, mut host) = fixture();
    host.attribution_b4 = Some(18);
    arena
        .change_actor_state(actor, request(2), &mut host)
        .unwrap();
    assert!(host.events.iter().any(|event| matches!(
        event,
        Event::Effect(
            ActorStateChangeEffect::IncrementSelector18Statistic { owner: 42 },
            ..
        )
    )));
    host.events.clear();
    arena.actor_mut(actor).unwrap().word_c4 = 3;
    host.attribution_b4 = Some(0);
    arena
        .change_actor_state(actor, request(2), &mut host)
        .unwrap();
    assert!(!host.events.iter().any(|event| matches!(
        event,
        Event::Effect(
            ActorStateChangeEffect::IncrementSelector18Statistic { .. },
            ..
        )
    )));
}

#[test]
fn exclusion_skips_attribution_but_keeps_derived_query() {
    let (mut arena, actor, mut host) = fixture();
    arena.actor_mut(actor).unwrap().word_b4 = 18;
    host.exclude = true;
    arena
        .change_actor_state(actor, request(2), &mut host)
        .unwrap();
    assert_eq!(
        host.events,
        vec![
            Event::Query(ActorStateChangeQuery::ExcludesAttribution68, 2, 500),
            Event::Query(ActorStateChangeQuery::HasDerived58, 2, 500),
        ]
    );
}

#[test]
fn failing_derived_query_retains_state_and_attribution_before_flag_write() {
    let (mut arena, actor, mut host) = fixture();
    host.fail_derived = true;
    assert_eq!(
        arena.change_actor_state(actor, request(0), &mut host),
        Err(ActorStateChangeError::Host("derived query"))
    );
    assert_eq!(arena.actor(actor).unwrap().word_c4, 2);
    assert_eq!(arena.actor(actor).unwrap().health, 500);
    assert_eq!(arena.object(actor).unwrap().base.flags, 0);
    assert!(host.events.contains(&Event::Attribute(actor, SOURCE)));
}

#[test]
fn failing_state_one_keeps_published_zero_health() {
    let (mut arena, actor, mut host) = fixture();
    arena.actor_mut(actor).unwrap().word_c4 = 2;
    host.fail_one = true;
    assert_eq!(
        arena.retire_actor(actor, Some(NULL), &mut host),
        Err(ActorStateChangeError::Host("state one"))
    );
    assert_eq!(arena.actor(actor).unwrap().word_c4, 1);
    assert_eq!(arena.actor(actor).unwrap().health, 0);
    assert_eq!(
        host.events,
        vec![Event::Effect(
            ActorStateChangeEffect::StateOne {
                actor,
                source: NULL
            },
            1,
            0,
            0
        )]
    );
}

#[test]
fn downward_loop_rereads_state_written_by_callback() {
    let (mut arena, actor, mut host) = fixture();
    arena.actor_mut(actor).unwrap().word_c4 = 2;
    host.one_state = Some(0);
    arena
        .change_actor_state(actor, request(0), &mut host)
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().word_c4, 0);
    assert_eq!(host.events.len(), 1); // State-zero callback was bypassed.
}

#[test]
fn ascending_states_use_signed_comparison_and_only_revive_zero_health() {
    let (mut arena, actor, mut host) = fixture();
    arena.actor_mut(actor).unwrap().word_c4 = (-2_i32) as u32;
    arena.actor_mut(actor).unwrap().health = 0x8000;
    arena.owned_mut(actor).unwrap().state.base.flags = 0x8000;
    arena
        .change_actor_state(actor, request(3), &mut host)
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().word_c4, 3);
    assert_eq!(arena.actor(actor).unwrap().health, 0x8000);
    assert_eq!(arena.object(actor).unwrap().base.flags, 0);
    assert!(host.events.is_empty());
    arena.actor_mut(actor).unwrap().word_c4 = 1;
    arena.actor_mut(actor).unwrap().health = 0;
    arena
        .change_actor_state(actor, request(2), &mut host)
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().health, 1);
}

#[test]
fn retirement_guard_accepts_only_states_two_and_three() {
    for state in [0, 1, 2, 3, 4, u32::MAX] {
        let (mut arena, actor, mut host) = fixture();
        arena.actor_mut(actor).unwrap().word_c4 = state;
        assert_eq!(
            arena.retire_actor(actor, Some(NULL), &mut host).unwrap(),
            matches!(state, 2 | 3)
        );
        assert_eq!(
            arena.actor(actor).unwrap().word_c4,
            if matches!(state, 2 | 3) { 1 } else { state }
        );
        assert!(!host.events.iter().any(|event| matches!(
            event,
            Event::Effect(ActorStateChangeEffect::StateZero { .. }, ..)
        )));
    }
}

#[test]
fn retirement_reads_explicit_process_null_after_state_guard() {
    let (mut arena, actor, mut host) = fixture();
    let source = ActorHandle::from_raw(0x1234_5678);
    assert_ne!(source, arena.actor_handles().unwrap().null_handle());
    arena.actor_mut(actor).unwrap().word_c4 = 2;
    arena.retire_actor(actor, Some(source), &mut host).unwrap();
    assert!(host.events.iter().any(|event| matches!(event,
        Event::Effect(ActorStateChangeEffect::StateOne { source: reached, .. }, ..)
        if *reached == source
    )));

    for state in [0, 1, 4, u32::MAX] {
        arena.actor_mut(actor).unwrap().word_c4 = state;
        host.events.clear();
        assert!(!arena.retire_actor(actor, None, &mut host).unwrap());
        assert!(host.events.is_empty());
        assert_eq!(arena.actor(actor).unwrap().word_c4, state);
    }
    arena.actor_mut(actor).unwrap().word_c4 = 3;
    let health = arena.actor(actor).unwrap().health;
    assert_eq!(
        arena.retire_actor(actor, None, &mut host),
        Err(ActorStateChangeError::MissingProcessNullHandle(actor))
    );
    assert!(host.events.is_empty());
    assert_eq!(arena.actor(actor).unwrap().word_c4, 3);
    assert_eq!(arena.actor(actor).unwrap().health, health);
}
