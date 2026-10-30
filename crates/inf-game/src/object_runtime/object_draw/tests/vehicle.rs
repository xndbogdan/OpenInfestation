//! Supplied phase fixtures for native420510, not a renderer or timing substitute.
//! Host records the required model boundary; production software tests own its
//! execution and retained RNG/process-scratch custody separately. Constructor
//! mode zero emits no visible child model; it is not a vehicle-pixel witness.

use super::*;
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling};
use inf_formats::{
    parse_terrain_bank, parse_vehicle_behavior_table, ByteSpan, HandlerFraming, HandlerId,
    HandlerRegion, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_GRID_SIDE, TERRAIN_PREFIX_BYTES,
};

const SAVED_TINT: u32 = 0xa1cc_8844;
const HALF_TINT: u32 = 0xa166_4422;

/// Parsed supplied H12 bytes. All cells have shade8 except the independently
/// computed native fixed7/mask origin. No runtime cells setter is introduced.
fn shaded_terrain(spacing: TerrainSpacing, xz: [u16; 2], shade: u8) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for index in 0..TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE {
        let at = TERRAIN_PREFIX_BYTES + index * 2;
        bytes[at..at + 2].copy_from_slice(&0x8000_u16.to_le_bytes());
    }
    let mask = match spacing {
        TerrainSpacing::Units128 => 0x01ff,
        TerrainSpacing::Units256 => 0x01fe,
    };
    let x = usize::from((xz[0] >> 7) & mask);
    let z = usize::from((xz[1] >> 7) & mask);
    let at = TERRAIN_PREFIX_BYTES + (x * TERRAIN_GRID_SIDE + z) * 2;
    bytes[at..at + 2].copy_from_slice(&(u16::from(shade) << 12).to_le_bytes());
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing,
            height_scale: TerrainHeightScale::Units16,
        },
    )
}

fn draw_vehicle(arena: &mut ObjectArena, value: &TerrainRuntime, xyz: [u16; 3]) -> ObjectId {
    let object = vehicle(arena, value);
    position(arena, object, xyz, 0);
    model_word(arena, object, 23); // Actual supplied model packet, neverFFFF parking.
    arena.owned_mut(object).unwrap().state.base.flags &= !0x80;
    object
}

fn set_special_view(arena: &mut ObjectArena, object: ObjectId, value: u8) {
    match &mut arena.derived_actor_mut(object).unwrap().class {
        DerivedActorClass::Selector2(state) => state.byte_1c0 = value,
        _ => panic!("actual selector2 state"),
    }
}

fn admit_empty_behavior(
    arena: &mut ObjectArena,
    object: ObjectId,
    executable: FrontendExecutable<'_>,
) {
    // Explicit supplied health lets the genuine41A620 guard run. Its original
    // executable descriptors and48CC70 allocation own the empty140 invariant.
    arena.actor_mut(object).unwrap().maximum_health = Some(700);
    let table = parse_vehicle_behavior_table(executable).unwrap();
    assert!(arena.admit_selector2_actor(object, &table, None).unwrap());
    assert!(arena.derived_actor(object).unwrap().behavior_140.is_some());
}

fn assert_half_lighting(events: &[Event]) {
    let lighting: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::Scene(ScenePhase::Ambient(rgb)) => Some(ScenePhase::Ambient(*rgb)),
            Event::Scene(ScenePhase::Directional { direction, rgb }) => {
                Some(ScenePhase::Directional {
                    direction: *direction,
                    rgb: *rgb,
                })
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        lighting,
        vec![
            ScenePhase::Ambient([8, 17, 25]),
            ScenePhase::Directional {
                direction: [0.25, -0.5, 0.75],
                rgb: [34, 68, 102]
            },
        ]
    );
}

