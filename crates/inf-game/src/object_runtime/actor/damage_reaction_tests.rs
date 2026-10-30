use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorDamagePacket, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};
use std::collections::VecDeque;

#[derive(Default)]
struct Host {
    process_null: Option<ActorHandle>,
    process_null_reads: std::cell::Cell<usize>,
    events: Vec<&'static str>,
    effects: Vec<ActorDamageReactionEffect>,
    network: bool,
    multiplier: i16,
    bonuses: Option<[i8; 6]>,
    clocks: VecDeque<u32>,
    role: i32,
    exclude: bool,
    recent: Option<(u16, u32)>,
    recent_tick_on_clock: Option<u32>,
    status_on_exclusion: Option<(u16, u16)>,
    fail_effect: bool,
    retaliation_health: Option<u16>,
    clear_player_on_lethal: bool,
}

impl ActorDamageReactionHost for Host {
    type Error = &'static str;
    type RecentSource = u8;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.process_null_reads
            .set(self.process_null_reads.get() + 1);
        self.process_null
    }
    fn network_active(&mut self) -> Result<bool, Self::Error> {
        self.events.push("network");
        Ok(self.network)
    }
    fn difficulty_multiplier(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<i16, Self::Error> {
        self.events.push("difficulty");
        Ok(self.multiplier)
    }
    fn source_bonuses(
        &mut self,
        _: &mut ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<[i8; 6]>, Self::Error> {
        self.events.push("bonuses");
        Ok(self.bonuses)
    }
    fn animation_tick(&mut self) -> Result<u32, Self::Error> {
        self.events.push("clock");
        if let Some(tick) = self.recent_tick_on_clock.take() {
            self.recent.as_mut().expect("retained source").1 = tick;
        }
        Ok(self.clocks.pop_front().expect("reached clock read"))
    }
    fn network_role(&mut self) -> Result<i32, Self::Error> {
        self.events.push("role");
        Ok(self.role)
    }
    fn local_player_byte(&mut self) -> Result<u8, Self::Error> {
        self.events.push("player");
        Ok(7)
    }
    fn excludes_status(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<bool, Self::Error> {
        self.events.push("excludes");
        if let Some((cc, ce)) = self.status_on_exclusion.take() {
            let state = arena.actor_mut(actor).unwrap();
            state.word_cc = cc;
            state.word_ce = ce;
        }
        Ok(self.exclude)
    }
    fn recent_damage_source(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<Self::RecentSource>, Self::Error> {
        self.events.push("recent");
        Ok(self.recent.map(|_| 3))
    }
    fn recent_source_tick(
        &mut self,
        _: &mut ObjectArena,
        source: Self::RecentSource,
    ) -> Result<u32, Self::Error> {
        assert_eq!(source, 3);
        self.events.push("recent_tick");
        Ok(self.recent.expect("retained source").1)
    }
    fn recent_source_owner_word(
        &mut self,
        _: &mut ObjectArena,
        source: Self::RecentSource,
    ) -> Result<u16, Self::Error> {
        assert_eq!(source, 3);
        self.events.push("recent_owner");
        Ok(self.recent.expect("retained source").0)
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: ActorDamageReactionEffect,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push("effect");
        self.effects.push(effect);
        if self.fail_effect {
            return Err("required service");
        }
        if matches!(effect, ActorDamageReactionEffect::Retaliation { .. }) {
            if let Some(health) = self.retaliation_health {
                arena.actor_mut(actor).unwrap().health = health;
            }
        }
        if matches!(effect, ActorDamageReactionEffect::Lethal { .. }) && self.clear_player_on_lethal
        {
            let base = &mut arena.owned_mut(actor).unwrap().state.base;
            base.flags &= !0x10;
        }
        Ok(())
    }
}

fn setup() -> (ObjectArena, ObjectId, Host, u32) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
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
                },
                world_mask_limits: [0, 0],
            },
            &mut 1,
        )
        .unwrap();
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.flags = 0;
    base.dirty_word = 0;
    arena.actor_mut(actor).unwrap().maximum_health = Some(1000);
    (
        arena,
        actor,
        Host {
            process_null: Some(ActorHandle::from_raw(0)),
            multiplier: 256,
            ..Host::default()
        },
        1,
    )
}

