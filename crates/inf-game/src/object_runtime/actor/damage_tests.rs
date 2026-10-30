use std::collections::VecDeque;

use super::*;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs,
        DerivedActorConstructionRequest,
    },
    world_mission::RECORD_0C_BYTES,
    world_statistics::{ActorRetirementAttribution, ActorStatisticCategory, WorldStatistics},
};

const NULL: ActorHandle = ActorHandle::from_raw(0xffff_ffff);
const OTHER: ActorHandle = ActorHandle::from_raw(0x0800_0077);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    B4(ObjectId, ActorDamageCallback),
    Task(u32),
    ReadTask(usize),
    SetTask(usize, ActorHandle),
    Component(ObjectId, ObjectId),
    Capture,
    Statistic(usize, ActorDamageStatistic),
}

struct Host {
    process_null: Option<ActorHandle>,
    process_null_reads: std::cell::Cell<usize>,
    replace_process_null_on_second_source: Option<ActorHandle>,
    b4_process_null: Option<ActorHandle>,
    events: Vec<Event>,
    task_slots: VecDeque<Option<usize>>,
    task_sources: [ActorHandle; 2],
    b4_fields: Option<(u32, u32, u16)>,
    first_task_changes_class: Option<u32>,
    dispose_source: bool,
    notification_target_class: Option<u32>,
    capture_target: Option<ObjectId>,
    capture_target_class: Option<u32>,
    current_statistics: usize,
    statistics: [[u8; RECORD_0C_BYTES]; 2],
    fail: Option<ActorDamageOperation>,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            process_null: Some(NULL),
            process_null_reads: std::cell::Cell::new(0),
            replace_process_null_on_second_source: None,
            b4_process_null: None,
            events: Vec::new(),
            task_slots: VecDeque::new(),
            task_sources: [OTHER; 2],
            b4_fields: None,
            first_task_changes_class: None,
            dispose_source: false,
            notification_target_class: None,
            capture_target: None,
            capture_target_class: None,
            current_statistics: 0,
            statistics: [[0; RECORD_0C_BYTES]; 2],
            fail: None,
        }
    }
}

impl Host {
    fn reached(&self, operation: ActorDamageOperation) -> Result<(), &'static str> {
        if self.fail == Some(operation) {
            Err("required service")
        } else {
            Ok(())
        }
    }
}

impl ActorDamageHost for Host {
    type Error = &'static str;
    type TaskOwner = usize;
    type StatisticsOwner = usize;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.process_null_reads
            .set(self.process_null_reads.get() + 1);
        self.process_null
    }

    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<(), Self::Error> {
        self.events.push(Event::B4(actor, callback));
        if let Some(value) = self.b4_process_null {
            self.process_null = Some(value);
        }
        if let Some((flags, class, health)) = self.b4_fields {
            let base = &mut arena.owned_mut(actor).unwrap().state.base;
            base.flags = flags;
            let actor = arena.actor_mut(actor).unwrap();
            actor.word_b4 = class;
            actor.health = health;
        }
        self.reached(ActorDamageOperation::VirtualB4)
    }

    fn task_owner(
        &mut self,
        _: &mut ObjectArena,
        classification: u32,
    ) -> Result<Option<usize>, Self::Error> {
        self.events.push(Event::Task(classification));
        self.reached(ActorDamageOperation::TaskOwner { classification })?;
        Ok(self.task_slots.pop_front().unwrap_or(Some(0)))
    }

    fn task_source(
        &mut self,
        arena: &mut ObjectArena,
        owner: usize,
    ) -> Result<ActorHandle, Self::Error> {
        self.events.push(Event::ReadTask(owner));
        if self.process_null_reads.get() != 0 {
            if let Some(value) = self.replace_process_null_on_second_source.take() {
                self.process_null = Some(value);
            }
        }
        if let Some(class) = self.first_task_changes_class.take() {
            arena
                .actor_mut(self.capture_target.unwrap())
                .unwrap()
                .word_b4 = class;
        }
        self.reached(ActorDamageOperation::TaskSource)?;
        Ok(self.task_sources[owner])
    }

    fn set_task_source(
        &mut self,
        _: &mut ObjectArena,
        owner: usize,
        source: ActorHandle,
    ) -> Result<(), Self::Error> {
        self.events.push(Event::SetTask(owner, source));
        self.task_sources[owner] = source;
        self.reached(ActorDamageOperation::SetTaskSource)
    }

    fn source_component_108(
        &mut self,
        arena: &mut ObjectArena,
        component: ObjectId,
        target: ObjectId,
    ) -> Result<(), Self::Error> {
        self.events.push(Event::Component(component, target));
        if let Some(class) = self.notification_target_class {
            arena.actor_mut(target).unwrap().word_b4 = class;
        }
        if self.dispose_source {
            arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: component,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .unwrap();
        }
        self.reached(ActorDamageOperation::SourceComponent108)
    }

    fn statistics_owner(&mut self, arena: &mut ObjectArena) -> Result<usize, Self::Error> {
        self.events.push(Event::Capture);
        let captured = self.current_statistics;
        self.current_statistics = 1;
        if let Some(class) = self.capture_target_class {
            arena
                .actor_mut(self.capture_target.unwrap())
                .unwrap()
                .word_b4 = class;
        }
        self.reached(ActorDamageOperation::StatisticsOwner)?;
        Ok(captured)
    }

    fn record_statistics(
        &mut self,
        _: &mut ObjectArena,
        owner: usize,
        statistic: ActorDamageStatistic,
    ) -> Result<(), Self::Error> {
        self.events.push(Event::Statistic(owner, statistic));
        WorldStatistics::from_record(&mut self.statistics[owner]).record_damage(statistic);
        self.reached(ActorDamageOperation::RecordStatistics)
    }
}

