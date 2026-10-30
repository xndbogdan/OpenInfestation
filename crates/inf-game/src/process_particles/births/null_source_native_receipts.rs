//! Independent implicit-source producer receipts. Native raw list pointers
//! are represented by actual pool identities and ordered memberships. A
//! deliberately unavailable process DWORD is a required-input prefix, not a
//! native memory fault or a claim about synchronous source mutation.

use std::convert::Infallible;

use inf_formats::FrontendExecutable;
use serde_json::{json, Value};

use super::*;
use crate::object_runtime::ActorHandle;

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn bytes(value: &Value) -> Vec<Option<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_u64().map(|value| u8::try_from(value).unwrap()))
        .collect()
}

fn dword(value: &[Option<u8>]) -> Option<u32> {
    let known: Vec<_> = value.iter().copied().collect::<Option<_>>()?;
    Some(u32::from_le_bytes(known.try_into().unwrap()))
}

fn words(value: &Value) -> [u16; 3] {
    let value = bytes(value)
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .expect("admitted producer input has all six vector bytes");
    assert_eq!(value.len(), 6);
    std::array::from_fn(|axis| u16::from_le_bytes([value[axis * 2], value[axis * 2 + 1]]))
}

fn identity(id: Option<ParticleId>) -> Value {
    match id {
        Some(ParticleId::Pool(slot)) => Value::from(slot),
        Some(ParticleId::Scratch) => Value::from("scratch"),
        None => Value::Null,
    }
}

fn slots(value: &Value) -> Vec<u16> {
    let mut result = Vec::new();
    for run in value.as_array().unwrap() {
        let run = run.as_array().unwrap();
        assert_eq!(run.len(), 3);
        let mut current = run[0].as_i64().unwrap();
        let last = run[1].as_i64().unwrap();
        let stride = run[2].as_i64().unwrap();
        assert!(stride != 0 || current == last);
        loop {
            result.push(u16::try_from(current).unwrap());
            if current == last {
                break;
            }
            current += stride;
            assert!((stride > 0 && current <= last) || (stride < 0 && current >= last));
        }
    }
    result
}

fn compare_payload(record: &super::super::ParticleRecord, expected: &Value, name: &str) {
    let expected = bytes(expected);
    assert_eq!(expected.len(), 52);
    for (offset, wanted) in (0..0x18).chain(0x24..64).zip(expected) {
        // The compact fixture omits native node/manager pointer bytes.
        let actual = record.initialized[offset].then_some(record.bytes[offset]);
        assert_eq!(actual, wanted, "{name} payload+{offset:02x}");
    }
}

fn compare_state(
    pool: &ProcessParticleRuntime,
    process: &ParticleFactoryProcessState,
    phase: Option<u32>,
    rng: u32,
    expected: &Value,
    name: &str,
) {
    let mut backed = [false; 400];
    for group in expected["payload_groups"].as_array().unwrap() {
        for slot in slots(&group["slots"]) {
            let slot = usize::from(slot);
            assert!(slot < 400 && !backed[slot]);
            backed[slot] = true;
            compare_payload(&pool.records[slot], &group["bytes"], name);
        }
    }
    for (slot, supplied) in backed.into_iter().enumerate() {
        if !supplied {
            for offset in (0..0x18).chain(0x24..64) {
                assert!(
                    !pool.records[slot].initialized[offset],
                    "{name} undeclared heap slot {slot}+{offset:02x}"
                );
            }
        }
    }
    compare_payload(&pool.scratch, &expected["scratch"], name);
    assert_eq!(
        pool.free.iter().rev().copied().collect::<Vec<_>>(),
        slots(&expected["free_order"]),
        "{name} complete free order"
    );
    assert!(expected["free_boundary"].is_null());
    assert_eq!(
        pool.allocation_marker.map(u32::from),
        expected["allocation_marker"].as_u64().map(|v| v as u32),
        "{name} allocated but unpublished cell"
    );
    let actual: Vec<_> = pool
        .occupants
        .iter()
        .enumerate()
        .filter_map(|(slot, id)| {
            id.map(|id| {
                assert_eq!(id, ParticleId::Pool(slot as u16));
                slot as u16
            })
        })
        .collect();
    assert_eq!(
        actual,
        slots(&expected["occupied_slots"]),
        "{name} complete occupied-cell custody"
    );
    for category in 0..6 {
        for (key, actual) in [
            ("pending", &pool.pending[category]),
            ("active", &pool.active[category]),
        ] {
            let actual: Vec<_> = actual
                .iter()
                .map(|id| match id {
                    ParticleId::Pool(slot) => *slot,
                    ParticleId::Scratch => panic!("scratch has no owned list node"),
                })
                .collect();
            assert_eq!(
                actual,
                slots(&expected[key][category]),
                "{name} {key}/{category}"
            );
        }
    }
    assert_eq!(dword(&bytes(&expected["rng"])), Some(rng), "{name} RNG");
    assert_eq!(
        process.process_null_handle.map(ActorHandle::raw),
        dword(&bytes(&expected["process_null"])),
        "{name} live process source masks"
    );
    assert_eq!(
        dword(&bytes(&expected["network"])),
        Some(process.network_flag)
    );
    assert_eq!(dword(&bytes(&expected["phase"])), phase);
    // ParticleFactoryProcessState has no SessionState field. The actual
    // supplied phase remains compared immediately above, independently.
    assert_eq!(dword(&bytes(&expected["cursor"])), Some(pool.vector_cursor));
    assert!(expected["foreign_header"].is_null());
    // None of these producers owns manager update-global publication.
    assert_eq!(
        pool.update_globals,
        super::super::update::ParticleUpdateGlobals::default()
    );
}

