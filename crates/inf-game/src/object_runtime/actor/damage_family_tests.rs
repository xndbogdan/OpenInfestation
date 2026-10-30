use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorDamagePacket, ActorDamageReactionEffect, ActorRegistryConfig,
    ActorTerrainInputs, DerivedActorConstructionRequest, FamilyHandleEntry,
};
use std::collections::VecDeque;

#[path = "damage_family_tests/combat_native.rs"]
mod combat_native;
#[path = "damage_family_tests/mydon_retail.rs"]
mod mydon_retail;

fn construction(position: [u16; 3]) -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some(position),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &[0; 4096],
            },
        },
        world_mask_limits: [0, 0],
    }
}

fn setup() -> (ObjectArena, ObjectId, Host, u32) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_family428650_actor(construction([0; 3]), &mut 1)
        .unwrap();
    arena.owned_mut(actor).unwrap().state.base.flags = 0x4000;
    arena.owned_mut(actor).unwrap().state.base.dirty_word = 0;
    (arena, actor, Host::default(), 1)
}

fn callback(damage: i32, channel: u8, source: ActorHandle) -> ActorDamageCallback {
    ActorDamageCallback {
        adjusted_damage: damage,
        packet: ActorDamagePacket {
            strength: damage as i16,
            channel,
            byte_03: None,
        },
        source,
    }
}

fn request(source: u32, word_08: u32, word_04: u32) -> ActorFamilyTargetRequest {
    ActorFamilyTargetRequest {
        source: ActorHandle::from_raw(source),
        word_08,
        word_04,
    }
}

#[derive(Default)]
struct Host {
    events: Vec<&'static str>,
    channels: VecDeque<Result<u8, &'static str>>,
    fail_effect: bool,
    reaction_mutation: bool,
    fail_selection: bool,
    mutate_and_append_on_selection: bool,
    appended: Option<ObjectId>,
}

