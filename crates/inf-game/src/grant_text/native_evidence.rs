//! Independent original-CRT receipt audit. No portable formatter is called here.
//! Known stack accesses were validated online by the producing checker; this
//! module audits their counts/full masked shape and the actually logged finite
//! transitions/ABI events, not a fabricated trace of every known stack access.

use std::{collections::BTreeMap, path::Path};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/grant_formatter_native.json");
// The producing checker stays private; the fixture records its SHA-256.
const FIXTURE_HASH: &str = "d20a0d9ce38bcc198966dc7bd10ec96f0c1422377243445ad0eb73ea1be3f252";
const PRODUCER_HASH: &str = "7d1a02c85b75bb2b2c6af53c2d7d11f4710a7f5dcf8c600cd72e07abe13ca73e";
const EXE_HASH: &str = "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956";
const TEXT_HASH: &str = "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2";
const STACK: u32 = 0x730000;
const RETURN: u32 = 0xf00000;
const OUTPUT: u32 = 0x722000;
const STREAM: u32 = STACK - 0x24;
const EMITTED_COUNT: u32 = STACK - 0x50;
const PREFIX: u32 = STACK - 0x52;
const ALT_NULL: u32 = 0x721300;

// Exact authenticated spans, not an inferred equivalence among CRT handlers.
const BODIES: [(&str, u32, usize, &str); 6] = [
    (
        "wrapper",
        0x511b56,
        82,
        "3f7a046b2346e33e537bbc4b050c2f5d5c20b41787b1306559ea0bbbc9698e9c",
    ),
    (
        "parser",
        0x514598,
        1825,
        "0ca20d15f97b50e67655b1771a23d9a8fd52e9f8147678f2b374e979233dd8df",
    ),
    (
        "emitter",
        0x514cd9,
        53,
        "bdd9c94b0f84d222abba49b0dd9c6ff49164736dc4a0b443c3c5514c565947fe",
    ),
    (
        "padding",
        0x514d0e,
        49,
        "d5748a1236092028ae8b199bac0bb65729e23f668a9cebb0ddbc9575a160e454",
    ),
    (
        "copy",
        0x514d3f,
        56,
        "9bfcb6c87e82e7113264f86175a304743888349564947d1146fe8ecdccf6f9ee",
    ),
    (
        "vararg",
        0x514d77,
        13,
        "639148f2f162bda27c1f66ab90d68794bac59a533c7cbf65580a2c6853ef771f",
    ),
];
const DATA: [(&str, u32, usize, &str); 9] = [
    (
        "parser_jump_words",
        0x514cb9,
        32,
        "9d14fccc0e431f3c1acab2ac6e75f5611dc86705cdeeb363ffe1288db3fd0df7",
    ),
    (
        "parser_table",
        0x529988,
        89,
        "dc1a434091c9556591738b4d80d697c4e4d915c53fc9c26667938b7cb1f48f17",
    ),
    (
        "single_format",
        0x531d7c,
        3,
        "2f3a3f9b55f5df846abf81c7d13ff74998bdda8664df664173220675150020ff",
    ),
    (
        "pair_format",
        0x537ca4,
        7,
        "a6d22300889f5f3733cb3cf72fe79644d7be1540f7994ed0704a5f2178b4a4d4",
    ),
    (
        "startup_null_pointer",
        0x545900,
        4,
        "c4daae6757559f0fc2fb698d621cad67f4886eec3a0d1cb8937e505c218b0707",
    ),
    (
        "startup_null_string",
        0x5299f4,
        7,
        "d95f1f34940146cc9423fe21df0befdaa58ecb3be841de0fd1ae06820284e44d",
    ),
    (
        "startup_ctype_pointer",
        0x545c74,
        4,
        "1f6a828c2cfae38e756a4f22fd4bf388ac0b048ef4c84262c5f632a30adf802b",
    ),
    (
        "startup_colon_high",
        0x545cf3,
        1,
        "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
    ),
    (
        "startup_space_high",
        0x545cbf,
        1,
        "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
    ),
];
const INSTRUCTIONS: [(u32, &str); 12] = [
    (0x511b77, "c745e4ffffff7f"),
    (0x511b91, "802000"),
    (0x511ba7, "c3"),
    (0x5145bd, "8b4df0"),
    (0x51460d, "834df0ff"),
    (0x514737, "8b0d745c5400"),
    (0x514743, "f644410180"),
    (0x5149ef, "8b0d00595400"),
    (0x514a01, "803800"),
    (0x514ce9, "8802"),
    (0x514d55, "0fbe06"),
    (0x514d77, "8b442404"),
];
const REGIONS: [(&str, u32, usize); 15] = [
    ("first", 0x721000, 224),
    ("second", 0x721100, 224),
    ("alternate_null", ALT_NULL, 224),
    ("output", OUTPUT, 192),
    ("output_guard", OUTPUT + 192, 16),
    ("null_pointer", 0x545900, 4),
    ("null_string", 0x5299f4, 7),
    ("ctype_pointer", 0x545c74, 4),
    ("colon_high", 0x545cf3, 1),
    ("space_high", 0x545cbf, 1),
    ("single_format", 0x531d7c, 3),
    ("pair_format", 0x537ca4, 7),
    ("parser_table", 0x529988, 89),
    ("parser_jump_words", 0x514cb9, 32),
    ("stack", STACK - 0x500, 0x540),
];
const HISTORIES: [&str; 12] = [
    "single-raw-percent-and-high-bytes",
    "single-empty-bypasses-unavailable-globals",
    "single-null-current-substitute",
    "pair-known-category-and-raw-text",
    "pair-both-null-separate-current-reads",
    "pair-empty-operands-still-classifies-literals",
    "single-exact191-output",
    "pair-exact191-including-separators",
    "current-null-substitute-binding-change-between-requests",
    "diagnostic-null-substitute-dword-unavailable",
    "diagnostic-unknown-source-byte-before-nul",
    "diagnostic-pair-ctype-pointer-unavailable",
];
const DIAGNOSTICS: [&str; 3] = [
    "diagnostic-null-substitute-dword-unavailable",
    "diagnostic-pair-ctype-pointer-unavailable",
    "diagnostic-unknown-source-byte-before-nul",
];
const STACK_COUNTS: [(u32, u32); 13] = [
    (103, 89),
    (57, 57),
    (125, 106),
    (210, 174),
    (265, 216),
    (129, 118),
    (2160, 1585),
    (2212, 1630),
    (125, 106),
    (125, 106),
    (18, 31),
    (18, 31),
    (62, 65),
];
const COUNTS: &str = r#"{
    "histories":12,"invocations":13,"bounded_completed_histories":9,"bounded_completed_invocations":10,
    "between_request_binding_change_histories":1,"between_request_binding_change_invocations":2,
    "missing_data_diagnostic_histories":3,"missing_data_diagnostic_invocations":3,
    "events":{"finite_unknown_read":36,"precision_reset":17,"read":1076,"required_read_boundary":3,
    "routine_entry":518,"routine_return":512,"supplied_effect":2,"write":440},
    "entries":{"copy":30,"emitter":430,"padding":15,"parser":13,"vararg":17,"wrapper":13},
    "returns":{"copy":30,"emitter":430,"padding":15,"parser":10,"vararg":17,"wrapper":10},
    "outcomes":{"original_body_complete":10,"missing_data_diagnostic":3},
    "unlisted_body_import_service_fallback_entries":0
}"#;

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}
fn array(value: &Value) -> &[Value] {
    value.as_array().unwrap()
}
fn masked(value: &Value) -> Vec<Option<u8>> {
    array(value)
        .iter()
        .map(|value| {
            if value.is_null() {
                None
            } else {
                Some(number(value).try_into().unwrap())
            }
        })
        .collect()
}
fn word(value: Option<u32>) -> Vec<Option<u8>> {
    value.map_or_else(
        || vec![None; 4],
        |value| value.to_le_bytes().map(Some).to_vec(),
    )
}
fn dword(bytes: &[Option<u8>]) -> u32 {
    u32::from_le_bytes(
        bytes
            .iter()
            .map(|byte| byte.unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
    )
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap())
        .collect()
}
fn region(name: &str) -> (u32, usize) {
    REGIONS
        .iter()
        .find(|(candidate, _, _)| *candidate == name)
        .map(|(_, address, count)| (*address, *count))
        .unwrap()
}
fn body(name: &str) -> (u32, usize) {
    BODIES
        .iter()
        .find(|(candidate, _, _, _)| *candidate == name)
        .map(|(_, address, count, _)| (*address, *count))
        .unwrap()
}
fn format_address(mode: &str) -> u32 {
    match mode {
        "single" => 0x531d7c,
        "pair" => 0x537ca4,
        _ => panic!("unadmitted mode"),
    }
}
fn argument_pointer(argument: &Value) -> u32 {
    if argument.is_null() {
        0
    } else {
        match argument.as_str().unwrap() {
            "first" => 0x721000,
            "second" => 0x721100,
            _ => panic!("unadmitted source"),
        }
    }
}

