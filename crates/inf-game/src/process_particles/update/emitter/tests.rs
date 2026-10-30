use super::*;
use crate::object_runtime::ActorHandle;
use crate::process_particles::{
    factories::ParticleFactoryProcessState, ParticleAllocationRequest, ParticleConstruction,
};
use serde_json::Value;

fn raw() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .unwrap()
}
fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}
fn request(
    process: &mut ParticleFactoryProcessState,
    delta: i32,
    load_factor: i32,
    phase: u32,
) -> ParticleUpdateRequest<'_> {
    ParticleUpdateRequest {
        delta_microseconds: delta,
        load_factor,
        birth_inputs: ParticleUpdateBirthInputs {
            network_active: false,
            phase: Some(phase),
            process,
        },
        terrain: None,
    }
}

#[test]
fn emitter_update_matches_thirty_native_cases_including_actual_trail_birth() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/particle_emitter_update_native.json"
    ))
    .unwrap();
    let bytes = raw();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let output = &case["output"];
        let mut pool = ProcessParticleRuntime::default();
        // The native receipt supplies an unregistered receiver; scratch is
        // likewise outside the physical walk. It leaves slot399 available.
        let id = ParticleId::Scratch;
        let record = pool.record_mut(id).unwrap();
        record.write_dword(0, 0x526f80).unwrap();
        record.write_position(words(&input["position"])).unwrap();
        for (axis, value) in words(&input["velocity"]).into_iter().enumerate() {
            record.write_word(0x0a + axis * 2, value).unwrap();
        }
        record
            .write_dword(0x3c, input["selector"].as_u64().unwrap() as u32)
            .unwrap();
        let before = record.source_bytes::<24>(0).unwrap();
        let mut process = ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(ActorHandle::from_raw(
                input["source"].as_u64().unwrap() as u32
            )),
        };
        let mut rng = input["rng"].as_u64().unwrap() as u32;
        let phase = input["phase"].as_u64().unwrap() as u32;
        let result: Result<bool, ParticleUpdateError<&str>> = pool.update_emitter(
            id,
            &mut request(
                &mut process,
                input["delta"].as_i64().unwrap() as i32,
                input["load_factor"].as_i64().unwrap() as i32,
                phase,
            ),
            executable,
            &mut rng,
        );
        assert!(!result.unwrap());
        let children: Vec<_> = pool.pending(1).unwrap().iter().copied().collect();
        assert_eq!(
            children.len(),
            output["children"].as_array().unwrap().len(),
            "case{index}"
        );
        let child = if output["scratch"] == true {
            Some(ParticleId::Scratch)
        } else {
            children.first().copied()
        };
        if let Some(child) = child {
            for (field, expected) in fixture["fields"]
                .as_array()
                .unwrap()
                .iter()
                .zip(output["fields"].as_array().unwrap())
            {
                let offset = field[0].as_u64().unwrap() as usize;
                let record = pool.record(child).unwrap();
                let actual = if field[1] == 2 {
                    u32::from(record.read_word(offset).unwrap())
                } else {
                    record.read_dword(offset).unwrap()
                };
                assert_eq!(
                    u64::from(actual),
                    expected.as_u64().unwrap(),
                    "case{index} offset{offset:x}"
                );
            }
        }
        if phase == 4 {
            assert_eq!(
                pool.record(id).unwrap().source_bytes::<24>(0).unwrap(),
                before
            );
        }
        assert_eq!(
            u64::from(rng),
            output["rng"].as_u64().unwrap(),
            "case{index}"
        );
    }
}

struct Host;
impl ParticleUpdateHost for Host {
    type Error = &'static str;
    fn impact_sound(
        &mut self,
        _: ParticleSoundRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        panic!("emitter has no sound")
    }
}
fn parent(pool: &mut ProcessParticleRuntime) -> ParticleId {
    let id = pool
        .allocate(ParticleAllocationRequest {
            payload_bytes: 64,
            last_reclaim_category: 3,
            network_active: false,
            phase: Some(4),
        })
        .unwrap();
    pool.construct(
        id,
        ParticleConstruction {
            category: 3,
            position: [100, 200, 300].into(),
            velocity: Some([0; 3].into()),
            source_handle: 12,
            flags: 2,
        },
    )
    .unwrap();
    let record = pool.record_mut(id).unwrap();
    record.write_dword(0, 0x526f80).unwrap();
    record.write_dword(0x3c, 10).unwrap();
    record.write_word(0x10, 1535).unwrap();
    id
}

#[test]
fn emitter_manager_birth_failure_retains_rng_allocation_and_does_not_replay() {
    let bytes = raw();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    let mut pool = ProcessParticleRuntime::default();
    let id = parent(&mut pool);
    let mut process = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    };
    let mut rng = 0x12345678;
    let report = pool.tick_particles(
        request(&mut process, 33333, 0, 4),
        executable,
        &mut rng,
        &mut Host,
    );
    assert_eq!(report.omissions.len(), 1);
    assert_eq!(report.omissions[0].phase, ParticleStopPhase::Update);
    assert!(matches!(
        report.omissions[0].error,
        ParticleUpdateError::Source(_)
    ));
    assert_eq!(pool.free_count(), 398);
    assert!(pool.pending(1).unwrap().is_empty());
    assert_eq!(pool.record(id).unwrap().read_word(0x10).unwrap(), 1503);
    assert_ne!(rng, 0x12345678);
    let stopped_rng = rng;
    process.process_null_handle = Some(ActorHandle::from_raw(12));
    let report = pool.tick_particles(
        request(&mut process, 33333, 0, 4),
        executable,
        &mut rng,
        &mut Host,
    );
    assert_eq!(report.stopped, 1);
    assert!(report.omissions.is_empty());
    assert_eq!(rng, stopped_rng);
    assert_eq!(pool.free_count(), 398);
    assert_eq!(pool.record(id).unwrap().read_word(0x10).unwrap(), 1503);
}

#[test]
fn emitter_manager_runs_real_trail_then_retires_parent_without_removing_child() {
    let bytes = raw();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    let mut pool = ProcessParticleRuntime::default();
    let id = parent(&mut pool);
    let mut process = ParticleFactoryProcessState::native_startup();
    let mut rng = 0x12345678;
    let report = pool.tick_particles(
        request(&mut process, 33333, 0, 4),
        executable,
        &mut rng,
        &mut Host,
    );
    assert_eq!(report.updated, 1);
    assert!(report.omissions.is_empty());
    let child = *pool.pending(1).unwrap().front().unwrap();
    assert_eq!(pool.record(child).unwrap().read_dword(0).unwrap(), 0x5288f4);
    assert_eq!(
        pool.record(id).unwrap().position().unwrap(),
        [100, 200, 300]
    );
    pool.record_mut(id).unwrap().write_word(0x10, 1).unwrap();
    let report = pool.tick_particles(
        request(&mut process, 1024, 0, 4),
        executable,
        &mut rng,
        &mut Host,
    );
    assert_eq!(report.updated, 1);
    assert_eq!(report.destroyed, 1);
    assert!(report.omissions.is_empty());
    assert!(pool.pending(3).unwrap().is_empty());
    assert_eq!(pool.pending(1).unwrap().front(), Some(&child));
    assert_eq!(pool.free_count(), 399);
}
