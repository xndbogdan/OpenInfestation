//! The light sink is supplied; birth, frame selection and light arguments are
//! authenticated by the original instruction bodies in the receipt.
use super::*;
use crate::object_runtime::ActorHandle;
use crate::process_particles::factories::ParticleFactoryProcessState;
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/terrain_flash_native.json"
    ))
    .unwrap()
}
fn number(value: &Value) -> u32 {
    value.as_i64().unwrap() as u32
}
fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|i| number(&value[i]) as u16)
}
fn executable_bytes() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .unwrap()
}
fn process() -> ParticleFactoryProcessState {
    ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(0x1234_5678)),
    }
}

#[test]
fn terrain_flash_births_match_native_pool_records_rng_and_unsupported_selector() {
    let fixture = fixture();
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let process = process();
    for row in fixture["births"].as_array().unwrap() {
        let input = &row["input"];
        let output = &row["output"];
        let mut particles = ProcessParticleRuntime::default();
        particles.vector_cursor = number(&input["cursor"]);
        let mut rng = number(&input["rng"]);
        let result = particles
            .birth_terrain_flash(
                ParticleBirthRequest {
                    particle_type: number(&input["selector"]),
                    position: words(&input["position"]),
                    // Native 48AD70 always supplies NULL velocity, even if a
                    // caller has carried available velocity in this request.
                    velocity: Some([1, 2, 3]),
                    flags: number(&input["flags"]),
                    inputs: ParticleBirthInputs {
                        network_active: false,
                        phase: Some(number(&input["phase"])),
                        process: &process,
                    },
                },
                executable,
                &mut rng,
            )
            .unwrap();
        assert_eq!(
            result.is_some(),
            output["returned"].as_bool().unwrap(),
            "{input}"
        );
        assert_eq!(
            result == Some(ParticleId::Scratch),
            output["scratch"].as_bool().unwrap(),
            "{input}"
        );
        assert_eq!(rng, number(&output["rng"]), "{input}");
        assert_eq!(
            particles.vector_cursor,
            number(&output["cursor"]),
            "{input}"
        );
        assert_eq!(
            output["allocations"],
            if result.is_some() {
                json!([[52, 1]])
            } else {
                json!([])
            }
        );
        if let Some(id) = result {
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
                if (10..=14).contains(&offset) && id != ParticleId::Scratch {
                    assert!(
                        record.read_word(offset).is_err(),
                        "NULL velocity must retain unknown heap bytes"
                    );
                    continue;
                }
                let actual = if width == 4 {
                    record.read_dword(offset).unwrap()
                } else {
                    u32::from(record.read_word(offset).unwrap())
                };
                assert_eq!(
                    actual,
                    number(&output["fields"][index]),
                    "offset {offset:X}: {input}"
                );
            }
        }
        for category in 0..6 {
            let slots: Vec<_> = particles
                .pending(category)
                .unwrap()
                .iter()
                .map(|id| match id {
                    ParticleId::Pool(slot) => *slot,
                    ParticleId::Scratch => panic!("scratch is never published"),
                })
                .collect();
            assert_eq!(json!(slots), output["pending"][category], "{input}");
        }
    }
    assert_eq!(fixture["births"].as_array().unwrap().len(), 16);
}

#[derive(Default)]
struct LightHost {
    lights: Vec<Value>,
    fail: bool,
}
impl ParticleDrawLighting for LightHost {
    type Error = &'static str;
    fn terrain_light(
        &mut self,
        request: TerrainLightRequest,
        _: [i32; 3],
    ) -> Result<(), Self::Error> {
        self.lights
            .push(json!({"world_xz":request.world_xz,"radius":request.radius,"rgb":request.rgb}));
        if self.fail {
            Err("supplied light sink stop")
        } else {
            Ok(())
        }
    }
}
impl<'r> ParticleDrawHost<'r> for LightHost {
    fn sprite(&mut self, _: i16) -> Result<Option<RenderResource<'r>>, Self::Error> {
        panic!("light-only draw requested a sprite")
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        panic!("light-only draw requested sorting")
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        panic!("light-only draw requested a texture")
    }
    fn submit(&mut self, _: PrimitiveCommand<'r>) -> Result<(), Self::Error> {
        panic!("light-only draw submitted geometry")
    }
}
fn light_request(terrain: Option<&TerrainRuntime>) -> ParticleDrawRequest<'_> {
    ParticleDrawRequest {
        bank: ParticleDrawBank::Pending,
        context: ParticleDrawContext {
            position: [123, 456, 789],
            basis: MenuBasis::INITIAL,
            view: SceneView {
                eye: [999, -999, 21],
                basis: MenuBasis::INITIAL,
            },
            projection: ModelProjection {
                focal: [128, 128],
                viewport: [1024, 768],
                center: [512, 384],
                fade: [1000, 2000],
            },
            coordinate_shift: 0,
            fog_factor: 0,
            projection_dispatch: ModelDispatch::Far,
            far_color: 0,
        },
        terrain,
    }
}