/// One scalar/result interpretation shared by the four real producer methods;
/// it does not merge their type-selection, velocity, audio, or RNG policies.
#[derive(Debug, PartialEq, Eq)]
enum Stop {
    Source(ParticleBirthProducer),
    Phase,
    Audio,
}

fn stopped<E>(error: ParticleBirthError<E>) -> Stop {
    match error {
        ParticleBirthError::Source(error) => Stop::Source(error.producer),
        ParticleBirthError::Particle(ParticleError::MissingPhase) => Stop::Phase,
        ParticleBirthError::Audio(_) => Stop::Audio,
        other => panic!(
            "unadmitted producer boundary: {}",
            match other {
                ParticleBirthError::Particle(error) => error.to_string(),
                ParticleBirthError::Executable(error) => error.to_string(),
                _ => unreachable!(),
            }
        ),
    }
}

#[derive(Default)]
struct Audio {
    requests: Vec<ParticleSoundRequest>,
    fail: bool,
}

impl ParticleBirthAudio for Audio {
    type Error = ();

    fn explosion_sound(
        &mut self,
        request: ParticleSoundRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), ()> {
        self.requests.push(request);
        if self.fail {
            Err(())
        } else {
            Ok(())
        }
    }
}

fn invoke(
    kind: &str,
    pool: &mut ProcessParticleRuntime,
    request: ParticleBirthRequest<'_>,
    executable: FrontendExecutable<'_>,
    rng: &mut u32,
    audio: &mut Audio,
) -> Result<Option<ParticleId>, Stop> {
    let no_audio = |result: Result<Option<ParticleId>, ParticleBirthError<Infallible>>| {
        result.map_err(stopped)
    };
    match kind {
        "central" => pool
            .birth_central(request, executable, rng, audio)
            .map_err(stopped),
        "debris" => no_audio(pool.birth_debris(request, executable, rng)),
        "trail" => no_audio(pool.birth_trail(request, executable, rng)),
        "splash" => no_audio(pool.birth_splash(request, executable, rng)),
        _ => panic!("unadmitted producer {kind}"),
    }
}

fn producer(kind: &str, selector: u32) -> ParticleBirthProducer {
    match (kind, selector) {
        ("central", _) => ParticleBirthProducer::Central,
        ("debris", _) => ParticleBirthProducer::Debris,
        ("splash", _) => ParticleBirthProducer::Splash,
        ("trail", 12) => ParticleBirthProducer::Trail12,
        ("trail", 13) => ParticleBirthProducer::Trail13,
        ("trail", _) => ParticleBirthProducer::Trail,
        _ => unreachable!(),
    }
}

fn entry(kind: &str) -> u32 {
    match kind {
        "central" => 0x48a0f0,
        "debris" => 0x48a3a0,
        "splash" => 0x4893a0,
        "trail" => 0x489d90,
        _ => unreachable!(),
    }
}

