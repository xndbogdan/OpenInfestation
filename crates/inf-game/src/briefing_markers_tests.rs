use std::path::Path;

use super::*;
use crate::{
    briefing_map::{briefing_map_backdrop, BriefingMapInputs},
    briefing_runtime::BriefingPanelAnimation,
    briefing_scene::BriefingSceneFrame,
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_display::frontend_startup,
    frontend_scene::SceneProjection,
    menu_text::MenuTextResources,
    model_primitives::PrimitiveTables,
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs, BaseContext,
    },
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
    },
    scene_resources::SceneResourceBackend,
    software_frame::SoftwareFrame,
    software_scene::{SoftwareScene, SoftwareSceneRequest},
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

fn inputs() -> BriefingMarkerInputs {
    BriefingMarkerInputs {
        tile: BriefingMarkerTile {
            origin: [10, 20],
            size: [256; 2],
        },
        scan_age: 0,
        jitter_radius: 0,
        initial_jitter_x: None,
        initial_jitter_y: 0,
        network_active: false,
        material_shifts: [6, 1, 4],
        session_phase: 3,
    }
}

fn actor() -> BriefingMapActor {
    BriefingMapActor {
        flags: 0x21,
        position_words: Some([0x8000, 123, 0x7f00]),
        material_word_b4: 1,
        pickup_spawner: None,
        vehicle: None,
        banner: false,
    }
}

/// The one marker an ordinary actor submits.
fn single(quads: Vec<DirectSpriteQuad>) -> DirectSpriteQuad {
    assert_eq!(quads.len(), 1);
    quads[0]
}

const MATERIAL: u32 = 0xffff;
const STYLE: u32 = 0xabff_ffff;

#[test]
fn briefing_marker_admitted_native_vtables_keep_null_special_marker_queries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let exe = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    // Shared/camera/30/88/90; derived base/11/60/77; the admitted family
    // vtables; selector-2 vehicle. Addresses identify original final classes.
    for table in [
        0x00527c00, 0x005204d0, 0x005289c0, 0x00524ac0, 0x00526b50, 0x00520e58, 0x0051f080,
        0x00527de8, 0x0051fbb0, 0x00520f34, 0x0052104c, 0x00521164, 0x00520b84, 0x00520cc8,
        0x00527058, 0x00527ee0, 0x00521eac, 0x0052273c, 0x00520860,
    ] {
        assert_eq!(
            exe.word(table + 0x70).unwrap(),
            0x00401300,
            "vtable {table:08x} +70"
        );
        assert_eq!(
            exe.word(table + 0xb8).unwrap(),
            0x00401300,
            "vtable {table:08x} +B8"
        );
        assert_eq!(
            exe.word(table + 0x64).unwrap(),
            if table == 0x00520860 {
                0x00404610
            } else {
                0x00401300
            }
        );
    }
}

#[test]
fn briefing_marker_palette_preserves_network_case_and_signed_b4_default() {
    for (word, slot) in [
        (1, 0x56),
        (2, 0x56),
        (3, 0x5a),
        (4, 0x5c),
        (5, 0x5d),
        (6, 0x5e),
        (7, 0x5f),
        (8, 0x60),
        (9, 0x61),
        (10, 0x5b),
        (11, 0x5a),
        (12, 0x5c),
        (13, 0x5d),
        (14, 0x5e),
        (15, 0x5f),
        (16, 0x60),
        (17, 0x61),
        (18, 0x57),
        (0, 0x59),
        (-1, 0x59),
        (i32::MIN, 0x59),
    ] {
        assert_eq!(
            briefing_marker_material_lookup(word, false),
            BriefingMarkerMaterialLookup {
                initial_slot: slot,
                replacement_slot: None
            }
        );
        assert_eq!(
            briefing_marker_material_lookup(word, true),
            BriefingMarkerMaterialLookup {
                initial_slot: slot,
                replacement_slot: if word == 2 { Some(0x5b) } else { None }
            }
        );
    }
}

