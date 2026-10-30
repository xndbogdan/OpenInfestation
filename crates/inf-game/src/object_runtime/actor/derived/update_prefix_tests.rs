use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, Selector12GrantContext,
};

const SINE: [u16; 4096] = [0; 4096];
const PLANE: ActorWaterPlane = ActorWaterPlane {
    height_fixed: 0,
    coordinate_shift: 8,
};

fn fixture(class: FamilyUpdateClass) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap();
    let request = DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([10, 20, 30]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        },
        world_mask_limits: [3, 3],
    };
    let grants = inf_formats::GrantTable {
        virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let mut rng = 0;
    let actor = match class {
        FamilyUpdateClass::Family4236d0 => arena.allocate_family4236d0_actor(request, &mut rng),
        FamilyUpdateClass::Selector10 => arena.allocate_selector10_actor(request, &mut rng),
        FamilyUpdateClass::Selector12 => arena.allocate_selector12_actor(
            request,
            &mut rng,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        ),
        FamilyUpdateClass::Selector36 => arena.allocate_selector36_actor(request, &mut rng),
        FamilyUpdateClass::Selector35 => arena.allocate_selector35_actor(request, &mut rng),
        FamilyUpdateClass::Selector34 => arena.allocate_selector34_actor(request, &mut rng),
    }
    .unwrap();
    arena.actor_mut(actor).unwrap().maximum_health = Some(1000);
    arena.owned_mut(actor).unwrap().state.base.word_28 = 200;
    assert_eq!(arena.family_update_class(actor).unwrap(), class);
    (arena, actor)
}

fn request(microseconds: u32) -> FamilyUpdateRequest {
    FamilyUpdateRequest {
        microseconds,
        water_plane: Some(PLANE),
        status_load_factor: Some(0),
    }
}

#[derive(Default)]
struct Host {
    effects: Vec<FamilyUpdateEffect>,
    /// Snapshots establish the field writes reached before a failed callback.
    snapshots: Vec<(i32, u16, u16, u16, u16, u16)>,
    fail: Option<FamilyUpdateEffect>,
    visual_replacement: Option<(u16, u16)>,
    visual_delay: Option<i32>,
    visual_timer_17a: Option<u16>,
    fail_status_trail: bool,
    rng: u32,
}

impl FamilyUpdateHost for Host {
    type Error = &'static str;

    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0x1234))
    }

    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(crate::name_entry::name_entry_rng_next(&mut self.rng))
    }

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        let shared = arena.actor(actor).unwrap();
        let derived = arena.derived_actor(actor).unwrap();
        self.snapshots.push((
            derived.word_144,
            shared.word_c8,
            shared.word_cc,
            shared.word_ce,
            shared.health,
            derived.word_17a,
        ));
        self.effects.push(effect);
        if self.fail == Some(effect) {
            return Err("unowned native callback");
        }
        if matches!(effect, FamilyUpdateEffect::StatusTrail { .. }) {
            if self.fail_status_trail {
                return Err("unowned native callback");
            }
            if let Some((strength, remaining)) = self.visual_replacement {
                let state = arena.actor_mut(actor).unwrap();
                state.word_cc = strength;
                state.word_ce = remaining;
            }
            if let Some(delay) = self.visual_delay {
                arena.derived_actor_mut(actor).unwrap().word_144 = delay;
            }
            if let Some(timer) = self.visual_timer_17a {
                arena.derived_actor_mut(actor).unwrap().word_17a = timer;
            }
        }
        Ok(())
    }
}

#[test]
fn above_water_and_zero_delta_never_discard_unknown_allocation_word() {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector10);
    arena.derived_actor_mut(actor).unwrap().word_144 = -1;
    arena.actor_mut(actor).unwrap().word_c8 = 100;
    let mut host = Host::default();
    for time in [0, 40_000] {
        assert_eq!(
            arena.update_family_prefix(actor, request(time), &mut host),
            Err(FamilyUpdateError::Uninitialized {
                actor,
                offset: 0x230,
            })
        );
        assert_eq!(arena.family429e20_actor(actor).unwrap().word_230, None);
        assert_eq!(arena.actor(actor).unwrap().word_c8, 100);
        assert_eq!(arena.derived_actor(actor).unwrap().word_144, -1);
    }
    assert!(host.effects.is_empty());
}

