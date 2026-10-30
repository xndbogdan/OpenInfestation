//! Existing original-CPU receipts and actual completed formatter parity.
//!
//! The native audit below checks serialized backing/events and exact producer
//! provenance. Known stack accesses were checked online by that producer;
//! their full trace is not serialized here. Rust calls compare only complete
//! result/current input custody, with no observer, native address or ABI claim.

use std::{collections::BTreeMap, collections::BTreeSet, path::Path};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{format_profile_name, FormattedProfileName, ProfileNameClassification};

const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/profile_name_format_native.json");
// The producing checker stays private; the fixture records its SHA-256.
const FIXTURE_HASH: &str = "84b52d4a662dd13370e4944440fcf4ce7853d8f92bf79cc90a8ef322c1b100d1";
const PRODUCER_HASH: &str = "31b22a9a9c704dc379c7cb3d3b443cfee750499a5490c8e8eb669087d2278c85";
const EXE_HASH: &str = "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956";
const TEXT_HASH: &str = "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2";
const STACK: u32 = 0x730000;
const RETURN: u32 = 0xf00000;
const FORMAT: u32 = 0x721000;
const OUTPUT: u32 = 0x722000;
const STARTUP_TABLE: u32 = 0x545c7e;
const ALTERNATE_TABLE: u32 = 0x723000;

const COMPLETED: [&str; 15] = [
    "empty-bypasses-all-current-classification",
    "one-literal",
    "literal-prefix-repeated-dead-precision",
    "raw80-ff-unchanged",
    "raw80-ff-high7f-unchanged",
    "escaped-percent",
    "repeated-escaped-percent",
    "literal-prefix-before-escaped-percent",
    "escaped-percent-count-drives-cursor",
    "exact31-output",
    "unused-classification-bytes-unavailable",
    "high7f-does-not-select-bit80",
    "known-destination-tail-preserved",
    "current-binding-change-between-requests",
    "format-change-retains-earlier-output-tail",
];

// Explicit native-only stopped scopes, excluded before any typed preparation.
// Prefixes are observations, not expected values fed to the actual formatter.
const DIAGNOSTICS: [(&str, u32, &str, &[u8]); 12] = [
    (
        "diagnostic-first-format-byte-unavailable",
        0x5145a9,
        "unknown_format",
        b"",
    ),
    (
        "diagnostic-format-byte-after-prefix-unavailable",
        0x514ca3,
        "unknown_format",
        b"A",
    ),
    (
        "diagnostic-current-ctype-binding-unavailable",
        0x514737,
        "unknown_ctype_pointer",
        b"",
    ),
    (
        "diagnostic-classification-after-prefix-unavailable",
        0x514743,
        "unknown_ctype_words",
        b"A",
    ),
    (
        "diagnostic-escaped-percent-classification-unavailable",
        0x514743,
        "unknown_ctype_words",
        b"A",
    ),
    (
        "diagnostic-bit80-lead-before-missing-trail",
        0x51475a,
        "unknown_format",
        b"A\x81",
    ),
    (
        "diagnostic-bit80-known-trail-frontier",
        0x51475a,
        "bit80_second_byte_boundary",
        b"A\x81",
    ),
    (
        "diagnostic-bit80-nul-trail-before-next-missing-format",
        0x514ca3,
        "unknown_format",
        b"A\x81\0",
    ),
    (
        "diagnostic-output32-frontier-retains31",
        0x514ce9,
        "complete_output_limit",
        b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
    ),
    (
        "diagnostic-conversion-after-prefix",
        0x51477b,
        "unsupported_conversion",
        b"A",
    ),
    (
        "diagnostic-percent-n-after-prefix",
        0x51477b,
        "unsupported_conversion",
        b"A",
    ),
    (
        "diagnostic-width-star-after-prefix",
        0x514d77,
        "unowned_vararg_body",
        b"A",
    ),
];

type Backing = BTreeMap<String, Vec<Option<u8>>>;

fn array(value: &Value) -> &[Value] {
    value.as_array().expect("fixture array")
}

fn number(value: &Value) -> u32 {
    u32::try_from(value.as_u64().expect("DWORD")).unwrap()
}

fn mask(value: &Value) -> Vec<Option<u8>> {
    array(value)
        .iter()
        .map(|byte| (!byte.is_null()).then(|| u8::try_from(number(byte)).unwrap()))
        .collect()
}

