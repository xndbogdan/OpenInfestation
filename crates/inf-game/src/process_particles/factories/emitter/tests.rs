use super::*;
use serde_json::Value;
use sha2::Digest;

fn raw() -> Option<Vec<u8>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe");
    let raw = std::fs::read(path).ok();
    if raw.is_none() {
        eprintln!("retail Infest.exe missing; copy the game into retail/ to run this test");
    }
    raw
}
fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}
#[derive(Default)]
struct Host {
    phase: Option<u32>,
    notices: usize,
    mutate: bool,
    fail: bool,
}
impl ParticleFactoryHost for Host {
    type Error = &'static str;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.phase)
    }
    fn network_notice(
        &mut self,
        request: ParticleCreationNotice,
        particles: &mut ProcessParticleRuntime,
        context: &mut ParticleFactoryContext<'_, '_>,
        sources: &mut ParticleFactorySources,
    ) -> Result<(), Self::Error> {
        assert_eq!(request.event, 0x17);
        assert_eq!(request.source_handle, None);
        assert_eq!(request.code, 0x2000a);
        assert_eq!(request.velocity_source, ParticleVelocitySource::Separate);
        assert_eq!(
            sources.separate_velocity,
            Some(particles.emitter_velocity_54ecb8)
        );
        self.notices += 1;
        if self.mutate {
            context.process.process_null_handle = Some(ActorHandle::from_raw(0xabcdef));
            context.process.network_flag = 0;
            sources.position = [70, 80, 90].into();
            particles.emitter_velocity_54ecb8 = [1, 2, 65535].into();
        }
        if self.fail {
            Err("notice")
        } else {
            Ok(())
        }
    }
    fn impact_489600(
        &mut self,
        _: ParticleFactoryImpactRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), Self::Error> {
        Err("unexpected impact")
    }
}

#[test]
fn emitter_factory_matches_ten_complete_native_constructions_and_rejections() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/particle_emitter_factory_native.json"
    ))
    .unwrap();
    let Some(bytes) = raw() else { return };
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let output = &case["output"];
        let mut pool = ProcessParticleRuntime::default();
        pool.emitter_velocity_54ecb8 = words(&input["velocity"]).into();
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
        let mut sources = ParticleFactorySources {
            position: words(&input["position"]).into(),
            separate_velocity: None,
        };
        let mut host = Host {
            phase: Some(input["phase"].as_u64().unwrap() as u32),
            ..Host::default()
        };
        let id = pool
            .create_emitter_particle(
                ParticleEmitterFactoryRequest {
                    selector: input["selector"].as_u64().unwrap() as u32,
                    flags: input["flags"].as_u64().unwrap() as u32,
                },
                &mut ParticleFactoryContext {
                    process: &mut process,
                    rng: &mut rng,
                    executable: exe,
                },
                &mut sources,
                &mut host,
            )
            .unwrap();
        assert_eq!(
            id.is_some(),
            output["returned"].as_bool().unwrap(),
            "case{index}"
        );
        if let Some(id) = id {
            assert_eq!(
                id == ParticleId::Scratch,
                output["scratch"].as_bool().unwrap()
            );
            for (field, expected) in fixture["fields"]
                .as_array()
                .unwrap()
                .iter()
                .zip(output["fields"].as_array().unwrap())
            {
                let record = pool.record(id).unwrap();
                let offset = field[0].as_u64().unwrap() as usize;
                let value = if field[1] == 2 {
                    u32::from(record.read_word(offset).unwrap())
                } else {
                    record.read_dword(offset).unwrap()
                };
                assert_eq!(
                    u64::from(value),
                    expected.as_u64().unwrap(),
                    "case{index} offset{offset:x}"
                );
            }
            if id != ParticleId::Scratch {
                assert!(matches!(
                    pool.record(id).unwrap().read_dword(0x30),
                    Err(ParticleError::Unwritten {
                        offset: 0x30,
                        bytes: 4
                    })
                ));
            }
        }
        let pending: Vec<u16> = pool
            .pending(3)
            .unwrap()
            .iter()
            .map(|id| match id {
                ParticleId::Pool(slot) => *slot,
                _ => panic!("scratch list"),
            })
            .collect();
        assert_eq!(serde_json::json!(pending), output["pending"]);
        assert_eq!(u64::from(rng), output["rng"].as_u64().unwrap());
        assert_eq!(host.notices, 0);
    }
}

#[test]
fn emitter_factory_rereads_process_sources_after_notice_and_preserves_them_across_clear() {
    let Some(bytes) = raw() else { return };
    let mut pool = ProcessParticleRuntime::default();
    let mut process = ParticleFactoryProcessState {
        network_flag: 1,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(7)),
    };
    let mut rng = 12;
    let mut sources = ParticleFactorySources {
        position: [10, 20, 30].into(),
        separate_velocity: None,
    };
    let mut host = Host {
        phase: Some(4),
        mutate: true,
        ..Host::default()
    };
    let id = pool
        .create_emitter_particle(
            ParticleEmitterFactoryRequest {
                selector: 10,
                flags: 0,
            },
            &mut ParticleFactoryContext {
                process: &mut process,
                rng: &mut rng,
                executable: FrontendExecutable::parse(&bytes).unwrap(),
            },
            &mut sources,
            &mut host,
        )
        .unwrap()
        .unwrap();
    assert_eq!(pool.record(id).unwrap().position().unwrap(), [70, 80, 90]);
    assert_eq!(pool.record(id).unwrap().read_dword(0x14).unwrap(), 0xabcdef);
    assert_eq!(
        pool.record(id).unwrap().source_bytes::<6>(0x0a).unwrap(),
        ParticleVectorBytes::from_words([1, 2, 65535]).0
    );
    assert_eq!(host.notices, 1);
    assert_eq!(rng, 12);
    pool.clear().unwrap();
    assert_eq!(
        pool.emitter_velocity_54ecb8,
        ParticleVectorBytes::from_words([1, 2, 65535])
    );
}

