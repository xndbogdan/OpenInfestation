use super::*;
use std::path::PathBuf;

#[test]
fn exports_every_constructor_prefix_and_world_placement_without_losing_raw_bytes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let out = root.join("../.tmp").join(format!(
        "inf-world-asset-export-test-{}",
        std::process::id()
    ));
    let report = export(&root, &out).unwrap();
    assert_eq!(
        report,
        WorldExportReport {
            constructor_banks: 5,
            constructor_resources: 162,
            initial_model_bindings: 155,
            shared_actor_prefixes: 123,
            damage_responses: 1230,
            derived_actor_prefixes: 52,
            constructor_sound_bindings: 208,
            flags_only_prefixes: 23,
            selector86_prefixes: 1,
            selector83_prefixes: 3,
            selector45_prefixes: 3,
            selector46_prefixes: 2,
            flags_words_prefixes: 2,
            goal_prefixes: 2,
            terrain_light_prefixes: 2,
            point_table_prefixes: 1,
            opaque_class_payloads: 0,
            world_banks: 33,
            world_resources: 33,
            placements: 4486,
            instance_commands: 11476,
        }
    );
    let target = out.join("world_assets");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(target.join("manifest.json")).unwrap()).unwrap();
    let sources = manifest["sources"].as_array().unwrap();
    assert_eq!(manifest["schema_version"], 10);
    assert_eq!(sources.len(), 38);
    let mut model_bindings = 0;
    let mut damage_responses = 0;
    let mut sound_bindings = 0;
    let mut points = 0;
    let mut commands = 0;
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    for source in sources {
        let ovl = source["ovl"].as_str().unwrap();
        let bytes = fs::read(root.join("Overlay").join(ovl)).unwrap();
        let bank: serde_json::Value = serde_json::from_slice(
            &fs::read(target.join(source["decoded"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        let key = OverlayKey::parse_filename(ovl).unwrap();
        let requests = preload
            .leaf(key.level)
            .unwrap()
            .requests(key.request_kind());
        let parsed = inf_formats::parse_overlay(&bytes, requests).unwrap();
        let handler = source["handler"].as_u64().unwrap() as usize;
        let region = parsed
            .regions
            .iter()
            .find(|r| usize::from(r.handler.get()) == handler)
            .unwrap();
        let raw = fs::read(target.join(source["raw_framed_region"].as_str().unwrap())).unwrap();
        assert_eq!(raw.as_slice(), region.span.get(&bytes).unwrap());
        assert_eq!(
            source["framed_region"],
            serde_json::to_value(region.span).unwrap()
        );
        assert_eq!(
            source["request"],
            serde_json::to_value(&requests.0[handler]).unwrap()
        );
        let expected_slots: Vec<_> = (0..region.requested_count)
            .map(|local| requests.0[handler].global_index(local as u16).unwrap())
            .collect();
        assert_eq!(
            source["global_slots_in_local_order"],
            serde_json::to_value(expected_slots).unwrap()
        );
        assert_eq!(
            (if handler == 13 {
                &bank["data"]["prefixes"]["entries"]
            } else {
                &bank["data"]["worlds"]["entries"]
            })
            .as_array()
            .unwrap()
            .len(),
            source["global_slots_in_local_order"]
                .as_array()
                .unwrap()
                .len()
        );
        if handler == 13 {
            for asset in bank["data"]["assets"].as_array().unwrap() {
                if asset["payload"]["kind"] == "point_table16" {
                    let data = &asset["payload"]["data"];
                    assert_eq!(data["word_04"]["value"], 3);
                    for point in data["points"].as_array().unwrap() {
                        let start = point["span"]["start"].as_u64().unwrap() as usize;
                        let end = point["span"]["end"].as_u64().unwrap() as usize;
                        assert_eq!(end - start, 4);
                        assert_eq!(
                            point["value"],
                            serde_json::json!([
                                u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()),
                                u16::from_le_bytes(bytes[start + 2..end].try_into().unwrap()),
                            ])
                        );
                        points += 1;
                    }
                }
                if asset["payload"]["kind"] == "shared_actor" {
                    if let Some(bindings) =
                        asset["payload"]["data"]["derived"]["sound_bindings"].as_array()
                    {
                        assert_eq!(bindings.len(), 4);
                        for binding in bindings {
                            let raw = &binding["raw"];
                            let start = raw["span"]["start"].as_u64().unwrap() as usize;
                            let end = raw["span"]["end"].as_u64().unwrap() as usize;
                            let value = u16::from_le_bytes(bytes[start..end].try_into().unwrap());
                            assert_eq!(raw["value"], value);
                            assert_eq!(binding["signed_slot"], i64::from(value as i16));
                            assert_eq!(
                                binding["lookup_slot"],
                                serde_json::to_value(((value as i16) > 0).then_some(value))
                                    .unwrap()
                            );
                            sound_bindings += 1;
                        }
                    }
                    let responses = asset["payload"]["data"]["damage_responses"]
                        .as_array()
                        .unwrap();
                    assert_eq!(responses.len(), 10);
                    for (channel, response) in responses.iter().enumerate() {
                        assert_eq!(response["channel"], channel);
                        for field in ["threshold", "multiplier_q8"] {
                            let start = response[field]["span"]["start"].as_u64().unwrap() as usize;
                            let end = response[field]["span"]["end"].as_u64().unwrap() as usize;
                            assert_eq!(end - start, 2);
                            assert_eq!(
                                response[field]["value"],
                                u16::from_le_bytes(bytes[start..end].try_into().unwrap())
                            );
                        }
                        damage_responses += 1;
                    }
                }
                let binding = &asset["model_binding"];
                if binding.is_null() {
                    continue;
                }
                let start = binding["source_span"]["start"].as_u64().unwrap() as usize;
                let end = binding["source_span"]["end"].as_u64().unwrap() as usize;
                assert_eq!(end - start, 2);
                let word = u16::from_le_bytes(bytes[start..end].try_into().unwrap());
                assert_eq!(binding["raw_word"], word);
                assert_eq!(binding["signed_slot"], i64::from(word as i16));
                assert_eq!(
                    raw[start - region.span.start..end - region.span.start],
                    bytes[start..end]
                );
                model_bindings += 1;
            }
        } else {
            for world in bank["data"]["instances"].as_array().unwrap() {
                for placement in world["placements"].as_array().unwrap() {
                    let list = placement["commands"].as_array().unwrap();
                    assert_eq!(
                        list.len() as i64,
                        placement["count"]["value"].as_i64().unwrap().max(0)
                    );
                    for (index, command) in list.iter().enumerate() {
                        assert_eq!(command["index"], index);
                        let pointer = command["pointer_span"]["start"].as_u64().unwrap() as usize;
                        assert_eq!(
                            command["allocation_offset"],
                            u32::from_le_bytes(bytes[pointer..pointer + 4].try_into().unwrap())
                        );
                        let code = command["code"]["span"]["start"].as_u64().unwrap() as usize;
                        assert_eq!(
                            command["code"]["value"],
                            i32::from_le_bytes(bytes[code..code + 4].try_into().unwrap())
                        );
                        assert_eq!(
                            raw[code - region.span.start..code - region.span.start + 4],
                            bytes[code..code + 4]
                        );
                        commands += 1;
                    }
                }
            }
        }
    }
    assert_eq!(model_bindings, 155);
    assert_eq!(sound_bindings, 208);
    assert_eq!(points, 16);
    assert_eq!((damage_responses, commands), (1230, 11476));
    // Publishing the same decoded family preserves deterministic metadata.
    let first = fs::read(target.join("manifest.json")).unwrap();
    export(&root, &out).unwrap();
    assert_eq!(fs::read(target.join("manifest.json")).unwrap(), first);
}