fn patch(value: &Value, process: &mut ParticleFactoryProcessState) {
    assert_eq!(value["region"], "process_null");
    assert_eq!(value["offset"], 0);
    let data = bytes(&value["bytes"]);
    assert_eq!(data.len(), 4);
    assert!(data.iter().all(Option::is_some) || data.iter().all(Option::is_none));
    process.process_null_handle = dword(&data).map(ActorHandle::from_raw);
}

fn prepare(pool: &mut ProcessParticleRuntime, input: &Value, phase: Option<u32>) {
    let preparation = &input["prepare"];
    for index in 0..number(&preparation["count"]) {
        // This preparation executes the real allocation/base construction,
        // using its declared explicit source operand. It does not invoke an
        // implicit-source birth with a substitute process NULL.
        let id = pool
            .allocate(ParticleAllocationRequest {
                payload_bytes: number(&preparation["payload_bytes"]),
                last_reclaim_category: number(&preparation["category"]) as i32,
                network_active: input["network_active"].as_bool().unwrap(),
                phase,
            })
            .unwrap();
        assert_eq!(id, ParticleId::Pool(399 - index as u16));
        pool.construct(
            id,
            ParticleConstruction {
                category: number(&preparation["category"]) as usize,
                position: words(&input["position6"]).into(),
                velocity: Some(words(&input["velocity6"]).into()),
                source_handle: number(&preparation["source_handle"]),
                flags: number(&preparation["flags"]),
            },
        )
        .unwrap();
    }
    if preparation["transfer"].as_bool().unwrap() {
        pool.transfer_pending();
    }
}

fn compare_audio(audio: &Audio, expected: &Value, name: &str) {
    assert_eq!(audio.requests.len(), expected.as_array().unwrap().len());
    for (actual, wanted) in audio.requests.iter().zip(expected.as_array().unwrap()) {
        assert_eq!(actual.sound, number(&wanted["sound"]), "{name} sound");
        assert_eq!(
            actual.position,
            words(&wanted["position6"]),
            "{name} sound XYZ"
        );
        assert_eq!(actual.volume, number(&wanted["volume"]));
        assert_eq!(actual.pitch, number(&wanted["pitch"]));
        assert_eq!(actual.flags, number(&wanted["flags"]));
        assert_eq!(audio.fail, wanted["error"].as_bool().unwrap());
    }
}