fn callback(amount: i32, channel: u8, source: ActorHandle) -> ActorDamageCallback {
    ActorDamageCallback {
        adjusted_damage: amount,
        packet: ActorDamagePacket {
            strength: amount as i16,
            channel,
            byte_03: None,
        },
        source,
    }
}
fn null(arena: &ObjectArena) -> ActorHandle {
    arena.actor_handles().unwrap().null_handle()
}

#[test]
fn zero_shared_damage_marks_health_dirty_but_derived_zero_only_activates() {
    let (mut arena, actor, mut host, mut rng) = setup();
    let cb = callback(0, 0, null(&arena));
    arena
        .react_actor_damage(actor, cb, &mut rng, &mut host)
        .unwrap();
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0x2000);
    assert_eq!(host.events, ["network", "difficulty"]);
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.derived_actor_mut(actor).unwrap().word_144 = -3;
    arena.owned_mut(actor).unwrap().state.base.flags = 0x0460_0000;
    arena
        .react_derived_actor_damage(actor, cb, &mut rng, &mut host)
        .unwrap();
    assert_eq!(arena.derived_actor(actor).unwrap().word_144, 3000);
    assert_eq!(arena.object(actor).unwrap().base.flags, 0);
    assert_eq!(arena.actor(actor).unwrap().word_ca, None);
    assert!(host.events.is_empty());
}

/// `0040E030`: zero damage and lifecycle 1 or less return at once; a hit
/// otherwise colours the actor and reaches `0047CBC0`, without
/// `004283E0`'s +144 rescale and flag clear.
#[test]
fn family_40d800_hit_colours_then_reacts() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.derived_actor_mut(actor).unwrap().word_144 = -3;
    arena.owned_mut(actor).unwrap().state.base.flags = 0x0460_0000;
    let hit = callback(40, 0, null(&arena));
    arena
        .react_family40d800_damage(actor, callback(0, 0, null(&arena)), &mut rng, &mut host)
        .unwrap();
    arena.actor_mut(actor).unwrap().word_c4 = 1;
    arena
        .react_family40d800_damage(actor, hit, &mut rng, &mut host)
        .unwrap();
    assert!(host.events.is_empty());
    assert_eq!(arena.actor(actor).unwrap().word_ca, None);
    arena.actor_mut(actor).unwrap().word_c4 = 3;
    arena
        .react_family40d800_damage(actor, hit, &mut rng, &mut host)
        .unwrap();
    let state = arena.actor(actor).unwrap();
    // (40 * 512 * 31250) >> 15.
    assert_eq!(
        (state.word_ca, state.word_c8, state.health),
        (Some(55), 19531, 960)
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_144, -3);
    assert_eq!(arena.object(actor).unwrap().base.flags, 0x0460_0000);
    assert_eq!(host.events, ["network", "difficulty"]);
}

#[test]
fn derived_hit_publishes_color_and_timer_before_a_failing_shared_service() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.derived_actor_mut(actor).unwrap().word_144 = i32::MIN;
    arena.owned_mut(actor).unwrap().state.base.flags = 0x8460_0000;
    host.fail_effect = true;
    let cb = callback(100, 0, null(&arena));
    assert!(arena
        .react_derived_actor_damage(actor, cb, &mut rng, &mut host)
        .is_err());
    assert_eq!(arena.derived_actor(actor).unwrap().word_144, 0);
    assert_eq!(arena.object(actor).unwrap().base.flags, 0x8000_0000);
    assert_eq!(arena.actor(actor).unwrap().word_ca, Some(55));
    assert_eq!(arena.actor(actor).unwrap().word_c8, 31249);
    assert_eq!(arena.actor(actor).unwrap().health, 1000);
    assert!(matches!(
        host.effects[0],
        ActorDamageReactionEffect::RemoteDamage { .. }
    ));
}