struct Original(Vec<u8>);
impl Original {
    fn load() -> Option<Self> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe");
        let Ok(raw) = std::fs::read(path) else {
            eprintln!("retail Infest.exe missing; copy the game into retail/ to run this test");
            return None;
        };
        assert_eq!(digest(&raw), EXE_HASH);
        inf_formats::FrontendExecutable::parse(&raw).unwrap();
        assert_eq!(
            u32::from_le_bytes(raw[0x3c..0x40].try_into().unwrap()),
            0xe8
        );
        assert_eq!(&raw[0x1e0..0x1e8], b".text\0\0\0");
        let fields: Vec<u32> = raw[0x1e8..0x1f8]
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(fields, [0x11d475, 0x1000, 0x11e000, 0x1000]);
        assert_eq!(digest(&raw[0x1000..0x11f000]), TEXT_HASH);
        Some(Self(raw))
    }
    // These finite guarded spans have this file mapping in the exact hashed PE.
    fn at(&self, address: u32, count: usize) -> &[u8] {
        let offset = (address.checked_sub(0x400000).unwrap()) as usize;
        &self.0[offset..offset + count]
    }
    fn guard(&self, value: &Value, address: u32, count: usize, hash: &str) {
        assert_eq!(value["address"], address);
        assert_eq!(value["bytes"], count);
        assert_eq!(value["sha256"], hash);
        assert_eq!(digest(self.at(address, count)), hash);
    }
    fn call_target(&self, return_ip: u32) -> u32 {
        let code = self.at(return_ip.checked_sub(5).unwrap(), 5);
        assert_eq!(code[0], 0xe8);
        return_ip.wrapping_add(u32::from_le_bytes(code[1..].try_into().unwrap()))
    }
}

