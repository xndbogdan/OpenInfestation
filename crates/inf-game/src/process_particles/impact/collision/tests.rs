use super::*;
use crate::process_particles::{
    factories::ParticleFactoryProcessState,
    update::{ParticleTerrainContext, ParticleUpdateBirthInputs},
};
use serde_json::{json, Value};

struct Host {
    input: Value,
    events: Vec<Value>,
    queries: usize,
    radius: i32,
}
impl Host {
    fn verdict(&mut self, stop: &str) -> Result<bool, &'static str> {
        self.queries += 1;
        if self.input[stop].as_u64() == Some(self.queries as u64) {
            return Err("query stop");
        }
        let answers = self.input["answers"].as_array().unwrap();
        Ok(answers[(self.queries - 1).min(answers.len() - 1)]
            .as_u64()
            .unwrap()
            != 0)
    }
}
impl ParticleUpdateHost for Host {
    type Error = &'static str;
    fn impact_sound(
        &mut self,
        request: ParticleSoundRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events
            .push(json!({"kind":"sound","sound":request.sound}));
        Ok(())
    }
}
impl ParticleProjectileHost for Host {
    fn actor_geometry(
        &mut self,
        request: ParticleActorGeometryRequest,
        _: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<bool, Self::Error> {
        self.events.push(json!({"kind":"geometry","candidate":request.actor.raw(),"radius":request.radius as u32,"position":request.center}));
        self.verdict("stop_geometry")
    }
    fn furniture_query(
        &mut self,
        center: [u16; 3],
        radius: i32,
        wave_clock: u32,
        _: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<Option<ParticleFurnitureContact>, Self::Error> {
        self.events.push(
            json!({"kind":"query","position":center,"radius":radius as u32,"clock":wave_clock}),
        );
        Ok(self
            .verdict("stop_query")?
            .then_some(ParticleFurnitureContact { xz: [100, 300] }))
    }
    fn actor_impact(
        &mut self,
        _: ParticleId,
        target: ActorHandle,
        _: u32,
        _: u32,
        _: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error> {
        self.events
            .push(json!({"kind":"hit","candidate":target.raw(),"radius":self.radius as u32}));
        if self.input["stop_hit"] == true {
            Err("hit stop")
        } else {
            Ok(())
        }
    }
    fn furniture_impact(
        &mut self,
        _: ParticleFurnitureContact,
        _: u32,
        _: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error> {
        self.events
            .push(json!({"kind":"hit","radius":self.radius as u32,"query_result":1}));
        if self.input["stop_hit"] == true {
            Err("hit stop")
        } else {
            Ok(())
        }
    }
    fn terrain_impact(
        &mut self,
        selector: u32,
        position: [u16; 3],
        strength: u32,
        _: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error> {
        self.events.push(
            json!({"kind":"terrain","selector":selector,"position":position,"strength":strength}),
        );
        Ok(())
    }
    fn radial_impact(
        &mut self,
        packet: u32,
        particle: ParticleId,
        source: u32,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error> {
        let position = context
            .particles
            .record(particle)
            .unwrap()
            .position()
            .unwrap();
        self.events
            .push(json!({"kind":"radial","packet":packet,"position":position,"source":source}));
        Ok(())
    }
}
fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|i| value[i].as_i64().unwrap() as u16)
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
fn particles(position: [u16; 3]) -> ProcessParticleRuntime {
    let mut particles = ProcessParticleRuntime::default();
    let record = particles.record_mut(ParticleId::Scratch).unwrap();
    record.write_position(position).unwrap();
    record.write_dword(0x14, 0x1234_5678).unwrap();
    record.write_dword(0x34, 0xabcdef00).unwrap();
    particles
}
fn terrain() -> crate::terrain_runtime::TerrainRuntime {
    crate::terrain_runtime::native_probe_fixture_terrain(
        &json!({"spacing":128,"height_shift":5,"plane_base":-256000,"plane_shift":8,"xz":[0,0],"heights":[0,0,0,0],"alternate":false}),
    )
}

#[test]
fn projectile_actor_broadphase_and_subdivision_match_native_queries_and_failure_prefixes() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/particle_actor_collision_native.json"
    ))
    .unwrap();
    let raw = super::super::tests::executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut process = process();
    let mut checked = 0;
    for (index, case) in receipt["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        // These segments substitute native allocator failure or a virtual+18
        // return0. The portable candidate Vec and real526EFC callback do not
        // claim those domains; the latter always returns1 after normal service.
        if input.get("null_allocation").is_some()
            || input.get("stop_allocation").is_some()
            || input.get("hit_result").is_some()
        {
            continue;
        }
        let mut particles = particles(words(&input["position"]));
        let mut rng = 123;
        let mut request = ParticleUpdateRequest {
            delta_microseconds: 0,
            load_factor: 256,
            birth_inputs: ParticleUpdateBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &mut process,
            },
            terrain: None,
        };
        let mut context = ParticleCollisionContext {
            particles: &mut particles,
            request: &mut request,
            executable,
            rng: &mut rng,
        };
        let radius = input["radius"].as_i64().unwrap() as i32;
        let candidates: Vec<_> = input["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, candidate)| ParticleActorCollisionCandidate {
                actor: ActorHandle::from_raw(i as u32),
                flags_2c: candidate["flags"].as_u64().unwrap() as u32,
                radius_28: candidate["radius"].as_u64().unwrap() as u16,
                position_32: words(&candidate["position"]),
                source_84: candidate["source"].as_u64().unwrap_or(0) as u32,
            })
            .collect();
        let mut host = Host {
            input: input.clone(),
            events: Vec::new(),
            queries: 0,
            radius,
        };
        let result = collide_actors(
            ParticleId::Scratch,
            words(&input["delta"]),
            radius,
            &candidates,
            &mut context,
            &mut host,
        );
        assert_eq!(
            result.is_ok(),
            case["normal_return"].as_bool().unwrap(),
            "case{index}"
        );
        if let Ok(hit) = result {
            assert_eq!(
                u64::from(hit),
                case["return_al"].as_u64().unwrap(),
                "case{index}"
            );
        }
        let expected: Vec<_> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] != "allocate" && event["kind"] != "free")
            .cloned()
            .collect();
        assert_eq!(host.events, expected, "case{index}");
        checked += 1;
    }
    assert_eq!(checked, 16);
}