#[test]
fn offline_signed_multiplier_and_signed_bonus_preserve_truncation_order() {
    let (mut arena, actor, mut host, mut rng) = setup();
    host.multiplier = 128;
    host.bonuses = Some([0, 0, -7, 0, 0, 0]);
    let cb = callback(103, 8, ActorHandle::from_raw(0x0c00_0001));
    arena
        .react_actor_damage(actor, cb, &mut rng, &mut host)
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().health, 963); //51+(-357/25)=37
    assert_eq!(host.events, ["network", "difficulty", "bonuses"]);
    host.events.clear();
    host.multiplier = -256;
    arena
        .react_actor_damage(actor, callback(10, 0, null(&arena)), &mut rng, &mut host)
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().health, 973);
}

#[test]
fn bypass_flag_and_network_skip_offline_bonus_services() {
    for (flags, network, events) in [
        (0x4000, false, Vec::<&str>::new()),
        (0, true, vec!["network"]),
    ] {
        let (mut arena, actor, mut host, mut rng) = setup();
        arena.owned_mut(actor).unwrap().state.base.flags = flags;
        host.network = network;
        arena
            .react_actor_damage(
                actor,
                callback(11, 9, ActorHandle::from_raw(0x0c00_0001)),
                &mut rng,
                &mut host,
            )
            .unwrap();
        assert_eq!(arena.actor(actor).unwrap().health, 989);
        assert_eq!(host.events, events);
    }
}

#[test]
fn channel_two_owns_status_before_zero_health_damage_and_wraps_healing() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena
        .react_actor_damage(actor, callback(499, 2, null(&arena)), &mut rng, &mut host)
        .unwrap();
    assert_eq!(
        (
            arena.actor(actor).unwrap().word_cc,
            arena.actor(actor).unwrap().word_ce
        ),
        (100, 499)
    );
    assert_eq!(arena.actor(actor).unwrap().health, 1000);
    arena
        .apply_channel_two_status(actor, 600, &mut rng, &mut host)
        .unwrap();
    assert_eq!(
        (
            arena.actor(actor).unwrap().word_cc,
            arena.actor(actor).unwrap().word_ce
        ),
        (120, 1099)
    );
    arena.actor_mut(actor).unwrap().word_ce = 2;
    arena.actor_mut(actor).unwrap().word_cc = 900;
    arena
        .apply_channel_two_status(actor, -1, &mut rng, &mut host)
        .unwrap();
    assert_eq!(
        (
            arena.actor(actor).unwrap().word_cc,
            arena.actor(actor).unwrap().word_ce
        ),
        (500, 0)
    );
    arena.actor_mut(actor).unwrap().word_ce = 1;
    arena
        .apply_channel_two_status(actor, -2, &mut rng, &mut host)
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().word_ce, 65533);
}

#[test]
fn excluded_positive_status_requires_retirement_before_any_status_write() {
    let (mut arena, actor, mut host, mut rng) = setup();
    host.exclude = true;
    host.fail_effect = true;
    assert!(arena
        .apply_channel_two_status(actor, 20, &mut rng, &mut host)
        .is_err());
    assert_eq!(host.effects, [ActorDamageReactionEffect::StatusRetirement]);
    assert_eq!(
        (
            arena.actor(actor).unwrap().word_cc,
            arena.actor(actor).unwrap().word_ce
        ),
        (0, 0)
    );
}

