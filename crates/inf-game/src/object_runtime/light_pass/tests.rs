use super::*;
use crate::object_runtime::CameraSettings;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        ActorTerrainSurface, BaseContext, DerivedActorConstructionRequest, ObjectClassKind,
        ObjectState, Selector2State, Selector30State, TerrainLight,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
    world_lighting::{TerrainLightColor, WORLD_LIGHT_BYTES},
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_ENTRY_FIXED_BYTES,
};
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, path::Path};

static UNUSED_SINE: [u16; 4096] = [0; 4096];

struct SuppliedPolicy {
    enabled: bool,
    deposits: VecDeque<TerrainLightPolicy>,
    reads: Vec<&'static str>,
}

impl SuppliedPolicy {
    fn new(enabled: bool, deposit_enabled: &[bool]) -> Self {
        Self {
            enabled,
            deposits: deposit_enabled
                .iter()
                .map(|&enabled| TerrainLightPolicy {
                    enabled,
                    spacing: TerrainSpacing::Units128,
                    color: TerrainLightColor::Unmodified,
                })
                .collect(),
            reads: Vec::new(),
        }
    }
}

impl ObjectLightPolicySource for SuppliedPolicy {
    fn dynamic_lighting(&mut self) -> bool {
        self.reads.push("entry");
        self.enabled
    }

    fn terrain_light_policy(&mut self) -> TerrainLightPolicy {
        self.reads.push("deposit");
        self.deposits
            .pop_front()
            .expect("only a reached deposit reads policy")
    }
}

fn run(
    arena: &mut ObjectArena,
    grid: &mut WorldLightGrid,
    source: &mut SuppliedPolicy,
    failure_policy: Flag0LightFailurePolicy,
) -> Result<Flag0LightPassOutcome, Flag0LightPassError> {
    arena.run_flag0_light_pass(Flag0LightPassRequest {
        grid,
        policy_source: source,
        failure_policy,
        terrain: None,
        rng: None,
        world_delta: 0,
        sine_quarter: &UNUSED_SINE,
        lamps: None,
        lamps_light: false,
    })
}

#[test]
fn stopped_construction_light_boundary_precedes_clamp_and_deposit_but_preserves_healthy_work() {
    for policy in [
        Flag0LightFailurePolicy::Stop,
        Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
    ] {
        let mut arena = arena();
        let stopped = arena.allocate_selector30_actor(allocation()).unwrap();
        selector30_mut(&mut arena, stopped).word_178 = u32::MAX;
        arena.apply_payload_flags(stopped, 0x0800_0003).unwrap();
        let native = arena.object(stopped).unwrap().clone();
        arena.stop_object_construction(stopped).unwrap();
        let healthy = light(&mut arena, Some([0, 0, 0]), [1, 2, 3], 512);
        let mut source = SuppliedPolicy::new(true, &[true, true]);
        let mut grid = WorldLightGrid::default();
        grid.set_anchor([0x1680; 2]);
        let result = run(&mut arena, &mut grid, &mut source, policy);
        assert_eq!(arena.object(stopped).unwrap(), &native);
        if policy == Flag0LightFailurePolicy::Stop {
            assert!(matches!(result, Err(Flag0LightPassError::UnownedCallback {
                object, reason: ObjectLightCallbackBoundary::StoppedConstruction,
            }) if object == stopped));
            assert_eq!(source.reads, ["entry"]);
            assert_eq!(grid.cells(), &[0; WORLD_LIGHT_BYTES]);
            assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&[healthy]));
            continue;
        }
        let result = result.unwrap();
        assert_eq!((result.visited, result.deposits), (2, 1));
        assert_eq!(
            result.omissions,
            [Flag0LightPassOmission {
                object: stopped,
                reason: ObjectLightCallbackBoundary::StoppedConstruction,
            }]
        );
        assert!(result.light.cell_writes > 0);
        assert_eq!(source.reads, ["entry", "deposit"]);
        let result = run(&mut arena, &mut grid, &mut source, policy).unwrap();
        assert_eq!(result.deposits, 1);
        assert_eq!(result.omissions[0].object, stopped);
        assert_eq!(source.reads, ["entry", "deposit", "entry", "deposit"]);
        assert_eq!(arena.object(stopped).unwrap(), &native);
    }
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
    // Actual H12 NULL shared-constructor arm never reads XYZ or this sine.
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

fn light(
    arena: &mut ObjectArena,
    position: Option<[u16; 3]>,
    rgb: [i8; 3],
    radius: u16,
) -> ObjectId {
    light_of(arena, ObjectClassKind::TerrainLight, position, rgb, radius)
}

fn light_of(
    arena: &mut ObjectArena,
    class: ObjectClassKind,
    position: Option<[u16; 3]>,
    rgb: [i8; 3],
    radius: u16,
) -> ObjectId {
    let object = arena.allocate(class);
    arena
        .initialize_terrain_light_payload(
            object,
            3,
            TerrainLight {
                channels: rgb,
                radius,
                draw_gate: 0,
            },
        )
        .unwrap();
    arena
        .initialize_context(
            object,
            BaseContext {
                position_words: position,
                placement_index: 7,
            },
        )
        .unwrap();
    object
}

