use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_display::frontend_startup,
    frontend_scene::{camera_basis, SceneView},
    menu::MenuBasis,
    model_context::{ModelDispatch, ModelProjection},
    model_projection::ModelProjectionBackend,
    process_particles::ParticleId,
    terrain_draw::{TerrainDrawInputs, TerrainDrawRequest, TerrainLighting},
    terrain_furniture::FurnitureDomain,
    terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing},
    world_furniture_effect::{FurnitureEffectProgram, FurnitureQueueRequest},
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};
use inf_formats::{
    native_basis::NativeBasisMath, parse_terrain_bank, ByteSpan, FrontendExecutable,
    HandlerFraming, HandlerId, HandlerRegion, ModelInstructionKind, TERRAIN_CELL_COUNT,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use std::path::Path;

fn fixture() -> (FrontendAssets, PrimitiveTables, ResidentRuntime) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 28] {
        resident
            .apply(ResidencyRequest::Activate { level })
            .unwrap();
    }
    (assets, tables, resident)
}

/// Original H11 links select real H8 programs. Test terrain/phase/view are
/// explicit supplied inputs; no model or overlay bytes are rewritten.
fn lookups(resident: &ResidentRuntime) -> (u8, u8) {
    let table = resident_placement_table(resident).unwrap();
    let mut null = None;
    let mut visible = None;
    let resources = RenderResources::new(resident);
    for (index, slot) in table.slots.iter().enumerate().take(256).skip(1) {
        let metadata =
            crate::object_runtime::resident_model_metadata(resident, slot.resource_word).unwrap();
        if metadata.is_none() {
            null.get_or_insert(index as u8);
            continue;
        }
        let RenderResourceData::Model(model) = resources
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: i32::from(slot.resource_word),
            })
            .unwrap()
            .data
        else {
            unreachable!()
        };
        if model.program.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                ModelInstructionKind::Triangle(_) | ModelInstructionKind::Quad(_)
            )
        }) {
            visible.get_or_insert(index as u8);
        }
    }
    (
        null.expect("resident H11 genuine null model"),
        visible.expect("resident furniture faces"),
    )
}

fn terrain(records: &[(u16, u8, u8)]) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    bytes[0..4].copy_from_slice(&(-2_000_000_i32).to_le_bytes());
    let sizes = TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2;
    bytes[sizes..sizes + 2].copy_from_slice(&((records.len() * 4) as u16).to_le_bytes());
    for &(x, state, lookup) in records {
        let cell = TERRAIN_PREFIX_BYTES + usize::from(x & 0x1ff) * 512 * 2;
        bytes[cell + 1] |= 4;
        bytes.extend_from_slice(&[x as u8, (x >> 8) as u8, state, lookup]);
    }
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
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

fn view() -> SceneView {
    let eye = [256, 512, -2048];
    SceneView {
        eye,
        basis: camera_basis(eye, [256, 0, 0]),
    }
}
fn inputs(tables: &PrimitiveTables) -> TerrainDrawInputs<'_> {
    TerrainDrawInputs {
        view: view(),
        context_projection: ModelProjection {
            focal: [320, 320],
            viewport: [640, 480],
            center: [320, 240],
            fade: [8192, 9216],
        },
        footprint: crate::terrain_draw::TerrainFootprintWidth::Retail,
        coordinate_shift: 16,
        tint: [128; 3],
        lighting: TerrainLighting::Disabled,
        clock: 0x1234_abcd,
        world_flags: 0,
        night: None,
        particles_enabled: false,
        tables,
    }
}
fn supplied_process() -> crate::process_particles::factories::ParticleFactoryProcessState {
    crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x1234_5678)),
    }
}

fn birth_inputs(
    process: &crate::process_particles::factories::ParticleFactoryProcessState,
    phase: u32,
    network_active: bool,
) -> ParticleBirthInputs<'_> {
    ParticleBirthInputs {
        network_active,
        phase: Some(phase),
        process,
    }
}

