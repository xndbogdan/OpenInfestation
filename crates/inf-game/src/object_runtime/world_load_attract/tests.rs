use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::object_runtime::actor::vehicle_component_tests::{
    allocation, configured_arena, vehicle,
};
use crate::object_runtime::{
    BaseContext, CameraSettings, DerivedActorConstructionRequest, ObjectClassKind,
};
use inf_formats::{constructor_dispatch, FrontendExecutable, CONSTRUCTOR_SELECTOR_COUNT};
use sha2::{Digest, Sha256};
use std::path::Path;

fn derived(arena: &mut ObjectArena, rng: &mut u32) -> ObjectId {
    arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: allocation(),
                world_mask_limits: [3, 3],
            },
            rng,
        )
        .unwrap()
}

fn set_delay(arena: &mut ObjectArena, object: ObjectId, delay: i32) {
    let ActorExtension::Derived(state) = &mut arena.actor_mut(object).unwrap().extension else {
        panic!("genuine derived constructor");
    };
    state.word_144 = delay;
}

fn native_span(bytes: &[u8], address: u32, length: usize) -> &[u8] {
    let word = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let half = |offset| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
    let pe = word(0x3c) as usize;
    let image_base = word(pe + 24 + 28);
    let rva = address.checked_sub(image_base).unwrap() as usize;
    let sections = pe + 24 + usize::from(half(pe + 20));
    for index in 0..usize::from(half(pe + 6)) {
        let section = sections + index * 40;
        let start = word(section + 12) as usize;
        let size = word(section + 16) as usize;
        if rva >= start && rva - start + length <= size {
            let file = word(section + 20) as usize + rva - start;
            return &bytes[file..file + length];
        }
    }
    panic!("unbacked original span {address:08X}");
}

#[test]
fn original_complete_suffix_and_all_canonical_query_bindings_are_authenticated() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(native_span(bytes, 0x0048_df5b, 93))),
        "b131b1267349db3474857f22dbf2c40cf087d5d73f0d8a37232e90c655afc74f"
    );
    assert_eq!(native_span(bytes, 0x0040_1300, 3), [0x33, 0xc0, 0xc3]);
    assert_eq!(native_span(bytes, 0x0040_4610, 3), [0x8b, 0xc1, 0xc3]);
    let executable = FrontendExecutable::parse(bytes).unwrap();
    let mut counts = [0; 4];
    for selector in 1..=CONSTRUCTOR_SELECTOR_COUNT {
        let dispatch = constructor_dispatch(selector).unwrap();
        match executable.word(dispatch.vtable + 0x18).unwrap() {
            0x0040_1300 => counts[0] += 1,
            0x0040_4610 => {
                counts[1] += 1;
                // Nonactor/short-table +58 words are never consumed.
                match executable.word(dispatch.vtable + 0x58).unwrap() {
                    0x0040_1300 => counts[2] += 1,
                    0x0040_4610 => counts[3] += 1,
                    body => panic!("unowned canonical +58 {selector}: {body:08X}"),
                }
            }
            body => panic!("unowned canonical +18 {selector}: {body:08X}"),
        }
    }
    assert_eq!(counts, [29, 65, 48, 17]);
}

