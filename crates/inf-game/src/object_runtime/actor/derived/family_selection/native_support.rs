//! Shared supplied state, live lower services and snapshots for original-byte
//! family selection receipts. Native +100/+104 traversal, operation/result
//! admission and stopped-boundary expectations remain in their own comparers.
//! Public pool state is compared; private free-stack order stays with the
//! independently owned particle-factory receipts.

use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::Arc};

use inf_formats::{
    parse_constructor_asset_bank, ActorConstructorPayload, ByteSpan, ConstructorPayloadView,
    HandlerFraming, HandlerId, HandlerRegion, RelocationTail,
};
use serde_json::{json, Value};

use super::*;
use crate::{
    object_runtime::{
        ActorBehaviorState, ActorBehaviorTable, ActorConstructionRequest, ActorDamageTable,
        ActorFollowTarget, ActorRegistryConfig, ActorServiceResourceKind,
        ActorServiceResourceState, ActorTerrainInputs, BehaviorActorResourceRequest,
        BehaviorComponentPacketSourceRequest, BehaviorComponentQuery, BehaviorPacketCopy,
        BehaviorParticleCreationSources, BehaviorParticleEffectRequest,
        BehaviorParticleServiceContext, DerivedActorConstructionRequest, FamilyHelper482ff0,
        FamilyHelperHistoryRecord, FixedPositionTarget, FixtureActorServiceAllocation,
        NativeTarget, NativeTargetClass, ObjectPositionTarget, Selector12GrantContext,
    },
    process_particles::{
        factories::{
            ParticleCreationNotice, ParticleFactoryContext, ParticleFactoryHost,
            ParticleFactoryImpactRequest, ParticleFactoryKind, ParticleFactoryRequest,
            ParticleFactorySources, ParticleVelocitySource,
        },
        ParticleId, ParticleRecord, ParticleVectorBytes, ProcessParticleRuntime,
    },
};

pub(in crate::object_runtime::actor::derived) const NAMES: [&str; 4] =
    ["self", "reference", "a", "b"];
pub(in crate::object_runtime::actor::derived) const ADDRESSES: [u32; 4] =
    [0x740000, 0x741000, 0x742000, 0x743000];

pub(in crate::object_runtime::actor::derived) fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

pub(in crate::object_runtime::actor::derived) fn bytes(value: &Value) -> Vec<Option<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().map(|v| v.try_into().unwrap()))
        .collect()
}

pub(in crate::object_runtime::actor::derived) fn dword(input: &[Option<u8>]) -> Option<u32> {
    Some(u32::from_le_bytes(
        input
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .unwrap(),
    ))
}

pub(in crate::object_runtime::actor::derived) fn word(input: &[Option<u8>]) -> Option<u16> {
    Some(u16::from_le_bytes(
        input
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .unwrap(),
    ))
}

pub(in crate::object_runtime::actor::derived) fn words(input: &[Option<u8>]) -> Option<[u16; 3]> {
    Some([word(&input[..2])?, word(&input[2..4])?, word(&input[4..6])?])
}

pub(in crate::object_runtime::actor::derived) fn xyz(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| number(&value[axis]) as u16)
}

pub(in crate::object_runtime::actor::derived) fn slots(value: &Value) -> Vec<u16> {
    let mut result = Vec::new();
    for run in value.as_array().unwrap() {
        let (mut current, last, stride) = (
            run[0].as_i64().unwrap(),
            run[1].as_i64().unwrap(),
            run[2].as_i64().unwrap(),
        );
        loop {
            result.push(current.try_into().unwrap());
            if current == last {
                break;
            }
            assert_ne!(stride, 0);
            current += stride;
        }
    }
    result
}

pub(in crate::object_runtime::actor::derived) fn payload(
    responses: &[Option<u8>],
) -> Arc<ActorConstructorPayload> {
    // Supply the canonical H13 ten-pair backing; no extra damage-table mode.
    let mut source = vec![0xa5; 9];
    source.extend_from_slice(&55_u16.to_le_bytes());
    source.extend_from_slice(&u16::MAX.to_le_bytes());
    let mut data = [0; 0x42];
    for (offset, value) in responses.iter().enumerate() {
        data[0x10 + offset] = value.expect("complete canonical response admission");
    }
    source.extend_from_slice(&data);
    let main = ByteSpan::new(9, source.len());
    let empty = ByteSpan::new(source.len(), source.len());
    source.extend_from_slice(&0_u32.to_le_bytes());
    let bank = parse_constructor_asset_bank(
        &source,
        &HandlerRegion {
            handler: HandlerId::new(13).unwrap(),
            requested_count: 1,
            span: ByteSpan::new(9, source.len()),
            framing: HandlerFraming::RelocationTail(RelocationTail {
                main_payload: main,
                relocation_count: 0,
                relocation_offsets: empty,
                entry_offsets: ByteSpan::new(main.end, source.len()),
            }),
        },
    )
    .unwrap();
    let ConstructorPayloadView::SharedActor(value) = &bank.assets[0].payload else {
        panic!("canonical shared actor payload")
    };
    Arc::new(value.clone())
}

pub(in crate::object_runtime::actor::derived) fn behavior() -> ActorBehaviorState {
    // Supplied native backing, not a claim that a constructor zeros its heap.
    ActorBehaviorState {
        fire_head_00: None,
        firing_stop_00: None,
        sound_28: None,
        word_04: 0,
        word_08: 0,
        byte_0c: 0,
        byte_0d: 0,
        byte_0e: 0,
        byte_0f: 0,
        byte_10: 0,
        byte_11: 0,
        table_14: ActorBehaviorTable::Mode01,
        entries_18: vec![],
        groups_1c: vec![],
        word_20: 0,
        words_24: vec![],
        word_2c: 0,
        word_30: None,
        word_34: None,
        word_38: 0,
    }
}