#[test]
fn retaliation_commits_rng_and_reread_clock_before_required_effect() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.actor_mut(actor).unwrap().byte_7c = 1;
    host.clocks = VecDeque::from([5, 17]);
    host.fail_effect = true;
    let mut expected_rng = rng;
    let draw = name_entry_rng_next(&mut expected_rng);
    assert!(arena
        .react_actor_damage(
            actor,
            callback(20, 0, ActorHandle::from_raw(0x0800_0001)),
            &mut rng,
            &mut host
        )
        .is_err());
    assert_eq!(rng, expected_rng);
    assert_eq!(arena.actor(actor).unwrap().word_80, 17 + (draw & 127) + 150);
    assert_eq!(arena.actor(actor).unwrap().health, 1000);
    assert_eq!(
        host.events,
        ["network", "difficulty", "clock", "clock", "effect"]
    );
}

#[test]
fn retaliation_reads_current_process_null_after_status_and_state_guards() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.owned_mut(actor).unwrap().state.base.flags = 0x4000;
    arena.actor_mut(actor).unwrap().word_c4 = 1;
    host.process_null = None;
    arena
        .react_actor_damage(
            actor,
            callback(20, 0, ActorHandle::from_raw(0)),
            &mut rng,
            &mut host,
        )
        .unwrap();
    assert_eq!(host.process_null_reads.get(), 0);
    arena.actor_mut(actor).unwrap().word_c4 = 3;
    assert_eq!(
        arena.react_actor_damage(
            actor,
            callback(20, 0, ActorHandle::from_raw(0)),
            &mut rng,
            &mut host
        ),
        Err(ActorDamageReactionError::MissingProcessNullHandle(actor))
    );
    assert_eq!(host.process_null_reads.get(), 1);
    assert_eq!(arena.actor(actor).unwrap().health, 1000);

    let process_null = ActorHandle::from_raw(0x0800_0011);
    assert_ne!(process_null, arena.actor_handles().unwrap().null_handle());
    host.process_null = Some(process_null);
    arena.actor_mut(actor).unwrap().byte_7c = 1;
    let before = rng;
    arena
        .react_actor_damage(actor, callback(20, 0, process_null), &mut rng, &mut host)
        .unwrap();
    assert_eq!(rng, before);
    assert!(host.effects.is_empty());
    assert_eq!(arena.actor(actor).unwrap().health, 980);

    //The registry null now denotes a different raw source and must pass the
    //process-null comparison, reaching the actual cooldown/RNG/effect prefix.
    host.clocks = VecDeque::from([5, 17]);
    arena
        .react_actor_damage(
            actor,
            callback(20, 0, ActorHandle::from_raw(0)),
            &mut rng,
            &mut host,
        )
        .unwrap();
    assert_ne!(rng, before);
    assert_eq!(
        host.effects,
        [ActorDamageReactionEffect::Retaliation {
            source: ActorHandle::from_raw(0)
        }]
    );
    assert_eq!(arena.actor(actor).unwrap().health, 960);
}

#[test]
fn signed_cooldown_and_mutable_retaliation_use_reached_health() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.actor_mut(actor).unwrap().byte_7d = 1;
    arena.actor_mut(actor).unwrap().word_80 = u32::MAX;
    host.clocks = VecDeque::from([0, 0]);
    host.retaliation_health = Some(50);
    arena
        .react_actor_damage(
            actor,
            callback(70, 0, ActorHandle::from_raw(0x0800_0001)),
            &mut rng,
            &mut host,
        )
        .unwrap();
    assert!(matches!(
        host.effects.as_slice(),
        [
            ActorDamageReactionEffect::Retaliation { .. },
            ActorDamageReactionEffect::Lethal { .. }
        ]
    ));
    assert_eq!(arena.actor(actor).unwrap().health, 50); //required lethal service owns transition
}

#[test]
fn healing_caps_signed_dword_result_then_narrows_even_after_overflow() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.owned_mut(actor).unwrap().state.base.flags = 0x4000;
    arena.actor_mut(actor).unwrap().health = 900;
    arena
        .react_actor_damage(actor, callback(-500, 0, null(&arena)), &mut rng, &mut host)
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().health, 1000);
    arena
        .react_actor_damage(
            actor,
            callback(i32::MIN, 0, null(&arena)),
            &mut rng,
            &mut host,
        )
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().health, 1000); //wrapped negativeDWORD keeps its lowWORD
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0);
}