fn word(bytes: &[Option<u8>]) -> Option<u32> {
    assert_eq!(bytes.len(), 4);
    if bytes.iter().all(Option::is_none) {
        return None;
    }
    Some(u32::from_le_bytes(std::array::from_fn(|index| {
        bytes[index].expect("partial pointers remain native-only")
    })))
}

fn known_word(value: u32) -> Vec<Option<u8>> {
    value.to_le_bytes().into_iter().map(Some).collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Finite original PE guard mapping, private to these tests. In particular,
/// FrontendExecutable.mapped is a data mapper and cannot authenticate .text.
fn original_bytes(raw: &[u8], address: u32, count: usize) -> &[u8] {
    let dword = |offset| u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap());
    let half = |offset| u16::from_le_bytes(raw[offset..offset + 2].try_into().unwrap());
    let pe = dword(0x3c) as usize;
    let optional = pe + 24;
    assert_eq!(dword(optional + 28), 0x400000);
    let sections = optional + usize::from(half(pe + 20));
    for index in 0..usize::from(half(pe + 6)) {
        let section = sections + 40 * index;
        let start = 0x400000 + dword(section + 12);
        let size = dword(section + 16) as usize;
        if let Some(offset) = address.checked_sub(start).map(|offset| offset as usize) {
            if offset.checked_add(count).is_some_and(|end| end <= size) {
                let file = dword(section + 20) as usize + offset;
                return &raw[file..file + count];
            }
        }
    }
    panic!("guard outside original file-backed sections")
}