fn validate_native_step(step: &Value, process: &ParticleFactoryProcessState) {
    let input = &step["input"];
    let kind = input["kind"].as_str().unwrap();
    let address = entry(kind);
    let events = step["timeline"].as_array().unwrap();
    let patches = input["patches"].as_array().unwrap();
    let expected_effects: Vec<_> = patches
        .iter()
        .map(|patch| {
            let mut event = patch.as_object().unwrap().clone();
            event.insert("kind".to_owned(), Value::from("supplied_effect"));
            Value::Object(event)
        })
        .collect();
    assert_eq!(&events[..patches.len()], expected_effects.as_slice());
    assert_eq!(
        events[patches.len()],
        json!({"kind":"entry", "address":address})
    );
    assert!(
        step["fault"].is_null(),
        "admitted required-input stops are not raw faults"
    );
    let mut entries = serde_json::Map::new();
    let mut returns = serde_json::Map::new();
    let mut reached_source = Vec::new();
    let mut reached_audio = Vec::new();
    let mut reached_effects = Vec::new();
    for event in events {
        match event["kind"].as_str().unwrap() {
            "entry" | "ret" => {
                let is_entry = event["kind"] == "entry";
                let routine = number(&event[if is_entry { "address" } else { "entry" }]);
                let counts = if is_entry { &mut entries } else { &mut returns };
                let key = format!("{routine:08x}");
                let count = counts.get(&key).map_or(0, number);
                counts.insert(key, Value::from(count + 1));
            }
            "source_read" => {
                reached_source.push(json!({"ip": event["ip"], "bytes":event["bytes"]}));
            }
            "supplied_audio" => {
                let mut audio = event.as_object().unwrap().clone();
                audio.remove("kind");
                reached_audio.push(Value::Object(audio));
            }
            "required_phase" => assert_eq!(step["stopped"], "missing_phase"),
            "supplied_effect" => reached_effects.push(event.clone()),
            other => panic!("unadmitted original boundary {other}"),
        }
    }
    assert_eq!(Value::Object(entries), step["entry_counts"]);
    assert_eq!(Value::Object(returns), step["ret_counts"]);
    assert_eq!(Value::Array(reached_source), step["source_reads"]);
    assert_eq!(Value::Array(reached_audio), step["audio"]);
    assert_eq!(reached_effects, expected_effects);
    let root_return = step["ret_counts"][format!("{address:08x}")].as_u64();
    assert_eq!(root_return, step["stopped"].is_null().then_some(1));
    if step["stopped"].is_null() {
        let pop_bytes = if kind == "splash" { 4 } else { 8 };
        assert_eq!(step["return_stack_delta"], pop_bytes + 4);
        let root_ret = events
            .iter()
            .find(|e| e["kind"] == "ret" && e["entry"] == address)
            .unwrap();
        assert_eq!(root_ret["pop_bytes"], pop_bytes);
    } else {
        assert!(step["return_stack_delta"].is_null());
    }
    let arguments = step["argument_reads"].as_array().unwrap();
    if kind == "trail" && !step["source_reads"].as_array().unwrap().is_empty() {
        let (flags_ip, velocity_ip) = match number(&input["selector"]) {
            12 => (0x489f36, 0x489f41),
            13 => (0x489fba, 0x489fca),
            _ => (0x489eae, 0x489eb7),
        };
        assert_eq!(
            arguments[0],
            json!({
                "ip":flags_ip,"field":"flags","stack_offset":8,
                "bytes":number(&input["flags"]).to_le_bytes()
            })
        );
        if step["stopped"].is_null() {
            let pointer = if input["has_velocity"] == true {
                0x721008_u32
            } else {
                0
            };
            assert_eq!(arguments.len(), 2);
            assert_eq!(
                arguments[1],
                json!({
                    "ip":velocity_ip,"field":"velocity_pointer","stack_offset":4,
                    "bytes":pointer.to_le_bytes()
                })
            );
        } else {
            assert_eq!(arguments.len(), 1);
        }
    } else {
        assert!(arguments.is_empty());
    }
    for read in step["source_reads"].as_array().unwrap() {
        let owner = producer(kind, number(&input["selector"]));
        assert_eq!(number(&read["ip"]), owner.source_read_address());
        let wanted = process
            .process_null_handle
            .map(|h| h.raw().to_le_bytes().map(Some));
        assert_eq!(bytes(&read["bytes"]), wanted.unwrap_or([None; 4]));
    }
}

/// The production executable view deliberately maps authored data only. This
/// test backing separately admits the supported file-backed .text profile for
/// instruction guards; it is not a runtime code loader or generic PE mapper.
fn original_text(raw: &[u8]) -> &[u8] {
    let pe = u32::from_le_bytes(raw[0x3c..0x40].try_into().unwrap()) as usize;
    assert_eq!(pe, 0xe8);
    assert_eq!(&raw[pe..pe + 4], b"PE\0\0");
    assert_eq!(
        u16::from_le_bytes(raw[pe + 6..pe + 8].try_into().unwrap()),
        6
    );
    let optional = pe + 24;
    let optional_size = u16::from_le_bytes(raw[pe + 20..pe + 22].try_into().unwrap()) as usize;
    assert_eq!(optional_size, 0xe0);
    assert_eq!(
        u32::from_le_bytes(raw[optional + 28..optional + 32].try_into().unwrap()),
        0x400000
    );
    let text_header = optional + optional_size;
    assert_eq!(&raw[text_header..text_header + 8], b".text\0\0\0");
    let fields: [u32; 4] = std::array::from_fn(|index| {
        let offset = text_header + 8 + index * 4;
        u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap())
    });
    assert_eq!(fields, [0x11d475, 0x1000, 0x11e000, 0x1000]);
    let [_, _, raw_size, raw_offset] = fields;
    &raw[raw_offset as usize..(raw_offset + raw_size) as usize]
}