#[test]
fn canonical_object_query_returns_self_or_null_without_reading_live_component_fields() {
    let mut arena = configured_arena();
    let mut rng = 1;
    let actors = [
        arena.allocate_actor(allocation()).unwrap(),
        derived(&mut arena, &mut rng),
        vehicle(&mut arena, &mut rng),
        arena
            .allocate_selector16(
                allocation(),
                CameraSettings {
                    first_person: 0,
                    camera_style: 0,
                },
            )
            .unwrap(),
        arena.allocate_selector30_actor(allocation()).unwrap(),
        arena.allocate_selector88(allocation()).unwrap(),
        arena.allocate_selector90(allocation()).unwrap(),
        arena.allocate_selector93(allocation()).unwrap(),
    ];
    let nonactors = [
        ObjectClassKind::Base,
        ObjectClassKind::Selector86,
        ObjectClassKind::MapAnnotation,
        ObjectClassKind::VisualBase,
        ObjectClassKind::RotatingVisualBase,
        ObjectClassKind::TerrainLight,
        ObjectClassKind::RouteNode,
    ]
    .map(|class| arena.allocate(class));
    arena.actor_handles = None;
    let before = format!("{arena:?}");
    for object in actors {
        assert_eq!(arena.object_actor_component(object).unwrap(), Some(object));
    }
    for object in nonactors {
        assert_eq!(arena.object_actor_component(object).unwrap(), None);
    }
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn zero_attract_bypasses_object_authentication_but_nonzero_preserves_reached_errors() {
    let mut arena = configured_arena();
    let actor = arena.allocate_actor(allocation()).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let mut other = configured_arena();
    let foreign = other.allocate_actor(allocation()).unwrap();
    let current = Cell::new(0);
    let before = format!("{arena:?}");
    for object in [actor, foreign] {
        arena
            .apply_world_load_attract(WorldLoadAttractRequest {
                object,
                current_attract: &current,
            })
            .unwrap();
        assert_eq!(
            arena.object_actor_component(object),
            Err(ObjectError::UnknownObject(object))
        );
        current.set(0xff);
        assert_eq!(
            arena.apply_world_load_attract(WorldLoadAttractRequest {
                object,
                current_attract: &current
            }),
            Err(ObjectError::UnknownObject(object))
        );
        current.set(0);
    }
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn nonactor_and_actor_null_derived_paths_preserve_all_owners_without_registry_or_sources() {
    let mut arena = configured_arena();
    let nonactor = arena.allocate(ObjectClassKind::TerrainLight);
    let shared = arena.allocate_actor(allocation()).unwrap();
    let camera = arena
        .allocate_selector16(
            allocation(),
            CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
        )
        .unwrap();
    let demo = arena.allocate_selector93(allocation()).unwrap();
    arena.actor_handles = None;
    let before = format!("{arena:?}");
    for object in [nonactor, shared, camera, demo] {
        arena
            .apply_world_load_attract(WorldLoadAttractRequest {
                object,
                current_attract: &Cell::new(1),
            })
            .unwrap();
    }
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn signed_delay_conversion_and_exact_mask_preserve_dirty_registry_and_other_storage() {
    for (input, expected) in [
        (-1, 1000),
        (-3, 3000),
        (i32::MIN, 0),
        (-2_147_483, 2_147_483_000),
        (-2_147_484, -2_147_483_296),
        (0, 0),
        (7, 7),
        (i32::MAX, i32::MAX),
    ] {
        for dirty in [0, 2, 0x8190] {
            let mut arena = configured_arena();
            let mut rng = 0x1234_5678;
            let object = derived(&mut arena, &mut rng);
            set_delay(&mut arena, object, input);
            let base = &mut arena.owned_mut(object).unwrap().state.base;
            base.flags = u32::MAX;
            base.dirty_word = dirty;
            arena.add_world_player([0x1234_5678; 6]);
            let before_rng = rng;
            let before = format!("{arena:?}");
            let before_state = arena.object(object).unwrap().clone();
            let current = Cell::new(0x80);
            arena
                .apply_world_load_attract(WorldLoadAttractRequest {
                    object,
                    current_attract: &current,
                })
                .unwrap();
            let mut expected_state = before_state.clone();
            let ObjectClass::Actor(actor) = &mut expected_state.class else {
                unreachable!()
            };
            let ActorExtension::Derived(derived) = &mut actor.extension else {
                unreachable!()
            };
            derived.word_144 = expected;
            expected_state.base.flags = 0xFB9F_FFFF;
            assert_eq!(arena.object(object).unwrap(), &expected_state);
            assert_eq!(arena.object(object).unwrap().base.dirty_word, dirty);
            assert_eq!(current.get(), 0x80);
            let after = format!("{arena:?}");
            let selected = format!("{expected_state:?}");
            assert_eq!(after.matches(&selected).count(), 1);
            assert_eq!(
                after.replacen(&selected, &format!("{before_state:?}"), 1),
                before
            );
            assert_eq!(rng, before_rng);
        }
    }
}

#[test]
fn reached_unknown_allocation_after_registration_keeps_completed_serial_prefix() {
    let mut arena = configured_arena();
    let actor = arena.allocate_actor(allocation()).unwrap();
    arena.begin_world_placements();
    arena
        .initialize_context(
            actor,
            BaseContext {
                position_words: Some([1, 2, 3]),
                placement_index: 7,
            },
        )
        .unwrap();
    arena.register_loaded_actor(actor, Some(&[0; 4])).unwrap();
    let retired = arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let ObjectClass::Actor(retired_actor) = retired.class else {
        unreachable!()
    };
    assert_eq!(retired_actor.byte_78, 0);
    assert_eq!(arena.world_load_state().actor_serial_3d0, 1);
    let before = format!("{arena:?}");
    assert_eq!(
        arena.apply_world_load_attract(WorldLoadAttractRequest {
            object: actor,
            current_attract: &Cell::new(1)
        }),
        Err(ObjectError::UnknownObject(actor))
    );
    assert_eq!(format!("{arena:?}"), before);
}