fn metadata(metadata: &Value, original: &Original) {
    assert_eq!(metadata["executable_sha256"], EXE_HASH);
    assert_eq!(metadata["text_sha256"], TEXT_HASH);
    assert_eq!(metadata["text_address"], 0x401000);
    assert_eq!(metadata["fpcw"], 0x027f);
    assert_eq!(
        metadata["execution"],
        "six_unchanged_CRT_bodies_for_two_fixed_formats"
    );
    for (key, profiles) in [
        ("bodies", BODIES.as_slice()),
        ("data_guards", DATA.as_slice()),
    ] {
        assert_eq!(array(&metadata[key]).len(), profiles.len());
        for (guard, &(name, address, count, hash)) in array(&metadata[key]).iter().zip(profiles) {
            assert_eq!(guard["name"], name);
            original.guard(guard, address, count, hash);
        }
    }
    assert_eq!(
        array(&metadata["instruction_guards"]).len(),
        INSTRUCTIONS.len()
    );
    for (guard, (address, code)) in array(&metadata["instruction_guards"])
        .iter()
        .zip(INSTRUCTIONS)
    {
        let bytes = hex(code);
        original.guard(guard, address, bytes.len(), &digest(&bytes));
    }
    original.guard(
        &metadata["static_unreached_fallback"],
        0x514480,
        280,
        "fb92821e6525c0711482e4d8fa1b8b7e9213119a5562780951c3dc19b0753802",
    );
    assert_eq!(
        metadata["native_regions"].as_object().unwrap().len(),
        REGIONS.len()
    );
    for (name, address, count) in REGIONS {
        assert_eq!(
            metadata["native_regions"][name],
            json!({"address":address,"bytes":count})
        );
    }
    assert_eq!(
        metadata["counts"],
        serde_json::from_str::<Value>(COUNTS).unwrap()
    );
    assert_eq!(metadata["diagnostics"], json!(DIAGNOSTICS));
    assert_eq!(
        metadata["finite_unknown_reads"],
        json!([
            {"ip":0x5145bd,"instruction":"8b4df0","policy":"dead_first_percent_precision_load"},
            {"ip":0x51460d,"instruction":"834df0ff","policy":"RMW_unknown_read_then_known_FFFFFFFF_write"},
            {"ip":0x511b91,"instruction":"802000","policy":"RMW_unknown_output_read_then_known_zero_write"}
        ])
    );
    assert_eq!(
        metadata["abi"],
        json!({"format_modes":["single","pair"],
        "return_contract":"full_DWORD_emitted_count","ret_pop_bytes":0,"direct_stack_delta":4,
        "caller_argument_cleanup":"outside_direct_request"})
    );
    assert_eq!(metadata["guard_file_mapping"], "VA-400000 equals file offset only for these file-backed spans in the exact authenticated supported PE; not a generic mapping contract");
    assert_eq!(metadata["stack_scope"], "known stack accesses validated during execution; full masked stack retained; only finite unknown reads/reset events and authentic entry/RET ABI cadence logged");
    assert_eq!(
        metadata["bounded_output"],
        "complete emitted bytes <=191 plus finalNUL in192; INT_MAX original stream, not truncation"
    );
    assert_eq!(metadata["scopes"], json!([
        "initial stable known strings through firstNUL; unknown tail unconsumed",
        "current545900 loaded separately for eachNULL argument; knownnonnull bypasses",
        "current545C74 and two high classification bytes only on pair literals; bit80 clear",
        "original imports/services/fallback entries0; inherited pool preparation outside recorded counts",
        "between-request known substitute binding change is supplied setup; current semantic owner represents it without native pointer or original writer parity",
        "no portable formatter result/state proof or caller/grant/message attachment yet"
    ]));
    assert_eq!(
        metadata["exclusions"],
        json!([
            "wide_number_float_generic_formats",
            "destination_source_global_format_aliases",
            "locale_high_bit80_branch",
            "unknown_unterminated_or_unmapped_strings_outside_named_diagnostics",
            "complete_output>=192_or_portable_partial_buffer_parity",
            "global_immutability_or_all_CRT_startup",
            "caller_probes_H0_grant_mutation_message_enabled_UI",
            "hardware_fault_claims"
        ])
    );
}