#[test]
fn early_attract_raw_network_and_special_view_keep_lazy_ordinary_reads() {
    let raw = executable_bytes();
    // Attract precedes network; network precedes1C0.1C0 precedes D0/H12/228.
    for (attract, network, special, succeeds, parks) in [
        (0x80, 0x100, 1, true, true),
        (0, 0x100, 1, false, false),
        (0, 0, 1, true, false),
    ] {
        let mut fixture = Fixture::new(&raw);
        fixture.tint = SAVED_TINT;
        fixture.network_flag = network;
        let value = shaded_terrain(TerrainSpacing::Units128, [0, 10], 4);
        let mut arena = arena();
        let object = draw_vehicle(&mut arena, &value, [0, 0, 10]);
        set_special_view(&mut arena, object, special);
        assert!(arena.apply_actor_view_mode(object, 1).unwrap());
        assert_eq!(arena.selector2_actor(object).unwrap().dword_228(), None);
        let headers = arena.list_owners;
        let (mut terrain, mut host, trace) = services(&value);
        host.attract = attract;
        let result = fixture.run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        );
        if succeeds {
            assert!(result.is_ok());
        } else {
            assert!(
                matches!(result, Err(Flag0ObjectDrawError::UnownedCallback { object: reached, .. })
                if reached == object)
            );
            assert_eq!(arena.list_owners, headers);
            assert_eq!(arena.list(ObjectList::Flag0Secondary), flag0(&[object]));
        }
        assert_eq!(
            trace.borrow().as_slice(),
            &[
                Event::Terrain(object, Some([0, 0, 10])),
                Event::Attract(attract),
            ]
        );
        assert_eq!(fixture.tint, SAVED_TINT);
        assert_eq!(arena.selector2_actor(object).unwrap().dword_228(), None);
        assert_eq!(arena.object(object).unwrap().base.flags & 1 == 0, parks);
    }
}

#[test]
fn signed_d0_selects_common_shading_or_positive_empty_pending_tail() {
    let raw = executable_bytes();
    for mode in [0, 0x80, 0xff, 1, 0x7f] {
        let mut fixture = Fixture::new(&raw);
        fixture.tint = SAVED_TINT;
        let value = shaded_terrain(TerrainSpacing::Units128, [0, 10], 4);
        let mut arena = arena();
        let object = draw_vehicle(&mut arena, &value, [0, 0, 10]);
        assert!(arena.apply_actor_view_mode(object, mode).unwrap());
        assert!(arena.derived_actor(object).unwrap().behavior_140.is_none());
        let (mut terrain, mut host, trace) = services(&value);
        let outcome = fixture
            .run(
                &mut arena,
                &mut terrain,
                &mut host,
                Flag0ObjectDrawFailurePolicy::Stop,
            )
            .unwrap();
        assert_eq!(outcome.near_callbacks, 1);
        assert!(outcome.omissions.is_empty());
        assert_eq!(fixture.tint, SAVED_TINT);
        assert_eq!(arena.selector2_actor(object).unwrap().dword_228(), Some(0));
        assert_eq!(outcome.empty_behaviors, 0); // Constructor-known NULL140, not a fabricated chain.
        if (mode as i8) <= 0 {
            assert_eq!(outcome.common_models, 1);
            assert_half_lighting(&trace.borrow());
            assert!(
                matches!(trace.borrow().last(), Some(Event::Model(packet, ModelRootMode::CommonVisual, _))
                if packet.source == object && packet.model_slot == 23)
            );
            assert_eq!(terrain.calls, 3); // manager, vehicle shade, common height
        } else {
            assert_eq!(outcome.common_models, 0);
            assert_eq!(
                trace.borrow().as_slice(),
                &[Event::Terrain(object, Some([0, 0, 10])), Event::Attract(0),]
            );
        }
    }
}

#[test]
fn unsigned_shade_lookup_captures_xz_before_mutating_h12_and_common_reads_current_tint() {
    let raw = executable_bytes();
    for spacing in [TerrainSpacing::Units128, TerrainSpacing::Units256] {
        let mut fixture = Fixture::new(&raw);
        fixture.tint = SAVED_TINT;
        fixture.spacing = spacing;
        fixture.context.view.eye = [65_534, 0, 65_500];
        let value = shaded_terrain(spacing, [u16::MAX, u16::MAX], 4);
        let mut arena = arena();
        let object = draw_vehicle(&mut arena, &value, [u16::MAX, 0, u16::MAX]);
        arena.actor_mut(object).unwrap().rotating.basis.words[0] = 0x1122_3344;
        let basis = arena.actor(object).unwrap().rotating.basis;
        let (mut terrain, mut host, trace) = services(&value);
        terrain.effects.push_back(TerrainEffect::None); // manager admission
        terrain
            .effects
            .push_back(TerrainEffect::PositionExtent([128, 0, 256], 0));
        let outcome = fixture
            .run(
                &mut arena,
                &mut terrain,
                &mut host,
                Flag0ObjectDrawFailurePolicy::Stop,
            )
            .unwrap();
        let expected_context = fixture.context;
        assert_eq!((outcome.common_models, outcome.late_models), (1, 0));
        assert_eq!(
            trace.borrow().as_slice(),
            &[
                Event::Terrain(object, Some([u16::MAX, 0, u16::MAX])),
                Event::Attract(0),
                Event::Terrain(object, Some([u16::MAX, 0, u16::MAX])),
                Event::Terrain(object, Some([128, 0, 256])),
                Event::Scene(ScenePhase::Ambient([8, 17, 25])),
                Event::Scene(ScenePhase::Directional {
                    direction: [0.25, -0.5, 0.75],
                    rgb: [34, 68, 102]
                }),
                Event::Model(
                    ObjectModelPacket {
                        source: object,
                        model_slot: 23,
                        position: [128, 0, 256],
                        basis
                    },
                    ModelRootMode::CommonVisual,
                    expected_context
                ),
            ]
        );
        assert_eq!(fixture.tint, SAVED_TINT);
        assert_eq!(arena.selector2_actor(object).unwrap().dword_228(), Some(0));
    }
}