pub(in crate::object_runtime::actor::derived) fn supplied_target(
    value: &Value,
    b: ObjectId,
) -> Option<NativeTarget> {
    if value.is_null() {
        return None;
    }
    let handle_04 = ActorHandle::from_raw(number(&value["handle"]));
    Some(
        match value
            .get("class_name")
            .unwrap_or(&value["class"])
            .as_str()
            .unwrap()
        {
            "follow" => NativeTarget::ActorFollow(ActorFollowTarget {
                handle_04,
                focus_08: Some(value.get("point_words").map(xyz).unwrap_or([11, 12, 13])),
                word_10: 2,
                focus_14: Some(value.get("follow_words").map(xyz).unwrap_or([1, 2, 3])),
                velocity_1a: Some([0; 3]),
            }),
            "fixed" => NativeTarget::FixedPosition(FixedPositionTarget {
                handle_04,
                focus_08: Some(value.get("point_words").map(xyz).unwrap_or([11, 12, 13])),
                word_10: 8,
            }),
            "object" => NativeTarget::ObjectPosition(ObjectPositionTarget {
                handle_04,
                focus_08: value.get("point_words").map(xyz).unwrap_or([11, 12, 13]),
                word_10: 16,
                object_14: b,
            }),
            other => panic!("unadmitted target class {other}"),
        },
    )
}

/// Explicit receipt classes and target fields, independent of either traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::object_runtime::actor::derived) enum FixtureLayout {
    InlineTable100,
    Selector12Inline100,
    ActorList104,
    Selector12ActorList104,
}

impl FixtureLayout {
    fn is_actor_list(self) -> bool {
        matches!(self, Self::ActorList104 | Self::Selector12ActorList104)
    }
}

pub(in crate::object_runtime::actor::derived) fn supplied_animation_clock(
    ticks: &Value,
) -> Option<crate::frontend_camera::AnimationClock> {
    let ticks = ticks.as_u64()?;
    let ticks: u32 = ticks.try_into().unwrap();
    let mut clock = crate::frontend_camera::AnimationClock::default();
    // Fixture preparation through the real public owner, not a private
    // scalar seed. No native 00439560 execution is claimed by this setup.
    let mut remaining = u64::from(ticks) * 20_000;
    while remaining != 0 {
        let delta = remaining.min(u64::from(u32::MAX)) as u32;
        clock.advance(delta);
        remaining -= u64::from(delta);
    }
    assert_eq!(clock.value(), ticks);
    Some(clock)
}

mod actors;

use actors::supply_actor_fields;
pub(in crate::object_runtime::actor::derived) use actors::{
    construct_supplied_actor, supplied_arena,
};

#[derive(Default)]
pub(in crate::object_runtime::actor::derived) struct ServiceOrder {
    pub(in crate::object_runtime::actor::derived) events: VecDeque<Value>,
}

impl ServiceOrder {
    fn next(&mut self, operation: &str) -> Value {
        let event = self.events.pop_front().expect("unexpected lower service");
        assert_eq!(event["operation"], operation, "lower service order");
        event
    }
}

pub(in crate::object_runtime::actor::derived) struct Services {
    pub(in crate::object_runtime::actor::derived) phase:
        crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
    pub(in crate::object_runtime::actor::derived) ids: [Option<ObjectId>; 4],
    pub(in crate::object_runtime::actor::derived) tasks: ProcessTaskRuntime,
    pub(in crate::object_runtime::actor::derived) animation_clock:
        Option<crate::frontend_camera::AnimationClock>,
    pub(in crate::object_runtime::actor::derived) invocation: Value,
    pub(in crate::object_runtime::actor::derived) order: Rc<RefCell<ServiceOrder>>,
    pub(in crate::object_runtime::actor::derived) network_index: usize,
    pub(in crate::object_runtime::actor::derived) impact_index: usize,
    pub(in crate::object_runtime::actor::derived) scratch: Option<[Option<u8>; 8]>,
    pub(in crate::object_runtime::actor::derived) layout: FixtureLayout,
    pub(in crate::object_runtime::actor::derived) input: Value,
    pub(in crate::object_runtime::actor::derived) retired_tasks:
        Vec<crate::process_tasks::TaskOwner>,
}

impl FamilySelectionServices for Services {
    fn process_tasks(&mut self) -> &mut ProcessTaskRuntime {
        &mut self.tasks
    }

    fn animation_clock(&self) -> Option<&dyn crate::frontend_camera::AnimationClockSource> {
        self.animation_clock
            .as_ref()
            .map(|clock| clock as &dyn crate::frontend_camera::AnimationClockSource)
    }
}