fn vehicle(arena: &mut ObjectArena) -> ObjectId {
    // Declared parsed flat H12 domain. Ground >= plane makes the constructor's
    // wave branch return before the unused sine table, independently of Draw.
    let bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    let span = ByteSpan::new(0, bytes.len());
    let terrain = parse_terrain_bank(
        &bytes,
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

// Supplied current scalar domains below do not claim authentic native writers.
fn vehicle_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut Selector2State {
    let ObjectClass::Actor(actor) = &mut arena.owned_mut(object).unwrap().state.class else {
        panic!("actual actor allocation")
    };
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("actual derived constructor")
    };
    let DerivedActorClass::Selector2(vehicle) = &mut derived.class else {
        panic!("actual selector2 constructor")
    };
    vehicle
}

fn selector30_mut(arena: &mut ObjectArena, object: ObjectId) -> &mut Selector30State {
    let ObjectClass::Actor(actor) = &mut arena.owned_mut(object).unwrap().state.class else {
        panic!("actual actor allocation")
    };
    let ActorExtension::Selector30(state) = &mut actor.extension else {
        panic!("actual selector30 constructor")
    };
    state
}

fn flag0(objects: &[ObjectId]) -> Vec<ObjectNode> {
    objects
        .iter()
        .map(|&object| ObjectNode {
            object,
            kind: ObjectNodeKind::Flag0,
        })
        .collect()
}

/// All owners besides the explicitly checked state/list/membership outputs.
fn other_custody(arena: &ObjectArena) -> String {
    let ObjectArena {
        identity,
        slots,
        free,
        live_count,
        lists: _,
        list_owners: _,
        routes,
        actors,
        map_annotations,
        spawn_markers,
        territories,
        race,
        race_gates,
        race_clock,
        powerball,
        powerball_goals,
        powerball_seen,
        network_sends,
        network_game,
        actor_handles,
        actor_allocation,
        actor_services,
        actor_payloads,
        constructor_allocations,
        selector60_rows,
        players,
        world_load,
        pickup_kind7_5b4,
        force_field_bits,
        capture_nodes,
        alien_brain,
        cleanup_exit_pending,
    } = arena;
    let generations: Vec<_> = slots
        .iter()
        .map(|slot| (slot.generation, slot.value.is_some()))
        .collect();
    format!(
        "{:?}",
        (
            (identity, generations, free, live_count),
            (
                routes,
                actors,
                map_annotations,
                actor_handles,
                actor_allocation,
                actor_services,
                actor_payloads,
                constructor_allocations,
                selector60_rows,
                players,
                world_load,
                (
                    pickup_kind7_5b4,
                    force_field_bits,
                    capture_nodes,
                    alien_brain,
                    cleanup_exit_pending,
                    spawn_markers,
                    territories,
                    (race, race_gates, race_clock),
                    (powerball, powerball_goals, powerball_seen),
                    network_sends,
                    network_game
                )
            )
        )
    )
}

fn assert_memberships(arena: &ObjectArena, objects: &[ObjectId]) {
    for &object in objects {
        let owned = arena.owned(object).unwrap();
        for kind in [
            ObjectNodeKind::Flag0,
            ObjectNodeKind::Flag1,
            ObjectNodeKind::Lifecycle,
        ] {
            let node = ObjectNode { object, kind };
            assert!(arena.lists[owned.memberships[kind as usize]].contains(&node));
            assert_eq!(
                arena
                    .lists
                    .iter()
                    .map(|list| list.iter().filter(|&&current| current == node).count())
                    .sum::<usize>(),
                1
            );
        }
    }
}

#[test]
fn complete_original_pass_and_callbacks_remain_authenticated() {
    let bytes =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
            .expect("required local original retail executable");
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    for (start, end, digest) in [
        (
            0x00479500,
            0x0047956b,
            "276723660d48bac4091f15ff7c0c1c86a9693e60eba3c505b0627bda4e14a62f",
        ),
        (
            0x00401350,
            0x0040136b,
            "e27b915ecf13a853b57ae2c1f157865114fda4863a1829969d96122bf6402fcb",
        ),
        (
            0x004481a0,
            0x004481b9,
            "60b57abe5c6b807b06472a4dd6d7ecf61d1ea0a883b6342c1b1684f062a78ba0",
        ),
        (
            0x0048bd40,
            0x0048bd82,
            "090142fefaabf215b04956154bf0eb7ff35039bbee5ad6ce948e65b38aefc268",
        ),
        (
            0x00420080,
            0x00420503,
            "8d0f7f2cbe856a9af079fa230d375e34a621cf6036225e4942dff0353237a60f",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[start - 0x00400000..end - 0x00400000])
            ),
            digest
        );
    }
}

#[test]
fn disabled_entry_preserves_whole_arena_grid_and_unavailable_fields() {
    let mut arena = arena();
    arena.allocate(ObjectClassKind::TerrainLight); // Both light and XYZ unknown.
    vehicle(&mut arena);
    let mut grid = WorldLightGrid::default();
    grid.set_anchor([7, -3]);
    let before = format!("{arena:?}");
    let grid_before = format!("{grid:?}");
    let mut source = SuppliedPolicy::new(false, &[]);
    assert_eq!(
        run(
            &mut arena,
            &mut grid,
            &mut source,
            Flag0LightFailurePolicy::Stop
        )
        .unwrap(),
        Flag0LightPassOutcome::default()
    );
    assert_eq!(source.reads, ["entry"]);
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(format!("{grid:?}"), grid_before);
}

