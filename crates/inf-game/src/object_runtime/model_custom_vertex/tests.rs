use super::*;
use crate::{
    menu::MenuBasis,
    model_context::ModelContext,
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        DerivedActorConstructionRequest, ObjectClassKind,
    },
};
use inf_formats::{
    decode_model_program, ByteSpan, ModelEntry, ModelFlags, ModelHeader, ModelIdentity,
    ModelVectorI16,
};

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn allocation() -> DerivedActorConstructionRequest<'static> {
    static UNUSED_SINE: [u16; 4096] = [0; 4096];
    // Real H12NULL construction reaches neither unknown XYZ nor this table.
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
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
        },
        world_mask_limits: [3, 3],
    }
}

fn subject(arena: &mut ObjectArena, behavior: bool) -> ObjectId {
    let source = arena
        .allocate_selector11_actor(allocation(), &mut 17)
        .unwrap();
    assert!(arena.derived_actor(source).unwrap().behavior_140.is_none());
    if behavior {
        // Supplied current guard input; the actual shared admission constructs
        // retained Mode06 storage. No original overlay or class is replaced.
        arena.derived_actor_mut(source).unwrap().byte_13d = Some(6);
        assert!(arena.admit_derived_actor(source).unwrap());
    }
    source
}

pub(crate) fn vertices(world: [i32; 3]) -> ModelVertices {
    let span = ByteSpan::new(0, 0);
    let model = ModelEntry {
        identity: ModelIdentity {
            local_index: 0,
            global_index: 0,
        },
        span,
        content_span: span,
        header_span: span,
        header: ModelHeader {
            visibility_extent: 0,
            command_word_count: 1,
            auxiliary_count: 0,
            flags: ModelFlags::from_raw(0),
            vertex_slot_count: 4,
            raw_vertex_count: 1,
            shade_slot_count: 0,
            raw_0e: 0,
            reserved_10: 0,
            vertex_ranges: [0, 0, 2, 2],
            reserved_1a: 0,
            shade_ranges: [0; 2],
        },
        auxiliary_span: span,
        auxiliary: vec![],
        auxiliary_program: None,
        vertices_span: span,
        vertices: vec![ModelVectorI16 { x: 0, y: 0, z: 0 }],
        normals_span: span,
        normals: vec![],
        command_span: span,
        program: decode_model_program(&[0]).unwrap(),
        name: None,
        padding: span,
    };
    let mut vertices = ModelVertices::new_invocation(4);
    vertices
        .initialize_world_ranges(
            &model,
            &ModelContext::new(world, MenuBasis::INITIAL),
            [0, 0, 2],
        )
        .unwrap();
    vertices
}

fn compute(
    arena: &mut ObjectArena,
    source: ObjectId,
    selector: i32,
    extras: &[u16],
    vertices: &ModelVertices,
    output: &mut Option<[i32; 3]>,
) -> Result<(), ObjectCustomVertexError> {
    arena.compute_model_custom_vertex(ObjectCustomVertexRequest {
        source,
        compute: ModelCustomVertexRequest { selector, extras },
        vertices,
        output,
        effects: None,
    })
}

#[derive(serde::Deserialize)]
struct Selector60Receipt {
    cases: Vec<Selector60Case>,
}

#[derive(serde::Deserialize)]
struct Selector60Case {
    name: String,
    selector: i32,
    extras: Vec<u16>,
    latch: u8,
    cache: [i16; 3],
    initial_output: [i32; 3],
    load_factor: i32,
    rng: u32,
    rows: std::collections::BTreeMap<String, [i32; 3]>,
    output: [i32; 3],
    events: Vec<serde_json::Value>,
    latch_after: u8,
    cache_after: [i16; 3],
    rng_after: u32,
}

/// Records each draw and trail in the receipt's shape.
struct RecordingTrailEffects {
    load_factor: i32,
    rng: u32,
    events: Vec<serde_json::Value>,
}

impl ObjectCustomVertexEffects for RecordingTrailEffects {
    fn load_factor(&mut self) -> Result<i32, String> {
        Ok(self.load_factor)
    }

    fn random(&mut self) -> Result<u32, String> {
        let value = crate::name_entry::name_entry_rng_next(&mut self.rng);
        self.events.push(
            serde_json::json!({"kind": "random", "value": value & 0xffff, "state": self.rng}),
        );
        Ok(value)
    }

    fn birth_trail(&mut self, trail: FurnitureCustomTrail) -> Result<(), String> {
        self.events.push(serde_json::json!({
            "kind": "trail",
            "particle_type": trail.particle_type,
            "position": trail.position,
            "velocity": trail.velocity,
            "flags": trail.flags,
        }));
        Ok(())
    }

    fn legs(&mut self) -> Result<ObjectLegServices<'_>, String> {
        Err("selector60 receipts lend no leg services".into())
    }

    fn birth_material_sprite(
        &mut self,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), String> {
        self.events.push(serde_json::json!({
            "kind": "sprite",
            "position": position,
            "velocity": velocity,
        }));
        Ok(())
    }

    fn birth_impact(&mut self, position: [u16; 3]) -> Result<(), String> {
        self.events
            .push(serde_json::json!({"kind": "impact", "position": position}));
        Ok(())
    }
}

/// 00480CF0 run unchanged: exact7's row, load-factor gate, both trail types
/// and the output-pointer type, plus exact9's latch and cache. The output
/// pointer is never a valid 00489D90 type, so the port births nothing there.
#[test]
fn selector60_callback_matches_original_00480cf0() {
    let receipt: Selector60Receipt = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector60_custom_native.json"
    ))
    .unwrap();
    let (mut trails, mut rejected) = (0, 0);
    for case in &receipt.cases {
        let mut arena = arena();
        let source = selector60_custom_subject(&mut arena);
        let crate::object_runtime::DerivedActorClass::Selector60(state) =
            &mut arena.derived_actor_mut(source).unwrap().class
        else {
            panic!("real selector60");
        };
        state.byte_184 = case.latch;
        state.position_198 = Some(case.cache.map(|word| word as u16));
        let mut vertices = crate::model_vertices::ModelVertices::new_invocation(0xffff);
        for (index, row) in &case.rows {
            vertices.seed_world(index.parse().unwrap(), *row);
        }
        let mut output = Some(case.initial_output);
        let mut effects = RecordingTrailEffects {
            load_factor: case.load_factor,
            rng: case.rng,
            events: Vec::new(),
        };
        arena
            .compute_model_custom_vertex(ObjectCustomVertexRequest {
                source,
                compute: ModelCustomVertexRequest {
                    selector: case.selector,
                    extras: &case.extras,
                },
                vertices: &vertices,
                output: &mut output,
                effects: Some(&mut effects),
            })
            .unwrap();
        assert_eq!(output, Some(case.output), "{}", case.name);
        let expected: Vec<_> = case
            .events
            .iter()
            .filter(|event| event["particle_type"] != "output")
            .cloned()
            .collect();
        rejected += case.events.len() - expected.len();
        trails += expected
            .iter()
            .filter(|event| event["kind"] == "trail")
            .count();
        assert_eq!(effects.events, expected, "{}", case.name);
        assert_eq!(effects.rng, case.rng_after, "{}", case.name);
        let state = arena.selector60_actor(source).unwrap();
        assert_eq!(state.byte_184, case.latch_after, "{}", case.name);
        assert_eq!(
            state.position_198,
            Some(case.cache_after.map(|word| word as u16)),
            "{}",
            case.name
        );
    }
    assert_eq!((receipt.cases.len(), trails, rejected), (46, 25, 4));
}

#[test]
fn selector60_full7_requires_trail_services_after_publishing_its_row() {
    let mut arena = arena();
    let source = selector60_custom_subject(&mut arena);
    let mut output = Some([7, 8, 9]);
    assert_eq!(
        compute(
            &mut arena,
            source,
            7,
            &[0, 1],
            &vertices([5, -6, 7]),
            &mut output
        ),
        Err(ObjectCustomVertexError::RequiredEffects {
            object: source,
            selector: 7
        })
    );
    assert_eq!(output, Some([5, -6, 7]));
}

