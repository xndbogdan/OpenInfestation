use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    process_particles::{ParticleId, ProcessParticleRuntime},
    terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, FrontendExecutable, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    spacing: u16,
    height_shift: u32,
    height_seed: u8,
    origin: [i32; 3],
    rows: Vec<[i16; 3]>,
    rng: u32,
    phase: Option<u32>,
    network_active: Option<bool>,
    requests: Vec<Birth>,
    records: Vec<Record>,
    pending: Vec<u16>,
    final_rng: u32,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Birth {
    particle_type: u32,
    position: [u16; 3],
    velocity: [u16; 3],
    flags: u32,
    rng_before: u32,
}
#[derive(Deserialize)]
struct Record {
    slot: u16,
    fields: Vec<u32>,
}

fn terrain(case: &Case) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for x in 0..512 {
        for z in 0..512 {
            bytes[TERRAIN_PREFIX_BYTES + (x * 512 + z) * 2] =
                (x * 13 + z * 7 + usize::from(case.height_seed)) as u8;
        }
    }
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: if case.spacing == 128 {
                TerrainSpacing::Units128
            } else {
                TerrainSpacing::Units256
            },
            height_scale: if case.height_shift == 4 {
                TerrainHeightScale::Units16
            } else {
                TerrainHeightScale::Units32
            },
        },
    )
}

fn fixture() -> Receipt {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/terrain_particle_emission_native.json"
    ))
    .unwrap()
}

#[test]
fn original_terrain_emission_and_real_trail_births_match_requests_records_and_rng() {
    let assets = FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for case in fixture().cases {
        let terrain = terrain(&case);
        let rows: Vec<_> = case
            .rows
            .iter()
            .map(|row| TerrainRow {
                z: row[0],
                left: row[1],
                right: row[2],
            })
            .collect();
        let mut particles = ProcessParticleRuntime::default();
        let mut rng = case.rng;
        let mut requests = Vec::new();
        let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x1234_5678)),
        };
        let result = emit_terrain_particles(
            TerrainParticleEmissionRequest {
                terrain: &terrain,
                context_position: case.origin,
                rows: &rows,
            },
            ParticleBirthInputs {
                network_active: case.network_active.unwrap_or(false),
                phase: Some(case.phase.unwrap_or(4)),
                process: &birth_process,
            },
            &mut rng,
            |request, rng| {
                requests.push(Birth {
                    particle_type: request.particle_type,
                    position: request.position,
                    velocity: request.velocity.unwrap(),
                    flags: request.flags,
                    rng_before: *rng,
                });
                particles.birth_trail(request, executable, rng).map(|_| ())
            },
        )
        .unwrap();
        assert_eq!(requests, case.requests, "{} requests", case.name);
        assert_eq!(result, requests.len());
        assert_eq!(rng, case.final_rng, "{} RNG", case.name);
        let pending: Vec<_> = particles
            .pending(1)
            .unwrap()
            .iter()
            .map(|id| match id {
                ParticleId::Pool(index) => *index,
                ParticleId::Scratch => panic!("scratch cannot publish"),
            })
            .collect();
        assert_eq!(pending, case.pending, "{} pending", case.name);
        let offsets = [
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
        ];
        for expected in case.records {
            let record = particles.record(ParticleId::Pool(expected.slot)).unwrap();
            let actual: Vec<_> = offsets
                .iter()
                .map(|&(offset, size)| {
                    if size == 4 {
                        record.read_dword(offset).unwrap()
                    } else {
                        u32::from(record.read_word(offset).unwrap())
                    }
                })
                .collect();
            assert_eq!(
                actual, expected.fields,
                "{} slot{}",
                case.name, expected.slot
            );
        }
    }
}

#[test]
fn terrain_emission_failure_commits_the_first_birth_rng_prefix() {
    let case = fixture()
        .cases
        .into_iter()
        .find(|case| !case.requests.is_empty())
        .unwrap();
    let terrain = terrain(&case);
    let rows: Vec<_> = case
        .rows
        .iter()
        .map(|row| TerrainRow {
            z: row[0],
            left: row[1],
            right: row[2],
        })
        .collect();
    let mut rng = case.rng;
    let mut calls = 0;
    let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0)),
    };
    let result = emit_terrain_particles(
        TerrainParticleEmissionRequest {
            terrain: &terrain,
            context_position: case.origin,
            rows: &rows,
        },
        ParticleBirthInputs {
            network_active: false,
            phase: Some(4),
            process: &birth_process,
        },
        &mut rng,
        |_, _| {
            calls += 1;
            Err("required birth")
        },
    );
    assert_eq!(result, Err("required birth"));
    assert_eq!(calls, 1);
    assert_eq!(rng, case.requests[0].rng_before);
}
