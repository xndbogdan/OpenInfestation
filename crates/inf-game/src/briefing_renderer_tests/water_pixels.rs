use super::*;

fn submerged_focus(terrain: &crate::terrain_runtime::TerrainRuntime) -> [i32; 3] {
    let plane = (terrain.plane_base() >> terrain.plane_shift()) as i16;
    let step = usize::from(terrain.sampling().spacing.units() / 128);
    let (_, x, z) = (step..512 - step)
        .step_by(step)
        .flat_map(|x| {
            (step..512 - step).step_by(step).filter_map(move |z| {
                let heights = [[x, z], [x + step, z], [x, z + step], [x + step, z + step]]
                    .map(|[x, z]| terrain.raw_height_at([(x * 128) as u16, (z * 128) as u16]));
                let highest = *heights.iter().max().unwrap();
                (i32::from(highest) < i32::from(plane) - 128).then_some((highest, x, z))
            })
        })
        .min()
        .expect("actual H12 contains a complete submerged tile");
    [
        i32::from(((x * 128 + step * 64) as u16) as i16),
        i32::from(plane),
        i32::from(((z * 128 + step * 64) as u16) as i16),
    ]
}

#[test]
fn authored_water_briefing_panes_use_world_wave_flags_and_process_clock_at_both_tiers() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, mut resident) = level1_fixture(tier);
        // Two independent authored wave worlds, not an intro-only terrain path.
        for leaf in [16, 34] {
            resident
                .apply(ResidencyRequest::Activate { level: leaf })
                .unwrap();
            let mut images = Vec::new();
            for clock in [0, 256] {
                images.push(level1_layer_pixels(
                    &assets,
                    &tables,
                    &resident,
                    dimensions,
                    |owner, mut frame, _| {
                        assert_ne!(owner.world.world_flags & 0x40, 0);
                        let focus = submerged_focus(owner.world.terrain);
                        let eye = [focus[0], focus[1] + 512, focus[2] - 2048];
                        let view = crate::frontend_scene::SceneView {
                            eye,
                            basis: crate::frontend_scene::camera_basis(eye, focus),
                        };
                        // Isolate actual H12 water/ground from furniture animation.
                        // Only the mutable runtime's supported lookup state changes;
                        // original H12 cells and H3/H7 bytes remain untouched.
                        let records: Vec<_> = owner
                            .world
                            .terrain
                            .buckets()
                            .iter()
                            .enumerate()
                            .flat_map(|(bucket, row)| {
                                row.records.iter().enumerate().map(move |(index, record)| {
                                    (
                                        crate::terrain_runtime::TerrainSparseRecordId {
                                            bucket: bucket as u16,
                                            index,
                                        },
                                        record.state,
                                    )
                                })
                            })
                            .collect();
                        for (record, state) in records {
                            owner
                                .world
                                .terrain
                                .write_sparse_payload(
                                    record,
                                    crate::terrain_runtime::TerrainSparsePayload {
                                        state,
                                        resource_lookup:
                                            inf_formats::TerrainResourceLookup::from_raw(0),
                                    },
                                )
                                .unwrap();
                        }
                        frame.animation.advance(clock * 20_000);
                        assert_eq!(frame.animation.value(), clock);
                        let setup = crate::frontend_scene::SceneSetup {
                            focal: frame.projection.focal,
                            viewport: frame.projection.viewport,
                            center: frame.projection.viewport.map(|v| v / 2),
                            fade: [8192, 9216],
                            fade_factor: 65536,
                        };
                        owner
                            .scene
                            .submit(ResolvedSceneCommand::Setup(setup))
                            .unwrap();
                        owner
                            .scene
                            .submit(ResolvedSceneCommand::PublishView(view))
                            .unwrap();
                        owner
                            .scene
                            .submit(ResolvedSceneCommand::CoordinateShift(16))
                            .unwrap();
                        owner
                            .terrain_ground(
                                BriefingTerrainPane {
                                    clip_rect: [
                                        0,
                                        0,
                                        dimensions[0] as i16 - 1,
                                        dimensions[1] as i16 - 1,
                                    ],
                                    context: setup,
                                    view,
                                },
                                frame,
                                false,
                                &FrontendProcessState::default(),
                                false,
                            )
                            .unwrap();
                        assert!(
                            owner.scene.stats.water_commands > 0,
                            "leaf{leaf}/tier{tier}/clock{clock}"
                        );
                        assert_eq!(owner.scene.stats.models, 0);
                        assert!(
                            owner.scene.pixels.stats.d6_quads > 0
                                || owner.scene.pixels.stats.textured_triangles > 0
                        );
                        assert!(owner.scene.pixels.stats.fragments > 0);
                    },
                ));
            }
            assert_ne!(
                images[0], images[1],
                "leaf{leaf}/tier{tier}: retained native clock must change authored water pixels"
            );
        }
    }
}