#[test]
fn emitter_required_sources_stop_at_distinct_native_read_sites() {
    let Some(bytes) = raw() else { return };
    let mut pool = ProcessParticleRuntime::default();
    let mut process = ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    };
    let mut rng = 12;
    let mut sources = ParticleFactorySources {
        position: [10, 20, 30].into(),
        separate_velocity: None,
    };
    let mut host = Host {
        phase: Some(4),
        ..Host::default()
    };
    let mut context = ParticleFactoryContext {
        process: &mut process,
        rng: &mut rng,
        executable: FrontendExecutable::parse(&bytes).unwrap(),
    };
    assert_eq!(
        pool.create_emitter_particle(
            ParticleEmitterFactoryRequest {
                selector: 0x1000a,
                flags: 0
            },
            &mut context,
            &mut sources,
            &mut host
        )
        .unwrap(),
        None
    );
    assert!(matches!(
        pool.create_emitter_particle(
            ParticleEmitterFactoryRequest {
                selector: 10,
                flags: 0
            },
            &mut context,
            &mut sources,
            &mut host
        ),
        Err(ParticleFactoryError::MissingEmitterProcessNull { address: 0x466a87 })
    ));
    assert_eq!(pool.free_count(), 400);
    assert!(matches!(
        pool.create_emitter_particle(
            ParticleEmitterFactoryRequest {
                selector: 10,
                flags: 1
            },
            &mut context,
            &mut sources,
            &mut host
        ),
        Err(ParticleFactoryError::MissingEmitterProcessNull { address: 0x466abe })
    ));
    assert_eq!(pool.free_count(), 399);
    assert_eq!(pool.allocation_marker, Some(399));
    assert!(pool.pending(3).unwrap().is_empty());
    assert_eq!(*context.rng, 12);
}

#[test]
fn emitter_source_guards_confirm_vtable_bodies_and_loader_zero_custody() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/particle_emitter_source_native.json"
    ))
    .unwrap();
    let Some(bytes) = raw() else { return };
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    for guard in fixture["guards"].as_array().unwrap() {
        let backing = pe_backing(
            &bytes,
            guard["address"].as_u64().unwrap() as u32,
            guard["length"].as_u64().unwrap() as usize,
        );
        let digest: String = sha2::Sha256::digest(backing)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(
            digest,
            guard["sha256"].as_str().unwrap(),
            "{}",
            guard["name"]
        );
    }
    // This mapper exposes only actual file backing; the retained initial
    // value instead follows the verified PE section's virtual zero tail.
    assert!(exe.mapped(0x54ecb8, 6).is_err());
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let pe = dword(0x3c) as usize;
    let section = pe
        + 24
        + usize::from(u16::from_le_bytes(
            bytes[pe + 20..pe + 22].try_into().unwrap(),
        ))
        + 2 * 40;
    assert_eq!(&bytes[section..section + 8], b".data\0\0\0");
    assert_eq!(dword(section + 12), 0x12e000);
    assert_eq!(dword(section + 16), 0x19000);
    assert_eq!(dword(section + 8), 0x132c38);
    let relative = 0x54ecb8 - 0x52e000;
    assert!(relative >= dword(section + 16) && relative + 6 <= dword(section + 8));
    let relocation_rva = dword(pe + 24 + 96 + 5 * 8);
    let relocation_size = dword(pe + 24 + 96 + 5 * 8 + 4);
    let relocation = pe_backing(&bytes, 0x400000 + relocation_rva, relocation_size as usize);
    let mut cursor = 0;
    while cursor < relocation.len() {
        let page = u32::from_le_bytes(relocation[cursor..cursor + 4].try_into().unwrap());
        let size =
            u32::from_le_bytes(relocation[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        assert!(size >= 8);
        for entry in relocation[cursor + 8..cursor + size].chunks_exact(2) {
            let entry = u16::from_le_bytes(entry.try_into().unwrap());
            match entry >> 12 {
                0 => {}
                3 => {
                    let address = 0x400000 + page + u32::from(entry & 0xfff);
                    assert!(address + 4 <= 0x54ecb8 || address >= 0x54ecbe);
                }
                kind => panic!("unowned relocation{kind}"),
            }
        }
        cursor += size;
    }
    assert_eq!(
        ProcessParticleRuntime::default().emitter_velocity_54ecb8,
        ParticleVectorBytes::from_words([0; 3])
    );
}

fn pe_backing(bytes: &[u8], address: u32, width: usize) -> &[u8] {
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let pe = dword(0x3c) as usize;
    let count = u16::from_le_bytes(bytes[pe + 6..pe + 8].try_into().unwrap());
    let first = pe
        + 24
        + usize::from(u16::from_le_bytes(
            bytes[pe + 20..pe + 22].try_into().unwrap(),
        ));
    for index in 0..usize::from(count) {
        let entry = first + index * 40;
        let start = 0x400000 + dword(entry + 12);
        let len = dword(entry + 16);
        if address >= start
            && u64::from(address) + width as u64 <= u64::from(start) + u64::from(len)
        {
            let offset = (dword(entry + 20) + address - start) as usize;
            return &bytes[offset..offset + width];
        }
    }
    panic!("unmapped PE source{address:08x}")
}