#[test]
fn projectile_furniture_subdivision_matches_native_queries_and_failure_prefixes() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/particle_furniture_collision_native.json"
    ))
    .unwrap();
    let raw = super::super::tests::executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut process = process();
    let sine = [0; 4096];
    let mut checked = 0;
    for (index, case) in receipt["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        // Supplied false virtual+14 is outside526EFC's real466CF0 domain.
        if input.get("hit_result").is_some() {
            continue;
        }
        let mut particles = particles(words(&input["position"]));
        let mut terrain = terrain();
        let mut rng = 123;
        let mut request = ParticleUpdateRequest {
            delta_microseconds: 0,
            load_factor: 256,
            birth_inputs: ParticleUpdateBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &mut process,
            },
            terrain: Some(ParticleTerrainContext {
                terrain: &mut terrain,
                sine_quarter: &sine,
                wave_clock: input["clock"].as_u64().unwrap() as u32,
                world_flags: 0,
                impact_selectors: [0, 1, 2, 3, 6],
            }),
        };
        let mut context = ParticleCollisionContext {
            particles: &mut particles,
            request: &mut request,
            executable,
            rng: &mut rng,
        };
        let radius = input["radius"].as_i64().unwrap() as i32;
        let mut host = Host {
            input: input.clone(),
            events: Vec::new(),
            queries: 0,
            radius,
        };
        let result = collide_furniture(
            ParticleId::Scratch,
            words(&input["delta"]),
            radius,
            &mut context,
            &mut host,
        );
        assert_eq!(
            result.is_ok(),
            case["normal_return"].as_bool().unwrap(),
            "case{index}"
        );
        if let Ok(hit) = result {
            assert_eq!(
                u64::from(hit),
                case["return_al"].as_u64().unwrap(),
                "case{index}"
            );
        }
        assert_eq!(
            host.events,
            case["events"].as_array().unwrap().clone(),
            "case{index}"
        );
        checked += 1;
    }
    assert_eq!(checked, 12);
}

#[test]
fn projectile_order_short_circuits_and_rereads_flags_before_radial_response() {
    let raw = super::super::tests::executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut process = process();
    for flags in [0, 1] {
        let mut particles = particles([100, 200, 300]);
        let mut rng = 123;
        particles
            .record_mut(ParticleId::Scratch)
            .unwrap()
            .write_word(0x12, flags)
            .unwrap();
        particles
            .record_mut(ParticleId::Scratch)
            .unwrap()
            .write_dword(0x38, 0x9988)
            .unwrap();
        let mut request = ParticleUpdateRequest {
            delta_microseconds: 0,
            load_factor: 256,
            birth_inputs: ParticleUpdateBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &mut process,
            },
            terrain: None,
        };
        let candidates = [ParticleActorCollisionCandidate {
            actor: ActorHandle::from_raw(0),
            flags_2c: 4,
            radius_28: 80,
            position_32: [100, 200, 300],
            source_84: 0,
        }];
        let mut host = Host {
            input: json!({"answers":[1]}),
            events: Vec::new(),
            queries: 0,
            radius: 4,
        };
        assert!(particles
            .collide_projectile_ordered(
                ParticleId::Scratch,
                [0; 3],
                4,
                &candidates,
                &mut request,
                executable,
                &mut rng,
                &mut host
            )
            .unwrap());
        let kinds: Vec<_> = host
            .events
            .iter()
            .map(|event| event["kind"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            if flags == 0 {
                vec!["geometry", "hit", "radial"]
            } else {
                vec!["geometry"]
            }
        );
    }
}

#[test]
fn projectile_negative_radius_nonprogress_preserves_reached_queries_and_stops() {
    let mut center = [1, 2, 3];
    let mut half = [0, 0, 0];
    let mut calls = 0;
    let result = subdivide(
        &mut center,
        &mut half,
        0,
        -1,
        &mut |_, _| {
            calls += 1;
            Ok::<_, &str>(Some(()))
        },
        || "portable nonprogress",
    );
    assert_eq!(result, Err("portable nonprogress"));
    assert_eq!(calls, 2);
    assert_eq!(center, [1, 2, 3]);
}
