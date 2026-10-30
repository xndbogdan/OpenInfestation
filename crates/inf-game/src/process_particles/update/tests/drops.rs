use super::*;
use serde_json::{json, Value};

fn terrain(input: &Value) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    bytes[..4].copy_from_slice(&(input["plane"].as_u64().unwrap() as u32).to_le_bytes());
    for index in 0..TERRAIN_CELL_COUNT {
        let material = if input["vary_material"] == true {
            (index / 512) as u8 % 4
        } else {
            input["material"].as_u64().unwrap() as u8
        };
        let cell = (input["height"].as_i64().unwrap() as u8 as u16) | (u16::from(material) << 8);
        let offset = TERRAIN_PREFIX_BYTES + index * 2;
        bytes[offset..offset + 2].copy_from_slice(&cell.to_le_bytes());
    }
    let span = ByteSpan::new(0, bytes.len());
    let source = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap()
    .entries
    .remove(0);
    TerrainRuntime::new(
        &source,
        TerrainSampling {
            spacing: if input["spacing"] == 256 {
                TerrainSpacing::Units256
            } else {
                TerrainSpacing::Units128
            },
            height_scale: if input["height_shift"] == 4 {
                TerrainHeightScale::Units16
            } else {
                TerrainHeightScale::Units32
            },
        },
    )
}
fn fields(particles: &ProcessParticleRuntime, id: ParticleId, expected: &Value, case: usize) {
    let record = particles.record(id).unwrap();
    for (index, (offset, width)) in [
        (0, 4),
        (4, 2),
        (6, 2),
        (8, 2),
        (10, 2),
        (12, 2),
        (14, 2),
        (16, 2),
        (18, 2),
        (20, 4),
        (40, 4),
        (44, 4),
        (48, 4),
    ]
    .into_iter()
    .enumerate()
    {
        let actual = if width == 4 {
            record.read_dword(offset)
        } else {
            record.read_word(offset).map(u32::from)
        };
        match actual {
            Ok(actual) => assert_eq!(
                actual,
                expected[index].as_u64().unwrap() as u32,
                "case{case} id{id:?} offset{offset:X}"
            ),
            Err(_) => {
                assert!((10..=14).contains(&offset), "unexpected unknown field");
                assert_eq!(expected[index], 0xa5a5, "native untouched velocity");
            }
        }
    }
}
#[test]
fn pickup_drop_contacts_match_native_dust_water_births_rng_and_live_scratch() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/pickup_drop_contact_native.json"
    ))
    .unwrap();
    let bytes = executable_bytes();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let table = exe.mapped(0x53df00, 8192).unwrap().bytes;
    let sine = std::array::from_fn(|i| u16::from_le_bytes([table[i * 2], table[i * 2 + 1]]));
    for (case, row) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &row["input"];
        let output = &row["output"];
        let u = |key: &str| input[key].as_u64().unwrap() as u32;
        let words = |key: &str| std::array::from_fn(|i| input[key][i].as_u64().unwrap() as u16);
        let mut particles = ProcessParticleRuntime::default();
        let mut rng = u("rng");
        let mut process = crate::process_particles::factories::ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x12345678)),
        };
        let id = particles
            .birth_falling_drop(
                ParticleBirthRequest {
                    particle_type: 0,
                    position: words("position"),
                    velocity: Some(words("velocity")),
                    flags: u("flags"),
                    inputs: ParticleBirthInputs {
                        network_active: false,
                        phase: Some(u("phase")),
                        process: &process,
                    },
                },
                exe,
                &mut rng,
            )
            .unwrap()
            .unwrap();
        for (axis, word) in words("velocity").into_iter().enumerate() {
            particles
                .record_mut(id)
                .unwrap()
                .write_word(10 + axis * 2, word)
                .unwrap();
        }
        rng = u("rng");
        let mut terrain = terrain(input);
        let mut effects = Effects::default();
        let mut request = ParticleUpdateRequest {
            delta_microseconds: 0,
            load_factor: u("load_factor") as i32,
            birth_inputs: ParticleUpdateBirthInputs {
                network_active: false,
                phase: Some(u("phase")),
                process: &mut process,
            },
            terrain: Some(ParticleTerrainContext {
                terrain: &mut terrain,
                sine_quarter: &sine,
                wave_clock: u("wave_clock"),
                world_flags: 0,
                impact_selectors: std::array::from_fn(|i| {
                    input["impact_selectors"][i].as_u64().unwrap() as u32
                }),
            }),
        };
        assert!(particles
            .falling_drop_contact(
                id,
                super::super::collision::ParticleTerrainContact {
                    selector: u("selector"),
                    height: u("contact_height") as i32
                },
                &mut request,
                exe,
                &mut rng,
                &mut effects
            )
            .unwrap());
        let mut occupied = 0;
        for record in output["records"].as_array().unwrap() {
            let id = ParticleId::Pool(record["slot"].as_u64().unwrap() as u16);
            fields(&particles, id, &record["fields"], case);
            occupied += 1;
        }
        assert_eq!(particles.free_count(), 400 - occupied, "case{case}");
        if id == ParticleId::Scratch {
            fields(&particles, id, &output["scratch"], case);
        }
        for category in 0..6 {
            let slots: Vec<_> = particles
                .pending(category)
                .unwrap()
                .iter()
                .map(|id| match id {
                    ParticleId::Pool(slot) => *slot,
                    ParticleId::Scratch => panic!(),
                })
                .collect();
            assert_eq!(json!(slots), output["pending"][category], "case{case}");
        }
        assert_eq!(rng, output["rng"].as_u64().unwrap() as u32, "case{case}");
        let audio:Vec<_>=effects.impacts.iter().map(|a|json!({"sound":a.sound,"position":a.position,"volume":a.volume,"pitch":a.pitch,"flags":a.flags})).collect();
        assert_eq!(json!(audio), output["audio"], "case{case}");
    }
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 24);
}