#[test]
fn null_behavior_reads_only_the_first_extra_and_masks_the_selector_to_a_byte() {
    let mut arena = arena();
    let source = subject(&mut arena, false);
    let vertices = vertices([0x1_8001, -0x1_8000, 0x1_ffff]);
    let before = format!("{arena:?}");
    for selector in [2, 0x102, -254, i32::MIN + 2] {
        let mut output = None;
        compute(&mut arena, source, selector, &[0], &vertices, &mut output).unwrap();
        assert_eq!(output, Some([-32767, -32768, -1]));
        assert_eq!(format!("{arena:?}"), before);
    }
    let mut output = Some([7, 8, 9]);
    assert_eq!(
        compute(&mut arena, source, 3, &[], &vertices, &mut output),
        Err(ObjectCustomVertexError::Unowned {
            object: source,
            selector: 3
        })
    );
    assert_eq!(output, Some([7, 8, 9]));
}

#[test]
fn current_slot_errors_precede_optional_behavior_and_unsigned_group_extra() {
    let mut arena = arena();
    let source = subject(&mut arena, true);
    let before = format!("{arena:?}");
    let mut output = Some([7, 8, 9]);
    let vertices = vertices([1, 2, 3]);
    for (extras, error) in [
        (
            vec![],
            ObjectCustomVertexError::RequiredExtra {
                object: source,
                selector: 2,
                index: 0,
            },
        ),
        (
            vec![0xffff],
            ObjectCustomVertexError::Vertex(ModelVertexFault::InvalidSlot {
                index: 65535,
                count: 4,
            }),
        ),
        (
            vec![2],
            ObjectCustomVertexError::Vertex(ModelVertexFault::UnknownWorld { index: 2 }),
        ),
        (
            vec![0],
            ObjectCustomVertexError::RequiredExtra {
                object: source,
                selector: 2,
                index: 1,
            },
        ),
    ] {
        assert_eq!(
            compute(&mut arena, source, 2, &extras, &vertices, &mut output),
            Err(error)
        );
        assert_eq!(output, Some([7, 8, 9]));
        assert_eq!(format!("{arena:?}"), before);
    }
    let unsigned = ModelVertices::new_invocation(u16::MAX);
    assert_eq!(
        compute(&mut arena, source, 2, &[0x8000], &unsigned, &mut output),
        Err(ObjectCustomVertexError::Vertex(
            ModelVertexFault::UnknownWorld { index: 32768 }
        ))
    );
}