#[test]
fn enabled_empty_pass_exchanges_only_the_two_header_owners() {
    let mut arena = arena();
    let before = format!("{arena:?}");
    let owners = arena.list_owners;
    let mut grid = WorldLightGrid::default();
    grid.set_anchor([7, -3]);
    let grid_before = format!("{grid:?}");
    let mut source = SuppliedPolicy::new(true, &[]);
    assert_eq!(
        run(
            &mut arena,
            &mut grid,
            &mut source,
            Flag0LightFailurePolicy::Stop
        )
        .unwrap(),
        Flag0LightPassOutcome::default()
    );
    let mut expected = owners;
    expected.swap(
        ObjectList::Flag0Primary as usize,
        ObjectList::Flag0Secondary as usize,
    );
    assert_eq!(arena.list_owners, expected);
    assert_eq!(source.reads, ["entry"]);
    assert_eq!(format!("{grid:?}"), grid_before);
    arena.swap_flag0_lists(); // Reverse only the independently asserted output.
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn common_callbacks_clear_only_high_flag_and_exchange_physical_headers() {
    let mut arena = arena();
    arena.swap_flag0_lists(); // Retained roles need not equal allocation indices.
    let mut objects: Vec<_> = [
        ObjectClassKind::Base,
        ObjectClassKind::VisualBase,
        ObjectClassKind::RotatingVisualBase,
        ObjectClassKind::RouteNode,
        ObjectClassKind::Selector86,
        ObjectClassKind::MapAnnotation,
    ]
    .into_iter()
    .map(|class| arena.allocate(class))
    .collect();
    objects.push(arena.allocate_actor(allocation()).unwrap());
    objects.push(
        arena
            .allocate_derived_actor(
                DerivedActorConstructionRequest {
                    actor: allocation(),
                    world_mask_limits: [0, 16],
                },
                &mut 23,
            )
            .unwrap(),
    );
    for (index, &object) in objects.iter().enumerate() {
        arena.owned_mut(object).unwrap().state.base.dirty_word = 0xa500 | index as u16;
        // Misleading resource/B4 fields cannot replace actual class identity.
        arena.owned_mut(object).unwrap().state.base.resource_word = 2;
        if let ObjectClass::Actor(actor) = &mut arena.owned_mut(object).unwrap().state.class {
            actor.word_b4 = 2;
        }
    }
    let prior = arena.allocate(ObjectClassKind::Base);
    arena.move_node(prior, ObjectNodeKind::Flag0, ObjectList::Flag0Secondary);
    let prior_before = arena.object(prior).unwrap().clone();
    let mut expected: Vec<_> = objects
        .iter()
        .map(|&id| arena.object(id).unwrap().clone())
        .collect();
    for state in &mut expected {
        state.base.flags &= !0x0800_0000;
    }
    let stable = other_custody(&arena);
    let owners = arena.list_owners;
    let flag1 = arena.list(ObjectList::Flag1Primary).to_vec();
    let parked = arena.list(ObjectList::Parking).to_vec();
    let mut source = SuppliedPolicy::new(true, &[]);
    let mut grid = WorldLightGrid::default();
    let result = run(
        &mut arena,
        &mut grid,
        &mut source,
        Flag0LightFailurePolicy::Stop,
    )
    .unwrap();
    assert_eq!(
        (result.visited, result.callbacks, result.deposits),
        (objects.len(), objects.len(), 0)
    );
    assert!(result.omissions.is_empty());
    assert_eq!(source.reads, ["entry"]);
    let mut expected_order = vec![prior];
    expected_order.extend_from_slice(&objects);
    assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&expected_order));
    assert!(arena.list(ObjectList::Flag0Secondary).is_empty());
    let mut expected_owners = owners;
    expected_owners.swap(
        ObjectList::Flag0Primary as usize,
        ObjectList::Flag0Secondary as usize,
    );
    assert_eq!(arena.list_owners, expected_owners);
    for (&id, state) in objects.iter().zip(&expected) {
        assert_eq!(arena.object(id).unwrap(), state);
    }
    assert_eq!(arena.object(prior).unwrap(), &prior_before);
    assert_eq!(arena.list(ObjectList::Flag1Primary), flag1);
    assert_eq!(arena.list(ObjectList::Parking), parked);
    assert_eq!(other_custody(&arena), stable);
    assert_memberships(&arena, &expected_order);
    assert_eq!(grid.cells(), &[0; WORLD_LIGHT_BYTES]);
}

#[test]
fn high_flag_clear_skips_unknown_callback_but_still_transfers_node() {
    let mut arena = arena();
    let object = arena.allocate(ObjectClassKind::TerrainLight);
    arena.owned_mut(object).unwrap().state.base.flags &= !0x0800_0000;
    let before = arena.object(object).unwrap().clone();
    let mut source = SuppliedPolicy::new(true, &[]);
    let result = run(
        &mut arena,
        &mut WorldLightGrid::default(),
        &mut source,
        Flag0LightFailurePolicy::Stop,
    )
    .unwrap();
    assert_eq!((result.visited, result.callbacks), (1, 0));
    assert_eq!(arena.object(object).unwrap(), &before);
    assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&[object]));
    assert_memberships(&arena, &[object]);
}

#[test]
fn terrain_light_signed_words_and_negative_rgb_use_actual_retained_grid() {
    let mut arena = arena();
    let object = light(
        &mut arena,
        Some([0xff00, 0x7777, 0xff80]),
        [-11, 22, -33],
        512,
    );
    let before = arena.object(object).unwrap().clone();
    let stable = other_custody(&arena);
    let mut grid = WorldLightGrid::default();
    grid.set_anchor([-256 + 0x1680, -128 + 0x1680]);
    let anchor = grid.anchors();
    let mut source = SuppliedPolicy::new(true, &[true]);
    let result = run(
        &mut arena,
        &mut grid,
        &mut source,
        Flag0LightFailurePolicy::Stop,
    )
    .unwrap();
    assert_eq!(
        (result.visited, result.callbacks, result.deposits),
        (1, 1, 1)
    );
    assert!(result.light.cell_writes > 0);
    assert_eq!(&grid.cells()[..3], &[245, 22, 223]);
    assert_eq!(grid.anchors(), anchor);
    assert_eq!(source.reads, ["entry", "deposit"]);
    assert_eq!(arena.object(object).unwrap(), &before); // No high-flag clear.
    assert_eq!(other_custody(&arena), stable);
    assert_memberships(&arena, &[object]);
}

/// Selector 54's 004649C0 is selector 19's 004481A0 reading +6C and +70.
#[test]
fn rotating_terrain_light_lights_as_selector_19() {
    let grids = [
        ObjectClassKind::TerrainLight,
        ObjectClassKind::RotatingTerrainLight,
    ]
    .map(|class| {
        let mut arena = arena();
        let object = light_of(
            &mut arena,
            class,
            Some([0xff00, 0x7777, 0xff80]),
            [-11, 22, -33],
            512,
        );
        let before = arena.object(object).unwrap().clone();
        let mut grid = WorldLightGrid::default();
        grid.set_anchor([-256 + 0x1680, -128 + 0x1680]);
        let mut source = SuppliedPolicy::new(true, &[true]);
        let result = run(
            &mut arena,
            &mut grid,
            &mut source,
            Flag0LightFailurePolicy::Stop,
        )
        .unwrap();
        assert_eq!(
            (result.visited, result.callbacks, result.deposits),
            (1, 1, 1)
        );
        assert_eq!(arena.object(object).unwrap(), &before);
        grid.cells().to_vec()
    });
    assert_eq!(&grids[1][..3], &[245, 22, 223]);
    assert_eq!(grids[0], grids[1]);
}