struct WaterSourceHost {
    inner: Host,
    source: Option<ActorHandle>,
    reads: std::cell::Cell<usize>,
}
impl FamilyUpdateHost for WaterSourceHost {
    type Error = &'static str;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.reads.set(self.reads.get() + 1);
        self.source
    }
    fn random(&mut self) -> Result<u32, Self::Error> {
        self.inner.random()
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        self.inner.effect(arena, actor, effect)
    }
}

#[test]
fn drowning_reads_current_process_null_only_after_water_timer_and_health_guards() {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector36);
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([0, (-201_i16) as u16, 0]);
    arena.family429e20_actor_mut(actor).unwrap().word_230 = Some(4000);
    let mut host = WaterSourceHost {
        inner: Host::default(),
        source: None,
        reads: std::cell::Cell::new(0),
    };
    arena
        .tick_actor_water(actor, 1024, PLANE, &mut host)
        .unwrap();
    assert_eq!(host.reads.get(), 0);
    assert_eq!(
        arena.family429e20_actor(actor).unwrap().word_230,
        Some(4001)
    );
    assert_eq!(
        arena.tick_actor_water(actor, 0, PLANE, &mut host),
        Err(FamilyUpdateError::MissingProcessNullHandle(actor))
    );
    assert_eq!(host.reads.get(), 1);
    assert!(host.inner.effects.is_empty());
    assert_eq!(
        arena.family429e20_actor(actor).unwrap().word_230,
        Some(4001)
    );
    let source = ActorHandle::from_raw(0x8765_4321);
    assert_ne!(source, arena.actor_handles().unwrap().null_handle());
    host.source = Some(source);
    arena.tick_actor_water(actor, 0, PLANE, &mut host).unwrap();
    assert_eq!(
        host.inner.effects,
        [FamilyUpdateEffect::Drowning(ActorWaterDamage {
            amount: 1000,
            channel: 5,
            source,
            microseconds: 0,
            period_microseconds: 4_000_000,
        })]
    );
    assert_eq!(host.reads.get(), 2);
    arena.actor_mut(actor).unwrap().maximum_health = None;
    assert!(matches!(
        arena.tick_actor_water(actor, 0, PLANE, &mut host),
        Err(FamilyUpdateError::Uninitialized { offset: 0xa2, .. })
    ));
    assert_eq!(host.reads.get(), 2);
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([0, 10, 0]);
    host.source = None;
    arena.tick_actor_water(actor, 0, PLANE, &mut host).unwrap();
    assert_eq!(host.reads.get(), 2);
}