#[test]
fn direct_furniture_defers_missing_source_until_admitted_birth() {
    let (assets, tables, resident) = fixture();
    let (null, _) = lookups(&resident);
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut process = supplied_process();
    process.process_null_handle = None;
    for sparse_state in [0, 1] {
        let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (startup.scene).environment,
            ),
            resources,
            tables: &tables,
            frame: &mut pixels,
            config: startup.scene,
        })
        .unwrap();
        let mut terrain = terrain(&[(0, sparse_state, null)]);
        let mut particles = ProcessParticleRuntime::default();
        let mut state = FurnitureDrawState::new(&NativeBasisMath::new(assets.sine_table().values));
        let pending = FurnitureEffectQueue::default();
        let mut context = FurnitureDrawContext {
            view: view(),
            fade: [8192, 9216],
            far_color: 17,
        };
        let mut rng = 0x1020_3040;
        let result = draw_furniture_item(
            FurnitureDrawRequest {
                terrain: &mut terrain,
                record: TerrainSparseRecordId {
                    bucket: 0,
                    index: 0,
                },
                position: [0; 3],
                context: &mut context,
                pending: &pending,
                load_factor: 0xc000,
                clock: 0,
                state: &mut state,
            },
            &mut SceneFurnitureHost {
                scene: &mut scene,
                particles: &mut particles,
                executable,
                birth_inputs: birth_inputs(&process, 4, false),
                load_factor: 0xc000,
            },
            &mut rng,
        );
        if sparse_state == 0 {
            assert_eq!(
                result.unwrap(),
                FurnitureDrawOutcome::Drawn(FurnitureModelOutcome::NoDraw)
            );
            assert_eq!(particles.free_count(), 400);
            assert_eq!(rng, 0x1020_3040);
        } else {
            assert!(matches!(
                result,
                Err(crate::terrain_furniture::draw::FurnitureDrawError::Host {
                    effect: "trail birth",
                    error: SoftwareFurnitureError::Birth(ParticleBirthError::Source(
                        crate::process_particles::births::ParticleBirthSourceError {
                            producer:
                                crate::process_particles::births::ParticleBirthProducer::Trail
                        }
                    ))
                })
            ));
            let mut expected_rng = 0x1020_3040;
            crate::name_entry::name_entry_rng_next(&mut expected_rng);
            assert_eq!(rng, expected_rng);
            assert_eq!(particles.free_count(), 399);
            assert_eq!(particles.allocation_marker(), Some(399));
            assert!(particles.pending(1).unwrap().is_empty());
        }
        assert_eq!(context.fade, [8192, 9216]);
        assert_eq!(context.far_color, 17);
    }
}

#[test]
fn direct_furniture_trail_uses_actual_pool_gate_tables_and_shared_rng() {
    let process = supplied_process();
    let (assets, tables, resident) = fixture();
    let (null, _) = lookups(&resident);
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for (phase, network, expected_pool) in [(4, false, true), (3, false, false), (3, true, true)] {
        let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (startup.scene).environment,
            ),
            resources,
            tables: &tables,
            frame: &mut pixels,
            config: startup.scene,
        })
        .unwrap();
        let mut terrain = terrain(&[(0, 1, null)]);
        let mut particles = ProcessParticleRuntime::default();
        let mut state = FurnitureDrawState::new(&NativeBasisMath::new(assets.sine_table().values));
        let pending = FurnitureEffectQueue::default();
        let mut context = FurnitureDrawContext {
            view: view(),
            fade: [8192, 9216],
            far_color: 17,
        };
        let mut rng = 0x1020_3040;
        let mut expected_rng = rng;
        let velocity_y = crate::name_entry::name_entry_rng_next(&mut expected_rng) as u16 & 63;
        let scale_draw = crate::name_entry::name_entry_rng_next(&mut expected_rng);
        let result = draw_furniture_item(
            FurnitureDrawRequest {
                terrain: &mut terrain,
                record: TerrainSparseRecordId {
                    bucket: 0,
                    index: 0,
                },
                position: [0; 3],
                context: &mut context,
                pending: &pending,
                load_factor: 0xc000,
                clock: 0,
                state: &mut state,
            },
            &mut SceneFurnitureHost {
                scene: &mut scene,
                particles: &mut particles,
                executable,
                birth_inputs: birth_inputs(&process, phase, network),
                load_factor: 0xc000,
            },
            &mut rng,
        )
        .unwrap();
        assert_eq!(
            result,
            FurnitureDrawOutcome::Drawn(FurnitureModelOutcome::NoDraw)
        );
        assert_eq!(rng, expected_rng);
        assert!(particles.active(1).unwrap().is_empty());
        let id = if expected_pool {
            assert_eq!(particles.pending(1).unwrap().len(), 1);
            *particles.pending(1).unwrap().front().unwrap()
        } else {
            assert!(particles.pending(1).unwrap().is_empty());
            ParticleId::Scratch
        };
        let record = particles.record(id).unwrap();
        assert_eq!(record.read_dword(0x28).unwrap(), 0x0053_d1f0);
        assert_eq!(record.read_dword(0x14).unwrap(), 0x1234_5678);
        assert_eq!(record.read_word(0x12).unwrap(), 0x9c);
        for axis in 0..3 {
            assert_eq!(record.read_word(4 + axis * 2).unwrap(), 0);
            assert_eq!(
                record.read_word(0x0a + axis * 2).unwrap(),
                [0, velocity_y, 0][axis]
            );
        }
        assert_eq!(
            record.read_dword(0x2c).unwrap(),
            0x200 + (scale_draw >> 9) - 64
        );
        assert_eq!(context.fade, [8192, 9216]);
    }
}