#[test]
fn missing_light_precedes_xyz_and_strict_stop_retains_earlier_callback_without_swap() {
    let mut arena = arena();
    let common = arena.allocate(ObjectClassKind::Base);
    let unknown = arena.allocate(ObjectClassKind::TerrainLight);
    let later = arena.allocate(ObjectClassKind::Base);
    let mut expected_common = arena.object(common).unwrap().clone();
    expected_common.base.flags &= !0x0800_0000;
    let unknown_before = arena.object(unknown).unwrap().clone();
    let later_before = arena.object(later).unwrap().clone();
    let owners = arena.list_owners;
    let stable = other_custody(&arena);
    let mut source = SuppliedPolicy::new(true, &[]);
    let mut grid = WorldLightGrid::default();
    assert_eq!(
        run(
            &mut arena,
            &mut grid,
            &mut source,
            Flag0LightFailurePolicy::Stop
        ),
        Err(Flag0LightPassError::UnownedCallback {
            object: unknown,
            reason: ObjectLightCallbackBoundary::TerrainLightStorage
        })
    );
    assert_eq!(arena.list_owners, owners);
    assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&[later]));
    assert_eq!(
        arena.list(ObjectList::Flag0Secondary),
        flag0(&[common, unknown])
    );
    assert_eq!(arena.object(common).unwrap(), &expected_common);
    assert_eq!(arena.object(unknown).unwrap(), &unknown_before);
    assert_eq!(arena.object(later).unwrap(), &later_before);
    assert_eq!(source.reads, ["entry"]);
    assert_eq!(grid.cells(), &[0; WORLD_LIGHT_BYTES]);
    assert_eq!(other_custody(&arena), stable);
    assert_memberships(&arena, &[common, unknown, later]);
}

#[test]
fn deposit_settings_are_fresh_and_not_read_before_a_missing_position() {
    let mut arena = arena();
    let first = light(&mut arena, Some([768, 900, 768]), [5, -7, 11], 512);
    let second = light(&mut arena, Some([768, 800, 768]), [-13, 17, -19], 512);
    let unknown = light(&mut arena, None, [99; 3], 0); // XYZ precedes zero-radius service.
    let mut grid = WorldLightGrid::default();
    grid.set_anchor([768 + 0x1680, 768 + 0x1680]);
    let mut source = SuppliedPolicy::new(true, &[false, true]);
    assert_eq!(
        run(
            &mut arena,
            &mut grid,
            &mut source,
            Flag0LightFailurePolicy::Stop
        ),
        Err(Flag0LightPassError::UnownedCallback {
            object: unknown,
            reason: ObjectLightCallbackBoundary::Position
        })
    );
    assert_eq!(source.reads, ["entry", "deposit", "deposit"]);
    assert!(source.deposits.is_empty());
    assert_eq!(&grid.cells()[..3], &[243, 17, 237]); // Only second deposit ran enabled.
    assert_eq!(
        arena.list(ObjectList::Flag0Secondary),
        flag0(&[first, second, unknown])
    );
    assert!(arena.list(ObjectList::Flag0Primary).is_empty());
}

#[test]
fn selector30_unsigned_clamp_precedes_disabled_and_late_unknown_xyz_paths() {
    for (value, disabled, missing_position) in [
        (u32::MAX, true, false),
        (u32::MAX, false, true),
        (0, false, false),
    ] {
        let mut arena = arena();
        let object = arena.allocate_selector30_actor(allocation()).unwrap();
        let state = selector30_mut(&mut arena, object);
        state.word_178 = value;
        state.byte_18e = u8::from(disabled);
        state.word_174 = 0xf00f_f704;
        let mut expected = arena.object(object).unwrap().clone();
        let ObjectClass::Actor(actor) = &mut expected.class else {
            unreachable!()
        };
        let ActorExtension::Selector30(state) = &mut actor.extension else {
            unreachable!()
        };
        state.word_178 = if value == 0 { 0 } else { 256 };
        let stable = other_custody(&arena);
        let owners = arena.list_owners;
        let mut source = SuppliedPolicy::new(true, &[]);
        let mut grid = WorldLightGrid::default();
        let result = run(
            &mut arena,
            &mut grid,
            &mut source,
            Flag0LightFailurePolicy::Stop,
        );
        if !missing_position {
            assert_eq!(result.unwrap().deposits, 0);
            assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&[object]));
        } else {
            assert_eq!(
                result,
                Err(Flag0LightPassError::UnownedCallback {
                    object,
                    reason: ObjectLightCallbackBoundary::Position
                })
            );
            assert_eq!(arena.list_owners, owners);
            assert_eq!(arena.list(ObjectList::Flag0Secondary), flag0(&[object]));
        }
        assert_eq!(arena.object(object).unwrap(), &expected);
        assert_eq!(source.reads, ["entry"]);
        assert_eq!(grid.cells(), &[0; WORLD_LIGHT_BYTES]);
        assert_eq!(other_custody(&arena), stable);
        assert_memberships(&arena, &[object]);
    }
}

#[test]
fn selector30_uses_clamped_shifted_radius_and_only_low_three_color_bytes() {
    let mut arena = arena();
    let object = arena.allocate_selector30_actor(allocation()).unwrap();
    arena
        .initialize_context(
            object,
            BaseContext {
                position_words: Some([768, 0x8000, 768]),
                placement_index: 12,
            },
        )
        .unwrap();
    let state = selector30_mut(&mut arena, object);
    state.word_178 = 0x8000_0000;
    state.word_174 = 0xf00f_f704;
    let mut expected = arena.object(object).unwrap().clone();
    let ObjectClass::Actor(actor) = &mut expected.class else {
        unreachable!()
    };
    let ActorExtension::Selector30(state) = &mut actor.extension else {
        unreachable!()
    };
    state.word_178 = 256;
    let mut grid = WorldLightGrid::default();
    grid.set_anchor([768 + 0x1680, 768 + 0x1680]);
    let mut source = SuppliedPolicy::new(true, &[true]);
    let result = run(
        &mut arena,
        &mut grid,
        &mut source,
        Flag0LightFailurePolicy::Stop,
    )
    .unwrap();
    assert_eq!(result.light.samples, 32 * 32); // 2048 radius at128 spacing.
    assert_eq!(&grid.cells()[..3], &[4, 247, 15]);
    assert_eq!(arena.object(object).unwrap(), &expected);
    assert_eq!(source.reads, ["entry", "deposit"]);
}