fn authenticate(fixture: &Value, raw: &[u8]) {
    assert_eq!(digest(FIXTURE), FIXTURE_HASH);
    let metadata = &fixture["metadata"];
    assert_eq!(metadata["producer_sha256"], PRODUCER_HASH);
    assert_eq!(metadata["executable_sha256"], EXE_HASH);
    assert_eq!(metadata["text_sha256"], TEXT_HASH);
    assert_eq!(metadata["fpcw"], 0x027f);
    assert_eq!(
        metadata["execution"],
        "three_unchanged_CRT_bodies_zero_supplied_varargs_literal_and_escaped_percent"
    );
    assert_eq!(
        metadata["abi"],
        json!({
            "wrapper_arguments": 2, "supplied_varargs": 0,
            "return_contract": "full_DWORD_emitted_count", "ret_pop_bytes": 0,
            "direct_stack_delta": 4, "caller_cleanup": "not_executed"
        })
    );
    assert_eq!(metadata["support"], "fresh_declared_unknown_stack_per_direct_request_known_accesses_checked_online_full_masked_state_retained_no_redundant_known_stack_timeline");
    assert_eq!(digest(raw), EXE_HASH);
    inf_formats::FrontendExecutable::parse(raw).unwrap();
    let pe = u32::from_le_bytes(raw[0x3c..0x40].try_into().unwrap()) as usize;
    let optional_size = u16::from_le_bytes(raw[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let first = pe + 24 + optional_size;
    assert_eq!(&raw[first..first + 8], b".text\0\0\0");
    let text_size = u32::from_le_bytes(raw[first + 16..first + 20].try_into().unwrap()) as usize;
    assert_eq!(digest(original_bytes(raw, 0x401000, text_size)), TEXT_HASH);
    for (group, expected_count) in [
        ("bodies", 3),
        ("data_guards", 4),
        ("instruction_guards", 20),
        ("static_caller_fragments", 3),
    ] {
        let guards = array(&metadata[group]);
        assert_eq!(guards.len(), expected_count);
        for guard in guards {
            let bytes = original_bytes(
                raw,
                number(&guard["address"]),
                number(&guard["bytes"]) as usize,
            );
            assert_eq!(digest(bytes), guard["sha256"].as_str().unwrap());
        }
    }
}

fn initial_backing(case: &Value, metadata: &Value, raw: &[u8]) -> Backing {
    let input = &case["input"];
    let mut source = BTreeMap::new();
    let mut format = mask(&input["format"]);
    format.resize(64, None);
    source.insert("format".to_owned(), format);
    source.insert("output".to_owned(), mask(&input["output"]));
    source.insert("output_guard".to_owned(), (0xc0..=0xcf).map(Some).collect());
    let pointer = if input["ctype_pointer"].is_null() {
        vec![None; 4]
    } else {
        known_word(number(&input["ctype_pointer"]))
    };
    source.insert("ctype_pointer".to_owned(), pointer);
    for (field, region) in [
        ("classification_high", "ctype_words"),
        ("alternate_classification_high", "alternate_ctype_words"),
    ] {
        let high = mask(&input[field]);
        assert_eq!(high.len(), 256);
        source.insert(
            region.to_owned(),
            high.into_iter().flat_map(|byte| [None, byte]).collect(),
        );
    }
    for region in ["parser_table", "parser_jump_words"] {
        let guard = array(&metadata["data_guards"])
            .iter()
            .find(|guard| guard["name"] == region)
            .unwrap();
        source.insert(
            region.to_owned(),
            original_bytes(
                raw,
                number(&guard["address"]),
                number(&guard["bytes"]) as usize,
            )
            .iter()
            .copied()
            .map(Some)
            .collect(),
        );
    }
    assert_state(&source, &case["initial_state"]);
    assert_eq!(mask(&case["initial_state"]["stack"]), vec![None; 1344]);
    source
}

fn assert_state(source: &Backing, state: &Value) {
    assert_eq!(state.as_object().unwrap().len(), source.len() + 1);
    for (region, bytes) in source {
        assert_eq!(&mask(&state[region]), bytes, "{region} mask custody");
    }
    assert_eq!(array(&state["stack"]).len(), 1344);
}

fn patch(source: &mut Backing, patch: &Value) {
    let region = patch["region"].as_str().unwrap();
    assert!(matches!(region, "format" | "ctype_pointer"));
    let offset = number(&patch["offset"]) as usize;
    let bytes = mask(&patch["bytes"]);
    source.get_mut(region).unwrap()[offset..offset + bytes.len()].clone_from_slice(&bytes);
}

fn diagnostic(name: &str) -> Option<(u32, &str, &[u8])> {
    DIAGNOSTICS
        .iter()
        .find(|entry| entry.0 == name)
        .map(|entry| (entry.1, entry.2, entry.3))
}

fn audit_step(
    source: &mut Backing,
    request: &Value,
    receipt: &Value,
    stopped: Option<(u32, &str, &[u8])>,
) {
    let events = array(&receipt["events"]);
    let patches = array(&request["patches"]);
    for (index, declared) in patches.iter().enumerate() {
        assert_eq!(
            events[index],
            json!({
                "kind": "supplied_effect", "patch": declared,
                "scope": "declared_between_requests_not_original_writer"
            })
        );
        patch(source, declared);
    }
    assert_state(source, &receipt["state_before"]);
    let mut entry_stack = vec![None; 1344];
    let args: Vec<_> = [RETURN, OUTPUT, FORMAT]
        .into_iter()
        .flat_map(known_word)
        .collect();
    entry_stack[0x500..0x50c].clone_from_slice(&args);
    assert_eq!(mask(&receipt["state_before"]["stack"]), entry_stack);
    assert_eq!(
        events[patches.len()],
        json!({
            "kind": "supplied_support", "scope": "fresh_unknown_direct_call_stack_and_exact_two_argument_frame",
            "entry_stack": STACK, "arguments": [OUTPUT, FORMAT], "supplied_varargs": 0
        })
    );
    let before_output = source["output"].clone();
    let mut frames: Vec<&Value> = Vec::new();
    let mut emitted = Vec::new();
    let mut nul_writes = 0;
    for (index, event) in events.iter().enumerate().skip(patches.len() + 1) {
        match event["kind"].as_str().unwrap() {
            "routine_entry" => {
                let (address, stack, args) = match event["function"].as_str().unwrap() {
                    "wrapper" => (0x511b56, STACK, json!([OUTPUT, FORMAT])),
                    "parser" => (
                        0x514598,
                        STACK - 0x38,
                        json!([STACK - 0x24, FORMAT, STACK + 12]),
                    ),
                    "emitter" => {
                        assert_eq!(frames.last().unwrap()["function"], "parser");
                        assert!(matches!(number(&event["return_ip"]), 0x51475a | 0x514773));
                        let args = array(&event["arguments"]);
                        assert_eq!(args.len(), 3);
                        assert_eq!(args[1], STACK - 0x24);
                        assert_eq!(args[2], STACK - 0x50);
                        (0x514cd9, STACK - 0x2a0, event["arguments"].clone())
                    }
                    other => panic!("unadmitted body {other}"),
                };
                assert_eq!(event["address"], address);
                assert_eq!(event["entry_stack"], stack);
                assert_eq!(event["arguments"], args);
                assert_eq!(number(&event["depth"]) as usize, frames.len() + 1);
                if frames.is_empty() {
                    assert_eq!(event["saved_registers"], request["saved_registers"]);
                    assert_eq!(event["return_ip"], RETURN);
                }
                frames.push(event);
            }
            "routine_return" => {
                let entry = frames.pop().expect("matching original entry");
                assert_eq!(event["function"], entry["function"]);
                assert_eq!(event["return_ip"], entry["return_ip"]);
                assert_eq!(
                    number(&event["stack_after"]),
                    number(&entry["entry_stack"]) + 4
                );
                assert_eq!(event["pop_bytes"], 0);
                let ret = match event["function"].as_str().unwrap() {
                    "wrapper" => 0x511ba7,
                    "parser" => 0x514cb8,
                    "emitter" => {
                        assert_eq!(event["raw_eax"], STACK - 0x50);
                        0x514d0d
                    }
                    _ => unreachable!(),
                };
                assert_eq!(event["ip"], ret);
                if event["function"] != "emitter" {
                    assert_eq!(event["raw_eax"], receipt["result"]);
                }
            }
            "read" => {
                let region = event["region"].as_str().unwrap();
                let offset = number(&event["offset"]) as usize;
                let count = number(&event["bytes"]) as usize;
                assert_eq!(mask(&event["data"]), source[region][offset..offset + count]);
                if region == "ctype_pointer" {
                    assert_eq!(event["ip"], 0x514737);
                    assert_eq!(offset, 0);
                    assert_eq!(count, 4);
                } else if matches!(region, "ctype_words" | "alternate_ctype_words") {
                    assert_eq!(events[index - 1]["region"], "ctype_pointer");
                    assert_eq!(event["ip"], 0x514743);
                    assert_eq!(count, 1);
                    assert_eq!(offset % 2, 1);
                    let selected = match word(&source["ctype_pointer"]).unwrap() {
                        STARTUP_TABLE => "ctype_words",
                        ALTERNATE_TABLE => "alternate_ctype_words",
                        _ => panic!("unadmitted physical backing"),
                    };
                    assert_eq!(region, selected);
                }
            }
            "write" => {
                assert_eq!(event["region"], "output");
                assert_eq!(event["bytes"], 1);
                let offset = number(&event["offset"]) as usize;
                assert_eq!(offset, emitted.len());
                let bytes = mask(&event["data"]);
                assert_eq!(bytes.len(), 1);
                let byte = bytes[0].expect("original initialized output write");
                match number(&event["ip"]) {
                    0x514ce9 => {
                        let frame = frames.last().unwrap();
                        assert_eq!(frame["function"], "emitter");
                        assert_eq!(number(&frame["arguments"][0]) as u8, byte);
                        emitted.push(byte);
                    }
                    0x511b91 => {
                        assert_eq!(frames.len(), 1);
                        assert_eq!(frames[0]["function"], "wrapper");
                        assert_eq!(byte, 0);
                        nul_writes += 1;
                    }
                    _ => panic!("unadmitted write instruction"),
                }
                source.get_mut("output").unwrap()[offset] = Some(byte);
            }
            "finite_unknown_read" => {
                assert!(mask(&event["data"]).iter().all(Option::is_none));
                let (allowance, region, bytes) = match number(&event["ip"]) {
                    0x5145bd => ("dead_initial_state0_precision_load", "stack", 4),
                    0x5145c4 => (
                        "dead_repeated_state0_precision_load_before_first_percent",
                        "stack",
                        4,
                    ),
                    0x51460d => ("deterministic_precision_or_read", "stack", 4),
                    0x511b91 => ("deterministic_final_nul_and_read", "output", 1),
                    _ => panic!("unadmitted unknown read"),
                };
                assert_eq!(event["allowance"], allowance);
                assert_eq!(event["region"], region);
                assert_eq!(event["bytes"], bytes);
            }
            "precision_reset" => {
                assert_eq!(event["ip"], 0x51460d);
                assert_eq!(event["data"], json!([255, 255, 255, 255]));
            }
            kind if kind.starts_with("required_") => {
                let (ip, reason, _) = stopped.expect("only explicitly stopped diagnostics");
                assert_eq!(index + 1, events.len());
                assert_eq!(event["ip"], ip);
                assert_eq!(event["reason"], reason);
                assert_eq!(
                    event["scope"],
                    "deliberate_reached_evidence_frontier_no_native_error_or_hardware_fault_claim"
                );
            }
            other => panic!("unadmitted event {other}"),
        }
    }
    assert!(receipt["invalid"].is_null(), "no retail fault claimed");
    assert_state(source, &receipt["state"]);
    assert!(number(&receipt["validated_stack_accesses"]["reads"]) > 0);
    assert!(number(&receipt["validated_stack_accesses"]["writes"]) > 0);
    if let Some((ip, reason, prefix)) = stopped {
        assert_eq!(receipt["outcome"], "stopped_diagnostic");
        assert_eq!(receipt["stop_ip"], ip);
        assert_eq!(receipt["stop"], reason);
        assert!(receipt["result"].is_null());
        assert_eq!(emitted, prefix);
        assert_eq!(nul_writes, 0);
        assert!(!frames.is_empty());
        assert_eq!(
            source["output"][prefix.len()..],
            before_output[prefix.len()..]
        );
    } else {
        assert_eq!(receipt["outcome"], "original_body_complete");
        assert!(receipt["stop"].is_null() && receipt["stop_ip"].is_null());
        assert_eq!(number(&receipt["result"]) as usize, emitted.len());
        assert!(emitted.len() <= 31);
        assert_eq!(nul_writes, 1);
        assert!(frames.is_empty());
        assert_eq!(receipt["final_stack"], STACK + 4);
        assert_eq!(
            source["output"][emitted.len() + 1..],
            before_output[emitted.len() + 1..]
        );
        let stack = mask(&receipt["state"]["stack"]);
        for (offset, value) in [
            (0, OUTPUT + emitted.len() as u32),
            (4, 0x7ffffffe - emitted.len() as u32),
            (8, OUTPUT),
            (12, 0x42),
        ] {
            assert_eq!(stack[0x4dc + offset..0x4e0 + offset], known_word(value));
        }
    }
}

fn load_and_audit() -> Option<(Value, Vec<u8>)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe");
    let Ok(raw) = std::fs::read(path) else {
        eprintln!("retail Infest.exe missing; copy the game into retail/ to run this test");
        return None;
    };
    let fixture: Value = serde_json::from_slice(FIXTURE).unwrap();
    authenticate(&fixture, &raw);
    let metadata = &fixture["metadata"];
    let cases = array(&fixture["cases"]);
    assert_eq!(cases.len(), 27);
    let mut names = BTreeSet::new();
    let mut event_counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut entries: BTreeMap<String, u32> = BTreeMap::new();
    let mut returns: BTreeMap<String, u32> = BTreeMap::new();
    let mut outcomes: BTreeMap<String, u32> = BTreeMap::new();
    let mut calls = 0;
    for case in cases {
        let name = case["input"]["name"].as_str().unwrap();
        assert!(names.insert(name));
        let stopped = diagnostic(name);
        assert_eq!(case["instruction_bytes_unchanged"], true);
        assert_eq!(
            case["evidence"],
            if stopped.is_some() {
                "stopped_diagnostic"
            } else {
                "bounded_original_body"
            }
        );
        assert!(stopped.is_some() || COMPLETED.contains(&name));
        let mut source = initial_backing(case, metadata, &raw);
        let requests = array(&case["input"]["steps"]);
        assert_eq!(requests.len(), array(&case["steps"]).len());
        for (request, receipt) in requests.iter().zip(array(&case["steps"])) {
            calls += 1;
            audit_step(&mut source, request, receipt, stopped);
            *outcomes
                .entry(receipt["outcome"].as_str().unwrap().to_owned())
                .or_default() += 1;
            for event in array(&receipt["events"]) {
                let kind = event["kind"].as_str().unwrap();
                *event_counts.entry(kind.to_owned()).or_default() += 1;
                if kind == "routine_entry" || kind == "routine_return" {
                    let map = if kind == "routine_entry" {
                        &mut entries
                    } else {
                        &mut returns
                    };
                    *map.entry(event["function"].as_str().unwrap().to_owned())
                        .or_default() += 1;
                }
            }
        }
    }
    assert_eq!(
        names,
        COMPLETED
            .into_iter()
            .chain(DIAGNOSTICS.iter().map(|entry| entry.0))
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(calls, 29);
    assert_eq!(
        metadata["counts"],
        json!({
            "histories": 27, "invocations": 29, "completed_histories": 15,
            "completed_invocations": 17, "diagnostic_histories": 12, "diagnostic_invocations": 12,
            "events": event_counts, "entries": entries, "returns": returns, "outcomes": outcomes
        })
    );
    assert_eq!(
        metadata["counts"]["entries"],
        json!({"emitter":116,"parser":29,"wrapper":29})
    );
    assert_eq!(
        metadata["counts"]["returns"],
        json!({"emitter":115,"parser":17,"wrapper":17})
    );
    Some((fixture, raw))
}

fn complete_format(bytes: &[Option<u8>]) -> Vec<u8> {
    let mut result = Vec::new();
    for byte in bytes {
        let byte = byte.expect("diagnostics excluded before known FORMAT construction");
        result.push(byte);
        if byte == 0 {
            return result;
        }
    }
    panic!("admitted FORMAT has its own NUL")
}

fn current_classification(source: &Backing) -> Option<[Option<u8>; 256]> {
    word(&source["ctype_pointer"]).map(|pointer| {
        let region = match pointer {
            STARTUP_TABLE => "ctype_words",
            ALTERNATE_TABLE => "alternate_ctype_words",
            _ => panic!("unadmitted semantic binding"),
        };
        std::array::from_fn(|byte| source[region][2 * byte + 1])
    })
}

#[test]
fn profile_name_completed_formatter_matches_original_cpu_receipts() {
    let Some((fixture, raw)) = load_and_audit() else {
        return;
    };
    let mut admitted = BTreeSet::new();
    let mut skipped = BTreeSet::new();
    let mut calls = 0;
    for case in array(&fixture["cases"]) {
        let name = case["input"]["name"].as_str().unwrap();
        if diagnostic(name).is_some() {
            assert!(skipped.insert(name));
            continue; // No partial unknown format/mask defaults or typed owners.
        }
        assert!(admitted.insert(name));
        let mut source = initial_backing(case, &fixture["metadata"], &raw);
        let mut owner = ProfileNameClassification {
            high_bytes: current_classification(&source),
        };
        let mut previous: Vec<(FormattedProfileName, Vec<u8>, u32)> = Vec::new();
        for (request, receipt) in array(&case["input"]["steps"])
            .iter()
            .zip(array(&case["steps"]))
        {
            for declared in array(&request["patches"]) {
                patch(&mut source, declared);
                if declared["region"] == "ctype_pointer" {
                    // Same semantic current owner, explicit input-driven
                    // between-call binding change. No native writer claim.
                    owner.high_bytes = current_classification(&source);
                }
            }
            let format = complete_format(&source["format"]);
            let before_format = format.clone();
            let before_owner = owner.clone();
            let before_source = source.clone();
            let output =
                format_profile_name(Some(&format), &owner).expect("completed native domain");
            calls += 1;
            assert_eq!(format, before_format);
            assert_eq!(owner, before_owner);
            assert_eq!(source, before_source);
            assert_eq!(output.length(), number(&receipt["result"]));
            let count = output.length() as usize;
            let native_output = mask(&receipt["state"]["output"]);
            assert_eq!(
                output
                    .bytes_with_nul()
                    .iter()
                    .copied()
                    .map(Some)
                    .collect::<Vec<_>>(),
                native_output[..count + 1]
            );
            assert_eq!(output.bytes_with_nul().last(), Some(&0));
            assert_eq!(owner.high_bytes, current_classification(&source));
            for (earlier, bytes, length) in &previous {
                assert_eq!(earlier.bytes_with_nul(), bytes);
                assert_eq!(earlier.length(), *length);
            }
            let bytes = output.bytes_with_nul().to_vec();
            let length = output.length();
            previous.push((output, bytes, length));
            // Native output tails and support storage have no typed buffer
            // counterpart. Retain them solely in the separate backing audit.
        }
    }
    assert_eq!(admitted, COMPLETED.into_iter().collect::<BTreeSet<_>>());
    assert_eq!(
        skipped,
        DIAGNOSTICS
            .iter()
            .map(|entry| entry.0)
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(calls, 17);
}

#[test]
fn profile_name_stopped_diagnostics_keep_native_only_prefix_scope() {
    let Some((fixture, _)) = load_and_audit() else {
        return;
    };
    let stopped: BTreeSet<_> = array(&fixture["cases"])
        .iter()
        .filter(|case| case["evidence"] == "stopped_diagnostic")
        .map(|case| case["input"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        stopped,
        DIAGNOSTICS
            .iter()
            .map(|entry| entry.0)
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        fixture["metadata"]["counts"]["outcomes"],
        json!({"original_body_complete":17,"stopped_diagnostic":12})
    );
}