#[test]
fn signed_group_count_bypass_does_not_read_group_position_or_basis() {
    let mut arena = arena();
    let source = subject(&mut arena, true);
    let vertices = vertices([1, 2, 3]);
    for (count, index) in [
        (0xff, 0),
        (0x80, 0),
        (0, 0),
        (1, 1),
        (127, 127),
        (1, u16::MAX),
    ] {
        let behavior = arena
            .derived_actor_mut(source)
            .unwrap()
            .behavior_140
            .as_mut()
            .unwrap();
        behavior.byte_0d = count;
        behavior.groups_1c.clear();
        let before = format!("{arena:?}");
        let mut output = None;
        compute(&mut arena, source, 2, &[0, index], &vertices, &mut output).unwrap();
        assert_eq!(output, Some([1, 2, 3]));
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn reached_short_group_backing_is_an_error_without_output_or_fabricated_storage() {
    let mut arena = arena();
    let source = subject(&mut arena, true);
    arena
        .derived_actor_mut(source)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap()
        .groups_1c
        .clear();
    let before = format!("{arena:?}");
    let mut output = None;
    assert_eq!(
        compute(
            &mut arena,
            source,
            2,
            &[0, 0],
            &vertices([1, 2, 3]),
            &mut output
        ),
        Err(ObjectCustomVertexError::BehaviorGroup {
            object: source,
            index: 0,
            groups: 0
        })
    );
    assert_eq!(output, None);
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn first_unknown_actor_x_retains_world_capture_and_cleared_bit_before_stopping() {
    let mut arena = arena();
    let source = subject(&mut arena, true);
    let behavior = arena
        .derived_actor_mut(source)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap();
    behavior.groups_1c[0].word_0c = 0xa4;
    assert_eq!(behavior.groups_1c[0].coordinates_00_0a, [None; 6]);
    let mut output = Some([7, 8, 9]);
    assert_eq!(
        compute(
            &mut arena,
            source,
            2,
            &[0, 0],
            &vertices([100, -200, 300]),
            &mut output
        ),
        Err(ObjectCustomVertexError::RequiredField {
            object: source,
            offset: 0x32
        })
    );
    let group = &arena
        .derived_actor(source)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .groups_1c[0];
    assert_eq!(
        group.coordinates_00_0a,
        [
            None,
            None,
            None,
            Some(100),
            Some(-200_i16 as u16),
            Some(300)
        ]
    );
    assert_eq!(
        group.dwords_00_08(),
        [None, None, Some(u32::from(-200_i16 as u16) | (300 << 16))]
    );
    assert_eq!(group.word_0c, 0xa0);
    assert_eq!(
        output,
        Some([7, 8, 9]),
        "failed source callback does not publish or rebase its temporary"
    );
}

#[test]
fn clear_initialization_bit_skips_unknown_relative_position_and_basis_reads() {
    let mut arena = arena();
    let source = subject(&mut arena, true);
    arena
        .derived_actor_mut(source)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap()
        .groups_1c[0]
        .word_0c = 0xa0;
    let mut output = None;
    compute(
        &mut arena,
        source,
        2,
        &[0, 0],
        &vertices([1, 2, 3]),
        &mut output,
    )
    .unwrap();
    let group = &arena
        .derived_actor(source)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .groups_1c[0];
    assert_eq!(
        group.coordinates_00_0a,
        [None, None, None, Some(1), Some(2), Some(3)]
    );
    assert_eq!(group.word_0c, 3);
    assert_eq!(output, Some([1, 2, 3]));
}

#[test]
fn first_capture_uses_wrapping_word_deltas_and_nearest_even_current_basis_then_retains_relative() {
    let mut arena = arena();
    let source = subject(&mut arena, true);
    arena.owned_mut(source).unwrap().state.base.position_words = Some([0xffff, 0x7fff, 5]);
    let basis = &mut arena.actor_mut(source).unwrap().rotating.basis.words;
    // X'=X/2, Y'=Y/2, Z'=Z. Fractional ties discriminate truncation.
    basis[1] = 0.5_f32.to_bits();
    basis[5] = 0.5_f32.to_bits();
    let mut output = None;
    compute(
        &mut arena,
        source,
        2,
        &[0, 0],
        &vertices([2, 0x8004, 11]),
        &mut output,
    )
    .unwrap();
    let group = &arena
        .derived_actor(source)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .groups_1c[0];
    assert_eq!(
        group.coordinates_00_0a,
        [Some(2), Some(2), Some(6), Some(2), Some(0x8004), Some(11)]
    );
    assert_eq!(group.word_0c, 3);
    assert_eq!(
        output,
        Some([2, -32764, 11]),
        "behavior transform does not modify captured slot output"
    );
    arena.owned_mut(source).unwrap().state.base.position_words = None;
    compute(
        &mut arena,
        source,
        2,
        &[0, 0],
        &vertices([9, 8, 7]),
        &mut output,
    )
    .unwrap();
    let group = &arena
        .derived_actor(source)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .groups_1c[0];
    assert_eq!(
        group.coordinates_00_0a,
        [Some(2), Some(2), Some(6), Some(9), Some(8), Some(7)]
    );
    assert_eq!(group.word_0c, 3);
    assert_eq!(output, Some([9, 8, 7]));
}

#[test]
fn supported_policy_comes_from_actual_nested_class_and_excludes_other_final_callbacks() {
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let grants =
        inf_formats::parse_grant_table(inf_formats::FrontendExecutable::parse(&raw).unwrap())
            .unwrap();
    let mut arena = arena();
    let mut rng = 17;
    let sources = [
        arena
            .allocate_derived_actor(allocation(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector11_actor(allocation(), &mut rng)
            .unwrap(),
        arena
            .allocate_family428650_actor(allocation(), &mut rng)
            .unwrap(),
        arena
            .allocate_family429e20_actor(allocation(), &mut rng)
            .unwrap(),
        arena
            .allocate_family4236d0_actor(allocation(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector34_actor(allocation(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector35_actor(allocation(), &mut rng)
            .unwrap(),
        arena
            .allocate_selector12_actor(
                allocation(),
                &mut rng,
                crate::object_runtime::Selector12GrantContext {
                    table: &grants,
                    first_player_grants: None,
                },
            )
            .unwrap(),
        arena
            .allocate_selector36_actor(allocation(), &mut rng)
            .unwrap(),
        // 00425870 forwards every low byte but 0, 1 and 5 to 004260C0.
        arena
            .allocate_selector10_actor(allocation(), &mut rng)
            .unwrap(),
    ];
    for source in sources {
        // Numeric B4 and resource identity cannot select a callback policy.
        arena.actor_mut(source).unwrap().word_b4 = 2;
        arena.owned_mut(source).unwrap().state.base.resource_word = 2;
        let mut output = None;
        compute(
            &mut arena,
            source,
            2,
            &[0],
            &vertices([1, 2, 3]),
            &mut output,
        )
        .unwrap();
        assert_eq!(output, Some([1, 2, 3]));
        arena.copy_actor_position(source, [7, 0xffff, 9]).unwrap();
        for selector in [10, 0x010a, i32::from(0xff0au16 as i16)] {
            compute(
                &mut arena,
                source,
                selector,
                &[0],
                &vertices([1, 2, 3]),
                &mut output,
            )
            .unwrap();
            assert_eq!(arena.actor(source).unwrap().word_74, 3);
            assert_eq!(output, Some([1, 2, 3]));
        }
    }
    // The legs' own low bytes need the world services before any state.
    let legs = sources[sources.len() - 1];
    let before = format!("{arena:?}");
    for selector in [0, 1, 5, 0x0105] {
        let mut output = None;
        assert_eq!(
            compute(
                &mut arena,
                legs,
                selector,
                &[0],
                &vertices([1, 2, 3]),
                &mut output
            ),
            Err(ObjectCustomVertexError::RequiredEffects {
                object: legs,
                selector
            })
        );
        assert_eq!(output, None);
        assert_eq!(format!("{arena:?}"), before);
    }
    let excluded = [
        arena.allocate(ObjectClassKind::Base),
        arena
            .allocate_selector60_actor(allocation(), &mut rng)
            .unwrap(),
    ];
    for source in excluded {
        let before = format!("{arena:?}");
        for selector in [2, 10, 0x010a] {
            let mut output = None;
            assert_eq!(
                compute(
                    &mut arena,
                    source,
                    selector,
                    &[],
                    &vertices([1, 2, 3]),
                    &mut output
                ),
                Err(ObjectCustomVertexError::Unowned {
                    object: source,
                    selector
                })
            );
            assert_eq!(output, None);
            assert_eq!(format!("{arena:?}"), before);
        }
    }
}

/// 0047A240 on the shared table and on a class that keeps it (Terra Nova's
/// generator, selector 39): exact full DWORD 10 is 004260C0's capture and
/// WORD 74; every other selector is the ground and needs its services.
#[test]
fn shared_actor_full10_captures_and_other_selectors_reach_the_ground() {
    let mut arena = arena();
    let sources = [
        arena.allocate_actor(allocation().actor).unwrap(),
        arena.allocate_selector39(allocation().actor).unwrap(),
    ];
    for source in sources {
        arena.copy_actor_position(source, [7, 0xffff, 9]).unwrap();
        let mut output = None;
        compute(
            &mut arena,
            source,
            10,
            &[0],
            &vertices([1, 2, 3]),
            &mut output,
        )
        .unwrap();
        assert_eq!(arena.actor(source).unwrap().word_74, 3);
        assert_eq!(output, Some([1, 2, 3]));
        let before = format!("{arena:?}");
        for selector in [0, 2, 0x010a, i32::from(0xff0au16 as i16)] {
            let mut output = None;
            assert_eq!(
                compute(
                    &mut arena,
                    source,
                    selector,
                    &[0],
                    &vertices([1, 2, 3]),
                    &mut output
                ),
                Err(ObjectCustomVertexError::RequiredEffects {
                    object: source,
                    selector
                })
            );
            assert_eq!(output, None);
            assert_eq!(format!("{arena:?}"), before);
        }
    }
}

#[test]
fn all_original_tables_and_complete_callback_bodies_guard_the_shared_policy() {
    use inf_formats::{constructor_dispatch, FrontendExecutable};
    use sha2::{Digest, Sha256};
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let executable = FrontendExecutable::parse(&raw).unwrap();
    //All94 final class rows, including unowned callback policies. The literal
    //native table/entry census is independent from Rust's class dispatch.
    for (selector, table, callback) in [
        (1, 0x00527b24, 0x0047a000_u32),
        (2, 0x00520860, 0x00415d30_u32),
        (3, 0x00527c00, 0x0047a240_u32),
        (4, 0x00527c00, 0x0047a240_u32),
        (5, 0x00527c00, 0x0047a240_u32),
        (6, 0x00527c00, 0x0047a240_u32),
        (7, 0x00520e58, 0x004260c0_u32),
        (8, 0x00520b84, 0x004260c0_u32),
        (9, 0x00520b84, 0x004260c0_u32),
        (10, 0x00520cc8, 0x00425870_u32),
        (11, 0x0051f080, 0x004260c0_u32),
        (12, 0x00527058, 0x004260c0_u32),
        (13, 0x005269c4, 0x004260c0_u32),
        (14, 0x00527208, 0x0047a1e0_u32),
        (15, 0x00521940, 0x0042e3d0_u32),
        (16, 0x005204d0, 0x0047a240_u32),
        (17, 0x005272a0, 0x0047a240_u32),
        (18, 0x005279e0, 0x0047a240_u32),
        (19, 0x00526978, 0x0047a1e0_u32),
        (20, 0x00525030, 0x0047a240_u32),
        (21, 0x0051f7a8, 0x00425870_u32),
        (22, 0x00521830, 0x0047a1e0_u32),
        (23, 0x005203c0, 0x0047a240_u32),
        (24, 0x0052577c, 0x0047a240_u32),
        (25, 0x00525b88, 0x0047a240_u32),
        (26, 0x005268ac, 0x0047a240_u32),
        (27, 0x00525848, 0x0047a240_u32),
        (28, 0x005259e8, 0x0047a240_u32),
        (29, 0x00525ab4, 0x0047a240_u32),
        (30, 0x005289c0, 0x0048b290_u32),
        (31, 0x00528524, 0x0047a240_u32),
        (32, 0x005285f0, 0x0047a240_u32),
        (33, 0x00520e58, 0x004260c0_u32),
        (34, 0x00521eac, 0x004260c0_u32),
        (35, 0x0052273c, 0x004260c0_u32),
        (36, 0x00527ee0, 0x004823f0_u32),
        (37, 0x0051ff54, 0x0047a240_u32),
        (38, 0x00525914, 0x0047a240_u32),
        (39, 0x00522398, 0x0047a240_u32),
        (40, 0x005260a0, 0x0047a240_u32),
        (41, 0x005267e0, 0x0047a240_u32),
        (42, 0x0051f018, 0x0047a1e0_u32),
        (43, 0x00525730, 0x0047a1e0_u32),
        (44, 0x00525648, 0x0047a240_u32),
        (45, 0x0052616c, 0x0047a1e0_u32),
        (46, 0x005278a0, 0x0047a1e0_u32),
        (47, 0x00527cd4, 0x0047fa80_u32),
        (48, 0x005225d0, 0x0047a1e0_u32),
        (49, 0x00527908, 0x0047a240_u32),
        (50, 0x00522888, 0x0047a1e0_u32),
        (51, 0x0051f658, 0x0047a1e0_u32),
        (52, 0x00526714, 0x0047a240_u32),
        (53, 0x00526648, 0x0047a240_u32),
        (54, 0x005265fc, 0x0047a1e0_u32),
        (55, 0x005274c4, 0x0047a240_u32),
        (56, 0x00526530, 0x0047a240_u32),
        (57, 0x00526464, 0x0047a240_u32),
        (58, 0x00527838, 0x0047a1e0_u32),
        (59, 0x0051f900, 0x0047a240_u32),
        (60, 0x00527de8, 0x00480cf0_u32),
        (61, 0x00520700, 0x00413060_u32),
        (62, 0x00526dc8, 0x0047a240_u32),
        (63, 0x005229d8, 0x0047a1e0_u32),
        (64, 0x00522a28, 0x0047a1e0_u32),
        (65, 0x0052187c, 0x0047a1e0_u32),
        (66, 0x0051f6c0, 0x0047a240_u32),
        (67, 0x005286d8, 0x0047a000_u32),
        (68, 0x00526398, 0x0047a240_u32),
        (69, 0x005205d8, 0x0047a240_u32),
        (70, 0x00527390, 0x0047a1e0_u32),
        (71, 0x005273dc, 0x0047a1e0_u32),
        (72, 0x00527428, 0x0047a1e0_u32),
        (73, 0x005262cc, 0x0047a240_u32),
        (74, 0x00521764, 0x0047a240_u32),
        (75, 0x00522008, 0x0047a000_u32),
        (76, 0x00521a40, 0x0047a1e0_u32),
        (77, 0x0051fbb0, 0x004074e0_u32),
        (78, 0x0051fa9c, 0x0047a240_u32),
        (79, 0x0051fc8c, 0x004074e0_u32),
        (80, 0x0051fd68, 0x004074e0_u32),
        (81, 0x0051fe44, 0x0047a000_u32),
        (82, 0x0051fe88, 0x0047a240_u32),
        (83, 0x00522570, 0x0047a1e0_u32),
        (84, 0x00522488, 0x0047a240_u32),
        (85, 0x0051f9cc, 0x0047a240_u32),
        (86, 0x005218cc, 0x0047a000_u32),
        (87, 0x00528ab0, 0x0047a000_u32),
        (88, 0x00524ac0, 0x0047a240_u32),
        (89, 0x00527254, 0x0047a1e0_u32),
        (90, 0x00526b50, 0x0047a240_u32),
        (91, 0x0051f1e0, 0x0047a240_u32),
        (92, 0x0051f2b0, 0x0047a1e0_u32),
        (93, 0x005228f0, 0x0047a240_u32),
        (94, 0x00524e20, 0x0047a000_u32),
    ] {
        assert_eq!(constructor_dispatch(selector).unwrap().vtable, table);
        assert_eq!(
            executable.mapped(table + 0x10, 4).unwrap().bytes,
            callback.to_le_bytes()
        );
    }
    for table in [0x0052_0f34, 0x0052_1164] {
        assert_eq!(
            executable.mapped(table + 0x10, 4).unwrap().bytes,
            0x0042_60c0_u32.to_le_bytes()
        );
    }
    //Whole source bodies through their final return. Guarding the complete
    //constructor/guard/destructor boundaries keeps actual NULL/nonNULL140
    //custody separate from an unsupported allocation alias or later writer.
    for (address, length, expected) in [
        (
            0x004260c0_usize,
            426,
            "c0f6e5a12139e537f429053b3d226941b64649482e2fc5941e1dbfc37f7da5ed",
        ),
        (
            0x0047a240_usize,
            166,
            "abe4213966999896af14f330161e6d78734bb09e46d673b413646cbd1f19e66f",
        ),
        (
            0x0048b290_usize,
            151,
            "2092feea2cc2b2afbd9882dd6f9f1697bd0bb754d9f96a3dc3c5ec21b4fd230c",
        ),
        (
            0x004823f0_usize,
            108,
            "99bb25d9ddf6911010bc26e0b579652ebd740303dceb0e61182505a335c4dbbb",
        ),
        (
            0x00439e10_usize,
            37,
            "e802424855ab95d69bc935e1e0b89bc44221cfde57a4efa68ae7cb28326c60fd",
        ),
        (
            0x0048cf70_usize,
            144,
            "8c857c0def30cfe9c95082bdf424646674abb1b441c85772db581a5fd65f123a",
        ),
        (
            0x0047a660_usize,
            21,
            "bb8af2f0dba9f6f5795022ec5f4692afd2a3c7986d8443a74725d8c63c4831ed",
        ),
        (
            0x00490a80_usize,
            145,
            "d794384cfa0e3e311dca206487ee2b5baa4c1eb67ae7fe6bce9333ffb918d909",
        ),
        (
            0x00478950_usize,
            91,
            "a090263b63504fd66bb7b7158fa14f0f7c528ee96cba8e7fc0b6bee0bad742ca",
        ),
        (
            0x004014f0_usize,
            114,
            "004fd768ae3275454ff756b683b34b95598bb57c1876e4c9fcfd3e639a31980c",
        ),
        (
            0x00425e60_usize,
            231,
            "bf30316b8a96753a1746060565737be661587136bd06968fe94e80224cb9bec7",
        ),
        (
            0x00425f70_usize,
            177,
            "01ce32808f9c2c5441b251f098b8772e345b727bb3ba8188e6521b24c1bd3199",
        ),
        (
            0x00426bf0_usize,
            31,
            "91db4b1faf71907cc01214bb33b9fe15f4a25f769145ab29d0dfe4665e01651c",
        ),
        (
            0x00426c10_usize,
            106,
            "28e2994e6804f8f87f3edb48ce54a185a9a9ae798a7f4d77c3b6da0493fa56a9",
        ),
        (
            0x00426c80_usize,
            1016,
            "da18a2212d63e5cfc42bd245d66ab615527536da28bc09bc6d884676a24f62f5",
        ),
        (
            0x0048cc70_usize,
            227,
            "6ac06000b2482525e1963898ef74cfa2e434ff6f5f24f003f2fd30529a65e205",
        ),
        (
            0x0048cd60_usize,
            76,
            "f9f2ac94ba9e961df31164a5b8bff439c2bb100d0f762979974f84bf1f63dc29",
        ),
        (
            0x00402590_usize,
            17,
            "f69bda2871690b05071d583d410632ec1a1e7171921abe2e39da1285181f0a70",
        ),
        (
            0x00401310_usize,
            1,
            "ae3f4619b0413d70d3004b9131c3752153074e45725be13b9a148978895e359e",
        ),
        (
            0x0047a7a0_usize,
            665,
            "9f2a0cf2da98108bf6fa5d08f8441628ad921648962be06f34c22a30b799d376",
        ),
        (
            0x0047a490_usize,
            370,
            "b224054ec56a4ed7cf37593d7baf2fdcf80c08b9f0bd9681f87e992194278b62",
        ),
        (
            0x0047a610_usize,
            67,
            "4b7377ed238bec1965fc1a1557dce0873ebc4ace0001b8475bb3f98a1127104f",
        ),
        (
            0x0047ac10_usize,
            240,
            "9d48b681cdd25d02786e8f75aefdaff13cdab1a1a96b25d729ddd7189215010d",
        ),
        (
            0x00428390_usize,
            80,
            "8068ad22aebf7aab9377ead79db925d8c2833302ae1cdb2e81b51fc5384d50ca",
        ),
        (
            0x00402330_usize,
            215,
            "d7e94cc9d0fd1e651d016ee663421c8b04e25a9c6b955237c73ad33edbbc53d1",
        ),
    ] {
        //Authenticated .text has matching VA-relative/raw offsets1000.
        let start = address - 0x0040_0000;
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw[start..start + length])),
            expected
        );
    }
}

fn lowbyte10_custody_except_word74(objects: &ObjectArena, source: ObjectId, prior: u16) -> String {
    let current = objects.object(source).unwrap();
    let exact_current = format!("{current:?}");
    let mut normalized = current.clone();
    let ObjectClass::Actor(actor) = &mut normalized.class else {
        panic!("genuine actor fixture required");
    };
    actor.word_74 = prior;
    let full = format!("{objects:?}");
    assert_eq!(full.matches(&exact_current).count(), 1);
    full.replacen(&exact_current, &format!("{normalized:?}"), 1)
}

#[test]
fn family_height_aliases_wrap_only_word74_and_publish_captured_signed_xyz() {
    let mut objects = arena();
    let source = subject(&mut objects, true);
    let other = subject(&mut objects, false);
    let other_handle = objects.actor(other).unwrap().handle;
    assert_eq!(
        objects.resolve_actor_handle(other_handle).unwrap(),
        Some(other)
    );
    let cache = objects.actor_handles().unwrap().cache();
    let other_before = objects.object(other).unwrap().clone();
    for (row_y, actor_y, expected_height, output_y) in [
        (0x8000u16, 0x7fff, 0x0001, -32768),
        (0x0000, 0xffff, 0x0001, 0),
        (0x7fff, 0x8000, 0xffff, 32767),
        (0xffff, 0x7fff, 0x8000, -1),
    ] {
        objects
            .copy_actor_position(source, [0x1357, actor_y, 0x2468])
            .unwrap();
        // Adjacent+76 is independent optional WORD custody.
        objects.actor_mut(source).unwrap().word_76 = Some(0xabcd);
        let rows = vertices([0x1_8001, 0x1_0000 + i32::from(row_y), 0x1_ffff]);
        let rows_before = format!("{rows:?}");
        for selector in [10, 0x010a, i32::from(0xff0au16 as i16)] {
            objects.actor_mut(source).unwrap().word_74 = 0x369c;
            let before = format!("{objects:?}");
            let mut output = Some([7, 8, 9]);
            // Only extra0 even with real nonnull140:10 must not read extra1.
            compute(&mut objects, source, selector, &[0], &rows, &mut output).unwrap();
            assert_eq!(objects.actor(source).unwrap().word_74, expected_height);
            assert_eq!(output, Some([-32767, output_y, -1]));
            assert_eq!(
                lowbyte10_custody_except_word74(&objects, source, 0x369c),
                before
            );
            assert_eq!(objects.object(other).unwrap(), &other_before);
            assert_eq!(objects.actor_handles().unwrap().cache(), cache);
            assert_eq!(format!("{rows:?}"), rows_before);
        }
    }
}

#[test]
fn family_height_row_errors_precede_required_y_and_preserve_full_custody() {
    let mut objects = arena();
    let source = subject(&mut objects, true);
    let other = subject(&mut objects, false);
    let other_handle = objects.actor(other).unwrap().handle;
    assert_eq!(
        objects.resolve_actor_handle(other_handle).unwrap(),
        Some(other)
    );
    assert_eq!(objects.object(source).unwrap().base.position_words, None);
    objects.actor_mut(source).unwrap().word_74 = 0x369c;
    let before = format!("{objects:?}");
    let cache = objects.actor_handles().unwrap().cache();
    let rows = vertices([1, 2, 3]);
    let rows_before = format!("{rows:?}");
    let mut output = Some([7, 8, 9]);
    for (extras, error) in [
        (
            vec![],
            ObjectCustomVertexError::RequiredExtra {
                object: source,
                selector: 0x010a,
                index: 0,
            },
        ),
        (
            vec![0xffff],
            ObjectCustomVertexError::Vertex(ModelVertexFault::InvalidSlot {
                index: 65535,
                count: 4,
            }),
        ),
        (
            vec![2],
            ObjectCustomVertexError::Vertex(ModelVertexFault::UnknownWorld { index: 2 }),
        ),
        (
            vec![0],
            ObjectCustomVertexError::RequiredField {
                object: source,
                offset: 0x34,
            },
        ),
    ] {
        assert_eq!(
            compute(&mut objects, source, 0x010a, &extras, &rows, &mut output),
            Err(error)
        );
        assert_eq!(output, Some([7, 8, 9]));
        assert_eq!(format!("{objects:?}"), before);
        assert_eq!(objects.actor_handles().unwrap().cache(), cache);
        assert_eq!(format!("{rows:?}"), rows_before);
    }
    let unsigned = ModelVertices::new_invocation(u16::MAX);
    assert_eq!(
        compute(&mut objects, source, 10, &[0x8000], &unsigned, &mut output),
        Err(ObjectCustomVertexError::Vertex(
            ModelVertexFault::UnknownWorld { index: 32768 }
        ))
    );
    assert_eq!(output, Some([7, 8, 9]));
    assert_eq!(format!("{objects:?}"), before);
    assert_eq!(
        compute(&mut objects, source, 0x010b, &[], &rows, &mut output),
        Err(ObjectCustomVertexError::Unowned {
            object: source,
            selector: 0x010b
        })
    );
    assert_eq!(output, Some([7, 8, 9]));
    assert_eq!(format!("{objects:?}"), before);
}

#[test]
fn selector36_wrapper_keeps_forwarded_family_aliases_unowned() {
    let mut objects = arena();
    let source = objects
        .allocate_selector36_actor(allocation(), &mut 17)
        .unwrap();
    assert_eq!(
        object_custom_callback_origin(&objects.derived_actor(source).unwrap().class),
        Some(ObjectCustomCallbackOrigin::Selector36Wrapper4823f0),
    );
    objects.copy_actor_position(source, [7, 0xffff, 9]).unwrap();
    let rows = vertices([1, 2, 3]);
    let before = format!("{objects:?}");
    let mut output = Some([7, 8, 9]);
    //Aliases forward to the still-unowned family default arm. They do not
    //select the exact11 capture or gain invented terrain/output backing.
    for selector in [0x010b, i32::from(0xff0bu16 as i16), 9] {
        assert_eq!(
            compute(&mut objects, source, selector, &[], &rows, &mut output),
            Err(ObjectCustomVertexError::Unowned {
                object: source,
                selector
            }),
        );
        assert_eq!(format!("{objects:?}"), before);
        assert_eq!(output, Some([7, 8, 9]));
    }
}

fn special11_custody_except_capture(
    objects: &ObjectArena,
    source: ObjectId,
    prior_word_264: u16,
    prior_capture: [Option<u16>; 3],
) -> String {
    let current = objects.object(source).unwrap();
    let exact = format!("{current:?}");
    let mut normalized = current.clone();
    let ObjectClass::Actor(actor) = &mut normalized.class else {
        panic!("actual actor");
    };
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("actual derived");
    };
    let DerivedActorClass::Family428650(family) = &mut derived.class else {
        panic!("actual family");
    };
    let Family428650Class::Family429e20(family) = &mut family.class else {
        panic!("actual family");
    };
    let Family429e20Class::Family4236d0(family) = &mut family.class else {
        panic!("actual family");
    };
    family.word_264 = prior_word_264;
    let Family4236d0Class::Selector36(state) = &mut family.class else {
        panic!("actual selector36");
    };
    state.words_278_27c = prior_capture;
    let whole = format!("{objects:?}");
    assert_eq!(whole.matches(&exact).count(), 1);
    whole.replacen(&exact, &format!("{normalized:?}"), 1)
}

#[test]
fn selector36_exact11_sets_capture_bit_and_actual_words_before_signed_output() {
    let mut objects = arena();
    let source = objects
        .allocate_selector36_actor(allocation(), &mut 17)
        .unwrap();
    assert_eq!(
        objects.selector36_actor(source).unwrap().words_278_27c,
        [None; 3]
    );
    // Supplied current bits distinguish an OR from replacing the whole WORD.
    // Native ctor itself initializes this WORD to0; no native later writer claim.
    objects.family4236d0_actor_mut(source).unwrap().word_264 = 0x9213;
    assert!(objects
        .object(source)
        .unwrap()
        .base
        .position_words
        .is_none());
    let other = subject(&mut objects, false);
    let handle = objects.actor(other).unwrap().handle;
    assert_eq!(objects.resolve_actor_handle(handle).unwrap(), Some(other));
    let cache = objects.actor_handles().unwrap().cache();
    let before = format!("{objects:?}");
    let rows = vertices([0x1_8001, -0x1_8000, 0x1_ffff]);
    let rows_before = format!("{rows:?}");
    let mut output = Some([7, 8, 9]);
    // No extra1, XYZ, behavior storage, terrain, basis or word74 is required.
    compute(&mut objects, source, 11, &[0], &rows, &mut output).unwrap();
    assert_eq!(objects.family4236d0_actor(source).unwrap().word_264, 0x9a13);
    assert_eq!(
        objects.selector36_actor(source).unwrap().words_278_27c,
        [Some(0x8001), Some(0x8000), Some(0xffff)]
    );
    assert_eq!(output, Some([-32767, -32768, -1]));
    assert_eq!(
        special11_custody_except_capture(&objects, source, 0x9213, [None; 3]),
        before
    );
    assert_eq!(objects.actor_handles().unwrap().cache(), cache);
    assert_eq!(format!("{rows:?}"), rows_before);
    // A second real row overwrites the same retained words; it is not a cache hit.
    compute(
        &mut objects,
        source,
        11,
        &[0],
        &vertices([4, 5, 6]),
        &mut output,
    )
    .unwrap();
    assert_eq!(
        objects.selector36_actor(source).unwrap().words_278_27c,
        [Some(4), Some(5), Some(6)]
    );
    assert_eq!(output, Some([4, 5, 6]));
    assert_eq!(objects.family4236d0_actor(source).unwrap().word_264, 0x9a13);
}

#[test]
fn selector36_exact11_row_errors_retain_the_earlier_bit_old_capture_and_output() {
    let mut objects = arena();
    let source = objects
        .allocate_selector36_actor(allocation(), &mut 17)
        .unwrap();
    let rows = vertices([1, 2, 3]);
    let mut output = Some([7, 8, 9]);
    let empty_before = format!("{objects:?}");
    assert_eq!(
        compute(&mut objects, source, 11, &[], &rows, &mut output),
        Err(ObjectCustomVertexError::RequiredExtra {
            object: source,
            selector: 11,
            index: 0
        })
    );
    assert_eq!(objects.family4236d0_actor(source).unwrap().word_264, 0x0800);
    assert_eq!(
        objects.selector36_actor(source).unwrap().words_278_27c,
        [None; 3]
    );
    assert_eq!(
        special11_custody_except_capture(&objects, source, 0, [None; 3]),
        empty_before
    );
    assert_eq!(output, Some([7, 8, 9]));

    for (extras, expected) in [
        (
            vec![],
            ObjectCustomVertexError::RequiredExtra {
                object: source,
                selector: 11,
                index: 0,
            },
        ),
        (
            vec![0xffff],
            ObjectCustomVertexError::Vertex(ModelVertexFault::InvalidSlot {
                index: 65535,
                count: 4,
            }),
        ),
        (
            vec![2],
            ObjectCustomVertexError::Vertex(ModelVertexFault::UnknownWorld { index: 2 }),
        ),
    ] {
        let state = objects.family4236d0_actor_mut(source).unwrap();
        state.word_264 = 0x9213;
        let Family4236d0Class::Selector36(state) = &mut state.class else {
            unreachable!();
        };
        state.words_278_27c = [Some(91), None, Some(93)];
        let before = format!("{objects:?}");
        assert_eq!(
            compute(&mut objects, source, 11, &extras, &rows, &mut output),
            Err(expected)
        );
        assert_eq!(objects.family4236d0_actor(source).unwrap().word_264, 0x9a13);
        assert_eq!(
            objects.selector36_actor(source).unwrap().words_278_27c,
            [Some(91), None, Some(93)]
        );
        assert_eq!(
            special11_custody_except_capture(&objects, source, 0x9213, [Some(91), None, Some(93)]),
            before
        );
        assert_eq!(output, Some([7, 8, 9]));
    }
    // Native zero-extends extra0; 0x8000 is not a negative index.
    let unsigned = ModelVertices::new_invocation(u16::MAX);
    assert_eq!(
        compute(&mut objects, source, 11, &[0x8000], &unsigned, &mut output),
        Err(ObjectCustomVertexError::Vertex(
            ModelVertexFault::UnknownWorld { index: 32768 }
        ))
    );

    let before = format!("{objects:?}");
    for alias in [0x010b, i32::from(0xff0bu16 as i16)] {
        assert_eq!(
            compute(&mut objects, source, alias, &[], &rows, &mut output),
            Err(ObjectCustomVertexError::Unowned {
                object: source,
                selector: alias
            })
        );
        assert_eq!(format!("{objects:?}"), before);
        assert_eq!(output, Some([7, 8, 9]));
    }
    // Exact11 on a genuine direct family source does not acquire class36 storage.
    let direct = subject(&mut objects, false);
    let before = format!("{objects:?}");
    assert_eq!(
        compute(&mut objects, direct, 11, &[], &rows, &mut output),
        Err(ObjectCustomVertexError::Unowned {
            object: direct,
            selector: 11
        })
    );
    assert_eq!(format!("{objects:?}"), before);
}

//00480D59..DAD: actual class60 first-row cache and reached failure custody.
fn selector60_custom_subject(
    arena: &mut crate::object_runtime::ObjectArena,
) -> crate::object_runtime::ObjectId {
    arena
        .allocate_selector60_actor(allocation(), &mut 17)
        .unwrap()
}

fn selector60_cache_custody(
    arena: &crate::object_runtime::ObjectArena,
    source: crate::object_runtime::ObjectId,
) -> String {
    let current = arena.object(source).unwrap();
    let original = format!("{current:?}");
    let mut normalized = (*current).clone();
    let crate::object_runtime::ObjectClass::Actor(actor) = &mut normalized.class else {
        panic!("real actor");
    };
    let crate::object_runtime::ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("real derived actor");
    };
    let crate::object_runtime::DerivedActorClass::Selector60(state) = &mut derived.class else {
        panic!("real selector60");
    };
    state.byte_184 = 0;
    state.position_198 = None;
    let whole = format!("{arena:?}");
    assert_eq!(whole.matches(&original).count(), 1);
    whole.replacen(&original, &format!("{normalized:?}"), 1)
}

#[test]
fn selector60_full9_captures_once_and_cache_hit_skips_extras_and_vertex_backing() {
    let mut arena = arena();
    let source = selector60_custom_subject(&mut arena);
    assert_eq!(arena.selector60_actor(source).unwrap().byte_184, 0);
    assert_eq!(arena.selector60_actor(source).unwrap().position_198, None);
    let custody = selector60_cache_custody(&arena, source);
    let mut output = Some([7, 8, 9]);
    compute(
        &mut arena,
        source,
        9,
        &[0],
        &vertices([0x1_0001, 0x1_8000, -1]),
        &mut output,
    )
    .unwrap();
    assert_eq!(output, Some([1, -32768, -1]));
    let state = arena.selector60_actor(source).unwrap();
    assert_eq!(
        (state.byte_184, state.position_198),
        (1, Some([1, 0x8000, 0xffff]))
    );
    assert_eq!(selector60_cache_custody(&arena, source), custody);
    // No allocated input slots and no extras: successful reuse proves that
    // neither input row backing nor extrasWORD0 is read on the nonzero path.
    output = None;
    compute(
        &mut arena,
        source,
        9,
        &[],
        &crate::model_vertices::ModelVertices::new_invocation(0),
        &mut output,
    )
    .unwrap();
    assert_eq!(output, Some([1, -32768, -1]));
    assert_eq!(selector60_cache_custody(&arena, source), custody);
}

#[test]
fn selector60_full9_first_row_stop_publishes_flag_and_next_call_never_resamples() {
    let mut arena = arena();
    let source = selector60_custom_subject(&mut arena);
    let custody = selector60_cache_custody(&arena, source);
    let mut output = Some([7, 8, 9]);
    assert_eq!(
        compute(
            &mut arena,
            source,
            9,
            &[0],
            &crate::model_vertices::ModelVertices::new_invocation(0),
            &mut output
        ),
        Err(ObjectCustomVertexError::Vertex(
            ModelVertexFault::InvalidSlot { index: 0, count: 0 }
        ))
    );
    let state = arena.selector60_actor(source).unwrap();
    assert_eq!((state.byte_184, state.position_198), (1, None));
    assert_eq!(output, Some([7, 8, 9]));
    // Known replacement row must not repair the retained first-call stop.
    assert_eq!(
        compute(
            &mut arena,
            source,
            9,
            &[0],
            &vertices([1, 2, 3]),
            &mut output
        ),
        Err(ObjectCustomVertexError::RequiredField {
            object: source,
            offset: 0x198
        })
    );
    // Missing extras and missing vertex backing also remain unread here.
    assert_eq!(
        compute(
            &mut arena,
            source,
            9,
            &[],
            &crate::model_vertices::ModelVertices::new_invocation(0),
            &mut output
        ),
        Err(ObjectCustomVertexError::RequiredField {
            object: source,
            offset: 0x198
        })
    );
    assert_eq!(output, Some([7, 8, 9]));
    assert_eq!(selector60_cache_custody(&arena, source), custody);
}

#[test]
fn selector60_full9_missing_first_extra_retains_flag_but_other_full_selectors_stay_unowned() {
    let mut arena = arena();
    let source = selector60_custom_subject(&mut arena);
    let custody = selector60_cache_custody(&arena, source);
    let mut output = Some([7, 8, 9]);
    for selector in [0x109, 0xff09_u16 as i16 as i32, 0x107, 10] {
        assert_eq!(
            compute(
                &mut arena,
                source,
                selector,
                &[],
                &crate::model_vertices::ModelVertices::new_invocation(0),
                &mut output
            ),
            Err(ObjectCustomVertexError::Unowned {
                object: source,
                selector
            })
        );
        assert_eq!(arena.selector60_actor(source).unwrap().byte_184, 0);
    }
    assert_eq!(
        compute(
            &mut arena,
            source,
            9,
            &[],
            &crate::model_vertices::ModelVertices::new_invocation(0),
            &mut output
        ),
        Err(ObjectCustomVertexError::RequiredExtra {
            object: source,
            selector: 9,
            index: 0
        })
    );
    assert_eq!(arena.selector60_actor(source).unwrap().byte_184, 1);
    assert_eq!(arena.selector60_actor(source).unwrap().position_198, None);
    assert_eq!(output, Some([7, 8, 9]));
    assert_eq!(selector60_cache_custody(&arena, source), custody);
}

#[test]
fn selector60_full9_any_nonzero_flag_uses_only_actual_cache_and_other_class_is_unowned() {
    let mut arena = arena();
    let source = selector60_custom_subject(&mut arena);
    // Explicit interrupted/current state supply, not a native reset writer.
    let crate::object_runtime::DerivedActorClass::Selector60(state) =
        &mut arena.derived_actor_mut(source).unwrap().class
    else {
        panic!("actual constructor60 class");
    };
    state.byte_184 = 0x80;
    state.position_198 = Some([0x8000, 0x7fff, 0xffff]);
    let before = format!("{arena:?}");
    let mut output = None;
    compute(
        &mut arena,
        source,
        9,
        &[],
        &crate::model_vertices::ModelVertices::new_invocation(0),
        &mut output,
    )
    .unwrap();
    assert_eq!(output, Some([-32768, 32767, -1]));
    assert_eq!(format!("{arena:?}"), before);
    // The same number in another actual class must not select class60 policy.
    // Family-default terrain9 is a distinct, still-unowned owner here.
    let family = subject(&mut arena, false);
    let before = format!("{arena:?}");
    output = Some([7, 8, 9]);
    assert_eq!(
        compute(
            &mut arena,
            family,
            9,
            &[0],
            &vertices([1, 2, 3]),
            &mut output
        ),
        Err(ObjectCustomVertexError::Unowned {
            object: family,
            selector: 9
        })
    );
    assert_eq!(output, Some([7, 8, 9]));
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn selector60_failed_recapture_retains_old_cache_and_next_call_reads_it_without_rows() {
    for (extras, rows, error) in [
        (vec![], vertices([1, 2, 3]), None),
        (
            vec![0xffff],
            vertices([1, 2, 3]),
            Some(ModelVertexFault::InvalidSlot {
                index: 65535,
                count: 4,
            }),
        ),
        (
            vec![2],
            vertices([1, 2, 3]),
            Some(ModelVertexFault::UnknownWorld { index: 2 }),
        ),
        // Genuine unsigned extra0 domain: a large admitted invocation capacity
        // makes8000 an unknown positive row, never a negative/bypassed index.
        (
            vec![0x8000],
            ModelVertices::new_invocation(u16::MAX),
            Some(ModelVertexFault::UnknownWorld { index: 32768 }),
        ),
    ] {
        let mut objects = arena();
        let source = selector60_custom_subject(&mut objects);
        let other = subject(&mut objects, false);
        let handle = objects.actor(other).unwrap().handle;
        assert_eq!(objects.resolve_actor_handle(handle).unwrap(), Some(other));
        let actual_handle_cache = objects.actor_handles().unwrap().cache();
        // Supplied current cache plus a zero capture flag; this is not a claim
        // that the original constructor initialized198 or that a reset writer
        // is owned. Both actual fields are retained by the real class60 state.
        let old = [0x8001, 0x7fff, 0xffff];
        let DerivedActorClass::Selector60(state) =
            &mut objects.derived_actor_mut(source).unwrap().class
        else {
            panic!("actual constructor60 class");
        };
        state.position_198 = Some(old);
        assert_eq!(state.byte_184, 0);
        let custody = selector60_cache_custody(&objects, source);
        let row_custody = format!("{rows:?}");
        let mut output = Some([7, 8, 9]);
        let expected = match error {
            None => ObjectCustomVertexError::RequiredExtra {
                object: source,
                selector: 9,
                index: 0,
            },
            Some(error) => ObjectCustomVertexError::Vertex(error),
        };
        assert_eq!(
            compute(&mut objects, source, 9, &extras, &rows, &mut output),
            Err(expected)
        );
        let state = objects.selector60_actor(source).unwrap();
        assert_eq!((state.byte_184, state.position_198), (1, Some(old)));
        assert_eq!(output, Some([7, 8, 9]));
        assert_eq!(selector60_cache_custody(&objects, source), custody);
        assert_eq!(
            objects.actor_handles().unwrap().cache(),
            actual_handle_cache
        );
        assert_eq!(format!("{rows:?}"), row_custody);
        let before_cached_call = format!("{objects:?}");
        compute(
            &mut objects,
            source,
            9,
            &[],
            &ModelVertices::new_invocation(0),
            &mut output,
        )
        .unwrap();
        assert_eq!(output, Some([-32767, 32767, -1]));
        assert_eq!(format!("{objects:?}"), before_cached_call);
    }
}

fn selector30_custody_except_capture(
    objects: &ObjectArena,
    source: ObjectId,
    prior: Option<[u16; 3]>,
) -> String {
    let current = objects.object(source).unwrap();
    let exact = format!("{current:?}");
    let mut normalized = current.clone();
    let ObjectClass::Actor(actor) = &mut normalized.class else {
        panic!("actual actor required");
    };
    let ActorExtension::Selector30(state) = &mut actor.extension else {
        panic!("actual selector30 required");
    };
    state.position_16c = prior;
    let full = format!("{objects:?}");
    assert_eq!(full.matches(&exact).count(), 1);
    full.replacen(&exact, &format!("{normalized:?}"), 1)
}

#[test]
fn selector30_exact8_freshly_captures_only16c_and_publishes_signed_words() {
    let mut objects = arena();
    let source = objects
        .allocate_selector30_actor(allocation().actor)
        .unwrap();
    let other = subject(&mut objects, true);
    let handle = objects.actor(other).unwrap().handle;
    assert_eq!(objects.resolve_actor_handle(handle).unwrap(), Some(other));
    assert_eq!(objects.selector30_actor(source).unwrap().position_16c, None);
    assert_eq!(objects.object(source).unwrap().base.position_words, None);
    objects.actor_mut(source).unwrap().word_74 = 0xbeef;
    objects.actor_mut(source).unwrap().word_b4 = 0x77;
    let before = format!("{objects:?}");
    let handle_cache = objects.actor_handles().unwrap().cache();
    for world in [[0x1_8001, -0x1_8000, 0x1_ffff], [-1, 0x1_7fff, 0x2_0000]] {
        let rows = vertices(world);
        let rows_before = format!("{rows:?}");
        let mut output = Some([91, 82, 73]);
        // No extra1, actorXYZ or earlier captured row is used.
        compute(&mut objects, source, 8, &[0], &rows, &mut output).unwrap();
        assert_eq!(
            output,
            Some(world.map(|axis| i32::from(axis as u16 as i16)))
        );
        assert_eq!(
            objects.selector30_actor(source).unwrap().position_16c,
            Some(world.map(|axis| axis as u16))
        );
        assert_eq!(
            selector30_custody_except_capture(&objects, source, None),
            before
        );
        assert_eq!(format!("{rows:?}"), rows_before);
        assert_eq!(objects.actor_handles().unwrap().cache(), handle_cache);
    }
}

#[test]
fn selector30_exact8_errors_preserve_prior_capture_output_and_all_arena_custody() {
    let mut objects = arena();
    let source = objects
        .allocate_selector30_actor(allocation().actor)
        .unwrap();
    let rows = vertices([11, 22, 33]);
    let mut output = None;
    compute(&mut objects, source, 8, &[0], &rows, &mut output).unwrap();
    let before = format!("{objects:?}");
    let rows_before = format!("{rows:?}");
    for (extras, expected) in [
        (
            vec![],
            ObjectCustomVertexError::RequiredExtra {
                object: source,
                selector: 8,
                index: 0,
            },
        ),
        (
            vec![0xffff],
            ObjectCustomVertexError::Vertex(ModelVertexFault::InvalidSlot {
                index: 65535,
                count: 4,
            }),
        ),
        (
            vec![0x8000],
            ObjectCustomVertexError::Vertex(ModelVertexFault::InvalidSlot {
                index: 32768,
                count: 4,
            }),
        ),
        (
            vec![2],
            ObjectCustomVertexError::Vertex(ModelVertexFault::UnknownWorld { index: 2 }),
        ),
    ] {
        output = Some([91, 82, 73]);
        assert_eq!(
            compute(&mut objects, source, 8, &extras, &rows, &mut output),
            Err(expected)
        );
        assert_eq!(output, Some([91, 82, 73]));
        assert_eq!(format!("{objects:?}"), before);
        assert_eq!(format!("{rows:?}"), rows_before);
    }
}

/// 0048B29D: every selector but the exact DWORD 8 (aliases included) is the
/// ground under the actor, which needs the process services.
#[test]
fn selector30_other_selectors_need_the_ground_and_other_classes_stay_unowned() {
    let mut objects = arena();
    let source = objects
        .allocate_selector30_actor(allocation().actor)
        .unwrap();
    objects.copy_actor_position(source, [7, 0, 9]).unwrap();
    let family = subject(&mut objects, false);
    let rows = ModelVertices::new_invocation(0);
    let before = format!("{objects:?}");
    for (source, selector) in [
        (source, 0x108),
        (source, 0xff08),
        (source, -248),
        (source, i32::MIN + 8),
        (source, 2),
        (source, 7),
        (source, 10),
        (family, 8),
    ] {
        let mut output = Some([91, 82, 73]);
        let expected = if source == family {
            ObjectCustomVertexError::Unowned {
                object: source,
                selector,
            }
        } else {
            ObjectCustomVertexError::RequiredEffects {
                object: source,
                selector,
            }
        };
        assert_eq!(
            compute(&mut objects, source, selector, &[], &rows, &mut output),
            Err(expected)
        );
        assert_eq!(output, Some([91, 82, 73]));
        assert_eq!(format!("{objects:?}"), before);
        assert!(rows.slots().is_empty());
    }
}

#[path = "selector77_tests.rs"]
mod selector77;

fn factory(arena: &mut ObjectArena) -> ObjectId {
    static SINE: [u16; 4096] = [0; 4096];
    arena
        .allocate_selector15(ActorConstructionRequest {
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
        .unwrap()
}

/// 0042E506: exact selector 6 captures the dock row at +190..+194, which
/// Infest01's factories publish every draw (their selectors 6, 4 and 7).
#[test]
fn factory_selector6_captures_the_dock_row() {
    let mut arena = arena();
    let source = factory(&mut arena);
    assert_eq!(arena.selector15(source).unwrap().dock_190, None);
    let mut output = None;
    compute(
        &mut arena,
        source,
        6,
        &[0, 0xda],
        &vertices([0x90dd, 0x50, 0x842b]),
        &mut output,
    )
    .unwrap();
    assert_eq!(output, Some([-28451, 80, -31701]));
    assert_eq!(
        arena.selector15(source).unwrap().dock_190,
        Some([0x90dd, 0x50, 0x842b])
    );
}

/// 0042E445: with the family's free capacity +13C - +140 at zero, exact 7
/// births the chimney trail after the load gate (slack / 8192).
#[test]
fn factory_selector7_trails_while_the_store_is_full() {
    let mut arena = arena();
    let source = factory(&mut arena);
    let mut effects = RecordingTrailEffects {
        load_factor: 0xa000,
        rng: 0x1234_5678,
        events: Vec::new(),
    };
    let mut output = None;
    arena
        .compute_model_custom_vertex(ObjectCustomVertexRequest {
            source,
            compute: ModelCustomVertexRequest {
                selector: 7,
                extras: &[0, 1],
            },
            vertices: &vertices([100, 410, -200]),
            output: &mut output,
            effects: Some(&mut effects),
        })
        .unwrap();
    assert_eq!(output, Some([100, 410, -200]));
    // A slack of 0x2000 divides to 1: no gate draw, then the velocity draw.
    let velocity = effects.events[0]["value"].as_u64().unwrap() as u16;
    assert_eq!(
        effects.events[1],
        serde_json::json!({
            "kind": "trail",
            "particle_type": 6,
            "position": [100, 410, 0xff38],
            "velocity": [0, 128 + (velocity >> 9), 0],
            "flags": 0x800,
        })
    );
    assert_eq!(effects.events.len(), 2);
}

/// 0042E3F7: every other selector, and 7 while the store has room, returns
/// the factory's X/Z at the ground height, which needs the process services.
#[test]
fn factory_other_selectors_need_the_ground() {
    let mut arena = arena();
    let source = factory(&mut arena);
    let state = arena.selector15_mut(source).unwrap();
    state.intermediate.word_13c = 2;
    for selector in [4, 7] {
        let mut output = None;
        assert_eq!(
            compute(
                &mut arena,
                source,
                selector,
                &[0x3c, 0xda],
                &vertices([1, 2, 3]),
                &mut output
            ),
            Err(ObjectCustomVertexError::RequiredEffects {
                object: source,
                selector
            })
        );
        assert_eq!(output, None);
    }
}