#[test]
fn briefing_marker_admission_skips_position_palette_and_rng_for_excluded_actors() {
    let mut rng = 0;
    let mut pass = BriefingMarkerPass::new(BriefingMarkerInputs {
        jitter_radius: 100,
        ..inputs()
    });
    for flags in [0, 1, 0x20, 0x10021, 0x10001] {
        assert_eq!(
            pass.actor(
                BriefingMapActor {
                    flags,
                    position_words: None,
                    material_word_b4: 1,
                    pickup_spawner: None,
                    vehicle: None,
                    banner: false,
                },
                &mut rng,
                |_| -> Result<u32, ()> { panic!("excluded palette read") },
                |_| panic!("excluded style read"),
            )
            .unwrap(),
            Vec::<DirectSpriteQuad>::new()
        );
    }
    assert_eq!(rng, 0);
    assert_eq!(
        pass.actor(
            BriefingMapActor {
                position_words: None,
                ..actor()
            },
            &mut rng,
            |_| -> Result<u32, ()> { panic!("unknown position palette read") },
            |_| panic!("unknown position style read"),
        ),
        Err(BriefingMarkerError::MissingPosition)
    );
    assert_eq!(rng, 0);
}

#[test]
fn briefing_marker_coordinates_scan_color_and_dim_mask_follow_native_words() {
    let mut rng = 123;
    let mut pass = BriefingMarkerPass::new(inputs());
    let quad = pass
        .actor(
            actor(),
            &mut rng,
            |lookup| {
                assert_eq!(
                    lookup,
                    BriefingMarkerMaterialLookup {
                        initial_slot: 0x56,
                        replacement_slot: None
                    }
                );
                Ok::<_, ()>(MATERIAL)
            },
            |_| Ok(STYLE),
        )
        .map(single)
        .unwrap();
    assert_eq!(quad.sprite_slot, 7);
    assert_eq!(quad.vertex_depths, [64; 4]);
    assert_eq!(quad.descriptor_word, 0);
    assert_eq!(
        quad.corners,
        [[135, 146], [141, 146], [141, 152], [135, 152]]
    );
    assert_eq!(
        quad.mode,
        DirectSpriteMode::Effect {
            color_word: 0xabf6_f6f6
        }
    );
    assert_eq!(rng, 123);
    let dim = pass
        .actor(
            BriefingMapActor {
                flags: 0x8021,
                ..actor()
            },
            &mut rng,
            |_| Ok::<_, ()>(MATERIAL),
            |_| Ok(STYLE),
        )
        .map(single)
        .unwrap();
    assert_eq!(marker_dim_mask([6, 1, 4]), 0x7bdf);
    assert_eq!(
        dim.mode,
        DirectSpriteMode::Effect {
            color_word: 0xab77_7777
        }
    );
    // One scan-age step is 64 world WORD units; the 256-unit boundary wraps.
    let mut shifted = BriefingMarkerPass::new(BriefingMarkerInputs {
        scan_age: 4,
        ..inputs()
    });
    let shifted = shifted
        .actor(actor(), &mut rng, |_| Ok::<_, ()>(MATERIAL), |_| Ok(STYLE))
        .map(single)
        .unwrap();
    assert_eq!(
        shifted.mode,
        DirectSpriteMode::Effect {
            color_word: 0xabf5_f5f5
        }
    );
    let mut words = BriefingMarkerPass::new(BriefingMarkerInputs {
        tile: BriefingMarkerTile {
            origin: [31_000, -32_000],
            size: [2048; 2],
        },
        ..inputs()
    });
    let words = words
        .actor(
            BriefingMapActor {
                position_words: Some([0xffff, 0, 0]),
                ..actor()
            },
            &mut rng,
            |_| Ok::<_, ()>(MATERIAL),
            |_| Ok(STYLE),
        )
        .map(single)
        .unwrap();
    assert_eq!(
        words.corners,
        [
            [-32505, -29968],
            [-32473, -29968],
            [-32473, -29936],
            [-32505, -29936]
        ]
    );
}