type Storage = BTreeMap<String, Vec<Option<u8>>>;
fn put(storage: &mut Storage, name: &str, offset: usize, bytes: &[Option<u8>]) {
    storage.get_mut(name).unwrap()[offset..offset + bytes.len()].copy_from_slice(bytes);
}
fn initial(input: &Value, original: &Original) -> Storage {
    let mut storage: Storage = REGIONS
        .iter()
        .map(|(name, _, count)| (name.to_string(), vec![None; *count]))
        .collect();
    for name in [
        "null_string",
        "single_format",
        "pair_format",
        "parser_table",
        "parser_jump_words",
    ] {
        let (address, count) = region(name);
        put(
            &mut storage,
            name,
            0,
            &original
                .at(address, count)
                .iter()
                .copied()
                .map(Some)
                .collect::<Vec<_>>(),
        );
    }
    for name in ["first", "second"] {
        put(&mut storage, name, 0, &masked(&input[name]));
    }
    for name in ["null_pointer", "ctype_pointer"] {
        put(
            &mut storage,
            name,
            0,
            &word(if input[name].is_null() {
                None
            } else {
                Some(number(&input[name]))
            }),
        );
    }
    for name in ["colon_high", "space_high"] {
        put(
            &mut storage,
            name,
            0,
            &[if input[name].is_null() {
                None
            } else {
                Some(number(&input[name]).try_into().unwrap())
            }],
        );
    }
    put(
        &mut storage,
        "output_guard",
        0,
        &(0xa0..0xb0).map(Some).collect::<Vec<_>>(),
    );
    storage
}
fn snapshot(value: &Value) -> Storage {
    assert_eq!(value.as_object().unwrap().len(), REGIONS.len());
    REGIONS
        .iter()
        .map(|(name, _, count)| {
            let bytes = masked(&value[*name]);
            assert_eq!(bytes.len(), *count);
            (name.to_string(), bytes)
        })
        .collect()
}

#[derive(Clone)]
struct Frame {
    name: String,
    stack: u32,
    return_ip: u32,
    args: Vec<u32>,
    writes: u32,
    pending_byte: Option<u8>,
}

fn read_ip(name: &str, ip: u32) -> &'static str {
    match name {
        "first" | "second" | "null_string" | "alternate_null" => match ip {
            0x514a01 => "parser",
            0x514d55 => "copy",
            _ => panic!("unadmitted source read"),
        },
        "single_format" | "pair_format" => {
            assert!([0x5145a9, 0x514ca3].contains(&ip));
            "parser"
        }
        "parser_table" => {
            assert!([0x5145e2, 0x5145ef].contains(&ip));
            "parser"
        }
        "parser_jump_words" => {
            assert_eq!(ip, 0x514606);
            "parser"
        }
        "null_pointer" => {
            assert_eq!(ip, 0x5149ef);
            "parser"
        }
        "ctype_pointer" => {
            assert_eq!(ip, 0x514737);
            "parser"
        }
        "colon_high" | "space_high" => {
            assert_eq!(ip, 0x514743);
            "parser"
        }
        "output" => {
            assert_eq!(ip, 0x511b91);
            "wrapper"
        }
        _ => panic!("unadmitted recorded read"),
    }
}