#[test]
fn vehicle_inactive_arm_is_lazy_and_active_branch_priority_is_current_data() {
    for (headlamp, camera, timer, boundary, pulsed) in [
        (0, 0, false, None, false),
        (0, 3, false, None, false),
        (0, u32::MAX, false, None, false),
        // The headlamp reads the position this fixture leaves unknown.
        (
            1,
            1,
            true,
            Some(ObjectLightCallbackBoundary::Position),
            false,
        ),
        (
            0,
            1,
            true,
            Some(ObjectLightCallbackBoundary::VehicleCamera),
            false,
        ),
        (
            0,
            2,
            true,
            Some(ObjectLightCallbackBoundary::VehicleCamera),
            false,
        ),
        // The pulse stores its phase +588 before it reads the position.
        (
            0,
            3,
            true,
            Some(ObjectLightCallbackBoundary::Position),
            true,
        ),
    ] {
        let mut arena = arena();
        let object = vehicle(&mut arena);
        let state = vehicle_mut(&mut arena, object);
        state.byte_208 = headlamp;
        state.dwords_1e8_1f4[0] = camera;
        if timer {
            arena.owned_mut(object).unwrap().state.base.flags |= 0x40000;
        }
        // The genuine vehicle constructor publishes its allocation position.
        // Supply unavailable XYZ only for this read-reach fixture; this
        // overwrite is not a claim about a native writer or constructor.
        assert_eq!(
            arena.object(object).unwrap().base.position_words,
            Some([768, 600, 768])
        );
        arena.owned_mut(object).unwrap().state.base.position_words = None;
        assert_eq!(arena.object(object).unwrap().base.position_words, None);
        let before = arena.object(object).unwrap().clone();
        let stable = other_custody(&arena);
        let owners = arena.list_owners;
        let mut source = SuppliedPolicy::new(true, &[]);
        let mut grid = WorldLightGrid::default();
        let result = run(
            &mut arena,
            &mut grid,
            &mut source,
            Flag0LightFailurePolicy::Stop,
        );
        if let Some(reason) = boundary {
            assert_eq!(
                result,
                Err(Flag0LightPassError::UnownedCallback { object, reason })
            );
            assert_eq!(arena.list_owners, owners);
            assert_eq!(arena.list(ObjectList::Flag0Secondary), flag0(&[object]));
        } else {
            assert_eq!(result.unwrap().callbacks, 1);
            assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&[object]));
        }
        let pulse = &mut vehicle_mut(&mut arena, object).pulse_588;
        assert_eq!(*pulse, pulsed.then_some(0));
        *pulse = None;
        assert_eq!(arena.object(object).unwrap(), &before);
        assert_eq!(source.reads, ["entry"]);
        assert_eq!(grid.cells(), &[0; WORLD_LIGHT_BYTES]);
        assert_eq!(other_custody(&arena), stable);
        assert_memberships(&arena, &[object]);
    }
}

/// `0042036B..00420404`: in the buggy or the truck the rear glow lights 150
/// behind the hull, amber in gear 0 and red in gear 1, unless the camera
/// `00478E00` finds is in first person (+188 = 2); with no camera handle the
/// port stops where `00478E00` would build one.
#[test]
fn the_rear_glow_waits_for_a_third_person_camera() {
    let deposits = |camera: Option<u32>, gear: u32| {
        let mut arena = arena();
        let object = vehicle(&mut arena);
        let state = vehicle_mut(&mut arena, object);
        state.dwords_1e8_1f4[0] = 1;
        state.dword_238 = gear;
        if let Some(first_person) = camera {
            let camera = arena
                .allocate_selector16(
                    allocation(),
                    CameraSettings {
                        first_person,
                        camera_style: 0,
                    },
                )
                .unwrap();
            arena.players.camera_74 = Some(arena.actor(camera).unwrap().handle);
        }
        let mut grid = WorldLightGrid::default();
        let mut source = SuppliedPolicy::new(true, &[true]);
        run(
            &mut arena,
            &mut grid,
            &mut source,
            Flag0LightFailurePolicy::Stop,
        )
        .map(|outcome| outcome.deposits)
    };
    assert_eq!(deposits(Some(0), 0), Ok(1));
    assert_eq!(deposits(Some(0), 1), Ok(1));
    assert_eq!(deposits(Some(0), 2), Ok(0));
    assert_eq!(deposits(Some(1), 0), Ok(0));
    assert!(matches!(
        deposits(None, 0),
        Err(Flag0LightPassError::UnownedCallback {
            reason: ObjectLightCallbackBoundary::VehicleCamera,
            ..
        })
    ));
}

/// A traced frame keeps a buggy's lit rear lamps for the tracer, the red
/// pair in gear 1 and the white in gear 0, and leaves its rear glow out of
/// the grid; an untraced one deposits the glow as retail does.
#[test]
fn a_traced_frame_keeps_the_rear_lamps_and_skips_the_glow() {
    use crate::object_runtime::RearLamps;
    for (gear, pair) in [(1, RearLamps::Red), (0, RearLamps::White)] {
        for traced in [false, true] {
            let mut arena = arena();
            let object = vehicle(&mut arena);
            let state = vehicle_mut(&mut arena, object);
            state.dwords_1e8_1f4[0] = 1;
            state.dword_238 = gear;
            let camera = arena
                .allocate_selector16(
                    allocation(),
                    CameraSettings {
                        first_person: 0,
                        camera_style: 0,
                    },
                )
                .unwrap();
            arena.players.camera_74 = Some(arena.actor(camera).unwrap().handle);
            let mut source = SuppliedPolicy::new(true, if traced { &[] } else { &[true] });
            let mut grid = WorldLightGrid::default();
            let mut lamps = Vec::new();
            let outcome = arena
                .run_flag0_light_pass(Flag0LightPassRequest {
                    grid: &mut grid,
                    policy_source: &mut source,
                    failure_policy: Flag0LightFailurePolicy::Stop,
                    terrain: None,
                    rng: None,
                    world_delta: 0,
                    sine_quarter: &UNUSED_SINE,
                    lamps: traced.then_some(&mut lamps),
                    lamps_light: true,
                })
                .unwrap();
            assert_eq!(
                outcome.deposits,
                usize::from(!traced),
                "gear {gear}, traced {traced}"
            );
            assert_eq!(
                lamps
                    .iter()
                    .map(|lamp| (lamp.head, lamp.rear))
                    .collect::<Vec<_>>(),
                if traced {
                    vec![(false, Some(pair))]
                } else {
                    vec![]
                }
            );
        }
    }
}