#[test]
fn shade_clamp_and_high_byte_survive_a_typed_model_failure_without_normal_restore() {
    let raw = executable_bytes();
    for (shade, modulated, rgb) in [
        (0, 0xa100_0000, [0, 0, 0]),
        (4, HALF_TINT, [34, 68, 102]),
        (8, SAVED_TINT, [68, 136, 204]),
        (15, SAVED_TINT, [68, 136, 204]),
    ] {
        let mut fixture = Fixture::new(&raw);
        fixture.tint = SAVED_TINT;
        fixture.process.completion_53b890 = 0x71;
        fixture.context.view.eye[1] = 256; // Ground excess261-0-256=5, view deltaY=5.
        let context = fixture.context;
        let value = shaded_terrain(TerrainSpacing::Units128, [0, 10], shade);
        let mut arena = arena();
        let object = draw_vehicle(&mut arena, &value, [0, 261, 10]);
        let headers = arena.list_owners;
        use crate::object_runtime::{ProfilingPhase, ProfilingRequest};
        fixture
            .process
            .profiling
            .record_milliseconds(
                ProfilingRequest {
                    index: 13,
                    bank: 1,
                    phase: ProfilingPhase::Begin,
                },
                0x1000_0000,
            )
            .unwrap();
        fixture
            .process
            .profiling
            .record_milliseconds(
                ProfilingRequest {
                    index: 13,
                    bank: 1,
                    phase: ProfilingPhase::End,
                },
                0x2000_0000,
            )
            .unwrap();
        let prior_marker = fixture.process.profiling.counters(13, 1).unwrap();
        let (mut terrain, mut host, trace) = services(&value);
        host.fail = Some((
            object,
            ObjectDrawServiceError::Backend("required model backend stopped"),
        ));

        assert!(matches!(
            fixture.run(
                &mut arena,
                &mut terrain,
                &mut host,
                Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation
            ),
            Err(Flag0ObjectDrawError::Host("required model backend stopped"))
        ));
        let marker = fixture.process.profiling.counters(13, 1).unwrap();
        // A fresh timeGetTime sample may equal the supplied prior start.
        assert_eq!(marker.1, prior_marker.1); // Failed Common never reaches end.
        assert_eq!(fixture.tint, modulated);
        assert_eq!(fixture.tint >> 24, SAVED_TINT >> 24);
        assert_eq!(arena.selector2_actor(object).unwrap().dword_228(), None);
        assert_eq!(fixture.context.far, context.far + 20); // actual ground excess5*4
        assert_eq!(fixture.context.near, context.near);
        assert_eq!(fixture.context.far_color, context.far_color);
        assert_eq!(fixture.process.completion_53b890, 0x71);
        assert_eq!(arena.list_owners, headers);
        assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&[]));
        assert_eq!(arena.list(ObjectList::Flag0Secondary), flag0(&[object]));
        let reached = trace.borrow();
        assert!(matches!(reached[4], Event::Scene(ScenePhase::Fade(value))
            if value == SceneFade::new([context.near, context.far + 20])));
        assert!(
            matches!(reached[6], Event::Scene(ScenePhase::Directional { rgb: actual, .. }) if actual == rgb)
        );
        assert!(
            matches!(reached.last(), Some(Event::Model(packet, ModelRootMode::CommonVisual, actual))
            if packet.source == object && packet.model_slot == 23 && actual.far == context.far + 20)
        );
        // No tail Fade/restore or fake model completion after the required stop.
        assert_eq!(reached.len(), 8);
    }
}