fn timeline(
    events: &[Value],
    request: &Value,
    storage: &mut Storage,
    original: &Original,
    complete: bool,
) {
    let mut frames: Vec<Frame> = Vec::new();
    let mut allowance = 0;
    let mut resets = 0;
    let mut varargs = 0;
    let mut output_length = 0;
    let arguments = array(&request["arguments"]);
    for (index, event) in events.iter().enumerate() {
        match event["kind"].as_str().unwrap() {
            "supplied_effect" => {
                assert!(frames.is_empty());
                assert!(index < array(&request["patches"]).len());
            }
            "routine_entry" => {
                let name = event["function"].as_str().unwrap();
                let args: Vec<u32> = array(&event["arguments"]).iter().map(number).collect();
                let stack = number(&event["entry_stack"]);
                let return_ip = number(&event["return_ip"]);
                let address = body(name).0;
                assert_eq!(event["address"], address);
                assert_eq!(event["depth"], frames.len() + 1);
                assert!((STACK - 0x500..STACK + 0x40).contains(&stack));
                if let Some(parent) = frames.last() {
                    assert!(stack < parent.stack);
                    assert_eq!(original.call_target(return_ip), address);
                    let (base, count) = body(&parent.name);
                    assert!((base..base + count as u32).contains(&return_ip));
                } else {
                    assert_eq!(name, "wrapper");
                    assert_eq!(stack, STACK);
                    assert_eq!(return_ip, RETURN);
                }
                match name {
                    "wrapper" => {
                        let mut expected =
                            vec![OUTPUT, format_address(request["mode"].as_str().unwrap())];
                        expected.extend(arguments.iter().map(argument_pointer));
                        assert_eq!(args, expected);
                    }
                    "parser" => {
                        assert_eq!(
                            args,
                            [
                                STREAM,
                                format_address(request["mode"].as_str().unwrap()),
                                STACK + 12
                            ]
                        );
                        assert_eq!(stack, STACK - 0x38);
                    }
                    "vararg" => {
                        assert_eq!(args, [STACK - 0x2c]);
                        assert_eq!(resets, varargs + 1);
                        varargs += 1;
                    }
                    "padding" => {
                        assert_eq!(args.len(), 4);
                        assert_eq!(args[0], 32);
                        assert!((args[1] as i32) <= 0);
                        assert_eq!(args[2..], [STREAM, EMITTED_COUNT]);
                    }
                    "copy" => {
                        assert_eq!(args.len(), 4);
                        assert_eq!(args[2..], [STREAM, EMITTED_COUNT]);
                        assert!(args[1] <= 191);
                        assert!(
                            args[0] == PREFIX && args[1] == 0
                                || REGIONS[..3]
                                    .iter()
                                    .any(|(_, address, _)| *address == args[0])
                                || args[0] == 0x5299f4
                        );
                    }
                    "emitter" => {
                        assert_eq!(args.len(), 3);
                        assert_eq!(args[1..], [STREAM, EMITTED_COUNT]);
                        let parent = frames.last_mut().unwrap();
                        assert!(["copy", "parser"].contains(&parent.name.as_str()));
                        let byte = parent
                            .pending_byte
                            .take()
                            .expect("source/literal read must precede its emitter");
                        assert_eq!(args[0], i32::from(byte as i8) as u32);
                    }
                    _ => unreachable!(),
                }
                frames.push(Frame {
                    name: name.to_owned(),
                    stack,
                    return_ip,
                    args,
                    writes: 0,
                    pending_byte: None,
                });
            }
            "routine_return" => {
                let frame = frames.pop().unwrap();
                let (address, count) = body(&frame.name);
                assert_eq!(event["function"], frame.name);
                assert_eq!(event["ip"], address + count as u32 - 1);
                assert_eq!(original.at(number(&event["ip"]), 1), [0xc3]);
                assert_eq!(event["return_ip"], frame.return_ip);
                assert_eq!(event["stack_after"], frame.stack + 4);
                assert_eq!(event["pop_bytes"], 0);
                assert!(frame.pending_byte.is_none());
                match frame.name.as_str() {
                    "vararg" => assert_eq!(
                        number(&event["raw_eax"]),
                        argument_pointer(&arguments[(varargs - 1) as usize])
                    ),
                    "emitter" => {
                        assert_eq!(frame.writes, 1);
                        assert_eq!(number(&event["raw_eax"]), EMITTED_COUNT);
                    }
                    "copy" => assert_eq!(number(&event["raw_eax"]), 0),
                    "padding" => assert_eq!(number(&event["raw_eax"]), frame.args[1]),
                    "parser" => assert_eq!(number(&event["raw_eax"]), output_length),
                    "wrapper" => assert_eq!(number(&event["raw_eax"]), output_length - 1),
                    _ => unreachable!(),
                }
            }
            "read" => {
                let name = event["region"].as_str().unwrap();
                let offset = number(&event["offset"]) as usize;
                let count = number(&event["bytes"]) as usize;
                let ip = number(&event["ip"]);
                assert_eq!(frames.last().unwrap().name, read_ip(name, ip));
                assert_eq!(
                    masked(&event["data"]),
                    storage[name][offset..offset + count]
                );
                assert_eq!(
                    count,
                    if ["null_pointer", "ctype_pointer", "parser_jump_words"].contains(&name) {
                        4
                    } else {
                        1
                    }
                );
                if ip == 0x514d55 {
                    let frame = frames.last_mut().unwrap();
                    assert_eq!(
                        region(name).0 + offset as u32,
                        frame.args[0] + offset as u32
                    );
                    assert!(offset < frame.args[1] as usize);
                    assert!(frame.pending_byte.is_none());
                    frame.pending_byte = masked(&event["data"])[0];
                }
                if ["colon_high", "space_high"].contains(&name) {
                    assert_eq!(offset, 0);
                    assert_eq!(masked(&event["data"])[0].unwrap() & 0x80, 0);
                    let frame = frames.last_mut().unwrap();
                    assert!(frame.pending_byte.is_none());
                    frame.pending_byte = Some(if name == "colon_high" { b':' } else { b' ' });
                }
                if masked(&event["data"]).iter().any(Option::is_none) {
                    assert!(matches!(
                        events[index + 1]["kind"].as_str().unwrap(),
                        "finite_unknown_read" | "required_read_boundary"
                    ));
                }
            }
            "write" => {
                assert_eq!(event["region"], "output");
                assert_eq!(event["bytes"], 1);
                assert_eq!(event["offset"], output_length);
                let bytes = masked(&event["data"]);
                assert_eq!(bytes.len(), 1);
                let byte = bytes[0].unwrap();
                let frame = frames.last_mut().unwrap();
                match number(&event["ip"]) {
                    0x514ce9 => {
                        assert_eq!(frame.name, "emitter");
                        assert_eq!(byte, frame.args[0] as u8);
                        frame.writes += 1;
                    }
                    0x511b91 => {
                        assert_eq!(frame.name, "wrapper");
                        assert_eq!(byte, 0);
                        assert_eq!(allowance, 3);
                        assert_eq!(index + 2, events.len());
                    }
                    _ => panic!("unadmitted output write"),
                }
                put(storage, "output", output_length as usize, &bytes);
                output_length += 1;
            }
            "finite_unknown_read" => {
                let (ip, name, count, expected) = match allowance {
                    0 => (0x5145bd, "stack", 4, "dead_initial_precision_load"),
                    1 => (0x51460d, "stack", 4, "deterministic_precision_or_read"),
                    2 => (0x511b91, "output", 1, "deterministic_final_nul_and_read"),
                    _ => panic!("extra finite allowance"),
                };
                assert_eq!(event["ip"], ip);
                assert_eq!(event["region"], name);
                assert_eq!(event["bytes"], count);
                assert_eq!(event["allowance"], expected);
                assert_eq!(masked(&event["data"]), vec![None; count]);
                assert_eq!(
                    event["offset"],
                    if name == "stack" {
                        0x4b4
                    } else {
                        output_length
                    }
                );
                assert_eq!(
                    frames.last().unwrap().name,
                    if name == "stack" { "parser" } else { "wrapper" }
                );
                if name == "stack" {
                    assert_eq!(resets, 0);
                } else {
                    assert_eq!(events[index - 1]["kind"], "read");
                }
                allowance += 1;
            }
            "precision_reset" => {
                assert_eq!(event["ip"], 0x51460d);
                assert_eq!(event["region"], "stack");
                assert_eq!(event["offset"], 0x4b4);
                assert_eq!(event["bytes"], 4);
                assert_eq!(masked(&event["data"]), vec![Some(255); 4]);
                assert_eq!(frames.last().unwrap().name, "parser");
                assert_eq!(allowance, 2);
                assert_eq!(resets, varargs);
                resets += 1;
            }
            "required_read_boundary" => {
                assert!(!complete);
                assert_eq!(index + 1, events.len());
                assert_eq!(
                    event["scope"],
                    "intentional_missing_data_stop_before_numeric_consumption_no_fault_claim"
                );
                let read = &events[index - 1];
                assert_eq!(read["kind"], "read");
                for key in ["region", "offset", "bytes", "ip"] {
                    assert_eq!(event[key], read[key]);
                }
                assert!(masked(&read["data"]).iter().any(Option::is_none));
            }
            _ => panic!("unadmitted native event"),
        }
    }
    assert_eq!(resets, varargs);
    assert_eq!(allowance, if complete { 3 } else { 2 });
    if complete {
        assert!(frames.is_empty());
        assert_eq!(varargs, arguments.len() as u32);
    } else {
        assert_eq!(
            frames
                .iter()
                .map(|frame| frame.name.as_str())
                .collect::<Vec<_>>(),
            ["wrapper", "parser"]
        );
    }
}

