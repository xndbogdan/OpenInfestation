use std::sync::Arc;

use super::*;
use crate::object_runtime::{
    construct_world_object, BaseContext, InstanceCommandAllocation, ObjectArena,
    ObjectConstructionOutcome, ObjectConstructionRequest,
};

/// Isolate the actual selector-87 placement/command stream. Other objects are
/// omitted explicitly so the pixel differences belong to these annotations.
fn authored_annotations(
    resident: &ResidentRuntime,
    placements: &WorldPlacements<'_>,
) -> ObjectArena {
    let allocation = Arc::new(
        InstanceCommandAllocation::new(
            Arc::from(placements.world_source().overlay.bytes()),
            placements.assets().worlds.main_payload,
            placements.assets().worlds.relocations.clone().into(),
        )
        .unwrap(),
    );
    let mut arena = ObjectArena::new();
    for index in 0..placements.world().placements.len() {
        let input = placements.input(index).unwrap();
        let resource = resident
            .resource(13, input.resource_argument as u16)
            .unwrap();
        let region = &resource.overlay.layout().regions[resource.registration.region_index];
        let bank =
            inf_formats::parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
        if bank.prefixes.entries[resource.registration.local_index as usize].constructor_selector
            != 87
        {
            continue;
        }
        assert!(matches!(
            construct_world_object(
                &mut arena,
                resident,
                ObjectConstructionRequest {
                    resource_argument: input.resource_argument,
                    context: BaseContext {
                        position_words: Some(input.position_words),
                        placement_index: input.placement_index,
                    },
                    suffix: Some(allocation.commands(input.instance)),
                    command_allocation: Some(&allocation),
                    network_active: false,
                    actor: None,
                },
            )
            .unwrap(),
            ObjectConstructionOutcome::Constructed(_)
        ));
    }
    arena
}

#[test]
fn authored_annotation_labels_and_radii_contribute_clipped_pixels_at_both_presentation_tiers() {
    for (tier, dimensions) in [(2, [640, 480]), (4, [1024, 768])] {
        let (assets, tables, mut resident) = level1_fixture(tier);
        for (leaf, count, radius_count) in [(28, 5, 0), (26, 9, 5)] {
            resident
                .apply(ResidencyRequest::Activate { level: leaf })
                .unwrap();
            let resources = RenderResources::new(&resident);
            let startup = frontend_startup(
                &resident,
                crate::presentation::NativeFrame::original(dimensions),
            )
            .unwrap();
            let placements = WorldPlacements::new(&resident, TerrainSpacing::Units256).unwrap();
            let mut arena = authored_annotations(&resident, &placements);
            assert_eq!(arena.map_annotation_list().len(), count);
            let mut empty = ObjectArena::new();
            let map_state = crate::world_map::WorldMapState::new();
            let process = FrontendProcessState::default();
            process.publish_colors(MenuDrawColors {
                selected_start: resources.style(4).unwrap(),
                ordinary: resources.style(5).unwrap(),
                selected_end: resources.style(6).unwrap(),
            });
            // Full four-tile framing and age are supplied comparison inputs,
            // not a claim about the exact retail screen-camera timeline.
            let inputs = BriefingMapInputs {
                frame: BriefingSceneFrame {
                    projection: startup.projection,
                    age_millis: 2000,
                    start_remaining_millis: 0,
                    animation: AnimationClock::default(),
                    selector_one: 1,
                },
                animation: open_panel(),
                map_scale_word: 0,
                camera_xz_words: [0; 2],
            };
            let plan = briefing_map_backdrop(inputs, resources.style(0x3b).unwrap()).unwrap();
            let mut images = Vec::new();
            for world_arena in [&mut empty, &mut arena] {
                let mut pixels = SoftwareFrame::new(dimensions[0], dimensions[1], PIXEL_GATE_CLEAR);
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
                scene.set_process_rng(Some(0x12345678));
                let mut state = FrontendRendererState::default();
                let mut terrain = owned_terrain(&placements);
                let mut effects = BriefingTestEffects::new(&assets);
                let mut object_light_policy =
                    FixtureObjectLightPolicy::new(true, terrain.sampling().spacing);
                let mut owner = SoftwareBriefingFrame {
                    scene: &mut scene,
                    resources,
                    sine_quarter: &assets.sine_table().values,
                    template_projection: startup.projection,
                    state: &mut state,
                    selector_one: 1,
                    world: BriefingWorld {
                        residency: &resident,
                        terrain_source: &crate::world_actor_terrain::WorldTerrainSource::retain(placements.terrain_source()),
                        arena: &mut *world_arena,
                        map_state: &map_state,
                        terrain: &mut terrain,
                        presentation: &mut crate::world_backend::load_world_presentation(
                            &resident,
                            &placements,
                        )
                        .unwrap(),
                        world_flags: placements.world().flags,
                        network_active: false,
                    },
                    effects: effects.inputs(),
                    dynamic_lighting: true,
                    object_light_policy: &mut object_light_policy,
                    object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation,
object_draw_failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
                    infrared: false,
                };
                owner.begin(true).unwrap();
                owner.map_backdrop(inputs, &process).unwrap();
                assert_eq!(owner.scene.process_rng(), Some(0x12345678));
                assert_eq!(owner.scene.pixels.stats.unsupported, 0);
                assert!(!owner.scene.stopped());
                if !owner.world.arena.is_empty() {
                    assert_eq!(owner.scene.stats.direct_sprites, 4 + radius_count * 4);
                    assert!(owner.state.menu.glyph_passes >= (count - radius_count) * 4 * 2);
                } else {
                    assert_eq!(owner.state.menu.glyph_passes, 0);
                }
                images.push(owner.scene.pixels.frame.rgba().to_vec());
            }
            let changed: Vec<_> = images[0]
                .chunks_exact(4)
                .zip(images[1].chunks_exact(4))
                .enumerate()
                .filter(|(_, (before, after))| before != after)
                .map(|(index, _)| index)
                .collect();
            assert!(
                !changed.is_empty(),
                "leaf {leaf} tier {tier}: actual H4/0 labels and H3/7 radii must contribute pixels"
            );
            for index in changed {
                let x = (index % dimensions[0] as usize) as i16;
                let y = (index / dimensions[0] as usize) as i16;
                assert!(x >= plan.clip_rect[0] && x <= plan.clip_rect[2]);
                assert!(y >= plan.clip_rect[1] && y <= plan.clip_rect[3]);
            }
        }
    }
}