#[test]
fn water_has_strict_half_size_threshold_masked_shift_and_word_wrap() {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector36);
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.word_28 = 3;
    base.position_words = Some([0, (-1_i16) as u16, 0]);
    arena.family429e20_actor_mut(actor).unwrap().word_230 = Some(1);
    let mut host = Host::default();
    // Exactly -floor(3/2) is not deeply submerged. The above-water branch
    // subtracts two from one without a floor at zero.
    arena
        .tick_actor_water(actor, 1024, PLANE, &mut host)
        .unwrap();
    assert_eq!(
        arena.family429e20_actor(actor).unwrap().word_230,
        Some(0xffff)
    );
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([0, (-2_i16) as u16, 0]);
    arena.family429e20_actor_mut(actor).unwrap().word_230 = Some(4000);
    arena
        .tick_actor_water(
            actor,
            1024,
            ActorWaterPlane {
                height_fixed: 0x0100_0000,
                coordinate_shift: 40, // shift 8; low WORD of 0x10000 is zero
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(
        arena.family429e20_actor(actor).unwrap().word_230,
        Some(4001)
    );
    assert!(host.effects.is_empty());
    arena
        .tick_actor_water(actor, 1024, PLANE, &mut host)
        .unwrap();
    assert_eq!(
        host.effects,
        [FamilyUpdateEffect::Drowning(ActorWaterDamage {
            amount: 1000,
            channel: 5,
            source: ActorHandle::from_raw(0x1234),
            microseconds: 1024,
            period_microseconds: 4_000_000,
        })]
    );
    assert_eq!(
        arena.family429e20_actor(actor).unwrap().word_230,
        Some(4001)
    );
}

#[test]
fn shallow_timer_increment_wraps_and_damage_failure_preserves_prior_word() {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector12);
    arena.owned_mut(actor).unwrap().state.base.word_28 = 0;
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([0, 0xffff, 0]);
    arena.family429e20_actor_mut(actor).unwrap().word_230 = Some(4000);
    let mut host = Host::default();
    arena
        .tick_actor_water(actor, 65_535 << 10, PLANE, &mut host)
        .unwrap();
    assert_eq!(
        arena.family429e20_actor(actor).unwrap().word_230,
        Some(3999)
    );
    arena.family429e20_actor_mut(actor).unwrap().word_230 = Some(u16::MAX);
    let effect = FamilyUpdateEffect::Drowning(ActorWaterDamage {
        amount: 1000,
        channel: 5,
        source: ActorHandle::from_raw(0x1234),
        microseconds: 40_000,
        period_microseconds: 4_000_000,
    });
    host.fail = Some(effect);
    assert_eq!(
        arena.tick_actor_water(actor, 40_000, PLANE, &mut host),
        Err(FamilyUpdateError::Host {
            effect,
            error: "unowned native callback",
        })
    );
    assert_eq!(
        arena.family429e20_actor(actor).unwrap().word_230,
        Some(u16::MAX)
    );
}

#[test]
fn water_reads_maximum_health_only_at_actual_damage_branch() {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector10);
    arena.owned_mut(actor).unwrap().state.base.word_28 = 0;
    arena.actor_mut(actor).unwrap().maximum_health = None;
    arena.family429e20_actor_mut(actor).unwrap().word_230 = Some(0);
    let mut host = Host::default();
    arena.tick_actor_water(actor, 0, PLANE, &mut host).unwrap();
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([0, 0xffff, 0]);
    arena.family429e20_actor_mut(actor).unwrap().word_230 = Some(4001);
    assert_eq!(
        arena.tick_actor_water(actor, 0, PLANE, &mut host),
        Err(FamilyUpdateError::Uninitialized {
            actor,
            offset: 0xa2,
        })
    );
    assert!(host.effects.is_empty());
}

#[test]
fn negative_activation_delays_preserve_each_class_status_order() {
    for class in [
        FamilyUpdateClass::Family4236d0,
        FamilyUpdateClass::Selector10,
        FamilyUpdateClass::Selector12,
        FamilyUpdateClass::Selector36,
        FamilyUpdateClass::Selector35,
    ] {
        let (mut arena, actor) = fixture(class);
        if class != FamilyUpdateClass::Selector35 {
            arena.family429e20_actor_mut(actor).unwrap().word_230 = Some(0);
        }
        arena.derived_actor_mut(actor).unwrap().word_144 = -1000;
        arena.actor_mut(actor).unwrap().word_c8 = 100;
        let mut host = Host::default();
        assert_eq!(
            arena.update_family_prefix(
                actor,
                FamilyUpdateRequest {
                    microseconds: 1600,
                    water_plane: (class != FamilyUpdateClass::Selector35).then_some(PLANE),
                    status_load_factor: Some(0),
                },
                &mut host,
            ),
            Ok(FamilyUpdateProgress::Returned)
        );
        assert_eq!(
            arena.actor(actor).unwrap().word_c8,
            // 00424830 ticks the status before the delay test.
            if matches!(
                class,
                FamilyUpdateClass::Family4236d0 | FamilyUpdateClass::Selector10
            ) {
                50
            } else {
                100
            }
        );
        assert_eq!(arena.derived_actor(actor).unwrap().word_144, -1000);
        assert!(host.effects.is_empty());
    }
}