impl Services {
    pub(in crate::object_runtime::actor::derived) fn apply(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        pool: &mut ProcessParticleRuntime,
        sources: Option<&mut BehaviorParticleCreationSources<'_>>,
        patch: &Value,
    ) {
        if let Some(operation) = patch["operation"].as_str() {
            assert!(self.layout.is_actor_list());
            match operation {
                "append_actor" => {
                    let index = NAMES
                        .iter()
                        .position(|v| Some(*v) == patch["actor"].as_str())
                        .unwrap();
                    assert!(self.ids[index].is_none());
                    let supplied = &self.input["actors"][NAMES[index]];
                    let id = construct_supplied_actor(
                        arena,
                        supplied,
                        &mut number(&patch["allocation_rng_seed"]),
                    );
                    let response_bytes: Vec<_> = self.input["responses"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|pair| {
                            pair.as_array()
                                .unwrap()
                                .iter()
                                .flat_map(|v| (number(v) as u16).to_le_bytes().map(Some))
                        })
                        .collect();
                    supply_actor_fields(
                        arena,
                        id,
                        supplied,
                        &payload(&response_bytes),
                        self.layout,
                    );
                    self.ids[index] = Some(id);
                }
                "retire_actor" => {
                    let index = NAMES
                        .iter()
                        .position(|v| Some(*v) == patch["actor"].as_str())
                        .unwrap();
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: self.ids[index].unwrap(),
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                "replace_task" => {
                    let class = number(&patch["classification"]);
                    let old = self.tasks.owner(class).unwrap().unwrap();
                    self.tasks.destroy(old).unwrap();
                    self.retired_tasks.push(old);
                    let new = self
                        .tasks
                        .construct(class, ActorHandle::from_raw(number(&patch["source"])))
                        .unwrap();
                    assert_eq!(self.tasks.record(new).unwrap().counters, [None; 5]);
                    assert_eq!(patch["counters"], json!([null, null, null, null, null]));
                    assert_eq!(patch["padding"], json!([null, null]));
                }
                "advance_animation_clock" => {
                    assert_eq!(self.layout, FixtureLayout::Selector12ActorList104);
                    self.animation_clock
                        .as_mut()
                        .expect("actual live clock owner for supplied advance")
                        .advance(number(&patch["delta_micros"]));
                }
                other => panic!("unadmitted owner effect {other}"),
            }
            return;
        }
        let region = patch["region"].as_str().unwrap();
        let offset = number(&patch["offset"]) as usize;
        let data = bytes(&patch["bytes"]);
        if let Some(index) = NAMES.iter().position(|name| *name == region) {
            let id = self.ids[index].expect("live mutation owner");
            // Numeric mutations use complete backed fields; wholly unavailable
            // XYZ has its separate Option admission. Unknown rows never become
            // numeric zero.
            if matches!((offset, data.len()), (0x32 | 0x34 | 0x36, 2) | (0x32, 4)) {
                // Complete coordinate WORDs update an already complete triple;
                // the admitted X/Y span preserves Z without inventing it.
                let values: Vec<_> = data
                    .chunks_exact(2)
                    .map(|span| word(span).expect("backed coordinate WORD"))
                    .collect();
                let position = arena
                    .owned_mut(id)
                    .unwrap()
                    .state
                    .base
                    .position_words
                    .as_mut()
                    .expect("known prior XYZ for coordinate WORD mutation");
                for (axis, value) in values.into_iter().enumerate() {
                    position[(offset - 0x32) / 2 + axis] = value;
                }
                return;
            }
            if matches!(offset, 0x32 | 0x8e) {
                assert_eq!(data.len(), 6);
                if offset == 0x32 {
                    let vector = words(&data);
                    assert!(
                        vector.is_some() || data.iter().all(Option::is_none),
                        "partial numeric XYZ remains diagnostic"
                    );
                    arena.owned_mut(id).unwrap().state.base.position_words = vector
                } else {
                    arena.actor_mut(id).unwrap().words_8e_92 =
                        words(&data).expect("complete supplied velocity WORD triple")
                }
                return;
            }
            if self.layout == FixtureLayout::Selector12ActorList104 {
                match (offset, data.len()) {
                    (0x7a, 2) => {
                        arena.actor_mut(id).unwrap().word_7a = word(&data).unwrap();
                        return;
                    }
                    (0x18e, 1) => {
                        let crate::object_runtime::ActorExtension::Selector30(state) =
                            &mut arena.actor_mut(id).unwrap().extension
                        else {
                            panic!("actual selector30 byte owner")
                        };
                        state.byte_18e = data[0].expect("complete admitted selector30 byte");
                        return;
                    }
                    (0x242, 1) => {
                        let super::super::Family428650Class::Selector35(state) =
                            &mut arena.family428650_actor_mut(id).unwrap().class
                        else {
                            panic!("actual selector35 byte owner")
                        };
                        state.bytes_240[2] = data[0];
                        return;
                    }
                    _ => {}
                }
            }
            assert_eq!(offset % 4, 0);
            assert_eq!(data.len() % 4, 0);
            for (index, span) in data.chunks_exact(4).enumerate() {
                let offset = offset + index * 4;
                let value = dword(span).expect("backed numeric actor field");
                match offset {
                    0x2c => arena.owned_mut(id).unwrap().state.base.flags = value,
                    0xb4 => arena.actor_mut(id).unwrap().word_b4 = value,
                    0xc4 if self.layout == FixtureLayout::Selector12ActorList104 => {
                        arena.actor_mut(id).unwrap().word_c4 = value;
                    }
                    0x244 if self.layout == FixtureLayout::Selector12ActorList104 => {
                        let super::super::Family428650Class::Selector35(state) =
                            &mut arena.family428650_actor_mut(id).unwrap().class
                        else {
                            panic!("actual selector35 notification destination")
                        };
                        state.word_244 = value;
                    }
                    0x250..=0x264 if self.layout == FixtureLayout::Selector12ActorList104 => {
                        let super::super::Family429e20Class::Selector12(state) =
                            &mut arena.family429e20_actor_mut(id).unwrap().class
                        else {
                            panic!("actual selector12 grant mask owner")
                        };
                        state.grants_24c.mask_04[(offset - 0x250) / 4] = value;
                    }
                    0x140 => {
                        arena.derived_actor_mut(id).unwrap().behavior_140 =
                            (value != 0).then(behavior)
                    }
                    0x20c => arena.family428650_actor_mut(id).unwrap().word_20c = value,
                    0x210 => arena.family428650_actor_mut(id).unwrap().word_210 = value,
                    0x214 => arena.family428650_actor_mut(id).unwrap().word_214 = value,
                    0x164 => arena.derived_actor_mut(id).unwrap().word_164 = value as i32,
                    0x1a0 => arena.family428650_actor_mut(id).unwrap().count_1a0 = value,
                    0x1a4..=0x1d0 => {
                        let entry = &mut arena.family428650_actor_mut(id).unwrap().entries_1a4
                            [(offset - 0x1a4) / 12];
                        match (offset - 0x1a4) % 12 {
                            0 => entry.handle = ActorHandle::from_raw(value),
                            4 => entry.word_04 = value,
                            8 => entry.word_08 = value,
                            _ => unreachable!(),
                        }
                    }
                    other => panic!("unadmitted actor mutation +{other:x}"),
                }
            }
        } else if let Some(slot) = region.strip_prefix("pool:") {
            assert!(
                offset + data.len() <= 0x18 || offset >= 0x24,
                "physical nodes excluded"
            );
            pool.record_mut(ParticleId::Pool(slot.parse().unwrap()))
                .unwrap()
                .copy_bytes(offset, &data)
                .unwrap();
        } else {
            match region {
                "shared" | "registry" | "default" => {
                    let base = match region {
                        "shared" => 0x530e60,
                        "registry" => 0x53dba8,
                        _ => 0x546a94,
                    };
                    context
                        .tables
                        .write_process_bytes(base + offset as u32, &data)
                        .unwrap();
                }
                "process_null" | "phase" | "network" | "rng" => {
                    assert_eq!((offset, data.len()), (0, 4));
                    match region {
                        "process_null" => {
                            context.process.process_null_handle =
                                dword(&data).map(ActorHandle::from_raw)
                        }
                        "phase" => self.phase.set(dword(&data)),
                        "network" => context.process.network_flag = dword(&data).unwrap(),
                        "rng" => *context.rng = dword(&data).unwrap(),
                        _ => unreachable!(),
                    }
                }
                "task" => {
                    assert_eq!((offset % 0x18, data.len()), (8, 4));
                    let owner = self.tasks.owner((offset / 0x18) as u32).unwrap().unwrap();
                    self.tasks
                        .write_source(owner, ActorHandle::from_raw(dword(&data).unwrap()))
                        .unwrap();
                }
                "cache_scratch" => {
                    let sources = sources.expect("real creation source at notice");
                    for (index, byte) in data.into_iter().enumerate() {
                        let index = offset + index;
                        if index < 6 {
                            sources.vectors.position.0[index] = byte;
                        } else {
                            sources.scratch_tail[index - 6] = byte;
                        }
                    }
                    self.scratch = Some(std::array::from_fn(|i| {
                        if i < 6 {
                            sources.vectors.position.0[i]
                        } else {
                            sources.scratch_tail[i - 6]
                        }
                    }));
                }
                other => panic!("unadmitted supplied region {other}"),
            }
        }
    }
}

impl BehaviorParticleServices for Services {
    type Error = String;
    type Component = u32;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.phase.current())
    }

    fn create_actor_resource(
        &mut self,
        _: &mut ObjectArena,
        _: &mut BehaviorPredicateContext<'_>,
        _: BehaviorActorResourceRequest,
    ) -> Result<Option<ObjectId>, String> {
        panic!("no actor resource service in admitted histories")
    }
    fn query_component(
        &mut self,
        _: &mut ObjectArena,
        _: &mut BehaviorPredicateContext<'_>,
        _: BehaviorComponentQuery<u32>,
    ) -> Result<Option<u32>, String> {
        panic!("no component service in admitted histories")
    }
    fn component_packet_source(
        &mut self,
        _: &ObjectArena,
        _: &BehaviorPredicateContext<'_>,
        _: BehaviorComponentPacketSourceRequest<u32>,
    ) -> Result<Option<BehaviorPacketCopy>, String> {
        panic!("no component packet service in admitted histories")
    }
    fn particle_effect(
        &mut self,
        context: BehaviorParticleServiceContext<'_, '_, '_>,
        request: BehaviorParticleEffectRequest<'_>,
    ) -> Result<(), String> {
        let mut live = BehaviorPredicateContext {
            tables: context.tables,
            process: context.factory.process,
            rng: context.factory.rng,
        };
        let (expected, supplied, mut sources) = match request {
            BehaviorParticleEffectRequest::NetworkNotice { notice, sources } => {
                let expected = self.order.borrow_mut().next("network_notice");
                let scratch = std::array::from_fn(|i| {
                    if i < 6 {
                        sources.vectors.position.0[i]
                    } else {
                        sources.scratch_tail[i - 6]
                    }
                });
                assert_eq!(bytes(&expected["request"]["scratch_bytes"]), scratch);
                assert_eq!(number(&expected["request"]["event"]), notice.event);
                assert_eq!(number(&expected["request"]["code"]), notice.code);
                assert_eq!(
                    expected["request"]["source_handle"]
                        .as_u64()
                        .map(|v| ActorHandle::from_raw(v as u32)),
                    notice.source_handle
                );
                assert_eq!(notice.velocity_source, ParticleVelocitySource::SamePosition);
                assert!(sources.vectors.separate_velocity.is_none());
                self.scratch = Some(scratch);
                let supplied = self.invocation["network_services"][self.network_index].clone();
                self.network_index += 1;
                (expected, supplied, Some(sources))
            }
            BehaviorParticleEffectRequest::Impact489600(request) => {
                let expected = self.order.borrow_mut().next("impact_489600");
                assert_eq!(number(&expected["request"]["selector"]), request.selector);
                assert_eq!(number(&expected["request"]["flags"]), request.flags);
                let record = context.particles.record(request.particle).unwrap();
                assert_eq!(
                    bytes(&expected["request"]["position"]),
                    record.source_bytes::<6>(4).unwrap()
                );
                assert_eq!(
                    bytes(&expected["request"]["velocity"]),
                    record.source_bytes::<6>(0x0a).unwrap()
                );
                let supplied = self.invocation["impact_services"][self.impact_index].clone();
                self.impact_index += 1;
                (expected, supplied, None)
            }
        };
        assert_eq!(supplied["error"], expected["error"]);
        for patch in supplied["effects"].as_array().unwrap() {
            self.apply(
                context.arena,
                &mut live,
                context.particles,
                sources.as_mut(),
                patch,
            );
        }
        supplied["error"]
            .as_str()
            .map_or(Ok(()), |value| Err(value.into()))
    }
}