fn sources(events: &[Value], request: &Value, storage: &Storage, complete: bool) {
    let format_name = if request["mode"] == "single" {
        "single_format"
    } else {
        "pair_format"
    };
    let format_reads: Vec<&Value> = events
        .iter()
        .filter(|event| {
            event["kind"] == "read"
                && ["single_format", "pair_format"]
                    .iter()
                    .any(|name| event["region"] == *name)
        })
        .collect();
    let expected = if complete {
        storage[format_name].len()
    } else if request["mode"] == "single" {
        2
    } else {
        3
    };
    assert_eq!(format_reads.len(), expected);
    for (offset, event) in format_reads.iter().enumerate() {
        assert_eq!(event["region"], format_name);
        assert_eq!(event["offset"], offset);
        assert_eq!(event["ip"], if offset == 0 { 0x5145a9 } else { 0x514ca3 });
    }
    let varargs: Vec<usize> = events
        .iter()
        .enumerate()
        .filter_map(|(i, event)| {
            (event["kind"] == "routine_entry" && event["function"] == "vararg").then_some(i)
        })
        .collect();
    let mut last_copy = 0;
    let mut argument_bytes = Vec::new();
    let mut copy_finishes = Vec::new();
    for (index, argument) in array(&request["arguments"])
        .iter()
        .take(varargs.len())
        .enumerate()
    {
        let begin = varargs[index];
        let end = varargs.get(index + 1).copied().unwrap_or(events.len());
        assert!(begin > last_copy);
        let window = &events[begin..end];
        let null_reads: Vec<usize> = window
            .iter()
            .enumerate()
            .filter_map(|(i, event)| {
                (event["kind"] == "read" && event["region"] == "null_pointer").then_some(begin + i)
            })
            .collect();
        let name = if argument.is_null() {
            assert_eq!(null_reads.len(), 1);
            let event = &events[null_reads[0]];
            assert_eq!(event["offset"], 0);
            assert_eq!(event["bytes"], 4);
            assert_eq!(event["ip"], 0x5149ef);
            if !complete && storage["null_pointer"].iter().any(Option::is_none) {
                continue;
            }
            match dword(&storage["null_pointer"]) {
                0x5299f4 => "null_string",
                ALT_NULL => "alternate_null",
                _ => panic!("unadmitted substitute pointer"),
            }
        } else {
            assert!(null_reads.is_empty());
            argument.as_str().unwrap()
        };
        let scans: Vec<usize> = (begin..end)
            .filter(|i| {
                events[*i]["kind"] == "read"
                    && events[*i]["region"] == name
                    && events[*i]["ip"] == 0x514a01
            })
            .collect();
        let copied: Vec<usize> = (begin..end)
            .filter(|i| {
                events[*i]["kind"] == "read"
                    && events[*i]["region"] == name
                    && events[*i]["ip"] == 0x514d55
            })
            .collect();
        if !complete
            && scans
                .last()
                .is_some_and(|i| masked(&events[*i]["data"]).iter().any(Option::is_none))
        {
            assert!(copied.is_empty());
            assert_eq!(scans.len(), 2);
            assert_eq!(events[scans[0]]["data"], json!([65]));
            assert_eq!(events[scans[1]]["data"], json!([null]));
            continue;
        }
        let nul = storage[name]
            .iter()
            .position(|value| *value == Some(0))
            .unwrap();
        assert!(storage[name][..nul].iter().all(Option::is_some));
        assert_eq!(scans.len(), nul + 1);
        assert_eq!(copied.len(), nul);
        for (offset, &i) in scans.iter().enumerate() {
            assert_eq!(events[i]["offset"], offset);
            assert_eq!(
                masked(&events[i]["data"]),
                storage[name][offset..offset + 1]
            );
        }
        for (offset, &i) in copied.iter().enumerate() {
            assert_eq!(events[i]["offset"], offset);
            assert_eq!(
                masked(&events[i]["data"]),
                storage[name][offset..offset + 1]
            );
        }
        if argument.is_null() {
            assert!(null_reads[0] < scans[0]);
        }
        let copy = (begin..end)
            .find(|i| {
                events[*i]["kind"] == "routine_entry"
                    && events[*i]["function"] == "copy"
                    && events[*i]["arguments"]
                        == json!([region(name).0, nul, STREAM, EMITTED_COUNT])
            })
            .unwrap();
        let finish = (copy + 1..end)
            .find(|i| events[*i]["kind"] == "routine_return" && events[*i]["function"] == "copy")
            .unwrap();
        assert!(*scans.last().unwrap() < copy && copy < finish);
        if let Some(first) = copied.first() {
            assert!(copy < *first && *copied.last().unwrap() < finish);
        }
        last_copy = finish;
        copy_finishes.push(finish);
        argument_bytes.push(
            storage[name][..nul]
                .iter()
                .map(|value| value.unwrap())
                .collect::<Vec<_>>(),
        );
    }
    let literals: Vec<usize> = events
        .iter()
        .enumerate()
        .filter_map(|(i, event)| {
            (event["kind"] == "read"
                && ["colon_high", "space_high"]
                    .iter()
                    .any(|name| event["region"] == *name))
            .then_some(i)
        })
        .collect();
    let pointers: Vec<usize> = events
        .iter()
        .enumerate()
        .filter_map(|(i, event)| {
            (event["kind"] == "read" && event["region"] == "ctype_pointer").then_some(i)
        })
        .collect();
    if request["mode"] == "single" {
        assert!(literals.is_empty() && pointers.is_empty());
    } else if complete {
        assert_eq!(literals.len(), 2);
        assert_eq!(pointers.len(), 2);
        let mut previous = copy_finishes[0];
        for (index, name) in ["colon_high", "space_high"].iter().enumerate() {
            let pointer = pointers[index];
            let literal = literals[index];
            assert!(previous < pointer && pointer + 1 == literal && literal < varargs[1]);
            assert_eq!(events[pointer]["data"], json!(storage["ctype_pointer"]));
            assert_eq!(dword(&storage["ctype_pointer"]), 0x545c7e);
            assert_eq!(events[literal]["region"], *name);
            assert_eq!(events[literal]["offset"], 0);
            assert_eq!(events[literal]["bytes"], 1);
            assert_eq!(storage[*name][0].unwrap() & 0x80, 0);
            let offset = argument_bytes[0].len() + index;
            let write = (literal + 1..varargs[1])
                .find(|i| events[*i]["kind"] == "write" && events[*i]["offset"] == offset)
                .unwrap();
            assert_eq!(
                events[write]["data"],
                json!([if index == 0 { 58 } else { 32 }])
            );
            previous = write;
        }
    } else {
        assert!(literals.is_empty());
        assert_eq!(pointers.len(), 1);
        assert!(copy_finishes[0] < pointers[0]);
        assert_eq!(events[pointers[0]]["data"], json!([null, null, null, null]));
    }
    if complete {
        let mut expected = argument_bytes[0].clone();
        if request["mode"] == "pair" {
            expected.extend_from_slice(b": ");
            expected.extend_from_slice(&argument_bytes[1]);
        }
        expected.push(0);
        assert_eq!(
            storage["output"][..expected.len()],
            expected.iter().copied().map(Some).collect::<Vec<_>>()
        );
        assert!(storage["output"][expected.len()..]
            .iter()
            .all(Option::is_none));
    }
}

