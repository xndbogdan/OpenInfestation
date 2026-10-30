use std::{cell::Cell, sync::Arc};

use inf_formats::{
    parse_constructor_asset_bank, ActorConstructorPayload, ByteSpan, ConstructorPayloadView,
    HandlerFraming, HandlerId, HandlerRegion, RelocationTail,
};

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
};

const NULL: ActorHandle = ActorHandle::from_raw(0);
const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn payload(selector: i32) -> Arc<ActorConstructorPayload> {
    // Supply the actual H13 selector field through the canonical payload
    // parser and initialization owner, without a private actor-field setter.
    let mut source = vec![0; 4 + 0x42];
    source[..2].copy_from_slice(&2_u16.to_le_bytes());
    source[4 + 0x0c..4 + 0x10].copy_from_slice(&selector.to_le_bytes());
    let main_payload = ByteSpan::new(0, source.len());
    source.extend_from_slice(&0_u32.to_le_bytes());
    let bank = parse_constructor_asset_bank(
        &source,
        &HandlerRegion {
            handler: HandlerId::new(13).unwrap(),
            requested_count: 1,
            span: ByteSpan::new(0, source.len()),
            framing: HandlerFraming::RelocationTail(RelocationTail {
                main_payload,
                relocation_count: 0,
                relocation_offsets: ByteSpan::new(main_payload.end, main_payload.end),
                entry_offsets: ByteSpan::new(main_payload.end, source.len()),
            }),
        },
    )
    .unwrap();
    let ConstructorPayloadView::SharedActor(payload) = &bank.assets[0].payload else {
        panic!("canonical shared actor payload")
    };
    Arc::new(payload.clone())
}

fn actor(selector: i32) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: NULL,
    })
    .unwrap();
    let actor = arena
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
                sine_quarter: &UNUSED_SINE,
            },
        })
        .unwrap();
    arena
        .initialize_actor_payload(actor, &payload(selector), 1)
        .unwrap();
    (arena, actor)
}

#[derive(Default)]
struct Effect {
    selector: Option<i32>,
    source: Option<(TaskOwner, ActorHandle)>,
    replace: Option<TaskOwner>,
}

struct Host {
    network: Option<u32>,
    network_reads: Cell<usize>,
    component: Result<Option<u32>, &'static str>,
    presence: Result<u32, &'static str>,
    events: Vec<&'static str>,
    captured_actor: Option<ObjectId>,
    expected_component: u32,
    component_effect: Effect,
    presence_effect: Effect,
    replacement: Option<TaskOwner>,
}

impl Host {
    fn new(network: Option<u32>) -> Self {
        Self {
            network,
            network_reads: Cell::new(0),
            component: Err("unexpected +5C query"),
            presence: Err("unexpected +7C query"),
            events: Vec::new(),
            captured_actor: None,
            expected_component: 0x1234_5678,
            component_effect: Effect::default(),
            presence_effect: Effect::default(),
            replacement: None,
        }
    }

    fn apply(&mut self, effect: Effect, context: TaskRelationContext<'_>, actor: ObjectId) {
        if let Some(selector) = effect.selector {
            context
                .arena
                .initialize_actor_payload(actor, &payload(selector), 1)
                .unwrap();
        }
        if let Some((owner, source)) = effect.source {
            context.tasks.write_source(owner, source).unwrap();
        }
        if let Some(owner) = effect.replace {
            context.tasks.destroy(owner).unwrap();
            self.replacement = Some(context.tasks.construct(18, NULL).unwrap());
        }
    }
}

impl TaskRelationHost for Host {
    type Error = &'static str;
    type Component = u32;

    fn network_flag(&self) -> Option<u32> {
        self.network_reads.set(self.network_reads.get() + 1);
        self.network
    }

    fn actor_component_5c(
        &mut self,
        context: TaskRelationContext<'_>,
        actor: ObjectId,
    ) -> Result<Option<Self::Component>, Self::Error> {
        self.events.push("5C");
        self.captured_actor = Some(actor);
        let effect = std::mem::take(&mut self.component_effect);
        self.apply(effect, context, actor);
        self.component
    }