#[test]
fn briefing_marker_jitter_keeps_previous_screen_cursor_and_reached_rng_prefix() {
    let source = BriefingMarkerInputs {
        jitter_radius: 100,
        initial_jitter_x: Some(1000),
        initial_jitter_y: 2000,
        ..inputs()
    };
    let mut rng = 0;
    let mut pass = BriefingMarkerPass::new(source);
    let first = pass
        .actor(actor(), &mut rng, |_| Ok::<_, ()>(MATERIAL), |_| Ok(STYLE))
        .map(single)
        .unwrap();
    assert_eq!(rng, 0x60a7_666a);
    assert_eq!(first.corners, [[11, 267], [15, 267], [15, 271], [11, 271]]);
    let second = pass
        .actor(actor(), &mut rng, |_| Ok::<_, ()>(MATERIAL), |_| Ok(STYLE))
        .map(single)
        .unwrap();
    assert_eq!(rng, 0x2284_07e4);
    assert_eq!(second.corners, [[8, 273], [12, 273], [12, 277], [8, 277]]);
    let mut missing = BriefingMarkerPass::new(BriefingMarkerInputs {
        initial_jitter_x: None,
        ..source
    });
    let mut prefix = 0;
    assert_eq!(
        missing.actor(
            actor(),
            &mut prefix,
            |_| -> Result<u32, ()> { panic!("unknown cursor palette read") },
            |_| panic!("unknown cursor style read"),
        ),
        Err(BriefingMarkerError::MissingInitialJitterX)
    );
    assert_eq!(
        prefix, 0x0013_d573,
        "native first RNG draw precedes the missing stack read"
    );
    let mut material_failure = BriefingMarkerPass::new(source);
    let mut prefix = 0;
    assert_eq!(
        material_failure.actor(
            actor(),
            &mut prefix,
            |_| Err("absent material"),
            |_| panic!("style after a failed material"),
        ),
        Err(BriefingMarkerError::Resource("absent material"))
    );
    assert_eq!(prefix, 0x60a7_666a);
}

#[test]
fn briefing_marker_arena_adapter_uses_g48_order_and_live_words() {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let sine = [0; 4096];
    let mut ids = Vec::new();
    for (index, position) in [[10, 20, 30], [40, 50, 60]].into_iter().enumerate() {
        let id = arena
            .allocate_actor(ActorConstructionRequest {
                constructor_null:
                    crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: Some(position),
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: Some(0),
                    sine_quarter: &sine,
                },
            })
            .unwrap();
        arena
            .initialize_context(
                id,
                BaseContext {
                    position_words: Some(position),
                    placement_index: index as u32,
                },
            )
            .unwrap();
        ids.push(id);
    }
    assert_eq!(arena.actor_list(), ids);
    let source: Vec<_> = arena
        .actor_list()
        .iter()
        .map(|id| BriefingMapActor::from_arena(&arena, *id).unwrap())
        .collect();
    assert_eq!(source[0].position_words, Some([10, 20, 30]));
    assert_eq!(source[1].position_words, Some([40, 50, 60]));
    assert_eq!(
        source[0].material_word_b4,
        arena.actor(ids[0]).unwrap().word_b4 as i32
    );
}