#[test]
fn approximation_omits_only_current_callback_then_drains_and_exchanges_headers() {
    let mut arena = arena();
    let unknown = arena.allocate(ObjectClassKind::TerrainLight);
    let actor = vehicle(&mut arena);
    vehicle_mut(&mut arena, actor).byte_208 = 1;
    let clamped = arena.allocate_selector30_actor(allocation()).unwrap();
    selector30_mut(&mut arena, clamped).word_178 = u32::MAX;
    let later = arena.allocate(ObjectClassKind::Base);
    let stable = other_custody(&arena);
    let mut expected: Vec<(ObjectId, ObjectState)> = [unknown, actor, clamped, later]
        .map(|id| (id, arena.object(id).unwrap().clone()))
        .into();
    let ObjectClass::Actor(value) = &mut expected[2].1.class else {
        unreachable!()
    };
    let ActorExtension::Selector30(value) = &mut value.extension else {
        unreachable!()
    };
    value.word_178 = 256;
    expected[3].1.base.flags &= !0x0800_0000;
    let owners = arena.list_owners;
    let mut grid = WorldLightGrid::default();
    let mut source = SuppliedPolicy::new(true, &[]);
    let result = run(
        &mut arena,
        &mut grid,
        &mut source,
        Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
    )
    .unwrap();
    assert_eq!(
        (result.visited, result.callbacks, result.deposits),
        (4, 4, 0)
    );
    assert_eq!(
        result.omissions,
        [
            Flag0LightPassOmission {
                object: unknown,
                reason: ObjectLightCallbackBoundary::TerrainLightStorage
            },
            Flag0LightPassOmission {
                object: actor,
                reason: ObjectLightCallbackBoundary::HeadlampTerrain
            },
            Flag0LightPassOmission {
                object: clamped,
                reason: ObjectLightCallbackBoundary::Position
            },
        ]
    );
    let mut expected_owners = owners;
    expected_owners.swap(
        ObjectList::Flag0Primary as usize,
        ObjectList::Flag0Secondary as usize,
    );
    assert_eq!(arena.list_owners, expected_owners);
    assert_eq!(
        arena.list(ObjectList::Flag0Primary),
        flag0(&[unknown, actor, clamped, later])
    );
    assert!(arena.list(ObjectList::Flag0Secondary).is_empty());
    for (id, state) in expected {
        assert_eq!(arena.object(id).unwrap(), &state);
    }
    assert_eq!(source.reads, ["entry"]);
    assert_eq!(other_custody(&arena), stable);
    assert_memberships(&arena, &[unknown, actor, clamped, later]);
}

#[test]
fn approximation_cannot_admit_an_expired_node_payload() {
    let mut arena = arena();
    let stale = arena.allocate(ObjectClassKind::Base);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: stale,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    // Deliberately supplied invalid typed backing, never a native writer claim.
    let physical = arena.list_owners[ObjectList::Flag0Primary as usize];
    arena.lists[physical].push(ObjectNode {
        object: stale,
        kind: ObjectNodeKind::Flag0,
    });
    let before = format!("{arena:?}");
    let mut source = SuppliedPolicy::new(true, &[]);
    let mut grid = WorldLightGrid::default();
    assert_eq!(
        run(
            &mut arena,
            &mut grid,
            &mut source,
            Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation
        ),
        Err(Flag0LightPassError::Object(ObjectError::UnknownObject(
            stale
        )))
    );
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(source.reads, ["entry"]);
    assert_eq!(grid.cells(), &[0; WORLD_LIGHT_BYTES]);

    let mut arena = self::arena();
    let object = arena.allocate(ObjectClassKind::Base);
    let physical = arena.list_owners[ObjectList::Flag0Primary as usize];
    arena.lists[physical][0].kind = ObjectNodeKind::Flag1;
    let before = format!("{arena:?}");
    let mut source = SuppliedPolicy::new(true, &[]);
    assert_eq!(
        run(
            &mut arena,
            &mut grid,
            &mut source,
            Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation
        ),
        Err(Flag0LightPassError::NodeCustody(ObjectNode {
            object,
            kind: ObjectNodeKind::Flag1
        }))
    );
    assert_eq!(format!("{arena:?}"), before);
    assert_eq!(source.reads, ["entry"]);
    assert_eq!(grid.cells(), &[0; WORLD_LIGHT_BYTES]);
}

/// `0043B8A0`: world 11's generators, radiation on and alive, each draw
/// twice from the process RNG and light the ground twice: a glow of radius
/// `((draw & 0x3F) + 0x60) * 4` in (8, 8, 24), then a shade of 256 in
/// (-8, -8, -24). Without the RNG the pass reports TargetFlicker instead.
#[test]
fn generators_glow_and_shade_the_ground() {
    let (_resident, _assets, mut world) = crate::test_worlds::offline_world_retained(
        11,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let generators: Vec<_> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| world.arena.selector39(actor).is_ok())
        .collect();
    assert_eq!(generators.len(), 3);
    let mut grid = WorldLightGrid::default();
    let mut source = SuppliedPolicy::new(true, &[true; 64]);
    let without = run(
        &mut world.arena,
        &mut grid,
        &mut source,
        Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
    )
    .unwrap();
    let flickers = without
        .omissions
        .iter()
        .filter(|omission| omission.reason == ObjectLightCallbackBoundary::TargetFlicker)
        .count();
    assert_eq!(flickers, 3);
    let mut rng = 0x1234_5678_u32;
    let mut expected = rng;
    let mut source = SuppliedPolicy::new(true, &[true; 64]);
    let with = world
        .arena
        .run_flag0_light_pass(Flag0LightPassRequest {
            grid: &mut grid,
            policy_source: &mut source,
            failure_policy: Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
            terrain: Some(&world.terrain),
            rng: Some(&mut rng),
            world_delta: 0,
            sine_quarter: &UNUSED_SINE,
            lamps: None,
            lamps_light: false,
        })
        .unwrap();
    assert!(with
        .omissions
        .iter()
        .all(|omission| omission.reason != ObjectLightCallbackBoundary::TargetFlicker));
    assert_eq!(with.deposits, without.deposits + 6);
    for _ in 0..6 {
        crate::name_entry::name_entry_rng_next(&mut expected);
    }
    assert_eq!(rng, expected);
}