fn construction() -> ActorConstructionRequest<'static> {
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

fn fixture() -> (ObjectArena, ObjectId, Host) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: NULL,
    })
    .unwrap();
    let target = arena.allocate_actor(construction()).unwrap();
    let base = &mut arena.owned_mut(target).unwrap().state.base;
    base.flags = 0;
    base.dirty_word = 0;
    arena.actor_mut(target).unwrap().word_b4 = 1;
    let host = Host {
        capture_target: Some(target),
        ..Host::default()
    };
    (arena, target, host)
}

fn request(strength: i16, source: ActorHandle) -> ActorDamageRequest {
    ActorDamageRequest {
        packet: ActorDamagePacket {
            strength,
            channel: 1,
            byte_03: None,
        },
        source,
    }
}

#[test]
fn zero_resisted_damage_still_dispatches_b4_and_task_but_skips_source_statistics() {
    let (mut arena, target, mut host) = fixture();
    let damage = request(2000, NULL);
    arena.apply_actor_damage(target, damage, &mut host).unwrap();
    assert_eq!(
        host.events,
        [
            Event::B4(
                target,
                ActorDamageCallback {
                    adjusted_damage: 0,
                    packet: damage.packet,
                    source: NULL,
                }
            ),
            Event::Task(1),
            Event::ReadTask(0),
        ]
    );
    assert_eq!(arena.actor(target).unwrap().health, 1000);
}

#[test]
fn b4_failure_keeps_callback_writes_and_prevents_outer_flags_and_task_calls() {
    let (mut arena, target, mut host) = fixture();
    host.b4_fields = Some((0x20_0000, 18, 123));
    host.fail = Some(ActorDamageOperation::VirtualB4);
    assert_eq!(
        arena.apply_actor_damage(target, request(2100, NULL), &mut host),
        Err(ActorDamageError::Host {
            operation: ActorDamageOperation::VirtualB4,
            error: "required service",
        })
    );
    assert_eq!(arena.object(target).unwrap().base.flags, 0x20_0000);
    assert_eq!(arena.actor(target).unwrap().health, 123);
    assert_eq!(host.events.len(), 1);
}

#[test]
fn post_b4_reads_live_flags_and_classification_without_dirtying_high_flag_changes() {
    let (mut arena, target, mut host) = fixture();
    host.b4_fields = Some((0x20_0010, 18, 123));
    arena
        .apply_actor_damage(target, request(2100, NULL), &mut host)
        .unwrap();
    assert_eq!(arena.object(target).unwrap().base.flags, 0x60_0010);
    assert_eq!(arena.object(target).unwrap().base.dirty_word, 0);
    assert_eq!(host.events[1], Event::Task(18));
    let Event::Statistic(owner, statistic) = host.events.last().unwrap() else {
        panic!("required damage statistic");
    };
    assert_eq!(*owner, 0);
    assert_eq!(statistic.amount, 100);
    assert_eq!(statistic.categories.target, ActorStatisticCategory::Flag10);
}

#[test]
fn task_table_slot_is_reread_but_classification_is_cached_before_source_clear() {
    let (mut arena, target, mut host) = fixture();
    host.task_slots = [Some(0), Some(1)].into();
    host.task_sources = [OTHER, OTHER];
    host.first_task_changes_class = Some(18);
    arena
        .apply_actor_damage(target, request(2000, OTHER), &mut host)
        .unwrap();
    assert_eq!(
        &host.events[1..],
        [
            Event::Task(1),
            Event::ReadTask(0),
            Event::Task(1),
            Event::ReadTask(1),
            Event::SetTask(1, NULL),
        ]
    );
    assert_eq!(host.task_sources, [OTHER, NULL]);
    assert_eq!(arena.actor(target).unwrap().word_b4, 18);
}