#[test]
fn briefing_marker_original_effect_pixels_keep_four_interleaved_tile_passes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let tables = PrimitiveTables::from_executable(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        resident
            .apply(ResidencyRequest::Select {
                selection: ResourceSelection {
                    difficulty: tier,
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
        let resources = RenderResources::new(&resident);
        let startup = frontend_startup(
            &resident,
            crate::presentation::NativeFrame::original(dimensions),
        )
        .unwrap();
        let source = BriefingMapInputs {
            frame: BriefingSceneFrame {
                projection: SceneProjection {
                    ..startup.projection
                },
                age_millis: 0,
                start_remaining_millis: 0,
                animation: AnimationClock::default(),
                selector_one: 1,
            },
            animation: BriefingPanelAnimation {
                current_stage: 2,
                requested_stage: 2,
                remaining_micros: 0,
                duration_micros: 500000,
                current_scale_word: 0,
                requested_scale_word: 0,
            },
            map_scale_word: 0,
            camera_xz_words: [0x8000; 2],
        };
        let plan = briefing_map_backdrop(source, resources.style(0x3b).unwrap()).unwrap();
        let mut images = Vec::new();
        let mut order = Vec::new();
        for markers in [false, true] {
            let mut pixels = SoftwareFrame::new(dimensions[0], dimensions[1], [17, 23, 31, 255]);
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
            scene.begin_frame().unwrap();
            scene.pixels.pipeline.viewport = plan.clip_rect;
            let mut rng = 123;
            for (index, tile) in plan.tiles.into_iter().enumerate() {
                scene.draw_sprite_direct(tile.backdrop).unwrap();
                if markers {
                    order.push((index, "tile"));
                    let mut pass = BriefingMarkerPass::new(BriefingMarkerInputs {
                        tile: tile.markers,
                        material_shifts: scene.pixels.renderer.material_shifts,
                        ..inputs()
                    });
                    let quad = pass
                        .actor(
                            actor(),
                            &mut rng,
                            |lookup| {
                                let resource = resources.resolve(RenderResourceRequest {
                                    kind: RenderResourceKind::Material,
                                    argument: lookup.initial_slot,
                                })?;
                                let RenderResourceData::Material(material) = resource.data else {
                                    unreachable!()
                                };
                                assert!(lookup.replacement_slot.is_none());
                                Ok::<_, crate::render_resources::RenderResourceError>(
                                    scene.color_layout().convert_material(material),
                                )
                            },
                            |slot| resources.style(slot),
                        )
                        .map(single)
                        .unwrap();
                    scene.draw_sprite_direct(quad).unwrap();
                    order.push((index, "marker"));
                }
            }
            assert_eq!(rng, 123, "ctor-owned zero jitter draws no RNG");
            images.push(scene.pixels.frame.rgba().to_vec());
        }
        assert_eq!(
            order,
            [
                (0, "tile"),
                (0, "marker"),
                (1, "tile"),
                (1, "marker"),
                (2, "tile"),
                (2, "marker"),
                (3, "tile"),
                (3, "marker")
            ]
        );
        let mut changed = 0;
        for (index, (before, after)) in images[0]
            .chunks_exact(4)
            .zip(images[1].chunks_exact(4))
            .enumerate()
        {
            if before == after {
                continue;
            }
            changed += 1;
            let x = (index % dimensions[0] as usize) as i16;
            let y = (index / dimensions[0] as usize) as i16;
            assert!(
                x >= plan.clip_rect[0]
                    && x <= plan.clip_rect[2]
                    && y >= plan.clip_rect[1]
                    && y <= plan.clip_rect[3]
            );
        }
        assert!(
            changed > 0,
            "original H3/7 markers must alter original backdrop pixels at tier {tier}"
        );
    }
}

#[test]
fn briefing_marker_spawner_takes_jitter_cursor_and_palette_before_its_gate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let exe = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    // 00526648, the pickup spawner's table, returns itself at +70 only.
    assert_eq!(exe.word(0x00526648 + 0x70).unwrap(), 0x00404610);
    assert_eq!(exe.word(0x00526648 + 0xb8).unwrap(), 0x00401300);
    assert_eq!(exe.word(0x00526648 + 0x64).unwrap(), 0x00401300);
    // Infest01's spawners carry low flags 0x0A, outside the ordinary gate.
    let spawner = BriefingMapActor {
        flags: 0x0a,
        material_word_b4: 3,
        pickup_spawner: Some(MapSpawner { kind: 2, count: 5 }),
        ..actor()
    };
    let source = BriefingMarkerInputs {
        jitter_radius: 100,
        initial_jitter_x: Some(1000),
        ..inputs()
    };
    let mut rng = 0;
    let mut lookups = Vec::new();
    let mut pass = BriefingMarkerPass::new(source);
    assert_eq!(
        pass.actor(
            spawner,
            &mut rng,
            |lookup| {
                lookups.push(lookup);
                Ok::<_, ()>(MATERIAL)
            },
            |_| panic!("C390's spawner reads no style"),
        ),
        Ok(Vec::new())
    );
    assert_eq!(rng, 0x60a7_666a, "both jitter draws");
    assert_eq!(lookups, [briefing_marker_material_lookup(3, false)]);
    // The spawner moved the jitter cursor exactly as a marker there would.
    let follower = pass
        .actor(actor(), &mut rng, |_| Ok::<_, ()>(MATERIAL), |_| Ok(STYLE))
        .unwrap();
    let mut marker = BriefingMarkerPass::new(source);
    let mut marker_rng = 0;
    marker
        .actor(
            actor(),
            &mut marker_rng,
            |_| Ok::<_, ()>(MATERIAL),
            |_| Ok(STYLE),
        )
        .unwrap();
    assert_eq!(
        marker
            .actor(
                actor(),
                &mut marker_rng,
                |_| Ok::<_, ()>(MATERIAL),
                |_| Ok(STYLE)
            )
            .unwrap(),
        follower
    );
    // Only the final argument's low byte draws a spawner marker.
    let mut pass = BriefingMarkerPass::new(BriefingMarkerInputs {
        initial_jitter_y: 0x100,
        ..inputs()
    });
    assert_eq!(
        pass.actor(
            spawner,
            &mut rng,
            |_| Ok::<_, ()>(MATERIAL),
            |_| panic!("spawner style"),
        ),
        Ok(Vec::new())
    );
}

#[test]
fn map_spawner_markers_take_their_kinds_style_and_twice_their_count() {
    let spawner = |kind, count| BriefingMapActor {
        flags: 0x0a,
        material_word_b4: 3,
        pickup_spawner: Some(MapSpawner { kind, count }),
        ..actor()
    };
    // The Resource scanner's byte: 00472EB5..00472F77.
    let scanner = BriefingMarkerInputs {
        initial_jitter_y: 1,
        ..inputs()
    };
    let mut rng = 0;
    let mut slots = Vec::new();
    let mut pass = BriefingMarkerPass::new(scanner);
    let mut style = |slot: i32| {
        slots.push(slot);
        Ok::<_, ()>(0x8000_0000 | slot as u32)
    };
    // The test actor projects to (138, 149) on the 256-pixel tile.
    assert_eq!(
        pass.actor(spawner(2, 5), &mut rng, |_| Ok(MATERIAL), &mut style),
        Ok(vec![blip([138, 149], 10, 0x8000_0023)])
    );
    // A kind outside 1..3 keeps the previous spawner's word.
    assert_eq!(
        pass.actor(spawner(0, -3), &mut rng, |_| Ok(MATERIAL), &mut style),
        Ok(vec![blip([138, 149], -6, 0x8000_0023)])
    );
    assert_eq!(
        pass.actor(spawner(3, 1), &mut rng, |_| Ok(MATERIAL), &mut style),
        Ok(vec![blip([138, 149], 2, 0x8000_0024)])
    );
    assert_eq!(slots, [0x23, 0x24]);
    // A pass's first spawner of another kind reads unwritten stack.
    let mut fresh = BriefingMarkerPass::new(scanner);
    assert_eq!(
        fresh.actor(
            spawner(4, 1),
            &mut rng,
            |_| Ok::<_, ()>(MATERIAL),
            |_| { panic!("no style for kind 4") }
        ),
        Err(BriefingMarkerError::SpawnerStyle { kind: 4 })
    );
    let quad = blip([138, 149], 10, 0x8000_0023);
    assert_eq!(quad.sprite_slot, 7);
    assert_eq!(
        quad.corners,
        [[128, 139], [148, 139], [148, 159], [128, 159]]
    );
    assert_eq!(quad.vertex_depths, [64; 4]);
    assert_eq!(quad.descriptor_word, 0);
}

#[test]
fn map_vehicle_marker_trails_eight_pairs_outside_phase_3_while_alive() {
    let identity = inf_formats::native_basis::NativeBasis::INITIAL.words;
    let vehicle = |word_c4, basis_x| BriefingMapActor {
        vehicle: Some(MapVehicle { word_c4, basis_x }),
        ..actor()
    };
    let marker = |phase, actor| {
        let mut pass = BriefingMarkerPass::new(BriefingMarkerInputs {
            session_phase: phase,
            ..inputs()
        });
        pass.actor(actor, &mut 0, |_| Ok::<_, ()>(MATERIAL), |_| Ok(STYLE))
            .unwrap()
    };
    let alive = vehicle(3, [identity[1], identity[3]]);
    // The briefing (phase 3) and a dying vehicle draw the marker alone.
    assert_eq!(marker(3, alive).len(), 1);
    assert_eq!(marker(7, vehicle(2, [identity[1], identity[3]])).len(), 1);
    let quads = marker(7, alive);
    assert_eq!(quads.len(), 17);
    let head = quads[0];
    let [[left, top], ..] = head.corners;
    let radius = 3;
    let center = [i32::from(left) + radius, i32::from(top) + radius];
    assert_eq!(center, [138, 149]);
    // X row (1, 0, 0): 16 along and 0 across, so the steps are 16, 8, 16
    // and 8 and the k-th pair lies at (x - k, y + 2k) and (x + k, y + 2k).
    for k in 0..8 {
        for (quad, x) in [
            (quads[1 + 2 * k], 138 - k as i32),
            (quads[2 + 2 * k], 138 + k as i32),
        ] {
            assert_eq!(
                quad,
                blip(
                    [x, 149 + 2 * k as i32],
                    radius,
                    match head.mode {
                        DirectSpriteMode::Effect { color_word } => color_word,
                        DirectSpriteMode::Textured => unreachable!(),
                    }
                )
            );
        }
    }
    // A turned row: X = cos, Z = sin of 30 degrees. ftol(0.866 * 16) = 13,
    // ftol(0.5 * -16) = -8; the steps are 17, -2, 9 and 14.
    let turned = marker(
        7,
        vehicle(3, [(0.866_025_4_f32).to_bits(), 0.5_f32.to_bits()]),
    );
    let centers: Vec<_> = turned[1..]
        .iter()
        .map(|quad| {
            [
                i32::from(quad.corners[0][0]) + 3,
                i32::from(quad.corners[0][1]) + 3,
            ]
        })
        .collect();
    assert_eq!(centers[2], [138 - (-2 / 8), 149 + 17 / 8]);
    assert_eq!(centers[3], [138 + 14 / 8, 149 + 9 / 8]);
    assert_eq!(centers[14], [138 - (-14 / 8), 149 + 119 / 8]);
    assert_eq!(centers[15], [138 + 98 / 8, 149 + 63 / 8]);
}

/// 00472D66..00472DC2: a banner (virtual +B8) enters whatever its flags and
/// blinks, drawn while the scan's age over 15 is even.
#[test]
fn briefing_marker_banner_blinks_with_the_scan() {
    let banner = BriefingMapActor {
        flags: 0,
        banner: true,
        ..actor()
    };
    for (age, drawn) in [(0, 1), (14, 1), (15, 0), (29, 0), (30, 1)] {
        let mut rng = 1;
        let mut pass = BriefingMarkerPass::new(BriefingMarkerInputs {
            scan_age: age,
            ..inputs()
        });
        let quads = pass
            .actor(banner, &mut rng, |_| Ok::<_, ()>(MATERIAL), |_| Ok(STYLE))
            .unwrap();
        assert_eq!(quads.len(), drawn, "age {age}");
    }
}