    fn component_presence_7c(
        &mut self,
        context: TaskRelationContext<'_>,
        component: Self::Component,
    ) -> Result<u32, Self::Error> {
        self.events.push("7C");
        assert_eq!(component, self.expected_component);
        let effect = std::mem::take(&mut self.presence_effect);
        self.apply(effect, context, self.captured_actor.unwrap());
        self.presence
    }
}

#[test]
fn constant_callbacks_bypass_stale_actor_and_missing_network() {
    let (mut arena, actor) = actor(1);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let mut tasks = ProcessTaskRuntime::default();
    for (classification, callback) in [(0, TaskCallback::Zero4027b0), (1, TaskCallback::Zero4efbf0)]
    {
        let owner = tasks.construct(classification, NULL).unwrap();
        assert_eq!(tasks.record(owner).unwrap().callback, callback);
        let before = *tasks.record(owner).unwrap();
        let mut host = Host::new(None);
        assert_eq!(
            tasks.query_relation(TaskRelationRequest { owner, actor }, &mut arena, &mut host),
            Ok(0)
        );
        assert_eq!(tasks.record(owner).unwrap(), &before);
        assert_eq!(host.network_reads.get(), 0);
        assert!(host.events.is_empty());
    }
}

#[test]
fn captured_callback_still_requires_a_live_dispatch_owner() {
    let (mut arena, actor) = actor(0);
    let mut tasks = ProcessTaskRuntime::default();
    let owner = tasks.construct(0, NULL).unwrap();
    tasks.destroy(owner).unwrap();
    let mut host = Host::new(None);
    assert_eq!(
        tasks.query_relation(TaskRelationRequest { owner, actor }, &mut arena, &mut host),
        Err(TaskRelationError::Storage(TaskStorageError::MissingOwner(
            owner
        )))
    );
    assert_eq!(host.network_reads.get(), 0);
    assert!(host.events.is_empty());
}

#[test]
fn team_class_eighteen_alone_bypasses_the_required_network_load() {
    let mut tasks = ProcessTaskRuntime::default();
    let owner = tasks.construct(3, NULL).unwrap();
    for (selector, expected) in [
        (2, Ok(2)),
        (
            0,
            Err(TaskRelationError::MissingNetworkFlag {
                address: 0x0048_2cf7,
            }),
        ),
        (
            1,
            Err(TaskRelationError::MissingNetworkFlag {
                address: 0x0048_2cf7,
            }),
        ),
        (
            5,
            Err(TaskRelationError::MissingNetworkFlag {
                address: 0x0048_2cf7,
            }),
        ),
    ] {
        let (mut arena, actor) = actor(selector);
        let mut host = Host::new(None);
        assert_eq!(
            tasks.query_relation(TaskRelationRequest { owner, actor }, &mut arena, &mut host),
            expected
        );
        assert_eq!(host.network_reads.get(), usize::from(selector != 2));
        assert!(host.events.is_empty());
    }
}

#[test]
fn team_uses_full_network_dword_and_exact_captured_task_classification() {
    let mut tasks = ProcessTaskRuntime::default();
    let owner = tasks.construct(3, NULL).unwrap();
    let before = *tasks.record(owner).unwrap();
    for network in [0, 1, 0x8000_0000] {
        // Selector -> actual +B4 after shared payload initialization.
        for (selector, classification) in [
            (0, 0),
            (1, 1),
            (3, 2),
            (5, 3),
            (19, 17),
            (2, 18),
            (-1, 0xffff_fffd),
        ] {
            let (mut arena, actor) = actor(selector);
            assert_eq!(arena.actor(actor).unwrap().word_b4, classification);
            let mut host = Host::new(Some(network));
            let expected = u32::from(
                classification == 18
                    || (network != 0
                        && classification != 0
                        && classification != 1
                        && classification != 3),
            ) * 2;
            assert_eq!(
                tasks.query_relation(TaskRelationRequest { owner, actor }, &mut arena, &mut host),
                Ok(expected),
                "classification {classification:08X}, network {network:08X}"
            );
            assert_eq!(host.network_reads.get(), usize::from(classification != 18));
            assert!(host.events.is_empty());
            assert_eq!(tasks.record(owner).unwrap(), &before);
        }
    }
}