/// `0043B8A0` against the 40 target lights of
/// `world_interactions_native.json`, run on the original instructions with
/// the real `0048F060`: each deposit's X/Z, radius and colour, and the RNG.
#[test]
fn target_lights_match_the_native_receipt() {
    use crate::object_runtime::ActorConstructorNull;
    let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/world_interactions_native.json"
    ))
    .unwrap();
    let cases = fixture["target_light"].as_array().unwrap();
    assert_eq!(cases.len(), 40);
    let number = |value: &serde_json::Value| value.as_u64().unwrap();
    let mut lit = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let position: Vec<u16> = case["position"]
            .as_array()
            .unwrap()
            .iter()
            .map(|word| number(word) as u16)
            .collect();
        let target = arena
            .allocate_selector39(ActorConstructionRequest {
                constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: Some([position[0], position[1], position[2]]),
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
        arena.selector39_mut(target).unwrap().byte_165 = number(&case["radiation_off"]) as u8;
        arena.actor_mut(target).unwrap().word_c4 = number(&case["state"]) as u32;
        let mut rng = number(&case["rng"]) as u32;
        let lights = arena.target_lights(target, &mut rng).unwrap().unwrap();
        let deposits = case["deposits"].as_array().unwrap();
        assert_eq!(lights.len(), deposits.len(), "{name}");
        for (light, deposit) in lights.iter().zip(deposits) {
            let colour = (number(&deposit["colour"]) as u32).to_le_bytes();
            assert_eq!(
                (
                    light.world_xz[0] as u32,
                    light.world_xz[1] as u32,
                    light.radius as u32,
                    light.rgb.map(|channel| channel as u8),
                ),
                (
                    number(&deposit["x"]) as u32,
                    number(&deposit["z"]) as u32,
                    number(&deposit["radius"]) as u32,
                    [colour[0], colour[1], colour[2]],
                ),
                "{name}"
            );
        }
        assert_eq!(u64::from(rng), number(&case["rng_after"]), "{name}");
        lit += usize::from(!lights.is_empty());
    }
    assert!(lit >= 10, "{lit}");
}

/// `00431DC0` against the 40 factory lights of
/// `world_interactions_native.json`, run on the original instructions with
/// the real `0048F060`: each deposit's X/Z, radius and colour, and the RNG.
#[test]
fn factory_lights_match_the_native_receipt() {
    use crate::object_runtime::ActorConstructorNull;
    let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/world_interactions_native.json"
    ))
    .unwrap();
    let cases = fixture["factory_light"].as_array().unwrap();
    assert_eq!(cases.len(), 40);
    let number = |value: &serde_json::Value| value.as_u64().unwrap();
    let (mut grants, mut chimneys) = (0, 0);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 17,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let factory = arena
            .allocate_selector15(ActorConstructionRequest {
                constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: Some([0, 0, 0]),
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
        arena.actor_mut(factory).unwrap().rotating.visual.model_word =
            number(&case["model"]) as u16;
        let dock: Vec<u16> = case["dock"]
            .as_array()
            .unwrap()
            .iter()
            .map(|word| number(word) as u16)
            .collect();
        let state = arena.selector15_mut(factory).unwrap();
        state.word_160 = number(&case["state"]) as u32;
        state.words_178_182 = Some([0, 0, 0, 0, number(&case["word_180"]) as u16, 0]);
        state.dock_190 = Some([dock[0], dock[1], dock[2]]);
        let mut rng = number(&case["rng"]) as u32;
        let lights = arena.factory_lights(factory, &mut rng).unwrap().unwrap();
        let deposits = case["deposits"].as_array().unwrap();
        assert_eq!(lights.len(), deposits.len(), "{name}");
        for (light, deposit) in lights.iter().zip(deposits) {
            let colour = (number(&deposit["colour"]) as u32).to_le_bytes();
            assert_eq!(
                (
                    light.world_xz[0] as u32,
                    light.world_xz[1] as u32,
                    light.radius as u32,
                    light.rgb.map(|channel| channel as u8),
                ),
                (
                    number(&deposit["x"]) as u32,
                    number(&deposit["z"]) as u32,
                    number(&deposit["radius"]) as u32,
                    [colour[0], colour[1], colour[2]],
                ),
                "{name}"
            );
            grants += usize::from(light.rgb == FACTORY_GRANT);
            chimneys += usize::from(light.rgb == FACTORY_CHIMNEY);
        }
        assert_eq!(u64::from(rng), number(&case["rng_after"]), "{name}");
    }
    assert!(grants >= 5 && chimneys >= 5, "{grants} {chimneys}");
}