#[test]
fn delayed_physics_failure_retains_signed_delay_write_before_callback() {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector35);
    arena.derived_actor_mut(actor).unwrap().word_144 = 5;
    arena.actor_mut(actor).unwrap().word_c8 = 100;
    let effect = FamilyUpdateEffect::DelayedPhysics {
        microseconds: u32::MAX,
        mask: 0x1f,
    };
    let mut host = Host {
        fail: Some(effect),
        ..Default::default()
    };
    assert_eq!(
        arena.update_family_prefix(actor, request(u32::MAX), &mut host),
        Err(FamilyUpdateError::Host {
            effect,
            error: "unowned native callback",
        })
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_144, 6);
    assert_eq!(arena.actor(actor).unwrap().word_c8, 100);
    assert_eq!(host.snapshots[0].0, 6);
}

#[test]
fn positive_delay_expires_then_returns_without_running_zero_delay_tail() {
    for class in [FamilyUpdateClass::Selector12, FamilyUpdateClass::Selector36] {
        let (mut arena, actor) = fixture(class);
        arena.family429e20_actor_mut(actor).unwrap().word_230 = Some(0);
        arena.derived_actor_mut(actor).unwrap().word_144 = 10;
        arena.actor_mut(actor).unwrap().word_c8 = 100;
        let mut host = Host::default();
        assert_eq!(
            arena.update_family_prefix(actor, request(100), &mut host),
            Ok(FamilyUpdateProgress::Returned)
        );
        assert_eq!(arena.derived_actor(actor).unwrap().word_144, 0);
        assert_eq!(arena.actor(actor).unwrap().word_c8, 100);
        assert_eq!(
            host.effects,
            [FamilyUpdateEffect::DelayedPhysics {
                microseconds: 100,
                mask: 0x1f
            }]
        );
    }
}

#[test]
fn selector10_rereads_delay_after_status_but_other_classes_already_passed_guard() {
    for class in [
        FamilyUpdateClass::Selector10,
        FamilyUpdateClass::Selector12,
        FamilyUpdateClass::Selector36,
        FamilyUpdateClass::Selector35,
    ] {
        let (mut arena, actor) = fixture(class);
        if class != FamilyUpdateClass::Selector35 {
            arena.family429e20_actor_mut(actor).unwrap().word_230 = Some(0);
        }
        arena.actor_mut(actor).unwrap().word_cc = 100;
        let mut host = Host {
            visual_delay: Some(-1),
            ..Default::default()
        };
        let expected = if class == FamilyUpdateClass::Selector10 {
            FamilyUpdateProgress::Returned
        } else {
            FamilyUpdateProgress::BeforeBehaviorDispatch { class }
        };
        assert_eq!(
            arena.update_family_prefix(actor, request(0), &mut host),
            Ok(expected)
        );
        assert_eq!(arena.derived_actor(actor).unwrap().word_144, -1);
        assert!(matches!(
            host.effects.as_slice(),
            [FamilyUpdateEffect::StatusTrail { .. }]
        ));
    }
}

#[test]
fn weapon_and_word17a_prefixes_stop_at_actual_class_dispatch_boundary() {
    for class in [
        FamilyUpdateClass::Selector10,
        FamilyUpdateClass::Selector12,
        FamilyUpdateClass::Selector36,
        FamilyUpdateClass::Selector35,
    ] {
        let (mut arena, actor) = fixture(class);
        if class != FamilyUpdateClass::Selector35 {
            arena.family429e20_actor_mut(actor).unwrap().word_230 = Some(0);
        }
        arena.actor_mut(actor).unwrap().word_cc = 100;
        let state = arena.derived_actor_mut(actor).unwrap();
        state.word_17a = 30;
        state.behavior_140 = crate::object_runtime::actor::behavior::build_actor_behavior(1, 0, 1);
        let mut host = Host {
            visual_timer_17a: Some(3),
            ..Default::default()
        };
        assert_eq!(
            arena.update_family_prefix(actor, request(4096), &mut host),
            Ok(FamilyUpdateProgress::BeforeBehaviorDispatch { class })
        );
        assert_eq!(
            arena.derived_actor(actor).unwrap().word_17a,
            if class == FamilyUpdateClass::Selector10 {
                0
            } else {
                3
            }
        );
        assert!(matches!(
            host.effects[0],
            FamilyUpdateEffect::StatusTrail { .. }
        ));
        if class == FamilyUpdateClass::Selector36 {
            assert_eq!(host.effects.len(), 1);
        } else {
            assert_eq!(
                host.effects[1],
                FamilyUpdateEffect::WeaponTick { microseconds: 4096 }
            );
            assert_eq!(host.snapshots[1].2, 96);
        }
        assert_eq!(arena.family428650_actor(actor).unwrap().word_184, -1);
        assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
    }
}