fn validate_metadata(metadata: &Value, raw: &[u8]) {
    let text = original_text(raw);
    let code_bytes = |address: u32, length: usize| {
        let offset = address.checked_sub(0x401000).unwrap() as usize;
        &text[offset..offset + length]
    };
    assert_eq!(
        metadata["base_commit"],
        "cc6e2ede9e8cbd4345f4307b489d02d44fe13fa6"
    );
    assert_eq!(
        metadata["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(
        metadata["text_sha256"],
        "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2"
    );
    assert_eq!(metadata["fpcw"], 0x027f);
    assert_eq!(metadata["instruction_patches"], 0);
    assert_eq!(metadata["payload_spans"], json!([[0, 24], [36, 28]]));
    assert_eq!(metadata["pool_slots"], 400);
    assert_eq!(metadata["pool_stride"], 72);
    assert_eq!(metadata["initial_heap_payload"], "unknown");
    assert_eq!(metadata["initial_scratch"], "known PE zero storage");
    assert_eq!(
        metadata["producer_abi"],
        json!({
            "central":{"stack":["velocity_pointer","flags"],"ret_pop_bytes":8},
            "debris":{"stack":["velocity_pointer","flags"],"ret_pop_bytes":8},
            "trail":{"stack":["velocity_pointer","flags"],"ret_pop_bytes":8},
            "splash":{"stack":["flags"],"ret_pop_bytes":4,"common_ctor_velocity":"NULL"}
        })
    );
    let argument_guards = [
        ("00489eae", "flags", "8b4c2428", [0x8b, 0x4c, 0x24, 0x28]),
        (
            "00489eb7",
            "velocity_pointer",
            "8b542424",
            [0x8b, 0x54, 0x24, 0x24],
        ),
        ("00489f36", "flags", "8b542428", [0x8b, 0x54, 0x24, 0x28]),
        (
            "00489f41",
            "velocity_pointer",
            "8b44242c",
            [0x8b, 0x44, 0x24, 0x2c],
        ),
        ("00489fba", "flags", "8b542428", [0x8b, 0x54, 0x24, 0x28]),
        (
            "00489fca",
            "velocity_pointer",
            "8b442430",
            [0x8b, 0x44, 0x24, 0x30],
        ),
    ];
    assert_eq!(
        metadata["trail_argument_guards"].as_object().unwrap().len(),
        6
    );
    for (ip, field, hex, actual) in argument_guards {
        assert_eq!(
            metadata["trail_argument_guards"][ip],
            json!({"field":field,"bytes":hex})
        );
        assert_eq!(code_bytes(u32::from_str_radix(ip, 16).unwrap(), 4), actual);
    }
    let guards = [
        ("debris", ParticleBirthProducer::Debris),
        ("central", ParticleBirthProducer::Central),
        ("splash", ParticleBirthProducer::Splash),
        ("trail_general", ParticleBirthProducer::Trail),
        ("trail12", ParticleBirthProducer::Trail12),
        ("trail13", ParticleBirthProducer::Trail13),
    ];
    assert_eq!(
        metadata["source_guards"].as_object().unwrap().len(),
        guards.len()
    );
    for (name, producer) in guards {
        let ip = producer.source_read_address();
        assert_eq!(
            metadata["source_guards"][name],
            json!({"ip":ip,"bytes":"a194e85400"})
        );
        assert_eq!(code_bytes(ip, 5), [0xa1, 0x94, 0xe8, 0x54, 0]);
    }
    let bodies = [
        (
            "debris",
            0x48a3a0,
            0x48a550,
            "a6678fd0a6a4c4fb373d06e08492d87ddac2bd78afbe1cb12b07166f6fed7fa7",
        ),
        (
            "central",
            0x48a0f0,
            0x48a390,
            "d8d0f1e14118921608f58f3f6dce2d2990bc3ea175574b54271c80c2a6d6c065",
        ),
        (
            "trail",
            0x489d90,
            0x48a060,
            "22b80fbde46b6084d641522b0b1cac7b52b81555c8164e43911e01dc78efdb3e",
        ),
        (
            "splash",
            0x4893a0,
            0x489488,
            "0852df4af90c07d403ee0e8a425da92df029359653af70b8a771c24d6c204cf6",
        ),
        (
            "allocator_wrapper",
            0x466100,
            0x46610e,
            "049de42a7991720b8a5be11b8e13a114a2f64b563d700d4ee9d7eea8d7bf9576",
        ),
        (
            "allocator",
            0x464ed0,
            0x464f60,
            "500bc4d63165fc4713c5fa2690a94af18b334a1548763dc36836b1e74f1172b6",
        ),
        (
            "common_ctor",
            0x464fb0,
            0x465025,
            "faed3d0870476a2c4a5dc2bdb8deb23dde7dcf8cb21898572707e1449937dd38",
        ),
        (
            "rng",
            0x48f060,
            0x48f079,
            "e7848b6caf06850740711a5eb206007001af8db642280353cd14fd509d714f86",
        ),
        (
            "transfer",
            0x464da0,
            0x464de9,
            "e9fbd82158119ff7ee567f87de6b331060d6b30488e0d616847229c154f53d26",
        ),
        (
            "free_admission",
            0x464e30,
            0x464e4c,
            "e1606f863e9c7d47fe5c71fcf180bc93ab88ed339ceb517341ff5010c984f042",
        ),
        (
            "publication",
            0x464e50,
            0x464e98,
            "7b5604b31f5ba04420ff87257a9760607aaafbbf01ea1bd26f53f0d8a99c04bd",
        ),
        (
            "free_pop",
            0x464ea0,
            0x464eb7,
            "de388a336419841f8c222031d2f1a176d7bfa829208f11e97230e96b1da52fe7",
        ),
        (
            "release",
            0x464ec0,
            0x464ed0,
            "53197c8f2028f396f4956d73737073c1367556b2d4aefaa3ca9cc3148733f152",
        ),
        (
            "common_deleting",
            0x465030,
            0x46504c,
            "31d099d6ef2433ffa1ecf128fa63160ec25f33f9696e58e492afc6809a537c30",
        ),
        (
            "common_unlink",
            0x465050,
            0x465068,
            "1ade7cec3420ca529fbb01f1911af227039ecc0e7fc7c812ed7c9b444307bc4d",
        ),
        (
            "release_wrapper",
            0x466110,
            0x466120,
            "55dddbabb752a15ab6efaff06e7632a45c272ea37aa0e0ae06e0d50e92febe32",
        ),
    ];
    assert_eq!(
        metadata["body_guards"].as_object().unwrap().len(),
        bodies.len()
    );
    for (name, start, end, hash) in bodies {
        assert_eq!(
            metadata["body_guards"][name],
            json!({"start":start,"end":end,"sha256":hash})
        );
    }
    assert_eq!(
        metadata["native_bindings"],
        json!({
            "manager":0x700000,"pool":0x710000,"world":0x720000,
            "position":0x721000,"velocity":0x721008,"scratch":0x54ec60,
            "process_null":0x54e894,"network":0x55f448,"rng":0x55f18c,
            "phase":0x72064c,"cursor":0x54ec5c
        })
    );
    assert_eq!(
        metadata["readonly_birth_tables"],
        json!({
            "start":0x53c9d8,"bytes":0xd28,
            "sha256":"5b2b9a0c31c4faabb1a4be58240fb458724571886f69642a734395e53880009b"
        })
    );
    assert_eq!(metadata["histories"], 27);
    assert_eq!(metadata["steps"], 29);
    assert_eq!(metadata["native_body_entries"], 253);
    assert_eq!(metadata["native_body_rets"], 238);
    assert_eq!(
        metadata["admission_counts"],
        json!({
            "original_body":13,"semantic_prefix":13,"source_only":1,
            "portable_histories":26,"portable_steps":28,"source_only_steps":1
        })
    );
    assert_eq!(
        metadata["outcomes"],
        json!({
            "completed":16,"missing_process_null":11,"missing_phase":1,"audio_error":1
        })
    );
    assert_eq!(
        metadata["source_read_counts"],
        json!({
            "0048a435":7,"0048a215":11,"00489409":4,
            "00489eb2":2,"00489f3a":3,"00489fbe":2
        })
    );
    assert_eq!(
        metadata["entry_counts"],
        serde_json::from_str::<Value>(
            r#"{
        "004391b0":30,"00464e30":1,"00464e50":18,"00464ea0":29,
        "00464ec0":1,"00464ed0":31,"00464fb0":18,"00465030":1,
        "00465050":1,"00466100":31,"00466110":1,"004893a0":5,
        "00489d90":9,"0048a0f0":6,"0048a3a0":9,"0048b090":1,
        "0048f060":59,"004923c0":2
    }"#
        )
        .unwrap()
    );
    assert_eq!(
        metadata["ret_counts"],
        serde_json::from_str::<Value>(
            r#"{
        "004391b0":30,"00464e30":1,"00464e50":18,"00464ea0":29,
        "00464ec0":1,"00464ed0":30,"00464fb0":18,"00465030":1,
        "00465050":1,"00466100":30,"00466110":1,"004893a0":3,
        "00489d90":5,"0048a0f0":3,"0048a3a0":5,"0048b090":1,
        "0048f060":59,"004923c0":2
    }"#
        )
        .unwrap()
    );
    assert_eq!(
        metadata["source_only_histories"],
        json!(["foreign-free-pointer-wraps-allocation-to-null"])
    );
    assert_eq!(
        metadata["semantic_prefix_histories"],
        json!([
            "debris-0-pool-missing-null",
            "central-0-pool-missing-null",
            "splash-0-pool-missing-null",
            "trail-2-pool-missing-null",
            "trail-12-pool-missing-null",
            "trail-13-pool-missing-null",
            "trail-rng-before-missing-phase",
            "phase-scratch-before-missing-null",
            "central-audio-error-before-allocation",
            "central-queued-audio-before-missing-null",
            "same-owner-source-changes-between-requests",
            "ordinary-common-reclaim-before-missing-null",
            "protected-final-category-exhausts-to-scratch"
        ])
    );
}