impl ActorDamageReactionHost for Host {
    type Error = &'static str;
    type RecentSource = ();
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0))
    }
    fn network_active(&mut self) -> Result<bool, Self::Error> {
        self.events.push("network");
        Ok(false)
    }
    fn difficulty_multiplier(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<i16, Self::Error> {
        self.events.push("difficulty");
        Ok(256)
    }
    fn source_bonuses(
        &mut self,
        _: &mut ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<[i8; 6]>, Self::Error> {
        Err("unreached source bonuses")
    }
    fn animation_tick(&mut self) -> Result<u32, Self::Error> {
        Err("unreached clock")
    }
    fn network_role(&mut self) -> Result<i32, Self::Error> {
        Err("unreached network role")
    }
    fn local_player_byte(&mut self) -> Result<u8, Self::Error> {
        Err("unreached local player")
    }
    fn excludes_status(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<bool, Self::Error> {
        self.events.push("status");
        Ok(false)
    }
    fn recent_damage_source(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<()>, Self::Error> {
        Err("unreached recent source")
    }
    fn recent_source_tick(&mut self, _: &mut ObjectArena, _: ()) -> Result<u32, Self::Error> {
        Err("unreached recent timestamp")
    }
    fn recent_source_owner_word(&mut self, _: &mut ObjectArena, _: ()) -> Result<u16, Self::Error> {
        Err("unreached recent owner")
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        _: ActorDamageReactionEffect,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push("reaction");
        if self.reaction_mutation {
            arena.derived_actor_mut(actor).unwrap().word_144 = -3;
            arena.family428650_actor_mut(actor).unwrap().word_22e = 0x8000;
            arena.owned_mut(actor).unwrap().state.base.flags = 0x8460_4000;
        }
        if self.fail_effect {
            Err("reaction effect")
        } else {
            Ok(())
        }
    }
}

impl ActorFamilyDamageHost for Host {
    fn packet_channel(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<u8, Self::Error> {
        self.events.push("packet");
        // This fixture owns an immutable literal packet unless an explicit
        // reached read/mutation/failure train is supplied by the test.
        self.channels
            .pop_front()
            .unwrap_or(Ok(callback.packet.channel))
    }
    fn select_target_100(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push("select");
        if self.mutate_and_append_on_selection {
            arena.actor_mut(actor).unwrap().word_b4 = 7;
            arena.derived_actor_mut(actor).unwrap().word_164 = 16;
            let base = &mut arena.owned_mut(actor).unwrap().state.base;
            base.resource_word = 44;
            base.position_words = Some([100, 0, 0]);
            let appended = add_neighbor(arena, [103, 0, 0], 7, 44, 0x20);
            self.appended = Some(appended);
        }
        if self.fail_selection {
            Err("virtual100")
        } else {
            Ok(())
        }
    }
}

#[test]
fn target_admission_updates_both_unsigned_maxima_but_matching_returns_false() {
    let (mut arena, actor, _, _) = setup();
    assert!(arena
        .record_family_damage_target(actor, request(9, 10, 20))
        .unwrap());
    assert!(arena
        .record_family_damage_target(actor, request(8, 30, 40))
        .unwrap());
    assert!(!arena
        .record_family_damage_target(actor, request(9, u32::MAX, 1))
        .unwrap());
    assert!(!arena
        .record_family_damage_target(actor, request(9, 0, u32::MAX))
        .unwrap());
    let state = arena.family428650_actor(actor).unwrap();
    assert_eq!(state.count_1a0, 2);
    assert_eq!(
        state.entries_1a4[0],
        FamilyHandleEntry {
            handle: ActorHandle::from_raw(9),
            word_04: u32::MAX,
            word_08: u32::MAX,
        }
    );
    assert_eq!(
        (state.entries_1a4[1].word_08, state.entries_1a4[1].word_04),
        (30, 40)
    );
}

#[test]
fn target_replacement_uses_quantized_unsigned_priority_and_first_minimum_tie() {
    let (mut arena, actor, _, _) = setup();
    for (source, a, b) in [
        (1, 0, 0x1234_0000),
        (2, 1, 0),
        (3, 0xffff, 0),
        (4, 0x8000_0000, 0),
    ] {
        assert!(arena
            .record_family_damage_target(actor, request(source, a, b))
            .unwrap());
    }
    // Any nonzero low-only +08 scores at least00010000, above slot0's1234.
    assert!(arena
        .record_family_damage_target(actor, request(5, 1, 0))
        .unwrap());
    assert_eq!(
        arena.family428650_actor(actor).unwrap().entries_1a4[0]
            .handle
            .raw(),
        5
    );
    // All first-three scores now tie at00010000. Equal priority cannot replace.
    assert!(!arena
        .record_family_damage_target(actor, request(6, 0x1_0000, 0xffff))
        .unwrap());
    assert!(arena
        .record_family_damage_target(actor, request(6, 0x1_0000, 0x1_0000))
        .unwrap());
    let state = arena.family428650_actor(actor).unwrap();
    assert_eq!(
        state.entries_1a4.map(|entry| entry.handle.raw()),
        [6, 2, 3, 4]
    );
    assert_eq!(state.count_1a0, 4);
}

#[test]
fn signed_nonpositive_counts_increment_original_bits_and_excess_count_fails_at_read() {
    let (mut arena, actor, _, _) = setup();
    arena.family428650_actor_mut(actor).unwrap().count_1a0 = u32::MAX;
    assert!(arena
        .record_family_damage_target(actor, request(9, 1, 2))
        .unwrap());
    assert_eq!(arena.family428650_actor(actor).unwrap().count_1a0, 0);
    let state = arena.family428650_actor_mut(actor).unwrap();
    state.count_1a0 = 5;
    // A match before the fifth read is still native-supported.
    assert!(!arena
        .record_family_damage_target(actor, request(9, 3, 4))
        .unwrap());
    assert_eq!(
        arena.record_family_damage_target(actor, request(10, 1, 2)),
        Err(ActorFamilyTargetError::TargetCount { actor, count: 5 })
    );
}

#[test]
fn d4_preserves_activation_prefix_before_reached_invalid_table_read() {
    let (mut arena, actor, _, _) = setup();
    arena.derived_actor_mut(actor).unwrap().word_144 = -5;
    arena.family428650_actor_mut(actor).unwrap().count_1a0 = 5;
    arena.family428650_actor_mut(actor).unwrap().word_22e = 0x28;
    arena.owned_mut(actor).unwrap().state.base.flags = 0x0460_0020;
    assert_eq!(
        arena.notify_family_damage_target(actor, request(11, 1, 2)),
        Err(ActorFamilyTargetError::TargetCount { actor, count: 5 })
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_144, 5000);
    assert_eq!(arena.object(actor).unwrap().base.flags, 0x20);
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0);
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0x28);
}

#[test]
fn reaction_failure_keeps_first_22e_write_and_does_not_execute_suffix() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.owned_mut(actor).unwrap().state.base.flags |= 0x8000_0000;
    host.reaction_mutation = true;
    host.fail_effect = true;
    let error = arena
        .react_family_actor_damage(
            actor,
            callback(1, 8, ActorHandle::from_raw(10)),
            &mut rng,
            &mut host,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ActorFamilyDamageError::Reaction(ActorDamageReactionError::Host("reaction effect"))
    ));
    assert_eq!(arena.derived_actor(actor).unwrap().word_144, -3);
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0x8000);
    assert_eq!(arena.object(actor).unwrap().base.flags, 0x8460_4000);
    assert_eq!(host.events, ["reaction"]);
}