#[test]
fn changed_second_task_source_is_not_cleared_and_null_source_skips_second_field_read() {
    let (mut arena, target, mut host) = fixture();
    host.task_slots = [Some(0), Some(1)].into();
    host.task_sources = [OTHER, ActorHandle::from_raw(7)];
    arena
        .apply_actor_damage(target, request(2000, OTHER), &mut host)
        .unwrap();
    assert!(!host
        .events
        .iter()
        .any(|event| matches!(event, Event::SetTask(..))));
    assert_eq!(host.task_sources[1].raw(), 7);

    host.events.clear();
    host.task_sources[0] = NULL;
    arena
        .apply_actor_damage(target, request(2000, NULL), &mut host)
        .unwrap();
    assert_eq!(
        &host.events[1..],
        [Event::Task(1), Event::ReadTask(0), Event::Task(1)]
    );
}

#[test]
fn damage_task_clear_captures_live_process_null_after_callback_and_matching_gate() {
    let (mut arena, target, mut host) = fixture();
    let process_null = ActorHandle::from_raw(0x0123_4567);
    let replacement = ActorHandle::from_raw(0x0765_4321);
    assert_ne!(process_null, arena.actor_handles().unwrap().null_handle());
    host.b4_process_null = Some(process_null);
    host.replace_process_null_on_second_source = Some(replacement);
    arena
        .apply_actor_damage(target, request(2000, OTHER), &mut host)
        .unwrap();
    assert_eq!(host.process_null, Some(replacement));
    assert_eq!(host.process_null_reads.get(), 1);
    assert_eq!(host.task_sources[0], process_null);

    let (mut arena, target, mut host) = fixture();
    host.process_null = Some(OTHER);
    arena
        .apply_actor_damage(target, request(2000, OTHER), &mut host)
        .unwrap();
    assert_eq!(host.process_null_reads.get(), 1);
    assert_eq!(host.task_sources[0], OTHER);
    assert_eq!(
        host.events
            .iter()
            .filter(|event| matches!(event, Event::ReadTask(_)))
            .count(),
        1
    );

    let (mut arena, target, mut host) = fixture();
    host.process_null = None;
    arena
        .apply_actor_damage(target, request(2000, NULL), &mut host)
        .unwrap();
    assert_eq!(host.process_null_reads.get(), 0); //mismatching task source
    host.b4_fields = Some((0x20_0000, 1, 123));
    assert_eq!(
        arena.apply_actor_damage(target, request(2000, OTHER), &mut host),
        Err(ActorDamageError::MissingProcessNullHandle(target))
    );
    assert_eq!(host.process_null_reads.get(), 1);
    assert_eq!(host.task_sources[0], OTHER);
    assert_eq!(arena.actor(target).unwrap().health, 123);
    assert_eq!(arena.object(target).unwrap().base.flags, 0x60_0000);
}

#[test]
fn null_and_unsigned_out_of_range_task_owners_fail_after_reached_b4_and_flags() {
    for class in [18, 19, u32::MAX] {
        let (mut arena, target, mut host) = fixture();
        host.b4_fields = Some((0x20_0000, class, 123));
        host.task_slots.push_back(None);
        assert_eq!(
            arena.apply_actor_damage(target, request(2000, NULL), &mut host),
            Err(ActorDamageError::NullTaskOwner {
                classification: class,
            })
        );
        assert_eq!(arena.object(target).unwrap().base.flags, 0x60_0000);
        assert_eq!(host.events.len(), if class < 19 { 2 } else { 1 });
    }
}

#[test]
fn invalid_channel_fails_at_retained_table_read_even_for_negative_strength() {
    let (mut arena, target, mut host) = fixture();
    let mut damage = request(-1, NULL);
    damage.packet.channel = 10;
    assert_eq!(
        arena.apply_actor_damage(target, damage, &mut host),
        Err(ActorDamageError::UnsupportedChannel {
            actor: target,
            channel: 10,
        })
    );
    assert!(host.events.is_empty());
}