#[test]
fn status_visual_failure_keeps_prior_c8_decay_and_no_later_damage() {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector35);
    let state = arena.actor_mut(actor).unwrap();
    state.word_c8 = 100;
    state.word_cc = 100;
    state.word_ce = 20;
    let mut host = Host {
        fail_status_trail: true,
        ..Default::default()
    };
    assert!(matches!(
        arena.tick_actor_status(actor, 1600, Some(0), &mut host),
        Err(FamilyUpdateError::Host {
            effect: FamilyUpdateEffect::StatusTrail { .. },
            error: "unowned native callback"
        })
    ));
    let state = arena.actor(actor).unwrap();
    assert_eq!(
        (state.word_c8, state.word_cc, state.word_ce, state.health),
        (50, 100, 20, 1000)
    );
    assert_eq!(host.snapshots[0].1, 50);
}

#[test]
fn status_rereads_visual_callback_fields_and_preserves_signed_product_surprise() {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector35);
    arena.actor_mut(actor).unwrap().word_ce = 1;
    let mut host = Host {
        visual_replacement: Some((20, 0)),
        ..Default::default()
    };
    arena
        .tick_actor_status(actor, 10_240, Some(0), &mut host)
        .unwrap();
    assert_eq!(arena.actor(actor).unwrap().word_cc, 10);
    assert_eq!(arena.actor(actor).unwrap().health, 1000);
    let state = arena.actor_mut(actor).unwrap();
    state.word_cc = 0x8000;
    state.word_ce = 100;
    host.visual_replacement = None;
    // DWORD product 0x40000000 >> 15 produces low WORD 0x8000, which the
    // retail MOVSX treats as -32768. Native subtraction increases both WORDs.
    arena
        .tick_actor_status(actor, 1_048_576, Some(0), &mut host)
        .unwrap();
    let state = arena.actor(actor).unwrap();
    assert_eq!((state.word_ce, state.health), (32_868, 33_768));
}

#[test]
fn status_expiry_writes_before_retirement_or_unknown_maximum_health() {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector35);
    let state = arena.actor_mut(actor).unwrap();
    state.word_cc = 1000;
    state.word_ce = 1;
    state.health = 1;
    state.maximum_health = Some(500);
    let mut host = Host {
        fail: Some(FamilyUpdateEffect::Retire),
        ..Default::default()
    };
    assert_eq!(
        arena.tick_actor_status(actor, 1024, Some(0), &mut host),
        Err(FamilyUpdateError::Host {
            effect: FamilyUpdateEffect::Retire,
            error: "unowned native callback",
        })
    );
    assert_eq!(host.snapshots[1].2, 2000);
    assert_eq!(host.snapshots[1].3, 0);
    assert_eq!(host.snapshots[1].4, 1);
    let state = arena.actor_mut(actor).unwrap();
    state.word_ce = 1;
    state.maximum_health = None;
    assert_eq!(
        arena.tick_actor_status(actor, 0, Some(0), &mut Host::default()),
        Err(FamilyUpdateError::Uninitialized {
            actor,
            offset: 0xa2
        })
    );
    assert_eq!(arena.actor(actor).unwrap().word_ce, 0);
    assert_eq!(arena.actor(actor).unwrap().health, 1);
    let state = arena.actor_mut(actor).unwrap();
    state.word_ce = 1;
    state.word_cc = 0;
    state.maximum_health = Some(0);
    assert_eq!(
        arena.tick_actor_status(actor, 0, Some(0), &mut Host::default()),
        Err(FamilyUpdateError::StatusDivisionByZero(actor))
    );
    assert_eq!(arena.actor(actor).unwrap().word_ce, 0);
}

#[path = "update_prefix/status_visual_tests.rs"]
mod status_visual_tests;