// Outcomes are supplied through the actual retained heap. Lower service order
// excludes allocator events; heap attempts separately pin the native sequence.
pub(in crate::object_runtime::actor::derived) struct AllocationPlan {
    pub(in crate::object_runtime::actor::derived) order: Rc<RefCell<ServiceOrder>>,
    pub(in crate::object_runtime::actor::derived) plans: Vec<Value>,
    pub(in crate::object_runtime::actor::derived) classes: &'static [NativeTargetClass],
    pub(in crate::object_runtime::actor::derived) before: usize,
    pub(in crate::object_runtime::actor::derived) bytes: Vec<u32>,
}
impl AllocationPlan {
    pub(in crate::object_runtime::actor::derived) fn install(&mut self, arena: &mut ObjectArena) {
        let expected: Vec<_> = self
            .order
            .borrow()
            .events
            .iter()
            .filter(|event| event["operation"] == "target_allocate")
            .cloned()
            .collect();
        assert_eq!(expected.len(), self.plans.len());
        self.bytes = expected
            .iter()
            .map(|event| number(&event["bytes"]))
            .collect();
        for (plan, event) in self.plans.iter().zip(&expected) {
            assert_eq!(plan["result"], event["result"]);
            assert_eq!(plan["error"], event["error"]);
        }
        self.order
            .borrow_mut()
            .events
            .retain(|event| event["operation"] != "target_allocate");
        arena
            .actor_service_heap_mut()
            .script(self.plans.iter().map(|plan| {
                if plan["error"].is_string() {
                    FixtureActorServiceAllocation::Error(71)
                } else if plan["result"] == "new" {
                    FixtureActorServiceAllocation::Allocate
                } else {
                    FixtureActorServiceAllocation::Null
                }
            }));
    }
    pub(in crate::object_runtime::actor::derived) fn compare(&self, arena: &ObjectArena) {
        let attempts = &arena.actor_service_heap().attempts()[self.before..];
        assert_eq!(
            attempts.len(),
            self.plans.len(),
            "native allocation attempt prefix"
        );
        for (attempt, bytes) in attempts.iter().zip(&self.bytes) {
            let ActorServiceResourceKind::Target(class) = attempt else {
                panic!("target allocation kind");
            };
            assert!(
                self.classes.contains(class),
                "admitted native allocation class"
            );
            assert_eq!(class.allocation_bytes(), *bytes, "native allocation width");
        }
    }
}