#[test]
fn scoped_custom_service_preserves_full_selector_and_real_trail_publication() {
    let process = supplied_process();
    let (assets, tables, resident) = fixture();
    let resources = RenderResources::new(&resident);
    //00439E10 reads actual mapped row XYZ. Select a resident model whose
    //authored preprocessing a..c range owns such rows; local projection alone
    //does not initialize world coordinates and must not be transformed here.
    let slot = (0..resident.resource_capacity(8).unwrap())
        .find_map(|slot| {
            let Ok(resource) = resources.resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: slot as i32,
            }) else {
                return None;
            };
            let RenderResourceData::Model(model) = resource.data else {
                return None;
            };
            (model.header.vertex_ranges[0] < model.header.vertex_ranges[2]).then_some(slot as u16)
        })
        .expect("authored resident H8 world preprocessing range");
    let RenderResourceData::Model(model) = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: i32::from(slot),
        })
        .unwrap()
        .data
    else {
        unreachable!()
    };
    let context = ModelContext::new([100, 200, 300], MenuBasis::INITIAL);
    let draw = inputs(&tables);
    let mut projection = ModelProjectionBackend {
        transform: context.publish(draw.view, 16),
        view: draw.view,
        projection: draw.context_projection,
        dispatch: ModelDispatch::Near,
        coordinate_shift: 16,
        fog_factor: 65536,
    };
    let mut vertices = ModelVertices::new_invocation(model.header.vertex_slot_count);
    vertices
        .initialize(model, &context, &mut projection)
        .unwrap();
    let source = vertices
        .slots()
        .iter()
        .position(|slot| slot.world.is_some())
        .unwrap() as u16;
    let position = vertices.world(source as i16).unwrap().map(|v| v as u16);
    let terrain = terrain(&[]);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for (kind, particle_type, table) in [
        (0, 5, 0x53d1e0),
        (1, 6, 0x53d1f0),
        (2, 8, 0x53d200),
        (3, 11, 0x53d210),
    ] {
        let mut particles = ProcessParticleRuntime::default();
        let mut callbacks = FurnitureModelCallbacks {
            terrain: &terrain,
            request: FurnitureModelRequest {
                record: TerrainSparseRecordId {
                    bucket: 0,
                    index: 0,
                },
                h1_slot: slot,
                position: context.position,
                basis: context.basis,
                clock: 0x1234_abcd,
            },
            particles: &mut particles,
            executable,
            birth_inputs: birth_inputs(&process, 4, false),
            load_factor: 0xc000,
        };
        assert_eq!(callbacks.packed_value(0).unwrap(), 0xabcd);
        assert_eq!(callbacks.packed_value(0x10001).unwrap(), 0);
        assert!(matches!(
            callbacks.packed_value(1),
            Err(SoftwareSceneServiceError::FurnitureRecord { .. })
        ));
        let mut output = None;
        callbacks
            .custom_vertex(
                ModelCustomVertexRequest {
                    selector: 0xff07_u16 as i16 as i32,
                    extras: &[],
                },
                &mut vertices,
                &mut output,
                None,
            )
            .unwrap();
        assert_eq!(
            output,
            Some([0; 3]),
            "whole signed selector dispatch before any host/RNG read"
        );
        // Factor/source output precede the reached RNG error. Non7 above
        // and a rejected gate below never require a real birth invocation.
        assert!(matches!(
            callbacks.custom_vertex(
                ModelCustomVertexRequest {
                    selector: 7,
                    extras: &[source, kind]
                },
                &mut vertices,
                &mut output,
                None
            ),
            Err(ModelCustomVertexError::Host(
                SoftwareSceneServiceError::FurnitureRng
            ))
        ));
        assert_eq!(output, Some(position.map(|v| i32::from(v as i16))));
        callbacks.load_factor = 0;
        let mut rejected_rng = 0x1020_3040;
        let mut expected_rejected_rng = rejected_rng;
        assert_ne!(
            crate::name_entry::name_entry_rng_next(&mut expected_rejected_rng) % 48,
            0
        );
        callbacks
            .custom_vertex(
                ModelCustomVertexRequest {
                    selector: 7,
                    extras: &[source],
                },
                &mut vertices,
                &mut output,
                Some(&mut rejected_rng),
            )
            .unwrap();
        assert_eq!(rejected_rng, expected_rejected_rng);
        assert!(callbacks.particles.pending(1).unwrap().is_empty());
        callbacks.load_factor = 0xc000;
        let mut rng = 0x1020_3040;
        let mut expected_rng = rng;
        let velocity_y =
            128 + ((crate::name_entry::name_entry_rng_next(&mut expected_rng) as u16) >> 9);
        crate::name_entry::name_entry_rng_next(&mut expected_rng);
        callbacks
            .custom_vertex(
                ModelCustomVertexRequest {
                    selector: 7,
                    extras: &[source, kind],
                },
                &mut vertices,
                &mut output,
                Some(&mut rng),
            )
            .unwrap();
        assert_eq!(output, Some(position.map(|v| i32::from(v as i16))));
        drop(callbacks);
        assert_eq!(
            rng, expected_rng,
            "selector{particle_type} includes the actual constructor draw"
        );
        assert!(particles.active(1).unwrap().is_empty());
        assert_eq!(particles.pending(1).unwrap().len(), 1);
        let id = *particles.pending(1).unwrap().front().unwrap();
        let record = particles.record(id).unwrap();
        assert_eq!(record.read_dword(0x28).unwrap(), table);
        assert_eq!(record.read_dword(0x14).unwrap(), 0x1234_5678);
        assert_eq!(record.read_word(0x12).unwrap(), 0x89c);
        for axis in 0..3 {
            assert_eq!(record.read_word(4 + axis * 2).unwrap(), position[axis]);
            assert_eq!(
                record.read_word(0x0a + axis * 2).unwrap(),
                [0, velocity_y, 0][axis]
            );
        }
    }
}

