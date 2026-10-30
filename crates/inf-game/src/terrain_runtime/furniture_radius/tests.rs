use super::*;
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};
use crate::{HandlerResources, Installation, ResidencyRequest};
use inf_formats::{ByteSpan, TerrainBucket, TerrainRecord};
use serde_json::Value;
use std::collections::BTreeMap;

fn word(value: &Value) -> u16 {
    value.as_u64().unwrap() as u16
}

fn fixture_terrain(spec: &Value) -> TerrainRuntime {
    let mut terrain = TerrainRuntime {
        sampling: TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
        //The producer never samples cells or applies sparse-marker/spacing gates.
        cells: Vec::new().into_boxed_slice(),
        buckets: (0..512)
            .map(|y| TerrainBucket {
                y,
                byte_size: 0,
                records_span: ByteSpan::new(0, 0),
                records: Vec::new(),
            })
            .collect(),
        plane_base: 0,
        slope_factors: [0; 2],
        light_direction_raw: [0; 3],
        ultra: None,
        ultra_footings: Vec::new(),
        puddle_carve: None,
    };
    for record in spec["records"].as_array().unwrap() {
        let bucket = &mut terrain.buckets[usize::from(word(&record["bucket"]))];
        bucket.records.push(TerrainRecord {
            x: word(&record["x"]),
            state: word(&record["state"]) as u8,
            resource_lookup: TerrainResourceLookup::from_raw(word(&record["lookup"]) as u8),
        });
        bucket.byte_size += 4;
    }
    terrain
}

struct Resources {
    h11_present: bool,
    capacity: u16,
    slots: BTreeMap<u8, u16>,
    models: BTreeMap<u16, Option<u16>>,
}

impl Resources {
    fn from_spec(spec: &Value) -> Self {
        Self {
            h11_present: spec["h11_present"].as_bool().unwrap_or(true),
            capacity: spec["capacity"].as_u64().unwrap_or(16) as u16,
            slots: spec["slots"]
                .as_array()
                .unwrap()
                .iter()
                .map(|slot| (word(&slot["lookup"]) as u8, word(&slot["slot"])))
                .collect(),
            models: spec["models"]
                .as_array()
                .unwrap()
                .iter()
                .map(|model| {
                    (
                        word(&model["slot"]),
                        model["present"]
                            .as_bool()
                            .unwrap_or(true)
                            .then(|| word(&model["radius"])),
                    )
                })
                .collect(),
        }
    }

    fn mutate(&mut self, request: &mut TerrainFurnitureRadiusRequest<'_>, mutation: &Value) {
        if let Some(output) = mutation["word"].as_u64() {
            request.write_process_radius_word(output as u16);
        }
        if let Some(payloads) = mutation["payloads"].as_array() {
            for payload in payloads {
                request
                    .write_sparse_payload(
                        TerrainSparseRecordId {
                            bucket: word(&payload["bucket"]),
                            index: word(&payload["index"]) as usize,
                        },
                        TerrainSparsePayload {
                            state: word(&payload["state"]) as u8,
                            resource_lookup: TerrainResourceLookup::from_raw(word(
                                &payload["lookup"],
                            )
                                as u8),
                        },
                    )
                    .unwrap();
            }
        }
        if let Some(present) = mutation["h11_present"].as_bool() {
            self.h11_present = present;
        }
        if let Some(capacity) = mutation["capacity"].as_u64() {
            self.capacity = capacity as u16;
        }
        if let Some(slots) = mutation["slots"].as_array() {
            for slot in slots {
                self.slots
                    .insert(word(&slot["lookup"]) as u8, word(&slot["slot"]));
            }
        }
        if let Some(models) = mutation["models"].as_array() {
            for model in models {
                self.models.insert(
                    word(&model["slot"]),
                    model["present"]
                        .as_bool()
                        .unwrap_or(true)
                        .then(|| word(&model["radius"])),
                );
            }
        }
    }

    fn resolve(
        &self,
        lookup: TerrainResourceLookup,
        acquisitions: &mut Vec<&'static str>,
    ) -> Result<Option<u16>, &'static str> {
        if !self.h11_present {
            acquisitions.push("h11");
            return Err("missing_h11");
        }
        let slot = self.slots.get(&lookup.raw()).copied().unwrap_or(u16::MAX);
        if (slot as i16) < 0 || slot > self.capacity {
            return Ok(None);
        }
        if slot == self.capacity {
            return Err("h8_boundary");
        }
        let radius = self.models.get(&slot).copied().flatten();
        if radius.is_none() {
            acquisitions.push("h8");
        }
        Ok(radius)
    }
}