pub(in crate::object_runtime::actor::derived) struct Preparation<'a> {
    pub(in crate::object_runtime::actor::derived) phase:
        &'a crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
}
impl ParticleFactoryHost for Preparation<'_> {
    type Error = std::convert::Infallible;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.phase.current())
    }
    fn network_notice(
        &mut self,
        _: ParticleCreationNotice,
        _: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
        _: &mut ParticleFactorySources,
    ) -> Result<(), Self::Error> {
        panic!("preparation flags bypass notice")
    }
    fn impact_489600(
        &mut self,
        _: ParticleFactoryImpactRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), Self::Error> {
        panic!("preparation does not reclaim")
    }
}

pub(in crate::object_runtime::actor::derived) fn compare_payload(
    record: &ParticleRecord,
    wanted: &Value,
    name: &str,
) {
    let wanted = bytes(wanted);
    assert_eq!(wanted.len(), 64);
    let prefix = record.source_bytes::<24>(0).unwrap();
    let suffix = record.source_bytes::<28>(0x24).unwrap();
    assert_eq!(
        prefix.as_slice(),
        &wanted[..0x18],
        "{name} particle prefix/mask"
    );
    assert_eq!(
        suffix.as_slice(),
        &wanted[0x24..],
        "{name} particle suffix/mask"
    );
}

pub(in crate::object_runtime::actor::derived) fn actor_pointer(
    ids: &[Option<ObjectId>; 4],
    actor: ObjectId,
) -> u32 {
    ADDRESSES[ids
        .iter()
        .position(|id| *id == Some(actor))
        .expect("supplied actor identity")]
}

pub(in crate::object_runtime::actor::derived) fn compare_target(
    arena: &ObjectArena,
    ids: &[Option<ObjectId>; 4],
    storage: &Value,
    released: &Value,
    actor: ObjectId,
    slot: FamilyTargetSlot,
    name: &str,
) {
    let state = bytes(&storage[NAMES[ids.iter().position(|id| *id == Some(actor)).unwrap()]]);
    let offset = slot.pointer_offset() as usize;
    let pointer = dword(&state[offset..offset + 4]).unwrap();
    let actual = arena.family_weighted_target(actor, slot).unwrap();
    assert_eq!(
        Some(actual.score),
        dword(&state[offset + 4..offset + 8]),
        "{name} {slot:?} score"
    );
    if pointer == 0 {
        assert_eq!(actual.target, None);
        return;
    }
    let (region, base, identity) = if pointer >= 0x748200 {
        (
            "old_targets",
            0x748200,
            format!("old:{}", (pointer - 0x748200) / 32),
        )
    } else {
        (
            "targets",
            0x748000,
            format!("new:{}", (pointer - 0x748000) / 32),
        )
    };
    if released
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == &identity)
    {
        let reference = actual
            .target
            .expect("released native pointer remains present");
        assert_eq!(
            arena.actor_service_heap().state(reference).unwrap(),
            ActorServiceResourceState::Released,
            "{name} released raw owner"
        );
        assert!(
            arena.native_target(reference).is_err(),
            "no live freed resource decoded"
        );
        return;
    }
    let target = arena
        .native_target(actual.target.expect("nonnull live target"))
        .unwrap();
    let raw = bytes(&storage[region]);
    let offset = (pointer - base) as usize;
    let raw = &raw[offset..];
    assert_eq!(
        Some(target.handle_04().raw()),
        dword(&raw[4..8]),
        "{name} raw target handle"
    );
    let (vtable, kind, focus) = match target {
        NativeTarget::ActorFollow(target) => {
            assert_eq!(
                target.focus_14,
                words(&raw[0x14..0x1a]),
                "{name} follow point"
            );
            assert_eq!(
                target.velocity_1a,
                words(&raw[0x1a..0x20]),
                "{name} follow velocity/mask"
            );
            (0x528038, target.word_10, target.focus_08)
        }
        NativeTarget::FixedPosition(target) => (0x51f154, target.word_10, target.focus_08),
        NativeTarget::FurniturePosition(_) => panic!("unadmitted target class"),
        NativeTarget::ObjectPosition(target) => {
            assert_eq!(
                Some(actor_pointer(ids, target.object_14)),
                dword(&raw[0x14..0x18]),
                "{name} retained object identity"
            );
            (0x51fb6c, target.word_10, Some(target.focus_08))
        }
    };
    assert_eq!(Some(vtable), dword(&raw[..4]), "{name} actual target class");
    assert_eq!(Some(kind), dword(&raw[0x10..0x14]), "{name} target kind");
    assert_eq!(focus, words(&raw[8..14]), "{name} raw target focus/mask");
}