#[test]
fn null_header_pending_item_is_contained_before_following_valid_h1_pixels() {
    let process = supplied_process();
    let (assets, tables, resident) = fixture();
    let (null, lookup) = lookups(&resident);
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut images = Vec::new();
    for with_failed_item in [false, true] {
        let records = if with_failed_item {
            vec![(0, 8, null), (2, 0, lookup)]
        } else {
            vec![(2, 0, lookup)]
        };
        let mut terrain = terrain(&records);
        let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
        let mut config = startup.scene;
        config.environment.view = view();
        config.environment.projection = inputs(&tables).context_projection;
        config.environment.coordinate_shift = 16;
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (config).environment,
            ),
            resources,
            tables: &tables,
            frame: &mut pixels,
            config,
        })
        .unwrap();
        scene.set_process_rng(Some(0x1020_3040));
        let mut state = FurnitureDrawState::new(&NativeBasisMath::new(assets.sine_table().values));
        let mut queue = FurnitureEffectQueue::default();
        queue
            .enqueue(
                &terrain,
                FurnitureQueueRequest {
                    xz: [0, 0],
                    program: FurnitureEffectProgram::new(0x0053_1250).unwrap(),
                },
            )
            .unwrap();
        queue.record_mut(0).unwrap().remaining_microseconds = 5_000_000;
        let lights = WorldLightGrid::default();
        let mut particles = ProcessParticleRuntime::default();
        let domain = FurnitureDomain {
            origin_x: 0,
            origin_z: 0,
            scale: 128,
            mask: 127,
            coords: vec![0, 0, 3],
            rows: 1,
        };
        let mut report = FurnitureSceneReport::default();
        draw_furniture(
            &mut scene,
            TerrainFurnitureRequest {
                terrain: &mut terrain,
                domain: &domain,
                inputs: inputs(&tables),
                far_color: 0,
            },
            &mut SoftwareFurnitureInputs {
                state: &mut state,
                pending: &queue,
                light_grid: &lights,
                particles: &mut particles,
                executable,
                birth_inputs: birth_inputs(&process, 4, false),
                load_factor: 4,
                crystal_flares: None,
            },
            &mut report,
        )
        .unwrap();
        assert_eq!(report.visited, if with_failed_item { 2 } else { 1 });
        assert_eq!(report.submitted_models, 1, "{report:?}");
        assert_eq!(report.omissions.len(), usize::from(with_failed_item));
        if with_failed_item {
            assert!(
                report.omissions[0].reason.contains("NullHeader"),
                "{report:?}"
            );
        }
        assert!(!scene.stopped());
        scene.submit(ResolvedSceneCommand::FlushBatch).unwrap();
        assert!(
            scene.pixels.stats.fragments > 0,
            "actual H1 must contribute pixels"
        );
        images.push(scene.pixels.frame.rgba().to_vec());
    }
    assert_eq!(
        images[0], images[1],
        "a reached required-header failure cannot abort the following item"
    );
}

