use std::{collections::BTreeMap, fs, path::Path};

use inf_formats::{
    decode_instance_command, parse_constructor_asset_bank, parse_overlay,
    parse_world_instance_bank, ConstructorPayloadView, InstanceCommandPayload,
    InstanceCommandRequest, InstanceCommandTarget, Preload,
};

#[test]
fn installed_route_node_commands_preserve_authored_inputs_without_building_routes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let mut slots = BTreeMap::<i16, Vec<u16>>::new();
    for level in [1, 6, 29, 30, 32] {
        let bytes = fs::read(root.join("Overlay").join(format!("{level}.ovl"))).unwrap();
        let leaf = preload.leaf(level).unwrap();
        let layout = parse_overlay(&bytes, &leaf.base).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 13)
            .unwrap();
        let bank = parse_constructor_asset_bank(&bytes, region).unwrap();
        for (prefix, asset) in bank.prefixes.entries.iter().zip(&bank.assets) {
            let slot = leaf
                .base
                .get(13)
                .unwrap()
                .global_index(prefix.local_index as u16)
                .unwrap() as i16;
            slots
                .entry(slot)
                .or_default()
                .push(prefix.constructor_selector);
            if prefix.constructor_selector == 51 {
                assert!(matches!(asset.payload, ConstructorPayloadView::Flags(_)));
                assert_eq!(asset.dispatch.unwrap().context_initializer, 0x0040_4c30);
                assert_eq!(asset.model_binding.unwrap().raw_word, prefix.argument_word);
            }
        }
    }
    let node_slots: Vec<_> = slots
        .iter()
        .filter_map(|(&slot, selectors)| {
            if !selectors.contains(&51) {
                return None;
            }
            // Route-node custody is scene independent only when no other bank
            // registers this slot; unrelated scene-local overlaps are allowed.
            assert_eq!(selectors, &[51], "overlapping route-node slot {slot}");
            Some(slot)
        })
        .collect();
    let mut placements = 0;
    let mut first_world_placements = 0;
    let mut codes = BTreeMap::new();
    let mut mode_values = BTreeMap::new();
    for level in 11..=43 {
        let bytes = fs::read(root.join("Overlay").join(format!("{level}.ovl"))).unwrap();
        let layout = parse_overlay(&bytes, &preload.leaf(level).unwrap().base).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 14)
            .unwrap();
        let bank = parse_world_instance_bank(&bytes, region).unwrap();
        for (world, instances) in bank.worlds.entries.iter().zip(&bank.instances) {
            for (placement, suffix) in world.placements.iter().zip(&instances.placements) {
                if !node_slots.contains(&placement.constructor_slot()) {
                    continue;
                }
                placements += 1;
                first_world_placements += usize::from(level == 32);
                for command in &suffix.commands {
                    let view = decode_instance_command(InstanceCommandRequest {
                        read_scope: inf_formats::InstanceCommandReadScope::Inspection,
                        source: &bytes,
                        main_payload: bank.worlds.main_payload,
                        relocations: &bank.worlds.relocations,
                        command_offset: command.allocation_offset,
                        target: InstanceCommandTarget::Constructor(51),
                    })
                    .unwrap();
                    assert_eq!(view.proven_method, Some(0x0040_4cc0));
                    *codes.entry(view.code.value).or_insert(0) += 1;
                    if (3..=8).contains(&view.code.value) {
                        assert_eq!(view.payload_consumer, Some(0x0040_4cc0));
                    }
                    match view.payload {
                        InstanceCommandPayload::RouteNodePlacementReference(reference) => {
                            assert_eq!(view.code.value, 3);
                            assert_eq!(reference.span.len(), 2);
                            assert_eq!(
                                reference.value,
                                u16::from_le_bytes(
                                    reference.span.get(&bytes).unwrap().try_into().unwrap()
                                )
                            );
                        }
                        InstanceCommandPayload::RouteNodeMode(mode) => {
                            assert_eq!(view.code.value, 4);
                            assert_eq!(mode.span.len(), 4);
                            assert_eq!(
                                mode.value,
                                u32::from_le_bytes(
                                    mode.span.get(&bytes).unwrap().try_into().unwrap()
                                )
                            );
                            *mode_values.entry(mode.value).or_insert(0) += 1;
                        }
                        InstanceCommandPayload::RouteNodeFlagOr { mask } => {
                            assert_eq!(
                                (view.code.value, mask),
                                if view.code.value == 5 {
                                    (5, 2)
                                } else {
                                    (8, 0x10)
                                }
                            );
                        }
                        InstanceCommandPayload::NoPayloadRead => {
                            assert!(matches!(view.code.value, 0 | 1 | 6 | 7))
                        }
                        InstanceCommandPayload::ModelSlot(model) => {
                            assert_eq!(view.code.value, 2);
                            assert_eq!(
                                model.raw.value,
                                u16::from_le_bytes(
                                    model.raw.span.get(&bytes).unwrap().try_into().unwrap()
                                )
                            );
                        }
                        other => panic!("unexpected route-node command {other:?}"),
                    }
                }
            }
        }
    }
    assert_eq!(first_world_placements, 40);
    assert_eq!(node_slots, [30]);
    assert_eq!(placements, 369);
    assert_eq!(
        codes,
        BTreeMap::from([(3, 369), (4, 282), (5, 58), (6, 11), (7, 8), (8, 36)])
    );
    assert_eq!(mode_values, BTreeMap::from([(0, 268), (1, 14)]));
}