#[test]
fn native_ordered_radius_scan_preserves_signed_words_live_services_and_failure_prefixes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/terrain_furniture_radius_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 73);
    for spec in cases {
        let name = spec["name"].as_str().unwrap();
        let mut terrain = fixture_terrain(spec);
        let mut output = spec["initial_word"].as_u64().unwrap_or(0xa55a) as u16;
        let mut resources = Resources::from_spec(spec);
        let visits = spec["visits"].as_array().unwrap();
        let mut service = 0;
        let mut acquisitions = Vec::new();
        let result = produce_furniture_radius(
            TerrainFurnitureRadiusRequest::new(&mut terrain, &mut output),
            |request, lookup| {
                let visit = &visits[service];
                assert_eq!(
                    lookup.raw(),
                    word(&visit["lookup"]) as u8,
                    "{name} lookup {service}"
                );
                assert_eq!(
                    request.process_radius_word(),
                    word(&visit["word_before"]),
                    "{name} prefix {service}"
                );
                let mutation = &spec["mutations"][service.to_string()];
                resources.mutate(request, mutation);
                assert_eq!(
                    request.process_radius_word(),
                    word(&visit["word_after_service"]),
                    "{name} service {service}"
                );
                service += 1;
                if mutation["fail"].as_bool().unwrap_or(false) {
                    return Err("resolver");
                }
                let radius = resources.resolve(lookup, &mut acquisitions)?;
                assert_eq!(
                    radius,
                    visit["radius"].as_u64().map(|radius| radius as u16),
                    "{name} radius {service}"
                );
                Ok(radius)
            },
        );
        let error = match result {
            Ok(()) => None,
            Err(TerrainFurnitureRadiusError::ZeroLookup { record }) => {
                let visit = visits.last().unwrap();
                assert_eq!(record.bucket, word(&visit["bucket"]), "{name}");
                assert_eq!(record.index, word(&visit["index"]) as usize, "{name}");
                Some("zero_lookup")
            }
            Err(TerrainFurnitureRadiusError::Resolver { record, error }) => {
                let visit = visits.last().unwrap();
                assert_eq!(record.bucket, word(&visit["bucket"]), "{name}");
                assert_eq!(record.index, word(&visit["index"]) as usize, "{name}");
                Some(error)
            }
            Err(error) => panic!("{name}: {error:?}"),
        };
        assert_eq!(error, spec["error"].as_str(), "{name} error");
        assert_eq!(
            service,
            visits
                .iter()
                .filter(|visit| word(&visit["lookup"]) != 0)
                .count(),
            "{name} services"
        );
        assert_eq!(output, word(&spec["final_word"]), "{name} output");
        assert_eq!(
            acquisitions,
            spec["acquisitions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item.as_str().unwrap())
                .collect::<Vec<_>>(),
            "{name} acquisitions"
        );
        for payload in spec["final_payloads"].as_array().unwrap() {
            let record = terrain.buckets()[usize::from(word(&payload["bucket"]))].records
                [usize::from(word(&payload["index"]))];
            assert_eq!(record.state, word(&payload["state"]) as u8, "{name}");
            assert_eq!(
                record.resource_lookup.raw(),
                word(&payload["lookup"]) as u8,
                "{name}"
            );
        }
    }
}

#[test]
fn reached_missing_storage_still_resets_output_before_error() {
    let mut terrain = fixture_terrain(&serde_json::json!({"records": []}));
    terrain.buckets.clear();
    let mut output = 600;
    let result = produce_furniture_radius::<()>(
        TerrainFurnitureRadiusRequest::new(&mut terrain, &mut output),
        |_, _| panic!("no record reached"),
    );
    assert_eq!(
        result,
        Err(TerrainFurnitureRadiusError::MissingBucket { bucket: 0 })
    );
    assert_eq!(output, 0);
}

#[test]
fn resident_adapter_reads_current_authored_h11_and_h8_without_radius_defaults() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    let mut allocations = Vec::new();
    for level in [32, 11] {
        resident
            .apply(ResidencyRequest::Activate { level: 48 })
            .unwrap();
        resident
            .apply(ResidencyRequest::Activate { level })
            .unwrap();
        let source = resident.resource(11, 0).unwrap();
        allocations.push(source.overlay.path().to_path_buf());
        let table =
            crate::world_furniture_effect::resident::resident_placement_table(&resident).unwrap();
        let (lookup, expected) = table
            .slots
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(lookup, slot)| {
                let resource = resident.resource(8, slot.resource_word)?;
                let HandlerResources::Models(bank) =
                    &resource.overlay.handlers()[resource.registration.region_index]
                else {
                    panic!("H8 model bank");
                };
                let radius = bank.entries[usize::from(resource.registration.local_index)]
                    .header
                    .raw_0e;
                (radius != 0).then_some((lookup, radius))
            })
            .expect("authored H11 has a resident model with nonzero radius");
        let mut terrain = fixture_terrain(
            &serde_json::json!({"records": [{"bucket": 511, "x": 3, "state": 0xff, "lookup": lookup}]}),
        );
        let mut output = 0xa55a;
        produce_resident_furniture_radius(
            TerrainFurnitureRadiusRequest::new(&mut terrain, &mut output),
            &resident,
        )
        .unwrap();
        assert_eq!(
            output,
            if (expected as i16) > 512 {
                512
            } else {
                expected
            }
        );
    }
    assert_ne!(
        allocations[0], allocations[1],
        "adapter exercised different current H11 allocations"
    );
    let fresh = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    let mut terrain = fixture_terrain(
        &serde_json::json!({"records": [{"bucket": 0, "x": 0, "state": 0, "lookup": 1}]}),
    );
    let mut output = 600;
    assert!(matches!(
        produce_resident_furniture_radius(
            TerrainFurnitureRadiusRequest::new(&mut terrain, &mut output),
            &fresh
        ),
        Err(TerrainFurnitureRadiusError::Resolver { .. })
    ));
    assert_eq!(output, 0, "missing H11 preserves the entry reset");
}