/// A port omission in the vehicle's model stands in for a draw native
/// completes: the shaded world tint returns before the next actor, so it
/// cannot compound frame after frame (the world darkening on shaded cells).
#[test]
fn unowned_vehicle_model_returns_the_tint_before_a_following_common_actor() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    fixture.tint = SAVED_TINT;
    fixture.context.view.eye[1] = 256;
    let value = shaded_terrain(TerrainSpacing::Units128, [0, 10], 4);
    let mut arena = arena();
    let object = draw_vehicle(&mut arena, &value, [0, 261, 10]);
    let next = actor(&mut arena, [0, 256, 15], 24);
    arena.owned_mut(next).unwrap().state.base.flags &= !0x80;
    let (mut terrain, mut host, trace) = services(&value);
    host.fail = Some((
        object,
        ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::ModelFeature {
            model_slot: 23,
            reason: "required supplied model feature".into(),
        }),
    ));
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
        )
        .unwrap();
    assert_eq!((outcome.visited, outcome.common_models), (2, 1));
    assert_eq!(outcome.omissions.len(), 1);
    assert_eq!(outcome.omissions[0].object, object);
    assert_eq!(fixture.tint, SAVED_TINT);
    assert_eq!(arena.selector2_actor(object).unwrap().dword_228(), None);
    // The raised far returns too, as native's normal return restores it.
    assert_eq!(fixture.context.far, 100);
    assert_eq!(fixture.process.completion_53b890, 1);
    assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&[object, next]));
    assert!(arena.list(ObjectList::Flag0Secondary).is_empty());
    let events = trace.borrow();
    let models: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::Model(packet, root, context) => Some((*packet, *root, *context)),
            _ => None,
        })
        .collect();
    assert_eq!(models.len(), 2);
    assert_eq!(
        (models[0].0.source, models[0].1),
        (object, ModelRootMode::CommonVisual)
    );
    assert_eq!(
        (models[1].0.source, models[1].1),
        (next, ModelRootMode::CommonVisual)
    );
    assert_eq!(models[1].2.far, 100);
    let directions: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::Scene(ScenePhase::Directional { rgb, .. }) => Some(*rgb),
            _ => None,
        })
        .collect();
    assert_eq!(directions, vec![[34, 68, 102], [68, 136, 204]]);
}