/// `00431DC0` over Helniazine's factories, each made a chimney in the grant
/// state: the pass lights each dock point twice and draws twice from the
/// RNG; without the RNG it reports FactoryProduction instead.
#[test]
fn producing_factories_light_their_dock_points() {
    let (_resident, _assets, mut world) = crate::test_worlds::offline_world_retained(
        6,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let factories: Vec<_> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| world.arena.selector15(actor).is_ok())
        .collect();
    assert!(!factories.is_empty());
    for &factory in &factories {
        world
            .arena
            .actor_mut(factory)
            .unwrap()
            .rotating
            .visual
            .model_word = FACTORY_CHIMNEY_MODEL;
        let state = world.arena.selector15_mut(factory).unwrap();
        state.word_160 = 5;
        state.words_178_182 = Some([0, 0, 0, 0, 1, 0]);
        state.dock_190 = Some([0x1000, 0, 0x2000]);
    }
    let mut grid = WorldLightGrid::default();
    let mut source = SuppliedPolicy::new(true, &[true; 512]);
    let without = run(
        &mut world.arena,
        &mut grid,
        &mut source,
        Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
    )
    .unwrap();
    let producing = without
        .omissions
        .iter()
        .filter(|omission| omission.reason == ObjectLightCallbackBoundary::FactoryProduction)
        .count();
    assert_eq!(producing, factories.len());
    // Its generators light from the RNG too, two lights and two draws each.
    let lit = factories.len()
        + without
            .omissions
            .iter()
            .filter(|omission| omission.reason == ObjectLightCallbackBoundary::TargetFlicker)
            .count();
    let mut rng = 0x1234_5678_u32;
    let mut expected = rng;
    let mut source = SuppliedPolicy::new(true, &[true; 512]);
    let with = world
        .arena
        .run_flag0_light_pass(Flag0LightPassRequest {
            grid: &mut grid,
            policy_source: &mut source,
            failure_policy: Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
            terrain: Some(&world.terrain),
            rng: Some(&mut rng),
            world_delta: 0,
            sine_quarter: &UNUSED_SINE,
            lamps: None,
            lamps_light: false,
        })
        .unwrap();
    assert!(with
        .omissions
        .iter()
        .all(|omission| omission.reason != ObjectLightCallbackBoundary::FactoryProduction));
    assert_eq!(with.deposits, without.deposits + 2 * lit);
    for _ in 0..2 * lit {
        crate::name_entry::name_entry_rng_next(&mut expected);
    }
    assert_eq!(rng, expected);
}

#[derive(serde::Deserialize)]
struct PulseFixture {
    cases: Vec<PulseCase>,
}

#[derive(serde::Deserialize)]
struct PulseCase {
    input: PulseInput,
    output: PulseOutput,
}

#[derive(serde::Deserialize)]
struct PulseInput {
    flags: u32,
    phase: u32,
    delta: u32,
    position: [u16; 3],
}

#[derive(serde::Deserialize)]
struct PulseOutput {
    phase: u32,
    light: Option<PulseLight>,
}

#[derive(serde::Deserialize)]
struct PulseLight {
    x: u32,
    z: u32,
    radius: u32,
    rgb: u32,
}

/// `0042041C`, the vehicle light's ECM pulse, against the original segment
/// under Unicorn (`check_vehicle_pulse.py`; no recording carries the ECM
/// Unit with the light pass on): with the flag, `+588` gains the world's
/// delta and the hull's X/Z light up within 0x400 in the three waves
/// (`>> 12`); without it nothing changes.
#[test]
fn the_ecm_pulse_matches_the_native_segment() {
    let fixture: PulseFixture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/vehicle_pulse_native.json"
    ))
    .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = crate::frontend_assets::FrontendAssets::open(&root)
        .unwrap()
        .sine_table()
        .values;
    assert_eq!(fixture.cases.len(), 26);
    for (index, case) in fixture.cases.iter().enumerate() {
        let mut arena = arena();
        let object = vehicle(&mut arena);
        // The hovercraft lights neither headlamp nor rear glow.
        let state = vehicle_mut(&mut arena, object);
        state.dwords_1e8_1f4[0] = 3;
        state.pulse_588 = Some(case.input.phase);
        let base = &mut arena.owned_mut(object).unwrap().state.base;
        base.flags = case.input.flags;
        base.position_words = Some(case.input.position);
        let lights = arena
            .vehicle_lights(object, None, case.input.delta, &sine)
            .unwrap()
            .unwrap();
        assert_eq!(lights.boundary, None, "case {index}");
        let expected: Vec<TerrainLightRequest> = case
            .output
            .light
            .iter()
            .map(|light| {
                let [r, g, b, _] = light.rgb.to_le_bytes();
                TerrainLightRequest {
                    world_xz: [light.x as i32, light.z as i32],
                    radius: light.radius as i32,
                    rgb: [r as i8, g as i8, b as i8],
                }
            })
            .collect();
        assert_eq!(lights.lights, expected, "case {index}");
        // The tracer takes the hull as where they shine from.
        assert_eq!(
            lights.hull,
            Some(case.input.position.map(|word| i32::from(word as i16))),
            "case {index}"
        );
        let phase = vehicle_mut(&mut arena, object).pulse_588;
        let unchanged = case.output.light.is_none().then_some(case.input.phase);
        assert_eq!(
            phase,
            Some(unchanged.unwrap_or(case.output.phase)),
            "case {index}"
        );
        assert_eq!(case.output.phase, unchanged.unwrap_or(case.output.phase));
    }
}

/// A traced frame keeps a lit vehicle's lamp for the tracer and leaves its
/// headlamp's pool out of the grid; the raster's keeps the lamp for its
/// water and deposits the pool as retail does, as a frame keeping no lamps
/// does.
#[test]
fn a_traced_frame_keeps_the_lamp_and_skips_the_pool() {
    let terrain = TerrainRuntime::from_raw_cells(
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
        0,
        std::iter::repeat(0).take(512 * 512),
    );
    for (kept, traced) in [(false, false), (true, true), (true, false)] {
        let mut arena = arena();
        let object = vehicle(&mut arena);
        let state = vehicle_mut(&mut arena, object);
        state.byte_208 = 1;
        // The hovercraft lights no rear glow.
        state.dwords_1e8_1f4[0] = 3;
        let mut source = SuppliedPolicy::new(true, if traced { &[] } else { &[true] });
        let mut grid = WorldLightGrid::default();
        let mut lamps = Vec::new();
        let outcome = arena
            .run_flag0_light_pass(Flag0LightPassRequest {
                grid: &mut grid,
                policy_source: &mut source,
                failure_policy: Flag0LightFailurePolicy::Stop,
                terrain: Some(&terrain),
                rng: None,
                world_delta: 0,
                sine_quarter: &UNUSED_SINE,
                lamps: kept.then_some(&mut lamps),
                lamps_light: traced,
            })
            .unwrap();
        assert_eq!(outcome.deposits, usize::from(!traced), "traced {traced}");
        assert_eq!(
            lamps.iter().map(|lamp| lamp.hull).collect::<Vec<_>>(),
            if kept { vec![[768, 600, 768]] } else { vec![] }
        );
    }
}