#[test]
fn suffix_rereads_reaction_mutations_and_zero_channel_one_still_owns_final_read() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.owned_mut(actor).unwrap().state.base.flags |= 0x8000_0000;
    host.reaction_mutation = true;
    let own = arena.actor(actor).unwrap().handle;
    arena
        .react_family_actor_damage(actor, callback(1, 8, own), &mut rng, &mut host)
        .unwrap();
    assert_eq!(arena.derived_actor(actor).unwrap().word_144, 3000);
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0x8001);
    assert_eq!(arena.object(actor).unwrap().base.flags, 0x8000_4000);
    assert_eq!(arena.derived_actor(actor).unwrap().word_17a, 5000);
    assert_eq!(host.events, ["reaction", "packet"]);
    host.events.clear();
    arena
        .react_family_actor_damage(
            actor,
            callback(0, 1, ActorHandle::from_raw(10)),
            &mut rng,
            &mut host,
        )
        .unwrap();
    assert_eq!(host.events, ["packet", "packet"]);
    assert_eq!(arena.family428650_actor(actor).unwrap().count_1a0, 0);
}

#[test]
fn friendly_source_return_skips_channel_eight_tail_and_packet_read() {
    let (mut arena, actor, mut host, mut rng) = setup();
    let source = arena.allocate_actor(construction([0; 3]).actor).unwrap();
    arena.actor_mut(source).unwrap().word_b4 = arena.actor(actor).unwrap().word_b4;
    let source = arena.actor(source).unwrap().handle;
    host.channels.push_back(Err("must not reach suffix packet"));
    arena
        .react_family_actor_damage(actor, callback(1, 8, source), &mut rng, &mut host)
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().health, 999);
    assert_eq!(arena.derived_actor(actor).unwrap().word_17a, 0);
    assert!(host.events.is_empty());
}

#[test]
fn both_reached_packet_reads_use_retained_context_and_preserve_failure_prefixes() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.derived_actor_mut(actor).unwrap().word_144 = -2;
    host.channels.push_back(Err("first packet read"));
    assert!(matches!(
        arena.react_family_actor_damage(
            actor,
            callback(0, 1, ActorHandle::from_raw(10)),
            &mut rng,
            &mut host
        ),
        Err(ActorFamilyDamageError::Host {
            operation: ActorFamilyDamageOperation::PacketChannel,
            error: "first packet read"
        })
    ));
    assert_eq!(arena.derived_actor(actor).unwrap().word_144, 2000);
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 1);
    assert_eq!(arena.family428650_actor(actor).unwrap().count_1a0, 0);
    host.channels = VecDeque::from([Ok(0), Ok(8)]);
    // Initial copied packet says1; actual first read0 admits, final read8 writes.
    arena
        .react_family_actor_damage(
            actor,
            callback(0, 1, ActorHandle::from_raw(10)),
            &mut rng,
            &mut host,
        )
        .unwrap();
    assert_eq!(arena.family428650_actor(actor).unwrap().count_1a0, 1);
    assert_eq!(arena.derived_actor(actor).unwrap().word_17a, 5000);
    host.channels = VecDeque::from([Err("suffix packet read")]);
    assert!(matches!(
        arena.react_family_actor_damage(
            actor,
            callback(1, 0, ActorHandle::from_raw(11)),
            &mut rng,
            &mut host
        ),
        Err(ActorFamilyDamageError::Host {
            operation: ActorFamilyDamageOperation::PacketChannel,
            error: "suffix packet read"
        })
    ));
    assert_eq!(arena.actor(actor).unwrap().health, 999);
    assert_eq!(arena.family428650_actor(actor).unwrap().count_1a0, 2);
}

