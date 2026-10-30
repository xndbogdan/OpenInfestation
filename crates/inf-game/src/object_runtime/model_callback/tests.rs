use super::*;
use crate::{
    frontend_camera::AnimationClock,
    frontend_screen::FrontendProcessState,
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        ActorTerrainSurface, CameraSettings, DerivedActorConstructionRequest, Family4236d0State,
        Family429e20State, ObjectClassKind, Selector11State, Selector12GrantContext,
        Selector12State, Selector36State, Selector60State, Selector77State,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    parse_grant_table, parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_ENTRY_FIXED_BYTES,
};
use sha2::{Digest, Sha256};
use std::path::Path;

mod combat_auxiliary;

static UNUSED_SINE: [u16; 4096] = [0; 4096];

fn executable_bytes() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe")).unwrap()
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn allocation() -> ActorConstructionRequest<'static> {
    // Actual shared H12 NULL branch does not read XYZ or the sine table.
    ActorConstructionRequest {
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
    }
}

fn derived_request() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: allocation(),
        world_mask_limits: [3, 3],
    }
}

fn vehicle(arena: &mut ObjectArena) -> ObjectId {
    // Declared parsed flat H12, not an original terrain mutation. The genuine
    // constructor's ground >= plane arm returns before its wave/sine phase.
    let raw = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    let span = ByteSpan::new(0, raw.len());
    let terrain = parse_terrain_bank(
        &raw,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap()
    .entries
    .remove(0);
    let terrain = TerrainRuntime::new(
        &terrain,
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    let mut request = allocation();
    request.allocation_position = Some([768, 600, 768]);
    request.terrain.terrain = Some(ActorTerrainSurface {
        terrain: &terrain,
        plane_height_fixed: terrain.plane_base(),
    });
    arena
        .allocate_selector2_actor(
            DerivedActorConstructionRequest {
                actor: request,
                world_mask_limits: [3, 3],
            },
            &mut 17,
        )
        .unwrap()
}

// Changes below explicitly supply current scalar/owner domains. They do not
// claim native writers or infer values from unwritten constructor backing.
fn actor_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut ActorState {
    let ObjectClass::Actor(actor) = &mut arena.owned_mut(object).unwrap().state.class else {
        panic!("actual actor constructor")
    };
    actor
}

fn derived_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut DerivedActorState {
    let ActorExtension::Derived(derived) = &mut actor_mut(arena, object).extension else {
        panic!("actual derived constructor")
    };
    derived
}

fn family429_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut Family429e20State {
    let DerivedActorClass::Family428650(family) = &mut derived_mut(arena, object).class else {
        panic!("actual family428650 constructor")
    };
    let Family428650Class::Family429e20(family) = &mut family.class else {
        panic!("actual family429e20 constructor")
    };
    family
}

fn family423_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut Family4236d0State {
    let Family429e20Class::Family4236d0(family) = &mut family429_mut(arena, object).class else {
        panic!("actual family4236d0 constructor")
    };
    family
}

fn vehicle_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut Selector2State {
    let DerivedActorClass::Selector2(state) = &mut derived_mut(arena, object).class else {
        panic!("actual selector2 constructor")
    };
    state
}

fn selector11_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut Selector11State {
    let DerivedActorClass::Selector11(state) = &mut derived_mut(arena, object).class else {
        panic!("actual selector11 constructor")
    };
    state
}

fn selector12_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut Selector12State {
    let Family429e20Class::Selector12(state) = &mut family429_mut(arena, object).class else {
        panic!("actual selector12 constructor")
    };
    state
}

fn selector36_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut Selector36State {
    let Family4236d0Class::Selector36(state) = &mut family423_mut(arena, object).class else {
        panic!("actual selector36 constructor")
    };
    state
}

fn selector60_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut Selector60State {
    let DerivedActorClass::Selector60(state) = &mut derived_mut(arena, object).class else {
        panic!("actual selector60 constructor")
    };
    state
}

fn selector77_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut Selector77State {
    let DerivedActorClass::Selector77(state) = &mut derived_mut(arena, object).class else {
        panic!("actual selector77 constructor")
    };
    state
}

fn selector30_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut Selector30State {
    let ActorExtension::Selector30(state) = &mut actor_mut(arena, object).extension else {
        panic!("actual selector30 constructor")
    };
    state
}

fn query(
    arena: &ObjectArena,
    object: ObjectId,
    raw: &[u8],
    selector: u32,
) -> Result<u16, ObjectModelCallbackError> {
    ObjectModelCallbacks {
        arena,
        source: object,
        animation: &AnimationClock::default(),
        executable: FrontendExecutable::parse(raw).unwrap(),
    }
    .packed_value(selector)
}

fn values(arena: &ObjectArena, object: ObjectId, raw: &[u8], cases: &[(u32, u16)]) {
    let before = format!("{arena:?}");
    for &(selector, expected) in cases {
        assert_eq!(
            query(arena, object, raw, selector),
            Ok(expected),
            "selector {selector:#x}"
        );
    }
    // Includes actual registry/cache, all actors, lists, storage and RNG owners.
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn zero_reads_actual_process_clock_fresh_before_foreign_or_dead_source() {
    let raw = executable_bytes();
    let mut arena = arena();
    let foreign = self::arena().allocate(ObjectClassKind::Base);
    let process = FrontendProcessState::default();
    let callbacks = ObjectModelCallbacks {
        arena: &arena,
        source: foreign,
        animation: &process,
        executable: FrontendExecutable::parse(&raw).unwrap(),
    };
    assert_eq!(callbacks.packed_value(0), Ok(0));
    process.advance_animation(1_500_000_000);
    assert_eq!(callbacks.packed_value(0), Ok(75_000_u32 as u16));
    assert!(matches!(
        callbacks.packed_value(1),
        Err(ObjectModelCallbackError::Object(_))
    ));
    let dead = arena.allocate(ObjectClassKind::Base);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: dead,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let mut clock = AnimationClock::default();
    clock.advance(140_000);
    let callbacks = ObjectModelCallbacks {
        arena: &arena,
        source: dead,
        animation: &clock,
        executable: FrontendExecutable::parse(&raw).unwrap(),
    };
    assert_eq!(callbacks.packed_value(0), Ok(7));
    assert!(matches!(
        callbacks.packed_value(1),
        Err(ObjectModelCallbackError::Object(_))
    ));
    assert_eq!(clock.value(), 7);
}

#[test]
fn common_tables_ignore_unavailable_payload_and_numeric_actor_identity() {
    let raw = executable_bytes();
    let mut arena = arena();
    for class in [
        ObjectClassKind::Base,
        ObjectClassKind::VisualBase,
        ObjectClassKind::RotatingVisualBase,
        ObjectClassKind::TerrainLight,
        ObjectClassKind::RouteNode,
        ObjectClassKind::Selector86,
        ObjectClassKind::MapAnnotation,
    ] {
        let object = arena.allocate(class);
        values(&arena, object, &raw, &[(1, 0), (9, 0), (u32::MAX, 0)]);
    }
    let actors = [
        arena.allocate_actor(allocation()).unwrap(),
        arena
            .allocate_selector16(
                allocation(),
                CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
            )
            .unwrap(),
        arena.allocate_selector88(allocation()).unwrap(),
        arena.allocate_selector90(allocation()).unwrap(),
        arena.allocate_selector93(allocation()).unwrap(),
    ];
    for object in actors {
        actor_mut(&mut arena, object).word_b4 = 2;
        arena.owned_mut(object).unwrap().state.base.resource_word = 2;
        values(
            &arena,
            object,
            &raw,
            &[(1, 0), (3, 0), (9, 0), (u32::MAX, 0)],
        );
    }
}

#[test]
fn derived_family_getters_return_selector_lowword_and_only_nine_reads_178() {
    let raw = executable_bytes();
    let mut arena = arena();
    let objects = [
        arena
            .allocate_derived_actor(derived_request(), &mut 17)
            .unwrap(),
        arena
            .allocate_family428650_actor(derived_request(), &mut 17)
            .unwrap(),
        arena
            .allocate_family429e20_actor(derived_request(), &mut 17)
            .unwrap(),
        arena
            .allocate_family4236d0_actor(derived_request(), &mut 17)
            .unwrap(),
        arena
            .allocate_selector10_actor(derived_request(), &mut 17)
            .unwrap(),
        arena
            .allocate_selector34_actor(derived_request(), &mut 17)
            .unwrap(),
        arena
            .allocate_selector35_actor(derived_request(), &mut 17)
            .unwrap(),
    ];
    for object in objects {
        actor_mut(&mut arena, object).word_b4 = 16;
        arena.owned_mut(object).unwrap().state.base.resource_word = 16;
        derived_mut(&mut arena, object).word_178 = 0x9abc;
        values(
            &arena,
            object,
            &raw,
            &[(1, 1), (9, 0x9abc), (0x10009, 9), (u32::MAX, 0xffff)],
        );
    }
}

#[test]
fn selector11_health_fraction_wraps_signed_dword_and_missing_maximum_is_lazy() {
    let raw = executable_bytes();
    let mut arena = arena();
    let object = arena
        .allocate_selector11_actor(derived_request(), &mut 17)
        .unwrap();
    selector11_mut(&mut arena, object).word_188 = 0x1234_abcd;
    selector11_mut(&mut arena, object).word_18c = 0x9876_5432;
    actor_mut(&mut arena, object).maximum_health = None;
    values(
        &arena,
        object,
        &raw,
        &[(3, 0xabcd), (1, 0x5432), (9, 0x5432)],
    );
    let before = format!("{arena:?}");
    assert_eq!(
        query(&arena, object, &raw, 2),
        Err(ObjectModelCallbackError::RequiredField {
            object,
            offset: 0xa2
        })
    );
    assert_eq!(format!("{arena:?}"), before);
    for (health, maximum, expected) in [
        (50, 100, 32443),
        (0, 65535, 65535),
        (65535, 65535, 0),
        (1, 0, 0),
    ] {
        let actor = actor_mut(&mut arena, object);
        actor.health = health;
        actor.maximum_health = Some(maximum);
        values(&arena, object, &raw, &[(2, expected)]);
    }
}

#[test]
fn selector36_reads_half_dword_aliases_and_bounds_negative_direct_selectors() {
    let raw = executable_bytes();
    let mut arena = arena();
    let object = arena
        .allocate_selector36_actor(derived_request(), &mut 17)
        .unwrap();
    let state = selector36_mut(&mut arena, object);
    state.word_268 = 0x2222_1111;
    state.word_26c = 0x4444_3333;
    state.word_270 = 0x6666_5555;
    derived_mut(&mut arena, object).word_178 = 0x8888;
    family423_mut(&mut arena, object).word_266 = 0x7777;
    values(
        &arena,
        object,
        &raw,
        &[
            (1, 0x1111),
            (2, 0x2222),
            (3, 0x3333),
            (4, 0x4444),
            (5, 0x5555),
            (6, 0x6666),
            (7, 7),
            (9, 0x8888),
            (0x10009, 9),
        ],
    );
    for selector in [u32::MAX, 0x8000_0000] {
        let before = format!("{arena:?}");
        assert_eq!(
            query(&arena, object, &raw, selector),
            Err(ObjectModelCallbackError::Selector36Backing {
                object,
                selector: selector as i32
            })
        );
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn selector60_and_77_keep_word_sign_negation_and_unsigned_selector_gates() {
    let raw = executable_bytes();
    let mut arena = arena();
    let sixty = arena
        .allocate_selector60_actor(derived_request(), &mut 17)
        .unwrap();
    selector60_mut(&mut arena, sixty).word_1a0 = 0xabcd;
    selector60_mut(&mut arena, sixty).word_19e = 0x9876;
    values(
        &arena,
        sixty,
        &raw,
        &[(6, 0xabcd), (7, 0x9876), (9, 0), (0x10006, 0)],
    );
    let seventyseven = arena
        .allocate_selector77_actor(derived_request(), &mut 17)
        .unwrap();
    selector77_mut(&mut arena, seventyseven).words_184_190 =
        [0x7777, 0x8000, 0xfffe, 0x7fff, 0x8001, 3, 0xabcd];
    values(
        &arena,
        seventyseven,
        &raw,
        &[
            (1, 0x8000),
            (2, 0xfffe),
            (3, 0x7fff),
            (4, 0x8001),
            (5, 0xfffd),
            (6, 0xabcd),
            (7, 0),
            (u32::MAX, 0),
        ],
    );
}

#[test]
fn selector12_speed_overflow_thresholds_and_current_loader_flag_are_real_owners() {
    let raw = executable_bytes();
    let grants = parse_grant_table(FrontendExecutable::parse(&raw).unwrap()).unwrap();
    let mut arena = arena();
    let object = arena
        .allocate_selector12_actor(
            derived_request(),
            &mut 17,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap();
    selector12_mut(&mut arena, object).byte_29c = 1;
    for (velocity, speed, gate) in [
        ([0, 0, 0], 0, 0x4000),
        ([3, 4, 0], 5, 0x4000),
        ([6, 0, 0], 6, 0x8000),
        ([100, 0, 0], 100, 0x8000),
        ([101, 0, 0], 101, 0xc000),
        ([0x8000; 3], 56754, 0xc000),
    ] {
        actor_mut(&mut arena, object).words_8e_92 = velocity;
        values(&arena, object, &raw, &[(1, speed), (2, gate), (3, 0)]);
    }
    selector12_mut(&mut arena, object).byte_29d = 1;
    arena.world_load.flag_54ece4 = false;
    values(&arena, object, &raw, &[(9, 0)]);
    arena.world_load.flag_54ece4 = true;
    values(&arena, object, &raw, &[(9, 1)]);
    selector12_mut(&mut arena, object).byte_29d = 0;
    values(&arena, object, &raw, &[(9, 0)]);
    actor_mut(&mut arena, object).words_8e_92 = [0; 3];
    selector12_mut(&mut arena, object).byte_29c = 0;
    values(&arena, object, &raw, &[(2, 0)]);
}

#[test]
fn selector30_gate_precedes_selector_and_health_style_arithmetic_wraps() {
    let raw = executable_bytes();
    let mut arena = arena();
    let object = arena.allocate_selector30_actor(allocation()).unwrap();
    selector30_mut(&mut arena, object).word_164 = 1;
    values(&arena, object, &raw, &[(1, 0), (2, 0)]);
    selector30_mut(&mut arena, object).word_164 = 0;
    selector30_mut(&mut arena, object).word_168 = 0x10001;
    values(&arena, object, &raw, &[(1, 0), (2, 1), (9, 0)]);
    selector30_mut(&mut arena, object).word_168 = 1;
    for (word, expected) in [(0, 65535), (1200, 0), (1201, 65482), (65535, 108)] {
        selector30_mut(&mut arena, object).word_17c = word;
        values(&arena, object, &raw, &[(1, expected)]);
    }
}

#[test]
fn vehicle_mode_reads_actual_mapped_word_and_native_wrapping_address() {
    let mut raw = executable_bytes();
    // Same-length supplied executable image, not an edit to original evidence.
    raw[0x0013_057a..0x0013_057c].copy_from_slice(&0xbeef_u16.to_le_bytes());
    let mut arena = arena();
    let object = vehicle(&mut arena);
    vehicle_mut(&mut arena, object).dwords_1e8_1f4[0] = 1;
    values(&arena, object, &raw, &[(1, 0xbeef)]);
    vehicle_mut(&mut arena, object).dwords_1e8_1f4[0] = 0x8000_0001;
    values(&arena, object, &raw, &[(1, 0xbeef)]);
    vehicle_mut(&mut arena, object).dwords_1e8_1f4[0] = 0x10_0000;
    let before = format!("{arena:?}");
    assert!(matches!(
        query(&arena, object, &raw, 1),
        Err(ObjectModelCallbackError::Executable(_))
    ));
    assert_eq!(format!("{arena:?}"), before);
    values(
        &arena,
        object,
        &raw,
        &[(5, 0), (9, 0), (13, 0), (u32::MAX, 0)],
    );
}

#[test]
fn vehicle_sqrt_sign_minimum_and_signed_division_keep_native_dword_rules() {
    let raw = executable_bytes();
    let mut arena = arena();
    let object = vehicle(&mut arena);
    for (mode, value, expected) in [
        (1, 225, 705),
        (1, -225, 64831),
        (2, 144, 336),
        (2, -144, 65200),
        (1, i32::MIN, 0),
        (1, 0, 0),
    ] {
        vehicle_mut(&mut arena, object).dwords_1e8_1f4[0] = mode;
        vehicle_mut(&mut arena, object).dword_244 = value as u32;
        values(&arena, object, &raw, &[(4, expected)]);
    }
    vehicle_mut(&mut arena, object).dwords_1e8_1f4[0] = 3;
    vehicle_mut(&mut arena, object).dwords_220_224[0] = (-19_i32) as u32;
    values(&arena, object, &raw, &[(4, 0xffff)]);
}

#[test]
fn vehicle_scalar_and_missing_1fc_branches_do_not_preflight_other_fields() {
    let raw = executable_bytes();
    let mut arena = arena();
    let object = vehicle(&mut arena);
    derived_mut(&mut arena, object).behavior_140 = None;
    let state = vehicle_mut(&mut arena, object);
    state.dwords_1e8_1f4[1] = 0x1234_abcd;
    state.word_20e = 0x5678;
    state.dwords_210_218[0] = 0x9876_5432;
    values(
        &arena,
        object,
        &raw,
        &[(2, 0xabcd), (6, 0x5678), (7, 0x5432), (5, 0), (9, 0)],
    );
    // 00415BF3: the track phase's low WORD, which construction starts at
    // VehicleTrackPhaseApproximation's zero; an unknown phase stays required.
    values(&arena, object, &raw, &[(3, 0)]);
    vehicle_mut(&mut arena, object).drive.dword_1fc = Some(0x2074_6f20);
    values(&arena, object, &raw, &[(3, 0x6f20)]);
    vehicle_mut(&mut arena, object).drive.dword_1fc = None;
    let before = format!("{arena:?}");
    assert_eq!(
        query(&arena, object, &raw, 3),
        Err(ObjectModelCallbackError::RequiredField {
            object,
            offset: 0x1fc
        })
    );
    assert_eq!(format!("{arena:?}"), before);
    for (flag, expected) in [(0, 0x8001), (1, 0x1000), (2, 0), (0x10001, 0)] {
        vehicle_mut(&mut arena, object).dword_238 = flag;
        values(&arena, object, &raw, &[(8, expected)]);
    }
    for (word, byte, expected) in [
        (0, 0, 0),
        (0, 1, 0),
        (1, 0, 0xffff),
        (1, 1, 0),
        (0x10000, 0, 0xffff),
    ] {
        vehicle_mut(&mut arena, object).dwords_220_224[1] = word;
        vehicle_mut(&mut arena, object).bytes_20a_20c[2] = byte;
        values(&arena, object, &raw, &[(10, expected)]);
    }
}

#[test]
fn both_vehicle_behavior_values_use_signed_byte10_and_retained_row_prefix() {
    let raw = executable_bytes();
    let mut arena = arena();
    let object = vehicle(&mut arena);
    derived_mut(&mut arena, object).behavior_140 = None;
    for selector in [11, 12] {
        assert_eq!(
            query(&arena, object, &raw, selector),
            Err(ObjectModelCallbackError::RequiredVehicleBehavior { object })
        );
    }
    // Supplied current guard mode is consumed by the existing public owner;
    // the callback fixture then supplies only its discriminating current rows.
    derived_mut(&mut arena, object).byte_13d = Some(1);
    assert!(arena.admit_derived_actor(object).unwrap());
    let behavior = derived_mut(&mut arena, object)
        .behavior_140
        .as_mut()
        .unwrap();
    behavior.entries_18 = vec![
        [0, 0x1111_abcd, 0x2222_5678, 0, 0, 0, 0, 0],
        [0, 0x3333_9876, 0x4444_5432, 0, 0, 0, 0, 0],
    ];
    behavior.byte_10 = 0;
    behavior.byte_11 = 1;
    values(&arena, object, &raw, &[(11, 0xabcd), (12, 0x5678)]);
    derived_mut(&mut arena, object)
        .behavior_140
        .as_mut()
        .unwrap()
        .byte_10 = 1;
    values(&arena, object, &raw, &[(11, 0x9876), (12, 0x5432)]);
    for row in [2_u8, 0xff] {
        derived_mut(&mut arena, object)
            .behavior_140
            .as_mut()
            .unwrap()
            .byte_10 = row;
        let before = format!("{arena:?}");
        for selector in [11, 12] {
            assert_eq!(
                query(&arena, object, &raw, selector),
                Err(ObjectModelCallbackError::VehicleBehaviorRow {
                    object,
                    row: row as i8,
                    rows: 2
                })
            );
        }
        assert_eq!(format!("{arena:?}"), before);
    }
    derived_mut(&mut arena, object)
        .behavior_140
        .as_mut()
        .unwrap()
        .entries_18
        .clear();
    derived_mut(&mut arena, object)
        .behavior_140
        .as_mut()
        .unwrap()
        .byte_10 = 0;
    assert_eq!(
        query(&arena, object, &raw, 12),
        Err(ObjectModelCallbackError::VehicleBehaviorRow {
            object,
            row: 0,
            rows: 0
        })
    );
}

#[test]
fn complete_original_callback_bodies_and_authored_packet_mode_bytes_are_guarded() {
    let raw = executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    for (address, length, expected) in [
        (
            0x0047_89b0,
            34,
            "c1a719f0ba43e0f862e14f6e13b0fef8bf3860cb7f283e70e711a2a9dbcb6b98",
        ),
        (
            0x0047_a020,
            6,
            "f0d994779fd80a5f7b93900d3fd895c8f58f622faa06107f7500f1da92444bef",
        ),
        (
            0x0042_60a0,
            19,
            "92bfdfdc4575f0958b999708fe6954d76fb6d72b25b7a02d1ab5391ed24c0f4d",
        ),
        (
            0x0040_15a0,
            68,
            "648c6462edee0c9e350735a1b1304c6e23e8bde22703446d66738ce001caa99f",
        ),
        (
            0x0041_5bc0,
            311,
            "81ad7f75f7194735b4eb521edd63cc28d683084b501d907e1edd14654793ee80",
        ),
        (
            0x0046_8920,
            166,
            "677de836678af62b0e5b668eb60eab6390241dc1bb6ff4b8939d0aa714eb2387",
        ),
        (
            0x0048_b330,
            89,
            "8c6f11778986a63740a0544bcca6385838290bde8409f4138bc2c05ab4ae89d6",
        ),
        (
            0x0048_1e50,
            34,
            "8cfbf06de9b728f075ec97d024fccbf04d223c1f5090bf727ad6eea77f849fb3",
        ),
        (
            0x0048_0e70,
            38,
            "e73c574e30aa75c4bb6ab8304a11628a18a1f7862fd57f5cfa23b94219b4ccfc",
        ),
        (
            0x0040_74a0,
            38,
            "4c750f41382932362490f890f29aa9c7bfdf99756e5af4c26e52b36f3f708470",
        ),
    ] {
        // Strict profile authenticates the canonical PE; these bodies are all
        // in its .text, whose authenticated VA and raw offset are both 1000.
        FrontendExecutable::parse(&raw).unwrap();
        let start = address - 0x0040_0000;
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw[start..start + length])),
            expected
        );
    }
    let mapped = FrontendExecutable::parse(&raw).unwrap();
    assert_eq!(
        mapped.mapped(0x0052_0a44, 12).unwrap().bytes,
        &[0xb0, 0x89, 0x47, 0, 0x50, 0x89, 0x47, 0, 0x70, 0x2d, 0x46, 0]
    );
    assert_eq!(
        mapped.mapped(0x0053_0578, 12).unwrap().bytes,
        &[0, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0]
    );
}

/// 0043BF10, selector 39's +14, over hand-chosen timers that reach every
/// branch: lifecycle for selector 1; inside the last 250 ms of the +160
/// window the signed /250 rise from 0xC000; under 200 ms the signed /200
/// fall from 0xFFFF; otherwise the wrapped 0xC000. Other selectors are 0.
#[test]
fn selector39_target_callback_follows_its_destruction_timer() {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    static SINE: [u16; 4096] = [0; 4096];
    let object = arena
        .allocate_selector39(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0, 0, 0]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        })
        .unwrap();
    let value = |arena: &ObjectArena, selector| {
        let actor = arena.actor(object).unwrap();
        let ActorExtension::Selector39(state) = &actor.extension else {
            unreachable!()
        };
        selector39_value(actor, state, selector)
    };
    assert_eq!(value(&arena, 1), 0); // fresh lifecycle 3
    arena.actor_mut(object).unwrap().word_c4 = 2;
    assert_eq!(value(&arena, 1), 1);
    assert_eq!(value(&arena, 0), 0);
    assert_eq!(value(&arena, 3), 0);
    let set = |arena: &mut ObjectArena, seconds: u8, timer: u32| {
        let ActorExtension::Selector39(state) = &mut arena.actor_mut(object).unwrap().extension
        else {
            unreachable!()
        };
        state.byte_160 = seconds;
        state.intermediate.word_150 = timer;
    };
    // A 30 s window: 29.900 s rises 150 ms past 29.750 s.
    set(&mut arena, 30, 29_900_000);
    assert_eq!(value(&arena, 2), (0xc000 + 150 * 16383 / 250) as u16);
    set(&mut arena, 30, 29_760_000);
    assert_eq!(value(&arena, 2), (0xc000 + 10 * 16383 / 250) as u16);
    // Under 200 ms the value falls from 0xFFFF.
    set(&mut arena, 30, 120_000);
    assert_eq!(value(&arena, 2), (0xffff - 120 * 16383 / 200) as u16);
    set(&mut arena, 30, 0);
    assert_eq!(value(&arena, 2), 0xffff);
    // Between them, any nonzero timer wraps to 0xC000.
    set(&mut arena, 30, 5_000_000);
    assert_eq!(value(&arena, 2), 0xc000);
    // A zero window wraps esi-250, so only the lower branches apply.
    set(&mut arena, 0, 100_000);
    assert_eq!(value(&arena, 2), (0xffff - 100 * 16383 / 200) as u16);
}

/// The guided missiles' value 7 follows the morph only across the ground
/// forms' boundary: folding with the old form, unfolding with the new.
#[test]
fn guided_mounts_fold_and_unfold_through_a_crossing_mode_change() {
    let mut arena = arena();
    let object = vehicle(&mut arena);
    let deploy = |arena: &mut ObjectArena, model: u16, modes: [u32; 4], progress210: u32| {
        let state = vehicle_mut(arena, object);
        state.word_20e = model;
        state.dwords_1e8_1f4 = modes;
        state.dwords_210_218[0] = progress210;
        super::guided_mount_deploy(state)
    };
    // [current, progress, requested, previous]
    // Buggy to skimmer: the roof launcher folds as the buggy morphs out.
    assert_eq!(deploy(&mut arena, 3, [1, 0x9000, 5, 0], 0xffff), 0x9000);
    // Then the side launchers unfold as the skimmer morphs in.
    assert_eq!(deploy(&mut arena, 3, [5, 0x2000, 5, 1], 0xffff), 0x2000);
    // A switch still under way keeps the lower of the two.
    assert_eq!(deploy(&mut arena, 3, [5, 0x8000, 5, 2], 0x1000), 0x1000);
    // Settled, the value is +210.
    assert_eq!(deploy(&mut arena, 3, [5, 0xffff, 5, 1], 0xffff), 0xffff);
    // Hovercraft to truck, the other way.
    assert_eq!(deploy(&mut arena, 3, [3, 0x4000, 2, 0], 0xffff), 0x4000);
    assert_eq!(deploy(&mut arena, 3, [2, 0x6000, 2, 3], 0xffff), 0x6000);
    // No crossing: buggy to truck, hovercraft to skimmer, the first form.
    assert_eq!(deploy(&mut arena, 3, [1, 0x9000, 2, 0], 0xffff), 0xffff);
    assert_eq!(deploy(&mut arena, 3, [3, 0x9000, 5, 0], 0xffff), 0xffff);
    assert_eq!(deploy(&mut arena, 3, [4, 0x2000, 4, 3], 0xffff), 0xffff);
    assert_eq!(deploy(&mut arena, 3, [1, 0x2000, 1, 0], 0xffff), 0xffff);
    // Other weapons keep retail's value.
    assert_eq!(deploy(&mut arena, 7, [1, 0x9000, 5, 0], 0xffff), 0xffff);
    assert_eq!(deploy(&mut arena, 1, [5, 0x2000, 5, 1], 0xabcd), 0xabcd);
}

/// The flamethrower's flame (selector 76, table 00521A40) answers through its
/// own +14, 004324E0: query 1 is 0x10000 less its growth +70, as a WORD;
/// every other query comes back as asked.
#[test]
fn the_flame_answers_through_004324e0() {
    let raw = executable_bytes();
    let mut arena = ObjectArena::new();
    let flame = arena.allocate(ObjectClassKind::Selector76);
    for (growth, left) in [
        (1_u32, 0xffff_u16),
        (0x8000, 0x8000),
        (0x1_0000, 0),
        (0x0f00, 0xf100),
    ] {
        arena.selector76_mut(flame).unwrap().dword_70 = growth;
        values(&arena, flame, &raw, &[(1, left), (2, 2), (0x1_0003, 3)]);
    }
}