#[test]
fn positive_d0_pending_uses_late_packet_before_actual_empty140_and_outer_pending_clear() {
    let raw = executable_bytes();
    for pending in [false, true] {
        let mut fixture = Fixture::new(&raw);
        fixture.tint = SAVED_TINT;
        let context = fixture.context;
        let value = shaded_terrain(TerrainSpacing::Units128, [0, 10], 4);
        let mut arena = arena();
        let object = draw_vehicle(&mut arena, &value, [0, 0, 10]);
        admit_empty_behavior(&mut arena, object, fixture.executable);
        assert!(arena.apply_actor_view_mode(object, 1).unwrap());
        if pending {
            arena.owned_mut(object).unwrap().state.base.flags |= 0x0100_0000;
        }
        let basis = arena.actor(object).unwrap().rotating.basis;
        let (mut terrain, mut host, trace) = services(&value);
        let outcome = fixture
            .run(
                &mut arena,
                &mut terrain,
                &mut host,
                Flag0ObjectDrawFailurePolicy::Stop,
            )
            .unwrap();
        assert_eq!(
            (outcome.common_models, outcome.late_models),
            (0, usize::from(pending))
        );
        // Ordinary42086D reads140 and bypasses48CDB0 when its native+00
        // head is NULL. Only the outer479570 pending+7C path invokes the
        // complete empty48CDB0 branch. Diagnostics count invocations only.
        assert_eq!(outcome.empty_behaviors, usize::from(pending));
        assert_eq!(arena.object(object).unwrap().base.flags & 0x0100_0000, 0);
        assert_eq!(fixture.tint, SAVED_TINT);
        assert_eq!(fixture.context, context);
        assert_eq!(arena.selector2_actor(object).unwrap().dword_228(), Some(0));
        let mut expected = vec![Event::Terrain(object, Some([0, 0, 10])), Event::Attract(0)];
        if pending {
            expected.push(Event::Model(
                ObjectModelPacket {
                    source: object,
                    model_slot: 23,
                    position: [0, 0, 10],
                    basis,
                },
                ModelRootMode::ManagerLate,
                context,
            ));
        }
        assert_eq!(*trace.borrow(), expected);
    }
}
#[test]
fn positive_pending_model_failure_precedes_empty_behavior_and_normal228_publication() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    fixture.tint = SAVED_TINT;
    fixture.process.completion_53b890 = 0x71;
    let context = fixture.context;
    let value = shaded_terrain(TerrainSpacing::Units128, [0, 10], 4);
    let mut arena = arena();
    let object = draw_vehicle(&mut arena, &value, [0, 0, 10]);
    admit_empty_behavior(&mut arena, object, fixture.executable);
    assert!(arena.apply_actor_view_mode(object, 0x7f).unwrap());
    arena.owned_mut(object).unwrap().state.base.flags |= 0x0100_0000;
    let behavior = arena.derived_actor(object).unwrap().behavior_140.clone();
    let basis = arena.actor(object).unwrap().rotating.basis;
    let headers = arena.list_owners;
    let (mut terrain, mut host, trace) = services(&value);
    host.fail = Some((
        object,
        ObjectDrawServiceError::Backend("late model backend stopped"),
    ));
    assert!(matches!(
        fixture.run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation
        ),
        Err(Flag0ObjectDrawError::Host("late model backend stopped"))
    ));
    assert_eq!(
        trace.borrow().as_slice(),
        &[
            Event::Terrain(object, Some([0, 0, 10])),
            Event::Attract(0),
            Event::Model(
                ObjectModelPacket {
                    source: object,
                    model_slot: 23,
                    position: [0, 0, 10],
                    basis,
                },
                ModelRootMode::ManagerLate,
                context
            ),
        ]
    );
    assert_eq!(fixture.tint, SAVED_TINT);
    assert_eq!(fixture.context, context);
    assert_eq!(arena.selector2_actor(object).unwrap().dword_228(), None);
    assert_eq!(arena.derived_actor(object).unwrap().behavior_140, behavior);
    assert_eq!(
        arena.object(object).unwrap().base.flags & 0x0100_0000,
        0x0100_0000
    );
    assert_eq!(arena.list_owners, headers);
    assert_eq!(arena.list(ObjectList::Flag0Secondary), flag0(&[object]));
    assert_eq!(fixture.process.completion_53b890, 0x71);
}
#[test]
fn complete_vehicle_constructor_basis_and_empty_behavior_producers_remain_authenticated() {
    let raw = executable_bytes();
    for (offset, length, expected) in [
        (
            132368,
            1368,
            "cff41b83b91ee363b2ad407ec8a8fd9b7058fd9d0611102d2c668832b8eb98bc",
        ), //complete vehicle +2C callback
        (
            9904,
            4,
            "b185eed3e098fd3dc404ff66db8be1121a2a2610cf0814ec915e002304c9bccf",
        ), //complete virtual+48 basis-header getter
        (
            238448,
            8,
            "701337372dfbd9b96ad7d135a9cf8b39467eb2aca1c8832cc7314ac5d0a90310",
        ), //complete tint publisher
        (
            87776,
            1009,
            "9852acd15d4225fc67ee2322a916d788ddec90c4ae8960fd7b701584e7d71b9d",
        ), //complete selector2 direct constructor through RET
        (
            155232,
            231,
            "bf30316b8a96753a1746060565737be661587136bd06968fe94e80224cb9bec7",
        ), //complete derived parent constructor
        (
            158704,
            31,
            "91db4b1faf71907cc01214bb33b9fe15f4a25f769145ab29d0dfe4665e01651c",
        ), //complete derived payload wrapper
        (
            158736,
            106,
            "28e2994e6804f8f87f3edb48ce54a185a9a9ae798a7f4d77c3b6da0493fa56a9",
        ), //complete derived payload stores
        (
            576944,
            436,
            "12ebe3590cd3c87772585b2882dd27f686ff650dc4aaba7210bd467fd2693aff",
        ), //behavior_chain_service_48CDB0
    ] {
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw[offset..offset + length])),
            expected
        );
    }
}