#[test]
fn component_exists_with_inactive_byte_and_notification_precedes_capture_and_fresh_categories() {
    let (mut arena, target, mut host) = fixture();
    let source = arena
        .allocate_family428650_actor(
            DerivedActorConstructionRequest {
                actor: construction(),
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap();
    assert_eq!(arena.family428650_actor(source).unwrap().byte_22c, 0);
    assert_eq!(
        arena.actor_damage_source_component(source).unwrap(),
        Some(source)
    );
    let handle = arena.actor(source).unwrap().handle;
    host.dispose_source = true;
    host.notification_target_class = Some(18);
    host.capture_target_class = Some(0);
    arena
        .apply_actor_damage(target, request(-3, handle), &mut host)
        .unwrap();
    assert_eq!(host.events[3], Event::Component(source, target));
    assert_eq!(host.events[4], Event::Capture);
    let categories = ActorRetirementAttribution {
        target: ActorStatisticCategory::B4ZeroOrUnresolved,
        source: ActorStatisticCategory::B4ZeroOrUnresolved,
    };
    assert_eq!(
        host.events[5],
        Event::Statistic(
            0,
            ActorDamageStatistic {
                amount: -3,
                categories
            }
        )
    );
    assert_eq!(host.current_statistics, 1);
    assert_eq!(
        WorldStatistics::from_record(&mut host.statistics[0]).damage_total(categories),
        (-3_i32) as u32
    );
    assert_eq!(
        WorldStatistics::from_record(&mut host.statistics[1]).damage_total(categories),
        0
    );
}

#[test]
fn source_notification_failure_retains_prior_task_clear_and_skips_statistics() {
    let (mut arena, target, mut host) = fixture();
    let source = arena
        .allocate_family428650_actor(
            DerivedActorConstructionRequest {
                actor: construction(),
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap();
    let handle = arena.actor(source).unwrap().handle;
    host.task_sources[0] = handle;
    host.notification_target_class = Some(18);
    host.fail = Some(ActorDamageOperation::SourceComponent108);
    assert_eq!(
        arena.apply_actor_damage(target, request(2100, handle), &mut host),
        Err(ActorDamageError::Host {
            operation: ActorDamageOperation::SourceComponent108,
            error: "required service",
        })
    );
    assert_eq!(host.task_sources[0], NULL);
    assert_eq!(arena.actor(target).unwrap().word_b4, 18);
    assert!(!host.events.contains(&Event::Capture));
}

#[test]
fn shared_source_skips_null_component_and_statistics_still_classify_live_actor() {
    let (mut arena, target, mut host) = fixture();
    let source = arena.allocate_actor(construction()).unwrap();
    arena.owned_mut(source).unwrap().state.base.flags = 0;
    arena.actor_mut(source).unwrap().word_b4 = 18;
    assert_eq!(arena.actor_damage_source_component(source).unwrap(), None);
    let handle = arena.actor(source).unwrap().handle;
    arena
        .apply_actor_damage(target, request(2100, handle), &mut host)
        .unwrap();
    assert!(host
        .events
        .iter()
        .all(|event| !matches!(event, Event::Component(..))));
    let Event::Statistic(_, statistic) = host.events.last().unwrap() else {
        panic!("required statistic");
    };
    assert_eq!(
        statistic.categories.source,
        ActorStatisticCategory::B4Eighteen
    );
    assert_eq!(statistic.categories.target, ActorStatisticCategory::B4One);
}

#[test]
fn radius_packet_conversion_keeps_unknown_byte_without_copying_impulse_state() {
    let packet = ActorDamagePacket::from(ScaledRadiusEffect {
        strength: -11,
        channel: 8,
        byte_03: None,
        impulse_magnitude: 123,
        impulse_vector: Some([1, 2, 3]),
    });
    assert_eq!(
        packet,
        ActorDamagePacket {
            strength: -11,
            channel: 8,
            byte_03: None
        }
    );
}

#[test]
fn intro_damage_vtables_pin_required_reaction_and_source_component_owners() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    assert_eq!(executable.word(0x0052_7c00 + 0xb4).unwrap(), 0x0047_cbc0);
    for (vtable, notify_108) in [
        (0x0052_0cc8, 0x0042_9c70),
        (0x0052_7058, 0x0042_9c70),
        (0x0052_7ee0, 0x0042_9c70),
        (0x0052_273c, 0x0046_6d40),
    ] {
        assert_eq!(executable.word(vtable + 0xb4).unwrap(), 0x0042_8040);
        assert_eq!(executable.word(vtable + 0x58).unwrap(), 0x0040_4610);
        assert_eq!(executable.word(vtable + 0xcc).unwrap(), 0x0040_4610);
        assert_eq!(executable.word(vtable + 0x108).unwrap(), notify_108);
    }
    assert_eq!(executable.word(0x0051_f080 + 0xb4).unwrap(), 0x0042_83e0);
    assert_eq!(executable.word(0x0051_f080 + 0xcc).unwrap(), 0x0040_1300);
}