#[test]
fn component_classification_bypasses_queries_outside_class_one() {
    let mut tasks = ProcessTaskRuntime::default();
    let owner = tasks.construct(18, NULL).unwrap();
    for (selector, expected) in [(0, 0), (2, 0), (3, 2), (5, 2), (19, 2), (-1, 2)] {
        let (mut arena, actor) = actor(selector);
        let mut host = Host::new(None);
        assert_eq!(
            tasks.query_relation(TaskRelationRequest { owner, actor }, &mut arena, &mut host),
            Ok(expected)
        );
        assert_eq!(host.network_reads.get(), 0);
        assert!(host.events.is_empty());
    }
}

#[test]
fn component_null_and_full_dword_presence_have_distinct_reached_calls() {
    let mut tasks = ProcessTaskRuntime::default();
    let owner = tasks.construct(18, NULL).unwrap();
    for (component, presence, expected, events) in [
        (None, 0, 1, vec!["5C"]),
        (Some(0x1234_5678), 0, 0, vec!["5C", "7C"]),
        (Some(0x1234_5678), 0x100, 1, vec!["5C", "7C"]),
        (Some(0x1234_5678), 0xffff_ffff, 1, vec!["5C", "7C"]),
    ] {
        let (mut arena, actor) = actor(1);
        let mut host = Host::new(None);
        host.component = Ok(component);
        host.presence = Ok(presence);
        assert_eq!(
            tasks.query_relation(TaskRelationRequest { owner, actor }, &mut arena, &mut host),
            Ok(expected)
        );
        assert_eq!(host.events, events);
        assert_eq!(host.network_reads.get(), 0);
    }
}

#[test]
fn mutable_component_callbacks_keep_captured_class_and_task_custody() {
    let (mut arena, actor) = actor(1);
    let mut tasks = ProcessTaskRuntime::default();
    let owner = tasks.construct(18, NULL).unwrap();
    let mut host = Host::new(None);
    host.component = Ok(Some(host.expected_component));
    host.presence = Ok(0x100);
    host.component_effect = Effect {
        selector: Some(2),
        replace: Some(owner),
        ..Effect::default()
    };
    assert_eq!(
        tasks.query_relation(TaskRelationRequest { owner, actor }, &mut arena, &mut host),
        Ok(1)
    );
    assert_eq!(arena.actor(actor).unwrap().word_b4, 18);
    assert_eq!(
        tasks.record(owner),
        Err(TaskStorageError::MissingOwner(owner))
    );
    assert_eq!(tasks.owner(18).unwrap(), host.replacement);
    assert_eq!(host.events, ["5C", "7C"]);
    assert_eq!(host.network_reads.get(), 0);
}

#[test]
fn host_failures_preserve_reached_mutations_and_skip_later_queries() {
    for failed_phase in ["5C", "7C"] {
        let (mut arena, actor) = actor(1);
        let mut tasks = ProcessTaskRuntime::default();
        let owner = tasks.construct(18, NULL).unwrap();
        let source_owner = tasks.construct(3, NULL).unwrap();
        let source = ActorHandle::from_raw(0x1000_0001);
        let mut host = Host::new(None);
        let effect = Effect {
            selector: Some(2),
            source: Some((source_owner, source)),
            replace: Some(owner),
        };
        let address = if failed_phase == "5C" {
            host.component_effect = effect;
            host.component = Err("stopped +5C");
            0x0048_2caa
        } else {
            host.component = Ok(Some(host.expected_component));
            host.presence_effect = effect;
            host.presence = Err("stopped +7C");
            0x0048_2cb5
        };
        assert_eq!(
            tasks.query_relation(TaskRelationRequest { owner, actor }, &mut arena, &mut host),
            Err(TaskRelationError::Host {
                address,
                error: if failed_phase == "5C" {
                    "stopped +5C"
                } else {
                    "stopped +7C"
                }
            })
        );
        assert_eq!(arena.actor(actor).unwrap().word_b4, 18);
        assert_eq!(tasks.record(source_owner).unwrap().source, source);
        assert_eq!(
            tasks.record(owner),
            Err(TaskStorageError::MissingOwner(owner))
        );
        assert_eq!(tasks.owner(18).unwrap(), host.replacement);
        assert_eq!(
            host.events,
            if failed_phase == "5C" {
                vec!["5C"]
            } else {
                vec!["5C", "7C"]
            }
        );
        assert_eq!(host.network_reads.get(), 0);
    }
}