#[test]
fn shared_terrain_replaces_stale_far_color_before_furniture_in_both_color_modes() {
    let process = supplied_process();
    let (assets, tables, resident) = fixture();
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let RenderResourceData::Material(material) = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument: 0x74,
        })
        .unwrap()
        .data
    else {
        unreachable!()
    };
    for night in [None, Some(0x123456)] {
        let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                (startup.scene).environment,
            ),
            resources,
            tables: &tables,
            frame: &mut pixels,
            config: startup.scene,
        })
        .unwrap();
        scene.primitive_state.far_color = 0xdead_beef;
        scene.set_process_rng(Some(0x10203040));
        let mut terrain = terrain(&[]);
        let mut state = FurnitureDrawState::new(&NativeBasisMath::new(assets.sine_table().values));
        let pending = FurnitureEffectQueue::default();
        let lights = WorldLightGrid::default();
        let mut particles = ProcessParticleRuntime::default();
        let mut draw = inputs(&tables);
        draw.night = night;
        scene
            .draw_terrain(SoftwareTerrainDrawRequest {
                traversal: TerrainDrawRequest {
                    terrain: &mut terrain,
                    inputs: draw,
                },
                furniture: SoftwareFurnitureInputs {
                    state: &mut state,
                    pending: &pending,
                    light_grid: &lights,
                    particles: &mut particles,
                    executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
                    birth_inputs: birth_inputs(&process, 4, false),
                    load_factor: 4,
                    crystal_flares: None,
                },
            })
            .unwrap();
        assert_eq!(
            scene.primitive_state.far_color,
            if night.is_some() {
                0
            } else {
                scene.color_layout.convert_material(material)
            }
        );
    }
}