/// A family neighbor whose resource WORD +30 is `resource`. Each neighbor
/// gets its own placement WORD +2A, which 004281B0 never compares.
fn add_neighbor(
    arena: &mut ObjectArena,
    position: [u16; 3],
    class: u32,
    resource: u16,
    flags: u32,
) -> ObjectId {
    let actor = arena
        .allocate_family428650_actor(construction(position), &mut 1)
        .unwrap();
    arena.actor_mut(actor).unwrap().word_b4 = class;
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.resource_word = resource;
    base.placement_word = 0x100u16.wrapping_add(position[0]);
    base.flags = flags;
    actor
}

#[test]
fn broadcast_uses_strict_wrapped_distance_and_component_existence_without_active_gate() {
    let (mut arena, actor, _, _) = setup();
    arena.derived_actor_mut(actor).unwrap().word_164 = 80;
    arena.owned_mut(actor).unwrap().state.base.resource_word = 3;
    let accepted = add_neighbor(&mut arena, [19, 0, 0], 1, 3, 0x20);
    arena.actor_mut(accepted).unwrap().word_c4 = 0; //broadcast has no lifecycle gate
    let wrapped = add_neighbor(&mut arena, [u16::MAX, 0, 0], 1, 3, 0x20);
    let boundary = add_neighbor(&mut arena, [20, 0, 0], 1, 3, 0x20);
    let blocked_flags = add_neighbor(&mut arena, [1, 0, 0], 1, 3, 0xa0);
    let blocked_class = add_neighbor(&mut arena, [1, 0, 0], 2, 3, 0x20);
    let blocked_resource = add_neighbor(&mut arena, [1, 0, 0], 1, 4, 0x20);
    // The victim's own placement on another resource is still filtered out.
    let same_placement = add_neighbor(&mut arena, [2, 0, 0], 1, 5, 0x20);
    let victim_placement = arena.object(actor).unwrap().base.placement_word;
    arena
        .owned_mut(same_placement)
        .unwrap()
        .state
        .base
        .placement_word = victim_placement;
    let plain = arena.allocate_actor(construction([1, 0, 0]).actor).unwrap();
    arena.actor_mut(plain).unwrap().word_b4 = 1;
    arena.owned_mut(plain).unwrap().state.base.flags = 0x20;
    arena.owned_mut(plain).unwrap().state.base.resource_word = 3;
    let target = request(999, 0x03c0_0000, 0);
    arena.broadcast_family_damage_target(actor, target).unwrap();
    for receiver in [accepted, wrapped] {
        let state = arena.family428650_actor(receiver).unwrap();
        assert_eq!(state.count_1a0, 1);
        assert_eq!(state.byte_22c, 0);
        assert_eq!(state.entries_1a4[0].handle, target.source);
    }
    for receiver in [
        actor,
        boundary,
        blocked_flags,
        blocked_class,
        blocked_resource,
        same_placement,
    ] {
        assert_eq!(arena.family428650_actor(receiver).unwrap().count_1a0, 0);
    }
}

#[test]
fn stopped_construction_damage_broadcast_receiver_keeps_native_state_and_later_receiver_runs() {
    let (mut arena, actor, _, _) = setup();
    arena.derived_actor_mut(actor).unwrap().word_164 = 80;
    arena.owned_mut(actor).unwrap().state.base.resource_word = 3;
    let stopped = add_neighbor(&mut arena, [1, 0, 0], 1, 3, 0x20);
    arena.owned_mut(stopped).unwrap().state.base.position_words = None;
    arena.stop_object_construction(stopped).unwrap();
    let healthy = add_neighbor(&mut arena, [2, 0, 0], 1, 3, 0x20);
    let before = arena.object(stopped).unwrap().clone();
    let nodes = arena.actor_list().to_vec();
    let target = request(999, 0x03c0_0000, 0);

    arena.broadcast_family_damage_target(actor, target).unwrap();

    let state = arena.family428650_actor(healthy).unwrap();
    assert_eq!(state.count_1a0, 1);
    assert_eq!(state.entries_1a4[0].handle, target.source);
    assert_eq!(arena.object(stopped).unwrap(), &before);
    assert_eq!(arena.actor_list(), nodes);
}

