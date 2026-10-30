use super::*;
use serde_json::{json, Value};

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}

fn projectile(input: &Value) -> (ProcessParticleRuntime, ParticleId) {
    let mut particles = ProcessParticleRuntime::default();
    let id = particles
        .allocate(ParticleAllocationRequest {
            payload_bytes: 60,
            last_reclaim_category: 1,
            network_active: false,
            phase: Some(4),
        })
        .unwrap();
    particles
        .construct(
            id,
            ParticleConstruction {
                category: 1,
                position: words(&input["position"]).into(),
                velocity: Some(words(&input["velocity"]).into()),
                source_handle: 12,
                flags: input["flags"].as_u64().unwrap() as u32,
            },
        )
        .unwrap();
    let record = particles.record_mut(id).unwrap();
    record.write_dword(0, 0x0052_6efc).unwrap();
    record
        .write_word(0x12, input["flags"].as_u64().unwrap() as u16)
        .unwrap();
    record
        .write_word(0x10, input["lifetime"].as_u64().unwrap() as u16)
        .unwrap();
    record
        .write_dword(0x28, input["animation"].as_u64().unwrap() as u32)
        .unwrap();
    record
        .write_dword(0x2c, input["scale"].as_u64().unwrap() as u32)
        .unwrap();
    // Native526EFC leaves+30 unwritten; the custom wrapper supplies shadow0.
    particles.transfer_pending();
    (particles, id)
}

#[test]
fn projectile_streak_native_prefixes_restore_only_after_all_ten_draws() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/particle_projectile_draw_native.json"
    ))
    .unwrap();
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let (mut particles, id) = projectile(input);
        let mut calls = Vec::new();
        let result: Result<(), ParticleDrawError<&str>> =
            particles.projectile_streak(id, executable, |particles, submission| {
                let record = particles.record(id)?;
                calls.push(
                    json!({"position": record.position()?, "animation": submission.table,
                "scale": submission.scale as u32, "shadow": submission.shadow_amount as u32,
                "flags": record.read_word(0x12)?}),
                );
                if input["stop_at"].as_u64().unwrap() == calls.len() as u64 {
                    return Err(ParticleDrawError::Host("supplied lower-draw stop"));
                }
                if input["mutation"]["after"].as_u64() == Some(calls.len() as u64) {
                    let mutation = &input["mutation"];
                    let record = particles.record_mut(id)?;
                    record.write_position(words(&mutation["position"]))?;
                    for (axis, velocity) in words(&mutation["velocity"]).into_iter().enumerate() {
                        record.write_word(0x0a + axis * 2, velocity)?;
                    }
                    record.write_word(0x12, mutation["flags"].as_u64().unwrap() as u16)?;
                    record.write_dword(0x28, mutation["animation"].as_u64().unwrap() as u32)?;
                    record.write_dword(0x2c, mutation["scale"].as_u64().unwrap() as u32)?;
                }
                Ok(())
            });
        assert_eq!(
            result.is_ok(),
            case["normal_return"].as_bool().unwrap(),
            "case{index}"
        );
        assert_eq!(json!(calls), case["calls"], "case{index}");
        let record = particles.record(id).unwrap();
        let velocity: [u16; 3] =
            std::array::from_fn(|axis| record.read_word(0x0a + axis * 2).unwrap());
        assert_eq!(
            json!({"position": record.position().unwrap(), "velocity": velocity,
            "lifetime": record.read_word(0x10).unwrap(), "flags": record.read_word(0x12).unwrap(),
            "animation": record.read_dword(0x28).unwrap(), "scale": record.read_dword(0x2c).unwrap()}),
            case["output"],
            "case{index}"
        );
    }
}

struct StopHost<'a> {
    host: Host<'a>,
    fail_at: usize,
    calls: usize,
}
impl ParticleDrawLighting for StopHost<'_> {
    type Error = &'static str;
    fn terrain_light(
        &mut self,
        request: TerrainLightRequest,
        emitter: [i32; 3],
    ) -> Result<(), Self::Error> {
        self.host.terrain_light(request, emitter)
    }
}
impl<'a> ParticleDrawHost<'a> for StopHost<'a> {
    fn sprite(&mut self, slot: i16) -> Result<Option<RenderResource<'a>>, Self::Error> {
        self.host.sprite(slot)
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.host.material_sorting()
    }
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        self.host.texture_sort_identity(texture)
    }
    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.calls += 1;
        if self.calls == self.fail_at {
            return Err("required packet submission");
        }
        self.host.submit(command)
    }
}

#[test]
fn projectile_real_draw_failure_keeps_shifted_xyz_and_stops_only_that_allocation() {
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/particle_draws_native.json"
    ))
    .unwrap();
    let case = receipt
        .cases
        .iter()
        .find(|case| {
            !case.pending && case.packets.len() == 1 && !case.shadow_prefix.unwrap_or(false)
        })
        .unwrap();
    let (runtime, assets) = setup();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let resource = RenderResources::new(&runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 1805,
        })
        .unwrap();
    let input = json!({"position": case.particles[0].position, "velocity": [100,0,0], "lifetime": 500,
        "flags": 8, "animation": 0x0053_d5f0u32, "scale": 128});
    let (mut particles, id) = projectile(&input);
    let mut host = StopHost {
        host: Host {
            resource,
            sorting: false,
            queries: 0,
            packets: Vec::new(),
            lights: Vec::new(),
            emitters: Vec::new(),
            fail_light: false,
            shadow_color: None,
        },
        fail_at: 3,
        calls: 0,
    };
    let report = particles.draw_particles(request(case), executable, &mut host);
    assert_eq!(report.commands, 2, "{:?}", report.omissions);
    assert!(matches!(
        report.omissions[0].error,
        ParticleDrawError::Host("required packet submission")
    ));
    let original = words(&input["position"]);
    let stopped_position = [original[0].wrapping_add(16), original[1], original[2]];
    assert_eq!(
        particles.record(id).unwrap().position().unwrap(),
        stopped_position
    );
    assert_eq!(
        particles.stopped_phase(id),
        Some(crate::process_particles::ParticleStopPhase::Draw)
    );
    let report = particles.draw_particles(request(case), executable, &mut host);
    assert!(report.omissions.is_empty());
    assert_eq!(report.stopped, 1);
    assert_eq!(host.calls, 3);
    assert_eq!(
        particles.record(id).unwrap().position().unwrap(),
        stopped_position
    );
    particles.destroy(id).unwrap();
    let reused = particles
        .allocate(ParticleAllocationRequest {
            payload_bytes: 60,
            last_reclaim_category: 1,
            network_active: false,
            phase: Some(4),
        })
        .unwrap();
    assert_eq!(reused, id);
    assert_eq!(particles.stopped_phase(reused), None);
}
