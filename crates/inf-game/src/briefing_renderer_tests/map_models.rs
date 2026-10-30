//! 0040B0F0 uses one begin/reset and one flush around C390 plus B840.
//! Original authored previews and an explicit camera/clock fixture exercise
//! the real producer, containment and pixel owners inside that bracket.

use super::*;

fn authored_model(owner: &SoftwareBriefingFrame<'_, '_, '_>, slot: i16) -> BriefingModel {
    let placements = WorldPlacements::new(owner.world.residency, TerrainSpacing::Units256).unwrap();
    inf_formats::parse_briefing_program(
        placements.world_source().overlay.bytes(),
        &placements.assets().worlds,
        placements.world(),
    )
    .unwrap()
    .records
    .iter()
    .find_map(|record| match record.payload {
        BriefingPayload::Model(model) if model.model_slot == slot => Some(model),
        _ => None,
    })
    .expect("original H14 preview payload")
}

fn map_inputs(frame: BriefingSceneFrame) -> BriefingMapInputs {
    BriefingMapInputs {
        frame,
        animation: open_panel(),
        map_scale_word: 0,
        camera_xz_words: [0, 0],
    }
}

#[test]
fn authored_map_model_bracket_matches_one_native_flush_including_omitted_preview() {
    let closed = BriefingPanelAnimation {
        current_stage: 0,
        requested_stage: 0,
        ..open_panel()
    };
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        // Level 1's ACAM completes. Leaf 12's thumper reads register 1 before
        // writing it: after the map frame it starts with that frame's bases
        // and completes; with the map panel closed it stops at an omittable
        // unowned register read, which no Level 1 preview reaches.
        for (slot, map) in [(117, open_panel()), (46, open_panel()), (46, closed)] {
            let map_open = map == open_panel();
            let preview = |owner: &SoftwareBriefingFrame<'_, '_, '_>| {
                if slot == 46 {
                    leaf12_thumper_preview()
                } else {
                    authored_model(owner, slot)
                }
            };
            let reference = with_batch_fixture(tier, dimensions, |owner, frame, math| {
                let model = preview(owner);
                owner.begin(true).unwrap();
                if map_open {
                    owner
                        .map_backdrop(map_inputs(frame), &FrontendProcessState::default())
                        .unwrap();
                    owner
                        .panel_frame(BriefingPanel::Map, open_panel(), frame, math)
                        .unwrap();
                }
                let before_preview = owner.scene.queue.state();
                let entry = owner.feature_entry().unwrap();
                let result = draw_briefing_model_preview(
                    model,
                    2000,
                    model_extent(owner.resources, slot).unwrap(),
                    map_open,
                    frame,
                    math,
                    &mut ResidentSceneHost {
                        resources: owner.resources,
                        backend: &mut *owner.scene,
                    },
                );
                if !map_open {
                    let error = result.expect_err("the thumper's register 1 is unowned");
                    assert!(is_thumper_register_stop(&error), "{error:?}");
                    owner
                        .omit_feature("preview reference", entry, error)
                        .unwrap();
                } else {
                    result.unwrap();
                }
                assert!(owner.scene.queue.state().command_count >= before_preview.command_count);
                assert_eq!(owner.scene.queue.state().last_flush_completed, 0);
                owner
                    .panel_frame(BriefingPanel::Model, open_panel(), frame, math)
                    .unwrap();
                let queued = owner.scene.queue.state().command_count;
                assert!(queued > before_preview.command_count);
                owner
                    .scene
                    .submit(ResolvedSceneCommand::FlushBatch)
                    .unwrap();
                assert_eq!(owner.scene.queue.state().open_groups, 0);
                // The allocation ledger includes group records as well as
                // replayed pixel commands; the latter is a strict subset.
                assert_eq!(owner.scene.queue.state().command_count, queued);
                assert!(owner.scene.queue.state().last_flush_completed > 0);
                (
                    owner.scene.pixels.frame.rgba().to_vec(),
                    owner.scene.queue.state(),
                    owner.scene.process_rng(),
                )
            });
            with_batch_fixture(tier, dimensions, |owner, frame, math| {
                let model = preview(owner);
                let begins = owner.scene.stats.begins;
                let resets = owner.scene.stats.depth_resets;
                let flushes = owner.scene.stats.flushes;
                owner
                    .scene_panels(
                        &[
                            (BriefingPanel::Map, map),
                            (BriefingPanel::Model, open_panel()),
                        ],
                        Some(model),
                        2000,
                        map_inputs(frame),
                        math,
                        &FrontendProcessState::default(),
                    )
                    .unwrap();
                assert_eq!(owner.scene.stats.begins, begins + 1, "H8/{slot}");
                assert_eq!(owner.scene.stats.depth_resets, resets + 1);
                assert_eq!(owner.scene.stats.flushes, flushes + 1);
                assert_eq!(owner.scene.queue.state(), reference.1, "H8/{slot}");
                assert_eq!(owner.scene.pixels.frame.rgba(), reference.0, "H8/{slot}");
                assert_eq!(owner.scene.process_rng(), reference.2);
                assert!(!owner.scene.stopped());
                assert_eq!(
                    owner.scene.scratch.state().unwrap(),
                    ModelScratchDrawState::Ready
                );

                // 0040B0F0 next begins/resets even when BB40 returns closed.
                let pixels = owner.scene.pixels.frame.rgba().to_vec();
                let mut terrain = pane_inputs(frame);
                terrain.animation.current_stage = 0;
                terrain.animation.requested_stage = 0;
                owner
                    .terrain_panel(
                        terrain,
                        BriefingTerrainEffects {
                            camera_disabled: false,
                            pulse_remaining_micros: 0,
                            report_omissions: false,
                        },
                        math,
                        &FrontendProcessState::default(),
                    )
                    .unwrap();
                assert_eq!(owner.scene.stats.begins, begins + 2);
                assert_eq!(owner.scene.stats.depth_resets, resets + 2);
                assert_eq!(owner.scene.stats.flushes, flushes + 1);
                assert_eq!(owner.scene.queue.state().command_count, 0);
                assert!(owner.scene.queue.has_only_implicit_root());
                assert_eq!(owner.scene.pixels.frame.rgba(), pixels);
            });
        }
    }
}
