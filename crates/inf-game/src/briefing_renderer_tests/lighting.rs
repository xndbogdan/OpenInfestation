use super::*;
use crate::world_lighting::{
    terrain_lighting_inputs, TerrainLightColor, TerrainLightPolicy, TerrainLightRequest,
};

fn seed_grid(grid: &mut crate::world_lighting::WorldLightGrid, position: [i32; 2], radius: i32) {
    // Explicit synthetic old-world light, while all assets/consumers remain
    // original. Setter operands place its retained anchor before center.
    grid.set_anchor(position.map(|v| v + 0x1680 - radius));
    let result = grid
        .add(
            TerrainLightRequest {
                world_xz: position,
                radius,
                rgb: [100, -80, 60],
            },
            TerrainLightPolicy {
                enabled: true,
                spacing: TerrainSpacing::Units256,
                color: TerrainLightColor::Unmodified,
            },
        )
        .unwrap();
    assert!(result.cell_writes > 0);
    assert!(grid.cells().iter().any(|&byte| byte != 0));
}

#[test]
fn visible_enabled_camera_clears_old_world_light_even_when_dynamic_setting_is_off() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, resident) = level1_fixture(tier);
        for dynamic in [false, true] {
            for (visible, disabled) in [(false, false), (true, true), (true, false)] {
                level1_layer_pixels(
                    &assets,
                    &tables,
                    &resident,
                    dimensions,
                    |owner, frame, math| {
                        owner.dynamic_lighting = dynamic;
                        seed_grid(owner.effects.light_grid, [0x1000, 0x2000], 1024);
                        let anchors = owner.effects.light_grid.anchors();
                        let cells = owner.effects.light_grid.cells().to_vec();
                        let mut animation = open_panel();
                        if !visible {
                            animation.current_stage = 0;
                            animation.requested_stage = 0;
                        }
                        owner
                            .terrain_panel(
                                BriefingTerrainInputs {
                                    frame,
                                    animation,
                                    camera_position_words: [0x1000, 400, 0x2000],
                                    heading: 0x1234,
                                },
                                BriefingTerrainEffects {
                                    camera_disabled: disabled,
                                    pulse_remaining_micros: 0,
                                    report_omissions: false,
                                },
                                math,
                                &FrontendProcessState::default(),
                            )
                            .unwrap();
                        assert_eq!(owner.effects.light_grid.anchors(), anchors);
                        if visible && !disabled {
                            assert!(owner.effects.light_grid.cells().iter().all(|&v| v == 0));
                        } else {
                            assert_eq!(owner.effects.light_grid.cells(), cells);
                        }
                        match terrain_lighting_inputs(
                            owner.effects.light_grid,
                            dynamic,
                            owner.effects.executable,
                        )
                        .unwrap()
                        {
                            crate::terrain_draw::TerrainLighting::Disabled => assert!(!dynamic),
                            crate::terrain_draw::TerrainLighting::Dynamic {
                                anchors: read_anchors,
                                cells: read_cells,
                                outside,
                            } => {
                                assert!(dynamic);
                                assert_eq!(read_anchors, anchors);
                                assert_eq!(read_cells, owner.effects.light_grid.cells());
                                assert_eq!(outside, [0; 3]);
                            }
                        }
                    },
                );
            }
        }
    }
}

#[test]
fn ground_bridge_uses_supplied_live_cells_only_with_real_dynamic_setting() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, resident) = level1_fixture(tier);
        let mut images = Vec::new();
        for dynamic in [false, true] {
            images.push(level1_layer_pixels(
                &assets,
                &tables,
                &resident,
                dimensions,
                |owner, frame, math| {
                    owner.dynamic_lighting = dynamic;
                    let pane = briefing_terrain_pane(
                        BriefingTerrainInputs {
                            frame,
                            animation: open_panel(),
                            camera_position_words: [0x1000, 400, 0x2000],
                            heading: 0x1234,
                        },
                        math,
                    )
                    .unwrap()
                    .unwrap();
                    owner.terrain_backdrop(pane, frame).unwrap();
                    // Inject a supplied light after the actual clear to isolate
                    // ground readback. Production phase3 has no writer here;
                    // this fixture does not claim one or retail light pixels.
                    seed_grid(owner.effects.light_grid, [0x1000, 0x2000], 4096);
                    owner
                        .terrain_ground(pane, frame, false, &FrontendProcessState::default(), false)
                        .unwrap();
                    assert!(owner.scene.stats.terrain_commands > 0);
                },
            ));
        }
        assert!(
            images[0] != images[1],
            "settings+80 must gate ground RGB at tier{tier}"
        );
    }
}

#[test]
fn terrain_prefix_prevents_old_world_light_from_reaching_furniture_at_both_tiers() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, resident) = level1_fixture(tier);
        level1_layer_pixels(
            &assets,
            &tables,
            &resident,
            dimensions,
            |owner, frame, math| {
                use crate::terrain_furniture::{
                    setup_furniture_item, FurnitureAnchors, FurnitureSetupInputs,
                };
                seed_grid(owner.effects.light_grid, [0x1000, 0x2000], 1024);
                let anchors = owner.effects.light_grid.anchors();
                // Synthetic sparse placement at the deposited light center.
                // It passes the original lookup-zero admission and height read.
                let record = inf_formats::TerrainRecord {
                    x: 0x1000 / 128,
                    resource_lookup: inf_formats::TerrainResourceLookup::from_raw(1),
                    state: 0,
                };
                let sample = |owner: &SoftwareBriefingFrame<'_, '_, '_>| {
                    setup_furniture_item(
                        record,
                        0x2000,
                        &FurnitureSetupInputs {
                            cells: owner.world.terrain.cells(),
                            height_shift: match owner.world.terrain.sampling().height_scale {
                                crate::terrain_runtime::TerrainHeightScale::Units16 => 4,
                                crate::terrain_runtime::TerrainHeightScale::Units32 => 5,
                            },
                            mask: 255,
                            anchors: FurnitureAnchors {
                                x: i32::from(anchors[0]),
                                z: i32::from(anchors[1]),
                            },
                            light_cells: owner.effects.light_grid.cells(),
                            outside_light_word: 0,
                            tint_word: owner.world.presentation.tint_word,
                        },
                    )
                    .unwrap()
                    .unwrap()
                };
                let old = sample(owner);
                assert!(old.in_light_grid);
                assert_eq!(old.light_rgb, [100, -80, 60]);
                let pane = briefing_terrain_pane(
                    BriefingTerrainInputs {
                        frame,
                        animation: open_panel(),
                        camera_position_words: [0x1000, 400, 0x2000],
                        heading: 0x1234,
                    },
                    math,
                )
                .unwrap()
                .unwrap();
                owner.terrain_backdrop(pane, frame).unwrap();
                let current = sample(owner);
                assert_eq!(current.light_rgb, [0; 3]);
                let tint = owner.world.presentation.tint_word.to_le_bytes();
                assert_eq!(
                    current.ambient_rgb,
                    [tint[0], tint[1], tint[2]].map(|v| i32::from(v >> 4))
                );
                assert_eq!(current.anchor_cell, old.anchor_cell);
            },
        );
    }
}