fn validate_corpus(fixture: &Value) {
    let mut entries = serde_json::Map::new();
    let mut returns = serde_json::Map::new();
    let mut reads = serde_json::Map::new();
    let mut outcomes = serde_json::Map::new();
    let mut step_count = 0;
    let mut rng_writes = 0;
    for case in fixture["cases"].as_array().unwrap() {
        for step in case["steps"].as_array().unwrap() {
            for (key, target) in [("entry_counts", &mut entries), ("ret_counts", &mut returns)] {
                for (routine, count) in step[key].as_object().unwrap() {
                    let prior = target.get(routine).map_or(0, number);
                    target.insert(routine.clone(), Value::from(prior + number(count)));
                }
            }
            for read in step["source_reads"].as_array().unwrap() {
                let key = format!("{:08x}", number(&read["ip"]));
                let count = reads.get(&key).map_or(0, number);
                reads.insert(key, Value::from(count + 1));
            }
            let key = step["stopped"].as_str().unwrap_or("completed");
            let count = outcomes.get(key).map_or(0, number);
            outcomes.insert(key.to_owned(), Value::from(count + 1));
            rng_writes += number(&step["rng_writes"]);
            step_count += 1;
        }
    }
    assert_eq!(Value::Object(entries), fixture["metadata"]["entry_counts"]);
    assert_eq!(Value::Object(returns), fixture["metadata"]["ret_counts"]);
    assert_eq!(
        Value::Object(reads),
        fixture["metadata"]["source_read_counts"]
    );
    assert_eq!(Value::Object(outcomes), fixture["metadata"]["outcomes"]);
    assert_eq!((step_count, rng_writes), (29, 59));
}