pub(super) fn load_and_validate_fixture() -> Option<Value> {
    assert_eq!(digest(FIXTURE), FIXTURE_HASH);
    assert_eq!(FIXTURE.len(), 582195);
    let fixture: Value = serde_json::from_slice(FIXTURE).unwrap();
    let original = Original::load()?;
    metadata(&fixture["metadata"], &original);
    assert_eq!(array(&fixture["cases"]).len(), HISTORIES.len());
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut entries: BTreeMap<String, u32> = BTreeMap::new();
    let mut returns: BTreeMap<String, u32> = BTreeMap::new();
    let mut outcomes: BTreeMap<String, u32> = BTreeMap::new();
    let mut call = 0;
    for (case, name) in array(&fixture["cases"]).iter().zip(HISTORIES) {
        let input = &case["input"];
        assert_eq!(input["name"], name);
        assert_eq!(case["instruction_bytes_unchanged"], true);
        let diagnostic = DIAGNOSTICS.contains(&name);
        assert_eq!(
            case["evidence"],
            if diagnostic {
                "missing_data_diagnostic"
            } else if name == HISTORIES[8] {
                "bounded_current_binding_change"
            } else {
                "bounded_original_body"
            }
        );
        let mut storage = initial(input, &original);
        assert_eq!(snapshot(&case["initial_state"]), storage);
        let requests = array(&input["steps"]);
        let receipts = array(&case["steps"]);
        assert_eq!(requests.len(), if name == HISTORIES[8] { 2 } else { 1 });
        assert_eq!(requests.len(), receipts.len());
        for (step, (request, receipt)) in requests.iter().zip(receipts).enumerate() {
            let complete = !diagnostic;
            assert_eq!(
                receipt["outcome"],
                if complete {
                    "original_body_complete"
                } else {
                    "missing_data_diagnostic"
                }
            );
            assert!(receipt["invalid"].is_null());
            *outcomes
                .entry(receipt["outcome"].as_str().unwrap().to_owned())
                .or_default() += 1;
            let mode = request["mode"].as_str().unwrap();
            format_address(mode);
            assert_eq!(
                request["saved_registers"],
                json!({"ebx":0x12345678u32,"ebp":0x89674523u32,
                "esi":0x76543210u32,"edi":0xfedcba98u32})
            );
            assert_eq!(
                array(&request["arguments"]).len(),
                if mode == "single" { 1 } else { 2 }
            );
            let patches = array(&request["patches"]);
            assert_eq!(
                patches.len(),
                if name == HISTORIES[8] && step == 1 {
                    2
                } else {
                    0
                }
            );
            if !patches.is_empty() {
                let mut alternate: Vec<Option<u8>> =
                    b"<none>\0".iter().copied().map(Some).collect();
                alternate.extend([None; 8]);
                assert_eq!(
                    patches[0],
                    json!({"region":"alternate_null","offset":0,"bytes":alternate})
                );
                assert_eq!(
                    patches[1],
                    json!({"region":"null_pointer","offset":0,"bytes":word(Some(ALT_NULL))})
                );
            }
            for patch in patches {
                put(
                    &mut storage,
                    patch["region"].as_str().unwrap(),
                    number(&patch["offset"]) as usize,
                    &masked(&patch["bytes"]),
                );
            }
            storage.get_mut("output").unwrap().fill(None);
            let events = array(&receipt["events"]);
            for (i, patch) in patches.iter().enumerate() {
                assert_eq!(
                    events[i],
                    json!({"kind":"supplied_effect","patch":patch,"scope":"declared_between_requests_not_original_writer_execution"})
                );
            }
            assert_eq!(events[patches.len()]["kind"], "routine_entry");
            timeline(events, request, &mut storage, &original, complete);
            sources(events, request, &storage, complete);
            let actual = snapshot(&receipt["state"]);
            for (name, bytes) in &storage {
                if name != "stack" {
                    assert_eq!(&actual[name], bytes, "{name}");
                }
            }
            let stack = &actual["stack"];
            assert_eq!(&stack[0x4b4..0x4b8], word(Some(u32::MAX)));
            let mut carrier = word(Some(RETURN));
            carrier.extend(word(Some(OUTPUT)));
            carrier.extend(word(Some(format_address(mode))));
            for argument in array(&request["arguments"]) {
                carrier.extend(word(Some(argument_pointer(argument))));
            }
            assert_eq!(&stack[0x500..0x500 + carrier.len()], carrier);
            assert_eq!(
                receipt["validated_stack_accesses"],
                json!({"reads":STACK_COUNTS[call].0,"writes":STACK_COUNTS[call].1})
            );
            if complete {
                let length = number(&receipt["result"]);
                assert!(length <= 191);
                assert_eq!(receipt["final_stack"], STACK + 4);
                assert!(receipt["stop"].is_null() && receipt["stop_ip"].is_null());
                assert_eq!(number(&events.last().unwrap()["raw_eax"]), length);
                for (offset, value) in [
                    (0x4dc, OUTPUT + length),
                    (0x4e0, 0x7ffffffe - length),
                    (0x4e4, OUTPUT),
                    (0x4e8, 0x42),
                ] {
                    assert_eq!(&stack[offset..offset + 4], word(Some(value)));
                }
                assert_eq!(
                    actual["output"]
                        .iter()
                        .position(|value| *value == Some(0))
                        .unwrap(),
                    length as usize
                );
            } else {
                assert!(receipt["result"].is_null());
                let (ip, region) = match name {
                    "diagnostic-null-substitute-dword-unavailable" => (0x5149ef, "null_pointer"),
                    "diagnostic-unknown-source-byte-before-nul" => (0x514a01, "first"),
                    "diagnostic-pair-ctype-pointer-unavailable" => (0x514737, "ctype_pointer"),
                    _ => unreachable!(),
                };
                assert_eq!(receipt["stop_ip"], ip);
                assert_eq!(receipt["stop"], format!("unknown_{region}"));
                assert_eq!(events.last().unwrap()["kind"], "required_read_boundary");
                assert!((STACK - 0x500..STACK).contains(&number(&receipt["final_stack"])));
            }
            for event in events {
                let kind = event["kind"].as_str().unwrap();
                *counts.entry(kind.to_owned()).or_default() += 1;
                if kind == "routine_entry" {
                    *entries
                        .entry(event["function"].as_str().unwrap().to_owned())
                        .or_default() += 1;
                }
                if kind == "routine_return" {
                    *returns
                        .entry(event["function"].as_str().unwrap().to_owned())
                        .or_default() += 1;
                }
            }
            storage = actual;
            call += 1;
        }
    }
    assert_eq!(call, 13);
    for (key, value) in [
        ("events", counts),
        ("entries", entries),
        ("returns", returns),
        ("outcomes", outcomes),
    ] {
        assert_eq!(fixture["metadata"]["counts"][key], json!(value));
    }
    Some(fixture)
}

#[test]
fn fixed_grant_formatter_native_protocol_and_masked_custody() {
    load_and_validate_fixture();
}