#[test]
fn terrain_flash_light_only_draw_matches_native_rgb_trig_division_and_floor_policy() {
    let fixture = fixture();
    let source = executable_bytes();
    let original = FrontendExecutable::parse(&source).unwrap();
    let process = process();
    for row in fixture["draws"].as_array().unwrap() {
        let input = &row["input"];
        let output = &row["output"];
        let mut particles = ProcessParticleRuntime::default();
        let mut rng = 0x7654_3210;
        let id = particles
            .birth_terrain_flash(
                ParticleBirthRequest {
                    particle_type: number(&input["selector"]),
                    position: words(&input["position"]),
                    velocity: None,
                    flags: 0,
                    inputs: ParticleBirthInputs {
                        network_active: false,
                        phase: Some(4),
                        process: &process,
                    },
                },
                original,
                &mut rng,
            )
            .unwrap()
            .unwrap();
        let record = particles.record_mut(id).unwrap();
        let table = record.read_dword(0x28).unwrap();
        record
            .write_word(0x10, number(&input["lifetime"]) as u16)
            .unwrap();
        record
            .write_word(0x12, number(&input["flags"]) as u16)
            .unwrap();
        record
            .write_word(0x24, number(&input["floor"]) as u16)
            .unwrap();
        let mut raw = source.clone();
        if !input["total"].is_null() {
            let offset = original.mapped(table + 12, 2).unwrap().file_span.start;
            raw[offset..offset + 2]
                .copy_from_slice(&(number(&input["total"]) as u16).to_le_bytes());
        }
        if !input["rgb"].is_null() {
            let frame = original.word(table).unwrap();
            let offset = original.mapped(frame + 4, 3).unwrap().file_span.start;
            for i in 0..3 {
                raw[offset + i] = number(&input["rgb"][i]) as u8;
            }
        }
        let executable = FrontendExecutable::parse(&raw).unwrap();
        let terrain = input["terrain_height"]
            .as_i64()
            .map(|height| flat_terrain_height(height as i8));
        let request = light_request(terrain.as_ref());
        let mut host = LightHost {
            fail: input["stop_light"].as_bool().unwrap_or(false),
            ..Default::default()
        };
        let report = particles.draw_particles(request, executable, &mut host);
        assert_eq!(report.visited, 1, "{input}");
        assert_eq!(report.commands, 0, "{input}");
        assert_eq!(json!(host.lights), output["lights"], "{input}");
        assert_eq!(
            report.omissions.is_empty(),
            output["normal_return"].as_bool().unwrap(),
            "{input}: {:?}",
            report.omissions
        );
        match output["fault"].as_str() {
            Some("divide") => assert!(matches!(
                report.omissions[0].error,
                ParticleDrawError::LightDivision { .. }
            )),
            Some("light") => assert!(matches!(
                report.omissions[0].error,
                ParticleDrawError::Host("supplied light sink stop")
            )),
            None => {}
            other => panic!("unexpected native fault {other:?}"),
        }
        let record = particles.record(id).unwrap();
        assert_eq!(
            u32::from(record.read_word(0x12).unwrap()),
            number(&output["flags"]),
            "{input}"
        );
        assert_eq!(
            u32::from(record.read_word(0x24).unwrap()),
            number(&output["floor"]),
            "{input}"
        );
        assert_eq!(rng, number(&output["rng"]), "{input}");
        if !report.omissions.is_empty() {
            let before = host.lights.clone();
            let again = particles.draw_particles(request, executable, &mut host);
            assert_eq!(again.stopped, 1);
            assert_eq!(
                host.lights, before,
                "stopped feature must not replay its light prefix"
            );
        }
    }
    assert_eq!(fixture["draws"].as_array().unwrap().len(), 36);
}

#[test]
fn terrain_flash_complete_native_bodies_and_light_only_vtable_are_guarded() {
    use sha2::{Digest, Sha256};
    let fixture = fixture();
    let raw = executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        fixture["retail_sha256"].as_str().unwrap()
    );
    assert_eq!(number(&fixture["fp_control_word"]), 0x027f);
    for guard in fixture["source_guards"].as_array().unwrap() {
        let start = number(&guard["address"]) as usize - 0x400000;
        let length = number(&guard["length"]) as usize;
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw[start..start + length])),
            guard["sha256"].as_str().unwrap()
        );
    }
    let executable = FrontendExecutable::parse(&raw).unwrap();
    assert_eq!(executable.word(0x0052_8974 + 8).unwrap(), 0x0048_ae60);
}