#[test]
fn particle_births_read_live_null_after_allocation_in_native_histories() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/particle_birth_null_source_native.json"
    ))
    .unwrap();
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .expect("implicit-source native receipts require the supported local executable");
    let executable = FrontendExecutable::parse(&raw).unwrap();
    validate_metadata(&fixture["metadata"], &raw);
    validate_corpus(&fixture);
    let mut compared = 0;
    let mut steps = 0;
    let mut diagnostics = Vec::new();
    let mut semantic_prefixes = Vec::new();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 27);
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        if input["evidence"] == "source_only" {
            assert_eq!(name, "foreign-free-pointer-wraps-allocation-to-null");
            assert_eq!(input["foreign_free_header"], true);
            diagnostics.push(name);
            continue;
        }
        assert!(matches!(
            input["evidence"].as_str().unwrap(),
            "original_body" | "semantic_prefix"
        ));
        assert_eq!(input["foreign_free_header"], false);
        if input["evidence"] == "semantic_prefix" {
            semantic_prefixes.push(Value::from(name));
        }
        let mut process = ParticleFactoryProcessState {
            network_flag: u32::from(input["network_active"].as_bool().unwrap()),
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            // SessionState phase is a separately supplied allocation capture,
            // not ownership acquired by this retained process carrier.
            process_null_handle: input["process_null"]
                .as_u64()
                .map(|v| ActorHandle::from_raw(v as u32)),
        };
        let phase = input["phase"].as_u64().map(|v| v as u32);
        let mut pool = ProcessParticleRuntime::default();
        let mut rng = number(&input["rng"]);
        prepare(&mut pool, input, phase);
        let count = number(&input["prepare"]["count"]);
        let mut preparation = serde_json::Map::new();
        if count != 0 {
            for address in [0x4391b0, 0x464e50, 0x464ea0, 0x464ed0, 0x464fb0, 0x466100] {
                preparation.insert(format!("{address:08x}"), Value::from(count));
            }
        }
        if input["prepare"]["transfer"] == true {
            preparation.insert("00464da0".to_owned(), Value::from(1));
        }
        assert_eq!(
            case["preparation_entry_counts"],
            Value::Object(preparation.clone())
        );
        assert_eq!(case["preparation_ret_counts"], Value::Object(preparation));
        compare_state(&pool, &process, phase, rng, &case["initial"], name);
        for step in case["steps"].as_array().unwrap() {
            let invocation = &step["input"];
            for effect in invocation["patches"].as_array().unwrap() {
                patch(effect, &mut process);
            }
            let before = process;
            let mut audio = Audio {
                requests: Vec::new(),
                fail: invocation["audio_error"].as_bool().unwrap(),
            };
            let kind = invocation["kind"].as_str().unwrap();
            let result = invoke(
                kind,
                &mut pool,
                ParticleBirthRequest {
                    particle_type: number(&invocation["selector"]),
                    position: words(&input["position6"]),
                    // Splash forces null velocity; trail shares the genuine
                    // velocity/flags stack argument pair with central/debris.
                    velocity: (kind != "splash" && invocation["has_velocity"].as_bool().unwrap())
                        .then(|| words(&input["velocity6"])),
                    flags: number(&invocation["flags"]),
                    inputs: ParticleBirthInputs {
                        network_active: input["network_active"].as_bool().unwrap(),
                        phase,
                        process: &process,
                    },
                },
                executable,
                &mut rng,
                &mut audio,
            );
            match step["stopped"].as_str() {
                None => {
                    let value = match result {
                        Ok(value) => value,
                        Err(error) => panic!("{name}: {error:?}"),
                    };
                    assert_eq!(
                        identity(value),
                        step["result"],
                        "{name} full producer result"
                    );
                    let native_pointer = match value {
                        Some(ParticleId::Pool(slot)) => 0x710000 + u32::from(slot) * 72 + 8,
                        Some(ParticleId::Scratch) => 0x54ec60,
                        None => 0,
                    };
                    assert_eq!(number(&step["raw_eax"]), native_pointer);
                }
                Some("missing_process_null") => {
                    assert_eq!(
                        result.unwrap_err(),
                        Stop::Source(producer(kind, number(&invocation["selector"])))
                    );
                }
                Some("missing_phase") => assert_eq!(result.unwrap_err(), Stop::Phase),
                Some("audio_error") => assert_eq!(result.unwrap_err(), Stop::Audio),
                Some(other) => panic!("unadmitted stopped boundary {other}"),
            }
            if !step["stopped"].is_null() {
                assert!(step["result"].is_null() && step["raw_eax"].is_null());
            }
            assert_eq!(process, before, "{name} no synchronous process mutation");
            compare_audio(&audio, &step["audio"], name);
            validate_native_step(step, &process);
            compare_state(&pool, &process, phase, rng, &step["state"], name);
            steps += 1;
        }
        compared += 1;
    }
    assert_eq!(
        diagnostics,
        ["foreign-free-pointer-wraps-allocation-to-null"]
    );
    assert_eq!(
        Value::Array(semantic_prefixes),
        fixture["metadata"]["semantic_prefix_histories"]
    );
    assert_eq!((compared, steps), (26, 28));
}