pub(in crate::object_runtime::actor::derived) fn compare_state(
    pool: &ProcessParticleRuntime,
    arena: &ObjectArena,
    live: &BehaviorPredicateContext<'_>,
    host: &Services,
    wanted: &Value,
    name: &str,
) {
    for record in wanted["records"].as_array().unwrap() {
        compare_payload(
            pool.record(ParticleId::Pool(number(&record["slot"]) as u16))
                .unwrap(),
            &record["payload"],
            name,
        );
    }
    compare_payload(
        pool.record(ParticleId::Scratch).unwrap(),
        &wanted["scratch"],
        name,
    );
    assert_eq!(
        pool.free_count(),
        slots(&wanted["free_order"]).len(),
        "{name} free count (not private stack order)"
    );
    assert!(wanted["free_boundary"].is_null());
    assert_eq!(
        pool.allocation_marker().map(u32::from),
        wanted["allocation_marker"].as_u64().map(|v| v as u32),
        "{name} marker"
    );
    let occupants: Vec<_> = (0..400)
        .filter_map(|slot| {
            pool.occupant(slot).map(|id| match id {
                ParticleId::Pool(payload) => json!([slot, payload]),
                ParticleId::Scratch => panic!("Scratch is not a pool occupant"),
            })
        })
        .collect();
    assert_eq!(
        occupants,
        *wanted["occupants"].as_array().unwrap(),
        "{name} occupants"
    );
    for category in 0..6 {
        for (key, actual) in [
            ("active", pool.active(category).unwrap()),
            ("pending", pool.pending(category).unwrap()),
        ] {
            let actual: Vec<_> = actual
                .iter()
                .map(|id| match id {
                    ParticleId::Pool(slot) => *slot,
                    ParticleId::Scratch => panic!("Scratch has no list node"),
                })
                .collect();
            assert_eq!(
                actual,
                slots(&wanted[key][category]),
                "{name} {key}/{category}"
            );
        }
    }
    for (region, base, size) in [
        ("shared", 0x530e60, 860),
        ("registry", 0x53dba8, 528),
        ("default", 0x546a94, 4),
    ] {
        assert_eq!(
            live.tables.read_process_bytes(base, size).unwrap(),
            bytes(&wanted[region]),
            "{name} {region} data/masks"
        );
    }
    assert_eq!(Some(*live.rng), dword(&bytes(&wanted["rng"])), "{name} RNG");
    assert_eq!(
        Some(live.process.network_flag),
        dword(&bytes(&wanted["network"])),
        "{name} network"
    );
    assert_eq!(
        host.phase.snapshot(),
        dword(&bytes(&wanted["phase"])),
        "{name} phase"
    );
    assert_eq!(
        live.process.process_null_handle.map(ActorHandle::raw),
        dword(&bytes(&wanted["process_null"])),
        "{name} fresh process null"
    );
    if host.layout == FixtureLayout::Selector12ActorList104 {
        assert_eq!(
            host.animation_clock.as_ref().map(|clock| clock.value()),
            wanted["clock_ticks"].as_u64().map(|value| value as u32),
            "{name} actual clock value"
        );
        assert_eq!(
            host.animation_clock
                .as_ref()
                .map(|clock| clock.accumulated_micros()),
            wanted["clock_accumulated_micros"].as_u64(),
            "{name} actual clock accumulated microseconds"
        );
        let actual_ticks = host
            .animation_clock
            .as_ref()
            .map(|clock| clock.value().to_le_bytes());
        assert_eq!(
            std::array::from_fn::<_, 4, _>(|byte| actual_ticks.map(|value| value[byte])),
            bytes(&wanted["animation_clock"]).as_slice(),
            "{name} clock WORD masks"
        );
        let actual_accumulated = host
            .animation_clock
            .as_ref()
            .map(|clock| clock.accumulated_micros().to_le_bytes());
        assert_eq!(
            std::array::from_fn::<_, 8, _>(|byte| actual_accumulated.map(|value| value[byte])),
            bytes(&wanted["animation_accumulator"]).as_slice(),
            "{name} clock accumulator masks"
        );
    }
    let storage = &wanted["selection_storage"];
    let shared_behavior = bytes(&wanted["behavior"]);
    for (index, id) in host.ids.into_iter().enumerate() {
        let live_actor = wanted
            .get("actor_custody")
            .map(|custody| {
                custody["live"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == NAMES[index])
            })
            .unwrap_or(true);
        if !live_actor {
            if let Some(id) = id {
                assert!(arena.object(id).is_err(), "retired actor not live");
            }
            continue;
        }
        let id = id.expect("live actor allocation identity");
        let raw = bytes(&storage[NAMES[index]]);
        if host.layout.is_actor_list() {
            // Each layout declares its initialized target pairs separately.
            // Remaining pairs and appended actors' target spans are untouched
            // native storage rather than zero pointer evidence.
            let untouched: &[usize] =
                if host.input["actors"][NAMES[index]]["initially_live"] == false {
                    &[0x1d4, 0x1dc, 0x1e4, 0x1ec, 0x1f4, 0x1fc]
                } else if host.layout == FixtureLayout::ActorList104 {
                    &[0x1fc]
                } else if index == 0 {
                    &[0x1e4, 0x1fc, 0x26c, 0x284]
                } else {
                    &[0x1e4, 0x1ec, 0x1f4, 0x1fc]
                };
            for &offset in untouched {
                assert_eq!(
                    &raw[offset..offset + 8],
                    &[None; 8],
                    "{name} untouched native target custody"
                );
            }
        }
        let object = arena.object(id).unwrap();
        assert_eq!(
            Some(object.base.word_28),
            word(&raw[0x28..0x2a]),
            "{name} {} extent",
            NAMES[index]
        );
        assert_eq!(
            object.base.position_words,
            words(&raw[0x32..0x38]),
            "{name} {} XYZ",
            NAMES[index]
        );
        if host.layout.is_actor_list() {
            assert_eq!(
                Some(object.base.flags),
                dword(&raw[0x2c..0x30]),
                "{name} flags"
            );
        }
        let actor = arena.actor(id).unwrap();
        if host.layout.is_actor_list() {
            assert_eq!(
                Some(actor.words_8e_92),
                words(&raw[0x8e..0x94]),
                "{name} velocity"
            );
        }
        for (offset, actual) in [
            (0x84, actor.handle.raw()),
            (0xb4, actor.word_b4),
            (0xc4, actor.word_c4),
        ] {
            assert_eq!(
                Some(actual),
                dword(&raw[offset..offset + 4]),
                "{name} {}+{offset:x}",
                NAMES[index]
            );
        }
        assert_eq!(Some(actor.word_74), word(&raw[0x74..0x76]));
        if host.layout == FixtureLayout::Selector12ActorList104 {
            assert_eq!(
                Some(actor.word_7a),
                word(&raw[0x7a..0x7c]),
                "{name} grant WORD"
            );
            if let crate::object_runtime::ActorExtension::Selector30(state) = &actor.extension {
                assert_eq!(
                    Some(state.byte_18e),
                    raw[0x18e],
                    "{name} numeric relation byte"
                );
            }
            if let Ok(family) = arena.family428650_actor(id) {
                if let super::super::Family428650Class::Selector35(state) = &family.class {
                    assert_eq!(state.bytes_240[2], raw[0x242], "{name} notification byte");
                    assert_eq!(
                        Some(state.word_244),
                        dword(&raw[0x244..0x248]),
                        "{name} notification deadline"
                    );
                }
            }
            if index == 0 {
                let mask = &arena.selector12_actor(id).unwrap().grants_24c.mask_04;
                for (index, actual) in mask.iter().enumerate() {
                    let offset = 0x250 + index * 4;
                    assert_eq!(
                        Some(*actual),
                        dword(&raw[offset..offset + 4]),
                        "{name} grant mask"
                    );
                }
            }
        }
        let response_bytes: Vec<_> = actor
            .damage_responses()
            .iter()
            .flat_map(|&(threshold, multiplier)| {
                threshold
                    .to_le_bytes()
                    .into_iter()
                    .chain(multiplier.to_le_bytes())
                    .map(Some)
            })
            .collect();
        assert_eq!(
            response_bytes,
            bytes(&wanted["responses"]),
            "{name} canonical damage responses"
        );
        if let Ok(derived) = arena.derived_actor(id) {
            assert_eq!(
                Some(derived.word_164 as u32),
                dword(&raw[0x164..0x168]),
                "{name} fresh radius"
            );
            assert_eq!(
                derived.behavior_140.is_some(),
                dword(&raw[0x140..0x144]).unwrap() != 0
            );
            if let Some(behavior) = &derived.behavior_140 {
                assert_eq!(Some(behavior.byte_10), shared_behavior[0x10]);
                assert_eq!(
                    Some(behavior.table_14 as u32),
                    dword(&shared_behavior[0x14..0x18])
                );
            }
        }
        if let Ok(family) = arena.family428650_actor(id) {
            for (offset, actual) in [
                (0x1a0, family.count_1a0),
                (0x20c, family.word_20c),
                (0x210, family.word_210),
                (0x214, family.word_214),
            ] {
                assert_eq!(
                    Some(actual),
                    dword(&raw[offset..offset + 4]),
                    "{name} {}+{offset:x}",
                    NAMES[index]
                );
            }
            assert_eq!(
                family.helper_218.is_some(),
                dword(&raw[0x218..0x21c]).unwrap() != 0
            );
            if let Some(helper) = &family.helper_218 {
                let helper_raw = bytes(&storage["selection_helper"]);
                assert_eq!(helper.words_00_04[0], dword(&helper_raw[..4]));
                assert_eq!(helper.words_00_04[1], dword(&helper_raw[4..8]));
                assert_eq!(Some(helper.word_08), dword(&helper_raw[8..12]));
                for (index, record) in helper.records.iter().enumerate() {
                    let offset = 12 + index * 20;
                    assert_eq!(
                        record.handle.map(ActorHandle::raw),
                        dword(&helper_raw[offset..offset + 4])
                    );
                    for axis in 0..3 {
                        let offset = offset + 4 + axis * 4;
                        assert_eq!(
                            record.normal_bits[axis],
                            dword(&helper_raw[offset..offset + 4])
                        );
                    }
                    assert_eq!(record.weight, word(&helper_raw[offset + 16..offset + 18]));
                    assert_eq!(record.padding, word(&helper_raw[offset + 18..offset + 20]));
                }
            }
            for (index, entry) in family.entries_1a4.iter().enumerate() {
                let offset = 0x1a4 + index * 12;
                for (delta, actual) in [
                    (0, entry.handle.raw()),
                    (4, entry.word_04),
                    (8, entry.word_08),
                ] {
                    assert_eq!(
                        Some(actual),
                        dword(&raw[offset + delta..offset + delta + 4]),
                        "{name} row{index}+{delta:x}"
                    );
                }
            }
            let fields: &[FamilyTargetSlot] = match host.layout {
                FixtureLayout::InlineTable100 | FixtureLayout::Selector12Inline100 => {
                    &[FamilyTargetSlot::Target1d4, FamilyTargetSlot::Target1dc]
                }
                FixtureLayout::ActorList104 => &[
                    FamilyTargetSlot::Target1d4,
                    FamilyTargetSlot::Target1dc,
                    FamilyTargetSlot::Target1e4,
                    FamilyTargetSlot::Target1ec,
                    FamilyTargetSlot::Target1f4,
                ],
                FixtureLayout::Selector12ActorList104 if index == 0 => &[
                    FamilyTargetSlot::Target1d4,
                    FamilyTargetSlot::Target1dc,
                    FamilyTargetSlot::Target1ec,
                    FamilyTargetSlot::Target274,
                    FamilyTargetSlot::Target27c,
                    FamilyTargetSlot::Target1f4,
                ],
                FixtureLayout::Selector12ActorList104 => {
                    &[FamilyTargetSlot::Target1d4, FamilyTargetSlot::Target1dc]
                }
            };
            for &slot in fields {
                compare_target(
                    arena,
                    &host.ids,
                    storage,
                    &wanted["released_targets"],
                    id,
                    slot,
                    name,
                );
            }
        }
    }
    let registry = arena.actor_handles().unwrap();
    assert_eq!(registry.owners().len(), 3);
    assert_eq!(registry.owner(1).unwrap().capacity(), 8);
    let entries = bytes(&storage["entries"]);
    for slot in 1..=4 {
        let actual = registry.owner(1).unwrap().slots()[slot];
        assert_eq!(
            Some(actual.handle.raw()),
            dword(&entries[slot * 12..slot * 12 + 4])
        );
        assert_eq!(
            actual.object.map(|id| actor_pointer(&host.ids, id)),
            dword(&entries[slot * 12 + 4..slot * 12 + 8]).filter(|v| *v != 0),
            "{name} actual registry slot{slot}"
        );
    }
    let cache = registry.cache();
    let raw = bytes(&storage["resolver_cache"]);
    assert_eq!(
        Some(cache.handle.raw()),
        dword(&raw[4..8]),
        "{name} resolver cache handle"
    );
    assert_eq!(
        Some(
            cache
                .object
                .map(|id| actor_pointer(&host.ids, id))
                .unwrap_or(0)
        ),
        dword(&raw[..4]),
        "{name} resolver cache object"
    );
    if host.layout.is_actor_list() {
        assert!(wanted["list_boundary"].is_null());
        let list: Vec<_> = arena
            .actor_list()
            .iter()
            .map(|id| NAMES[host.ids.iter().position(|v| *v == Some(*id)).unwrap()])
            .collect();
        assert_eq!(json!(list), wanted["list_order"], "{name} live actor list");
        assert_eq!(
            host.retired_tasks.len(),
            wanted["task_custody"]["retired_classes"]
                .as_array()
                .unwrap()
                .len()
        );
        for &owner in &host.retired_tasks {
            assert!(matches!(
                host.tasks.record(owner),
                Err(crate::process_tasks::TaskStorageError::MissingOwner(_))
            ));
        }
    }
    let task_registry = bytes(&storage["task_registry"]);
    let tasks = bytes(&storage["task"]);
    for class in 0..19 {
        let owner = host.tasks.owner(class as u32).unwrap();
        let pointer = dword(&task_registry[class * 4..class * 4 + 4]).unwrap();
        assert_eq!(owner.is_some(), pointer != 0, "{name} task{class} owner");
        if let Some(owner) = owner {
            let offset = if pointer == 0x746400 { 0 } else { class * 0x18 };
            let raw = if pointer == 0x746400 {
                assert_eq!(wanted["task_custody"]["replacement_class"], class);
                bytes(&wanted["task_custody"]["replacement_bytes"])
            } else {
                assert_eq!(
                    pointer,
                    0x746000 + class as u32 * 0x18,
                    "distinct supplied task records"
                );
                tasks.clone()
            };
            let record = host.tasks.record(owner).unwrap();
            assert_eq!(
                Some(record.source.raw()),
                dword(&raw[offset + 8..offset + 12]),
                "{name} task{class} source"
            );
            if host.layout.is_actor_list() {
                assert_eq!(
                    Some(record.classification),
                    dword(&raw[offset + 4..offset + 8])
                );
                let vtable = match record.callback {
                    crate::process_tasks::TaskCallback::Zero4027b0 => 0x5280a8,
                    crate::process_tasks::TaskCallback::Zero4efbf0 => 0x5280a4,
                    crate::process_tasks::TaskCallback::ComponentClassification => 0x5280a0,
                    crate::process_tasks::TaskCallback::TeamClassification => 0x52809c,
                };
                assert_eq!(Some(vtable), dword(&raw[offset..offset + 4]));
                for i in 0..5 {
                    assert_eq!(
                        record.counters[i],
                        word(&raw[offset + 12 + i * 2..offset + 14 + i * 2]),
                        "{name} task counter"
                    );
                }
            }
        }
    }
}