#[test]
fn network_damage_notice_uses_owner_high_bits_and_channel_kind() {
    for (channel, kind) in [(6, 1), (8, 1), (7, 10)] {
        let (mut arena, actor, mut host, mut rng) = setup();
        arena.owned_mut(actor).unwrap().state.base.flags = 0x10;
        host.network = true;
        arena
            .react_actor_damage(
                actor,
                callback(5, channel, ActorHandle::from_raw((19 << 27) | 0x0400_ffff)),
                &mut rng,
                &mut host,
            )
            .unwrap();
        assert_eq!(
            host.effects,
            [ActorDamageReactionEffect::NetworkDamageNotice {
                kind,
                local_player: 7,
                source_owner: 19
            }]
        );
        assert_eq!(arena.actor(actor).unwrap().health, 995);
    }
}

#[test]
fn lethal_callback_mutation_can_suppress_network_tail() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.owned_mut(actor).unwrap().state.base.flags = 0x10;
    host.network = true;
    host.clear_player_on_lethal = true;
    let cb = callback(1000, 0, null(&arena));
    arena
        .react_actor_damage(actor, cb, &mut rng, &mut host)
        .unwrap();
    assert_eq!(
        host.effects,
        [ActorDamageReactionEffect::Lethal { callback: cb }]
    );
    assert_eq!(host.events, ["network", "effect"]);
}

#[test]
fn recent_network_death_source_uses_unsigned_tick_delta_and_strict_200_gate() {
    for (elapsed, owner) in [(199, 21), (200, 0), (u32::MAX, 0)] {
        let (mut arena, actor, mut host, mut rng) = setup();
        arena.owned_mut(actor).unwrap().state.base.flags = 0x10;
        host.network = true;
        host.recent = Some(((21 << 11) | 0x77, 10));
        host.clocks = VecDeque::from([10u32.wrapping_add(elapsed)]);
        arena
            .react_actor_damage(actor, callback(1001, 0, null(&arena)), &mut rng, &mut host)
            .unwrap();
        assert_eq!(
            host.effects[1],
            ActorDamageReactionEffect::NetworkDeathNotice {
                incoming_damage_low_word: 1001,
                local_player: 7,
                source_owner: owner
            }
        );
        let mut events = vec![
            "network",
            "effect",
            "network",
            "recent",
            "clock",
            "recent_tick",
        ];
        if elapsed < 200 {
            events.push("recent_owner");
        }
        events.extend(["player", "effect"]);
        assert_eq!(host.events, events);
    }
}

#[test]
fn recent_source_timestamp_is_read_after_the_clock_query() {
    let (mut arena, actor, mut host, mut rng) = setup();
    arena.owned_mut(actor).unwrap().state.base.flags = 0x10;
    host.network = true;
    host.recent = Some((21 << 11, 0));
    host.recent_tick_on_clock = Some(500);
    host.clocks = VecDeque::from([500]);
    arena
        .react_actor_damage(actor, callback(1001, 0, null(&arena)), &mut rng, &mut host)
        .unwrap();
    assert_eq!(
        host.effects[1],
        ActorDamageReactionEffect::NetworkDeathNotice {
            incoming_damage_low_word: 1001,
            local_player: 7,
            source_owner: 21,
        }
    );
}

#[test]
fn status_uses_fields_changed_by_the_exclusion_callback() {
    let (mut arena, actor, mut host, mut rng) = setup();
    host.status_on_exclusion = Some((300, 40));
    arena
        .apply_channel_two_status(actor, 20, &mut rng, &mut host)
        .unwrap();
    let state = arena.actor(actor).unwrap();
    assert_eq!((state.word_cc, state.word_ce), (300, 60));
    assert_eq!(host.events, ["excludes"]);
}