// Append in software_scene/furniture/tests.rs. Reuses the existing real
// H11/H8 lookup and declared supplied terrain/view/particle construction domain.
#[test]
fn real_furniture_h1_stopped_scratch_is_hard_and_commits_prior_trail_rng() {
    use crate::model_scratch::{ModelScratchDrawExit, ModelScratchDrawState, ProcessModelScratch};

    let process = supplied_process();
    let (assets, tables, resident) = fixture();
    let (_, lookup) = lookups(&resident);
    let resources = RenderResources::new(&resident);
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([640, 480]),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut terrain = terrain(&[(2, 1, lookup)]);
    let mut pixels = SoftwareFrame::new(640, 480, [17, 23, 31, 255]);
    let mut config = startup.scene;
    config.environment.view = view();
    config.environment.projection = inputs(&tables).context_projection;
    config.environment.coordinate_shift = 16;
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            ProcessModelScratch::isolated_unknown_fixture(),
            (config).environment,
        ),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config,
    })
    .unwrap();
    scene.set_process_rng(Some(0x1020_3040));
    let scope = scene.scratch.begin_draw().unwrap();
    let allocation = scene.scratch.allocate(&scope, 2, 2).unwrap();
    scene
        .scratch
        .write_vertex(&allocation, 0, 9, [Some(0xe7)])
        .unwrap();
    scene
        .scratch
        .write_shade(
            &allocation,
            1,
            0,
            [Some(0x13), Some(0x27), Some(0x41), Some(0xb6)],
        )
        .unwrap();
    scene
        .scratch
        .finish_draw(&scope, ModelScratchDrawExit::StoppedOnError)
        .unwrap();
    let cursor_before = scene.scratch.cursor().unwrap();
    let pool_before = scene.scratch.read_pool::<56>(0).unwrap();
    let queue_before = format!("{:?}", scene.queue);
    let mut state = FurnitureDrawState::new(&NativeBasisMath::new(assets.sine_table().values));
    let pending = FurnitureEffectQueue::default();
    let lights = WorldLightGrid::default();
    let mut particles = ProcessParticleRuntime::default();
    let domain = FurnitureDomain {
        origin_x: 0,
        origin_z: 0,
        scale: 128,
        mask: 127,
        coords: vec![0, 0, 3],
        rows: 1,
    };
    let mut report = FurnitureSceneReport::default();
    let result = draw_furniture(
        &mut scene,
        TerrainFurnitureRequest {
            terrain: &mut terrain,
            domain: &domain,
            inputs: inputs(&tables),
            far_color: 0,
        },
        &mut SoftwareFurnitureInputs {
            state: &mut state,
            pending: &pending,
            light_grid: &lights,
            particles: &mut particles,
            executable,
            birth_inputs: birth_inputs(&process, 4, false),
            load_factor: 0xc000,
            crystal_flares: None,
        },
        &mut report,
    );
    assert!(
        matches!(&result, Err(FurniturePhaseError::Scene(error)) if error.is_model_scratch_fault()),
        "hard H1 scratch stop must retain its typed cause, not become an item omission: {result:?}"
    );
    assert_eq!(report.visited, 1);
    assert_eq!(report.submitted_models, 0);
    assert_eq!(report.no_draw, 0);
    assert!(report.omissions.is_empty());
    assert!(scene.furniture_omissions.is_empty());
    assert!(scene.stopped());
    assert_eq!(
        scene.scratch.state().unwrap(),
        ModelScratchDrawState::Stopped(scope)
    );
    assert_eq!(scene.scratch.cursor().unwrap(), cursor_before);
    assert_eq!(scene.scratch.read_pool::<56>(0).unwrap(), pool_before);
    assert_eq!(
        scene.scratch.read_vertex::<1>(&allocation, 0, 9).unwrap(),
        [Some(0xe7)]
    );
    assert_eq!(format!("{:?}", scene.queue), queue_before);
    let mut expected_rng = 0x1020_3040;
    // Low bits1 reaches velocity; load-factor0xc000 bypasses probability draw;
    // the genuine type6 trail constructor then consumes its one existing draw.
    crate::name_entry::name_entry_rng_next(&mut expected_rng);
    crate::name_entry::name_entry_rng_next(&mut expected_rng);
    assert_eq!(scene.process_rng(), Some(expected_rng));
    assert_eq!(particles.free_count(), 399);
    assert_eq!(particles.pending(1).unwrap().len(), 1);
    assert_eq!(scene.stats.models, 0);
}

mod scratch_custody;