#[test]
fn new_target_keeps_writes_on_selection_failure_and_broadcast_reads_current_fields_after_selection()
{
    let (mut arena, _, mut host, mut rng) = setup();
    let actor = arena
        .allocate_family4236d0_actor(construction([0; 3]), &mut 1)
        .unwrap();
    arena.owned_mut(actor).unwrap().state.base.flags = 0x4000;
    host.fail_selection = true;
    assert!(matches!(
        arena.react_family_actor_damage(
            actor,
            callback(1, 8, ActorHandle::from_raw(10)),
            &mut rng,
            &mut host
        ),
        Err(ActorFamilyDamageError::Host {
            operation: ActorFamilyDamageOperation::SelectTarget100,
            error: "virtual100"
        })
    ));
    assert_eq!(arena.actor(actor).unwrap().health, 999);
    assert_eq!(arena.family428650_actor(actor).unwrap().count_1a0, 1);
    assert_eq!(arena.derived_actor(actor).unwrap().word_17a, 0);
    assert_eq!(host.events, ["select"]);
    host.fail_selection = false;
    host.mutate_and_append_on_selection = true;
    let receiver = add_neighbor(&mut arena, [101, 0, 0], 7, 44, 0x20);
    arena
        .react_family_actor_damage(
            actor,
            callback(1, 8, ActorHandle::from_raw(11)),
            &mut rng,
            &mut host,
        )
        .unwrap();
    for receiver in [receiver, host.appended.unwrap()] {
        assert_eq!(
            arena.family428650_actor(receiver).unwrap().entries_1a4[0]
                .handle
                .raw(),
            11
        );
    }
    assert_eq!(arena.derived_actor(actor).unwrap().word_17a, 5000);
    assert_eq!(host.events, ["select", "select", "packet"]);
}

#[test]
fn existing_target_uses_one_shared_rng_draw_and_signed_channel_eight_floor() {
    for seed in [1u32, 2, 3, 4] {
        let (mut arena, actor, mut host, _) = setup();
        let mut rng = seed;
        arena
            .record_family_damage_target(actor, request(10, 1, 1))
            .unwrap();
        let neighbor = add_neighbor(&mut arena, [1, 0, 0], 1, u16::MAX, 0x20);
        arena.derived_actor_mut(actor).unwrap().word_17a = 0xffff;
        let mut expected = rng;
        let draw = name_entry_rng_next(&mut expected);
        arena
            .react_family_actor_damage(
                actor,
                callback(1, 8, ActorHandle::from_raw(10)),
                &mut rng,
                &mut host,
            )
            .unwrap();
        assert_eq!(rng, expected);
        assert_eq!(
            arena.family428650_actor(neighbor).unwrap().count_1a0,
            u32::from(draw & 3 == 0)
        );
        assert_eq!(arena.derived_actor(actor).unwrap().word_17a, 5000);
        assert_eq!(host.events, ["packet"]);
    }
}

#[test]
fn retail_vtables_pin_family_d4_and_actual_114_dispatch() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let image = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    for (vtable, selection) in [
        (0x0052_0f34u32, 0x0040_1310), //428650
        (0x0052_1164, 0x0040_1310),    //429E20
        (0x0052_0b84, 0x0043_f3d0),    //4236D0
        (0x0052_0cc8, 0x0043_f3d0),    //selector10
        (0x0052_7058, 0x0040_1310),    //selector12
        (0x0052_7ee0, 0x0043_f3d0),    //selector36
        (0x0052_273c, 0x0043_f3d0),    //selector35
        (0x0052_1eac, 0x0043_f3d0),    //selector34
    ] {
        assert_eq!(image.word(vtable + 0x58).unwrap(), 0x0040_4610);
        assert_eq!(image.word(vtable + 0xcc).unwrap(), 0x0040_4610);
        assert_eq!(image.word(vtable + 0xd4).unwrap(), 0x0042_82c0);
        assert_eq!(image.word(vtable + 0x114).unwrap(), selection);
    }
    // Original .text has VMA 00401000 and file offset1000; unlike the
    // read-only/data mapper above, these pins read the actual instruction bytes.
    assert_eq!(
        &bytes[0x3_f3d0..0x3_f3dd],
        &[0x8b, 0x01, 0x6a, 0x00, 0x6a, 0x00, 0xff, 0x90, 0x00, 0x01, 0x00, 0x00, 0xc3]
    );
    assert_eq!(&bytes[0x1310..0x1311], &[0xc3]);
}
