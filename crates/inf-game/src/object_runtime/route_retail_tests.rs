//! Original recording snapshots immediately around 00404D40. The production
//! factory uses current canonical H13/H14 custody and terrain-adjusted inputs.
//! This admits only the route subset, before later loader/manager passes.

use std::{collections::BTreeMap, path::Path};

use serde::Deserialize;

use super::*;
use crate::{
    object_runtime::{
        construct_world_object, BaseObject, ObjectConstructionOutcome, ObjectConstructionRequest,
        ObjectInstanceCommands, VisualObject,
    },
    terrain_runtime::TerrainSpacing,
    world_placement::WorldPlacements,
    Installation, ResidencyRequest,
};

#[derive(Deserialize)]
struct Receipt {
    leaf: u8,
    spacing: u16,
    resource_slot: u16,
    entry_groups: usize,
    entry_pending: Vec<u16>,
    entry_group_is_null: bool,
    entry_distance_squared: u32,
    groups: Vec<Group>,
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Group {
    flags: u8,
    members: Vec<u16>,
}

#[derive(Deserialize)]
struct Node {
    placement_index: u32,
    word_28: u16,
    placement_word: u16,
    base_flags: u32,
    resource_word: u16,
    position_words: [u16; 3],
    dirty_word: u16,
    word_3c: u32,
    radius_word: u16,
    model_word: u16,
    identity_word: u16,
    predecessor_word: u16,
    entry_route_flags: u8,
    route_flags: u8,
    distance_squared: u32,
    group_index: usize,
}

impl Node {
    fn check_common(&self, arena: &ObjectArena, object: ObjectId) {
        let state = arena.object(object).unwrap();
        assert_eq!(
            state.base,
            BaseObject {
                word_28: self.word_28,
                placement_word: self.placement_word,
                flags: self.base_flags,
                resource_word: self.resource_word,
                position_words: Some(self.position_words),
                dirty_word: self.dirty_word,
                word_3c: self.word_3c,
            },
            "placement {} common state",
            self.placement_index
        );
        let ObjectClass::RouteNode { visual, route } = &state.class else {
            panic!("placement {} has the wrong class", self.placement_index)
        };
        assert_eq!(
            *visual,
            VisualObject {
                radius_word: self.radius_word,
                model_word: self.model_word,
            },
            "placement {} visual state",
            self.placement_index
        );
        assert_eq!(route.identity_word, self.identity_word);
        assert_eq!(route.predecessor_word, self.predecessor_word);
    }
}

#[test]
fn first_world_route_factory_and_finalizer_match_original_recording() {
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../tests/fixtures/object_route_retail.json"
    ))
    .unwrap();
    assert_eq!(receipt.leaf, 32);
    assert_eq!(receipt.nodes.len(), 40);
    assert_eq!(receipt.groups.len(), 9);
    assert!(receipt.entry_group_is_null);
    assert_eq!(receipt.spacing, 256);

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, receipt.leaf] {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
    }
    // The same-load terrain-map observation establishes this process policy;
    // it is not an assumed default setting or a value inferred from route Y.
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    assert_eq!(placements.world_source().overlay.key().level, receipt.leaf);
    assert_eq!(
        placements.terrain_source().overlay.key().level,
        receipt.leaf
    );
    let source = placements.world_source().overlay.bytes();
    let mut arena = ObjectArena::new();
    let mut objects = BTreeMap::new();
    for input in placements
        .inputs()
        .filter(|input| input.resource_argument as u16 == receipt.resource_slot)
    {
        let result = construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                resource_argument: input.resource_argument,
                context: BaseContext {
                    position_words: Some(input.position_words),
                    placement_index: input.placement_index,
                },
                suffix: Some(ObjectInstanceCommands {
                    placement: input.instance,
                    source,
                    main_payload: placements.assets().worlds.main_payload,
                    relocations: &placements.assets().worlds.relocations,
                }),
                command_allocation: None,
                network_active: false,
                actor: None,
            },
        )
        .unwrap();
        let ObjectConstructionOutcome::Constructed(object) = result else {
            panic!("placement {} rejected: {result:?}", input.placement_index)
        };
        assert!(objects.insert(input.placement_index, object).is_none());
    }
    assert_eq!(arena.len(), receipt.nodes.len());
    assert_eq!(arena.route_groups().len(), receipt.entry_groups);
    let pending: Vec<_> = arena
        .route_pending()
        .iter()
        .map(|&id| arena.object(id).unwrap().base.placement_word)
        .collect();
    assert_eq!(pending, receipt.entry_pending);
    for expected in &receipt.nodes {
        let object = objects[&expected.placement_index];
        expected.check_common(&arena, object);
        let route = arena.route_node(object).unwrap();
        assert_eq!(route.group, None);
        assert_eq!(route.flags, expected.entry_route_flags);
        assert_eq!(route.distance_squared, receipt.entry_distance_squared);
    }

    arena.finalize_routes().unwrap();
    assert_eq!(arena.len(), receipt.nodes.len());
    assert!(arena.route_pending().is_empty());
    assert_eq!(arena.route_groups().len(), receipt.groups.len());
    for (actual, expected) in arena.route_groups().iter().zip(&receipt.groups) {
        assert_eq!(actual.flags, Some(expected.flags));
        let members: Vec<_> = actual
            .members
            .iter()
            .map(|&id| arena.object(id).unwrap().base.placement_word)
            .collect();
        assert_eq!(members, expected.members);
    }
    for expected in &receipt.nodes {
        let object = objects[&expected.placement_index];
        expected.check_common(&arena, object);
        let route = arena.route_node(object).unwrap();
        assert_eq!(route.flags, expected.route_flags);
        assert_eq!(
            route.distance_squared, expected.distance_squared,
            "placement {} distance",
            expected.placement_index
        );
        assert_eq!(
            route.group,
            Some(arena.route_groups()[expected.group_index].id)
        );
    }
    // The capture stops before 00479840 disposal and later virtual+20 passes.
    // It does not establish actor movement, final world admission or drawing.
}