/// Real preparation factory calls, including supplied source masks and the
/// explicit transfer edge. Network/impact callbacks remain forbidden here.
pub(in crate::object_runtime::actor::derived) fn prepare_particles(
    input: &Value,
    context: &mut ParticleFactoryContext<'_, '_>,
    phase: &crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
) -> ProcessParticleRuntime {
    let mut pool = ProcessParticleRuntime::default();
    let mut sources = ParticleFactorySources {
        position: ParticleVectorBytes(bytes(&input["position"]).try_into().unwrap()),
        separate_velocity: Some(ParticleVectorBytes(
            bytes(&input["velocity"]).try_into().unwrap(),
        )),
    };
    let prepare = &input["prepare_factory"];
    assert_eq!(prepare["address"], 0x466710);
    assert!(input["prepare_first_base"].is_null());
    for _ in 0..number(&input["prepare_count"]) {
        pool.create_factory_particle(
            ParticleFactoryRequest {
                kind: ParticleFactoryKind::Factory466710,
                selector: number(&prepare["selector"]),
                source_handle: ActorHandle::from_raw(number(&prepare["source_handle"])),
                flags: number(&prepare["flags"]),
                velocity_source: ParticleVelocitySource::Separate,
            },
            context,
            &mut sources,
            &mut Preparation { phase },
        )
        .unwrap();
    }
    if input["prepare_transfer"].as_bool().unwrap() {
        pool.transfer_pending();
    }
    pool
}
