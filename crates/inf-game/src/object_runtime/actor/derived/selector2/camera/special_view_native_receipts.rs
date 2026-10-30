//! Four-body original leaf cadence is audited separately from genuine Rust
//! source/temporary/basis parity. No full camera suffix, activation, pointer,
//! incidental EAX or measured Rust cadence claim. Known stack accesses and
//! control restoration were checked online, not serialized instruction by
//! instruction. Consumed nested argument slots may be scratch.

use std::{collections::BTreeMap, collections::BTreeSet, path::Path};

use inf_formats::{
    parse_terrain_bank, ByteSpan, FrontendExecutable, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_ENTRY_FIXED_BYTES,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::*;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        ActorTerrainSurface, BaseContext, DerivedActorConstructionRequest, ObjectId,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};

const FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/selector2_special_view_native.json"
));
// The producing checker stays private; the fixture records its SHA-256.
const FIXTURE_SHA: &str = "9ef5120e677e49a12bce817cd397df241526820e8cbafdee43f5f2c18b79ef9a";
const PRODUCER_SHA: &str = "b7a82ce543211cef6a45850626e0d73af47f53a27af5db0011ca720707886a48";
const HARNESS_SHA: &str = "fcd6591f3adca0cc5c8b3a6e5f1357cb496ed7bb6492d2aed3fb43564ed5e8c1";
const PE_HELPER_SHA: &str = "6172b7687bfe0326aa6a24a5e292fb7bb748a59c5ece3c848e328c355a0ea4ed";
const EXE_SHA: &str = "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956";
const TEXT_SHA: &str = "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2";
const SOURCE: u32 = 0x724000;
const XYZ: u32 = 0x725000;
const BASIS: u32 = 0x726000;
const STACK: u32 = 0x730000;
const RETURN: u32 = 0xf00000;
const SEEDS: [u32; 4] = [0x13579bdf, 0x2468ace0, 0x10203040, 0x50607080];
const DIAGNOSTICS: [&str; 2] = [
    "unknown-constructor-yaw-word",
    "unknown-constructor-pitch-word",
];
const COMPLETED: [&str; 9] = [
    "identity-wrap-y-and-reset-old-count",
    "positive-noncommuting-yaw-pitch",
    "signed-negative-yaw-pitch",
    "high-word-position-and-angle-wrap",
    "nonorthogonal-forward-up",
    "parallel-forward-up",
    "zero-forward-and-up",
    "signed-zero-and-small-cross",
    "fresh-source-between-leaf-calls",
];
const PREFIXES: [&str; 9] = [
    "unknown-position-x",
    "unknown-position-y",
    "unknown-position-z",
    "unknown-forward-x",
    "unknown-forward-y",
    "unknown-forward-z",
    "unknown-up-x",
    "unknown-up-y",
    "unknown-up-z",
];
const REGIONS: [(&str, u32, usize); 5] = [
    ("source", SOURCE, 0x300),
    ("temporary_xyz", XYZ, 6),
    ("basis", BASIS, 40),
    ("process_null", 0x54e894, 4),
    ("stack", STACK - 0x800, 0x900),
];
const BODIES: [(&str, u32, u32, u32, u32, &str); 4] = [
    (
        "special",
        0x421d50,
        0x421db1,
        8,
        0x421dae,
        "651b9bdfa6add54c68e5a2e05bd0d86c0a3281ba59d2cb1cd927aec39af9c888",
    ),
    (
        "orient",
        0x490370,
        0x490511,
        12,
        0x49050e,
        "e76aae66989144f5be92730daf67201470a2486653844150d04f9b8f3dc1d9b9",
    ),
    (
        "yaw",
        0x491530,
        0x4917d0,
        4,
        0x4917cd,
        "c2b2049b34fb459f4468d624bbd959eb891836ae30f0e86c21a7883a4ffed0d3",
    ),
    (
        "pitch",
        0x491290,
        0x49152d,
        4,
        0x49152a,
        "265dde1f612cf400d2bce397b17ef60077396688ad6497ea9db0ba6c5af062f1",
    ),
];
const SOURCE_READS: [(usize, usize, u32); 10] = [
    (0x1ca, 4, 0x421d64),
    (0x1ce, 2, 0x421d6c),
    (0x1d0, 4, 0x490377),
    (0x1d4, 4, 0x49037c),
    (0x1d8, 4, 0x490382),
    (0x1dc, 4, 0x49038c),
    (0x1e0, 4, 0x490391),
    (0x1e4, 4, 0x490397),
    (0x1c6, 2, 0x421d8e),
    (0x1c8, 2, 0x421d9d),
];
const COPY_WRITES: [(usize, u32); 6] = [
    (28, 0x490379),
    (32, 0x49037f),
    (36, 0x490385),
    (16, 0x49038e),
    (20, 0x490394),
    (24, 0x49039a),
];

fn number(v: &Value) -> u32 {
    u32::try_from(v.as_u64().unwrap()).unwrap()
}
fn optional(v: &Value) -> Option<u32> {
    (!v.is_null()).then(|| number(v))
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex_bytes(v: &Value) -> Vec<u8> {
    decode_hex(v.as_str().unwrap())
}
fn decode_hex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn body(name: &str) -> (&'static str, u32, u32, u32, u32, &'static str) {
    BODIES.into_iter().find(|row| row.0 == name).unwrap()
}

/// Finite supported PE mapping, including its separate .text file backing.
struct Original<'a> {
    raw: &'a [u8],
    mapped: FrontendExecutable<'a>,
    text_offset: usize,
}
impl<'a> Original<'a> {
    fn new(raw: &'a [u8]) -> Self {
        assert_eq!(digest(raw), EXE_SHA);
        let mapped = FrontendExecutable::parse(raw).unwrap();
        let pe = u32::from_le_bytes(raw[0x3c..0x40].try_into().unwrap()) as usize;
        let opt = u16::from_le_bytes(raw[pe + 20..pe + 22].try_into().unwrap()) as usize;
        let section = &raw[pe + 24 + opt..pe + 24 + opt + 40];
        assert_eq!(&section[..8], b".text\0\0\0");
        assert_eq!(
            u32::from_le_bytes(section[12..16].try_into().unwrap()),
            0x1000
        );
        assert_eq!(
            u32::from_le_bytes(section[16..20].try_into().unwrap()),
            0x11e000
        );
        let text_offset = u32::from_le_bytes(section[20..24].try_into().unwrap()) as usize;
        assert_eq!(digest(&raw[text_offset..text_offset + 0x11e000]), TEXT_SHA);
        Self {
            raw,
            mapped,
            text_offset,
        }
    }
    fn bytes(&self, address: u32, width: usize) -> &[u8] {
        if (0x401000..0x51f000).contains(&address) {
            assert!((address as usize) + width <= 0x51f000);
            let start = self.text_offset + (address - 0x401000) as usize;
            &self.raw[start..start + width]
        } else {
            self.mapped.mapped(address, width).unwrap().bytes
        }
    }
    fn authenticate(&self, metadata: &Value) {
        assert_eq!(metadata["executable_sha256"], EXE_SHA);
        assert_eq!(metadata["text_sha256"], TEXT_SHA);
        assert_eq!(metadata["text_address"], 0x401000);
        assert_eq!(metadata["text_bytes"], 0x11e000);
        assert_eq!(metadata["producer_sha256"], PRODUCER_SHA);
        assert_eq!(metadata["harness_sha256"], HARNESS_SHA);
        assert_eq!(metadata["pe_helper_sha256"], PE_HELPER_SHA);
        assert_eq!(metadata["bodies"].as_object().unwrap().len(), 4);
        for (name, address, end, cleanup, ret, hash) in BODIES {
            assert_eq!(
                metadata["bodies"][name],
                json!({"address":address,"end":end,
                "bytes":end-address,"return_cleanup":cleanup,"sha256":hash})
            );
            assert_eq!(digest(self.bytes(address, (end - address) as usize)), hash);
            assert_eq!(self.bytes(ret, 3), [0xc2, cleanup as u8, 0]);
        }
        assert_eq!(metadata["data"].as_object().unwrap().len(), 2);
        for (name, address, width, hash) in [
            (
                "selector2_table",
                0x520860,
                76,
                "8e311e5e648c8da048a2f4c56796f404667a51e6a75429faed0d05cedd438dd6",
            ),
            (
                "sine",
                0x53df00,
                8192,
                "4bb036e0600ba85ef736d572cc19fb28082d2ca81ba4c8995b69ffc893f9b3d8",
            ),
        ] {
            assert_eq!(
                metadata["data"][name],
                json!({"address":address,"bytes":width,"sha256":hash})
            );
            assert_eq!(digest(self.bytes(address, width)), hash);
        }
        let constants = [
            (0x5205b0_u32, 0x45800000_u32),
            (0x51f198, 0),
            (0x528ce0, 0x38000100),
            (0x520a70, 0x3f800000),
        ];
        assert_eq!(
            metadata["constants"].as_object().unwrap().len(),
            constants.len()
        );
        for (address, bits) in constants {
            assert_eq!(metadata["constants"][address.to_string()], bits);
            assert_eq!(self.bytes(address, 4), bits.to_le_bytes());
        }
        // Exact direct edges and count reset; no synthetic normalization edge.
        for (ip, target) in [
            (0x421d89, 0x490370),
            (0x421d98, 0x491530),
            (0x421da7, 0x491290),
        ] {
            let code = self.bytes(ip, 5);
            assert_eq!(code[0], 0xe8);
            assert_eq!(
                ip.wrapping_add(5)
                    .wrapping_add(i32::from_le_bytes(code[1..].try_into().unwrap()) as u32),
                target
            );
        }
        assert_eq!(self.bytes(0x490509, 2), [0x89, 0x11]);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Masked {
    bytes: Vec<u8>,
    known: Vec<bool>,
}
impl Masked {
    fn unknown(size: usize) -> Self {
        Self {
            bytes: vec![0xd3; size],
            known: vec![false; size],
        }
    }
    fn parse(span: &Value, address: u32, size: usize) -> Self {
        assert_eq!(span.as_object().unwrap().len(), 3);
        assert_eq!(number(&span["address"]), address);
        let bytes = hex_bytes(&span["bytes"]);
        let known = span["known"]
            .as_str()
            .unwrap()
            .bytes()
            .map(|b| match b {
                b'0' => false,
                b'1' => true,
                _ => panic!("invalid known mask"),
            })
            .collect::<Vec<_>>();
        assert_eq!(bytes.len(), size);
        assert_eq!(known.len(), size);
        Self { bytes, known }
    }
    fn supply(&mut self, offset: usize, value: Option<u32>, width: usize) {
        if let Some(value) = value {
            self.bytes[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
        }
        self.known[offset..offset + width].fill(value.is_some());
    }
    fn value(&self, offset: usize, width: usize) -> Option<u32> {
        let mask = &self.known[offset..offset + width];
        assert!(
            mask.iter().all(|b| *b) || mask.iter().all(|b| !*b),
            "partial scalar"
        );
        mask[0].then(|| {
            let mut bytes = [0; 4];
            bytes[..width].copy_from_slice(&self.bytes[offset..offset + width]);
            u32::from_le_bytes(bytes)
        })
    }
}
fn snapshot(value: &Value) -> BTreeMap<String, Masked> {
    assert_eq!(value.as_object().unwrap().len(), REGIONS.len());
    REGIONS
        .into_iter()
        .map(|(name, address, size)| (name.to_owned(), Masked::parse(&value[name], address, size)))
        .collect()
}

/// Declared setup is reconstructed before events or actual arena preparation.
struct Backing {
    inputs: Value,
    regions: BTreeMap<String, Masked>,
}
impl Backing {
    fn new(input: &Value) -> Self {
        assert_eq!(input.as_object().unwrap().len(), 6);
        let mut regions = REGIONS
            .into_iter()
            .map(|(name, _, size)| (name.to_owned(), Masked::unknown(size)))
            .collect::<BTreeMap<_, _>>();
        regions
            .get_mut("source")
            .unwrap()
            .supply(0, Some(0x520860), 4);
        let mut backing = Self {
            inputs: json!({}),
            regions,
        };
        backing.patch(input);
        backing
    }
    fn patch(&mut self, patch: &Value) {
        for (key, values) in patch.as_object().unwrap() {
            let (region, offset, width, count) = match key.as_str() {
                "position" => ("source", 0x1ca, 2, 3),
                "forward" => ("source", 0x1d0, 4, 3),
                "up" => ("source", 0x1dc, 4, 3),
                "angles" => ("source", 0x1c6, 2, 2),
                "basis_words" => ("basis", 0, 4, 10),
                "temporary_xyz" => ("temporary_xyz", 0, 2, 3),
                _ => panic!("undeclared setup field {key}"),
            };
            let values = values.as_array().unwrap();
            assert_eq!(values.len(), count);
            for (i, v) in values.iter().enumerate() {
                let value = optional(v);
                if width == 2 {
                    assert!(value.is_none_or(|v| v <= 0xffff));
                }
                self.regions
                    .get_mut(region)
                    .unwrap()
                    .supply(offset + i * width, value, width);
            }
            self.inputs[key] = Value::Array(values.clone());
        }
    }
    fn initial(&mut self, call: &Value) {
        self.patch(&call["request"]["patches"]);
        assert_eq!(call["request"].as_object().unwrap().len(), 2);
        assert_eq!(call["request"]["operation"], "special_view_leaf");
        assert_eq!(self.inputs, call["current_inputs"]);
        for (i, value) in [RETURN, XYZ, BASIS].into_iter().enumerate() {
            self.regions
                .get_mut("stack")
                .unwrap()
                .supply(0x800 + i * 4, Some(value), 4);
        }
        assert_eq!(self.regions, snapshot(&call["initial"]));
    }
}

#[derive(Default)]
struct Counts {
    entries: BTreeMap<String, u32>,
    returns: BTreeMap<String, u32>,
    reads: u32,
    writes: u32,
    stack_reads: u32,
    stack_writes: u32,
}
impl Counts {
    fn verify(&self, value: &Value) {
        assert_eq!(
            value,
            &json!({"histories":20,"calls":21,"completed":10,"prefix_calls":9,
            "diagnostic_calls":2,"body_entries":60,"body_returns":43,
            "entries_by_body":{"special":21,"orient":18,"yaw":11,"pitch":10},
            "returns_by_body":{"special":10,"orient":12,"yaw":11,"pitch":10},
            "nonstack_reads":903,"nonstack_writes":409,"stack_reads":440,"stack_writes":393})
        );
        assert_eq!(
            self.entries,
            BTreeMap::from([
                ("special".into(), 21),
                ("orient".into(), 18),
                ("yaw".into(), 11),
                ("pitch".into(), 10)
            ])
        );
        assert_eq!(
            self.returns,
            BTreeMap::from([
                ("special".into(), 10),
                ("orient".into(), 12),
                ("yaw".into(), 11),
                ("pitch".into(), 10)
            ])
        );
        assert_eq!(
            (self.reads, self.writes, self.stack_reads, self.stack_writes),
            (903, 409, 440, 393)
        );
    }
}

/// Strict frame/copy/phase replay. Numerical output comparison happens later
/// through the actual method, not a second handwritten rotation engine here.
fn audit_call(
    original: &Original<'_>,
    case: &Value,
    call: &Value,
    backing: &mut Backing,
    counts: &mut Counts,
) {
    backing.initial(call);
    let initial = backing.regions.clone();
    let events = call["events"].as_array().unwrap();
    let inputs = &call["current_inputs"];
    let mut frames: Vec<&Value> = Vec::new();
    let mut entries = Vec::new();
    let mut phases = 0;
    let mut reads = 0;
    let mut copies = 0;
    let mut temporary_writes = 0;
    let mut expected_next: Option<(String, usize, usize, u32, Vec<u8>)> = None;
    let mut returned_child: Option<&str> = None;
    for (index, event) in events.iter().enumerate() {
        let kind = event["kind"].as_str().unwrap();
        if let Some((region, offset, size, ip, bytes)) = expected_next.take() {
            assert_eq!(kind, "write");
            assert_eq!(event["region"], region);
            assert_eq!(
                (
                    number(&event["offset"]) as usize,
                    number(&event["size"]) as usize,
                    number(&event["ip"])
                ),
                (offset, size, ip)
            );
            assert_eq!(hex_bytes(&event["bytes"]), bytes);
        }
        if returned_child.is_some() {
            assert_eq!(kind, "phase", "phase must follow original child RET");
        }
        match kind {
            "entry" => {
                let name = event["body"].as_str().unwrap();
                let (_, start, _, _, _, _) = body(name);
                assert_eq!(number(&event["entry"]), start);
                assert_eq!(event["fpcw"], 0x027f);
                if frames.is_empty() {
                    assert_eq!(index, 0);
                    assert_eq!(name, "special");
                    assert_eq!(event["ecx"], SOURCE);
                    assert_eq!(event["sp"], STACK);
                    assert_eq!(event["arguments"], json!([XYZ, BASIS]));
                    assert_eq!(event["return_ip"], RETURN);
                    assert_eq!(event["saved"], json!(SEEDS));
                } else {
                    assert_eq!(frames.len(), 1);
                    assert_eq!(frames[0]["body"], "special");
                    assert_eq!(event["ecx"], BASIS);
                    assert_eq!(event["saved"], json!([SEEDS[0], SEEDS[1], SOURCE, BASIS]));
                    let (sp, return_ip, arguments) = match name {
                        "orient" => {
                            assert_eq!((temporary_writes, reads, phases), (3, 2, 0));
                            (
                                STACK - 24,
                                0x421d8e,
                                json!([SOURCE + 0x1d0, SOURCE + 0x1dc, 0]),
                            )
                        }
                        "yaw" => {
                            assert_eq!((copies, reads, phases), (6, 9, 1));
                            (
                                STACK - 16,
                                0x421d9d,
                                json!([(number(&inputs["angles"][0]) as u16 as i16 as i32) as u32]),
                            )
                        }
                        "pitch" => {
                            assert_eq!((copies, reads, phases), (6, 10, 2));
                            (
                                STACK - 16,
                                0x421dac,
                                json!([(number(&inputs["angles"][1]) as u16 as i16 as i32) as u32]),
                            )
                        }
                        _ => panic!("unexpected leaf edge"),
                    };
                    assert_eq!(
                        (number(&event["sp"]), number(&event["return_ip"])),
                        (sp, return_ip)
                    );
                    assert_eq!(event["arguments"], arguments);
                }
                *counts.entries.entry(name.to_owned()).or_default() += 1;
                frames.push(event);
                entries.push(name);
            }
            "return" => {
                let frame = frames.pop().unwrap();
                let name = frame["body"].as_str().unwrap();
                let (_, _, _, cleanup, ret, _) = body(name);
                assert_eq!(event["body"], name);
                assert_eq!(event["sp"], frame["sp"]);
                assert_eq!(event["return_ip"], frame["return_ip"]);
                assert_eq!(
                    (number(&event["ip"]), number(&event["cleanup"])),
                    (ret, cleanup)
                );
                assert_eq!(original.bytes(ret, 3), [0xc2, cleanup as u8, 0]);
                *counts.returns.entry(name.to_owned()).or_default() += 1;
                // EAX is authentic but incidental to this void leaf.
                number(&event["raw_eax"]);
                if name == "special" {
                    assert!(call["completed"].as_bool().unwrap());
                    assert_eq!(index + 1, events.len());
                    assert_eq!(phases, 3);
                    assert_eq!(reads, 10);
                } else {
                    returned_child = Some(name);
                }
            }
            "phase" => {
                assert_eq!(frames.len(), 1);
                assert_eq!(frames[0]["body"], "special");
                let (name, child, ip) = [
                    ("after_orient", "orient", 0x421d8e),
                    ("after_yaw", "yaw", 0x421d9d),
                    ("after_pitch", "pitch", 0x421dac),
                ][phases];
                assert_eq!(returned_child.take(), Some(child));
                assert_eq!(event["name"], name);
                assert_eq!(number(&event["ip"]), ip);
                assert_eq!(hex_bytes(&event["basis"]), backing.regions["basis"].bytes);
                assert_eq!(
                    hex_bytes(&event["temporary_xyz"]),
                    backing.regions["temporary_xyz"].bytes
                );
                assert_eq!(backing.regions["basis"].value(0, 4), Some(phases as u32));
                assert_eq!(copies, 6);
                phases += 1;
            }
            "read" | "write" | "pe_read" => {
                let frame = frames.last().unwrap();
                let name = frame["body"].as_str().unwrap();
                let (_, start, end, _, _, _) = body(name);
                let ip = number(&event["ip"]);
                assert!((start..end).contains(&ip));
                let size = number(&event["size"]) as usize;
                let bytes = hex_bytes(&event["bytes"]);
                assert_eq!(bytes.len(), size);
                if kind == "pe_read" {
                    let address = number(&event["address"]);
                    let allowed = [(0x520860, 0x5208ac), (0x53df00, 0x53ff00)]
                        .into_iter()
                        .any(|(start, end)| start <= address && address + size as u32 <= end)
                        || [0x5205b0, 0x51f198, 0x528ce0, 0x520a70]
                            .into_iter()
                            .any(|a| a <= address && address + size as u32 <= a + 4);
                    assert!(allowed);
                    assert_eq!(bytes, original.bytes(address, size));
                    counts.reads += 1;
                    continue;
                }
                let region = event["region"].as_str().unwrap();
                assert_ne!(region, "stack");
                let offset = number(&event["offset"]) as usize;
                let state = backing.regions.get_mut(region).unwrap();
                assert!(offset + size <= state.bytes.len());
                if kind == "read" {
                    counts.reads += 1;
                    assert_eq!(&state.bytes[offset..offset + size], bytes);
                    let known = state.known[offset..offset + size].iter().all(|b| *b);
                    assert_eq!(event["known"], known);
                    if region == "source" {
                        assert_eq!((offset, size, ip), SOURCE_READS[reads]);
                        reads += 1;
                        if known && reads <= 8 {
                            let (output, out_offset, write_ip) = match reads {
                                1 => ("temporary_xyz", 0, 0x421d66),
                                2 => ("temporary_xyz", 4, 0x421d70),
                                n => ("basis", COPY_WRITES[n - 3].0, COPY_WRITES[n - 3].1),
                            };
                            expected_next =
                                Some((output.into(), out_offset, size, write_ip, bytes.clone()));
                        }
                    } else if region == "temporary_xyz" {
                        assert_eq!((name, offset, size, ip), ("special", 2, 2, 0x421d74));
                        let value = u16::from_le_bytes(bytes.try_into().unwrap()).wrapping_add(50);
                        expected_next = Some((
                            "temporary_xyz".into(),
                            2,
                            2,
                            ip,
                            value.to_le_bytes().to_vec(),
                        ));
                    } else {
                        assert_eq!(region, "basis");
                        assert!(matches!(name, "orient" | "yaw" | "pitch"));
                        if name == "orient" {
                            assert_eq!(copies, 6);
                        }
                    }
                    if !known {
                        assert_eq!(index + 2, events.len());
                        assert_eq!(events[index + 1]["kind"], "stop");
                        assert_eq!(events[index + 1]["ip"], ip);
                        assert_eq!(events[index + 1]["region"], region);
                        assert_eq!(events[index + 1]["offset"], offset);
                        assert_eq!(events[index + 1]["size"], size);
                    }
                } else {
                    counts.writes += 1;
                    assert!(matches!(region, "basis" | "temporary_xyz"));
                    assert_eq!(
                        &state.bytes[offset..offset + size],
                        hex_bytes(&event["before"])
                    );
                    state.bytes[offset..offset + size].copy_from_slice(&bytes);
                    state.known[offset..offset + size].fill(true);
                    if region == "temporary_xyz" {
                        assert_eq!(name, "special");
                        assert_eq!(
                            (offset, size, ip),
                            [(0, 4, 0x421d66), (4, 2, 0x421d70), (2, 2, 0x421d74)]
                                [temporary_writes]
                        );
                        temporary_writes += 1;
                    } else if COPY_WRITES.iter().any(|row| row.1 == ip) {
                        assert_eq!(name, "orient");
                        assert_eq!((offset, ip), COPY_WRITES[copies]);
                        copies += 1;
                    } else {
                        assert!(matches!(name, "orient" | "yaw" | "pitch"));
                        if name == "orient" {
                            assert_eq!(copies, 6);
                        }
                        if offset == 0 {
                            assert_eq!(size, 4);
                            assert_eq!(
                                (ip, u32::from_le_bytes(bytes.try_into().unwrap())),
                                match name {
                                    "orient" => (0x490509, 0),
                                    "yaw" => (0x4915c5, 1),
                                    "pitch" => (0x491329, 2),
                                    _ => unreachable!(),
                                }
                            );
                        }
                    }
                }
            }
            "stop" => {
                assert_eq!(index + 1, events.len());
                assert!(!call["completed"].as_bool().unwrap());
                assert_eq!(
                    frames.last().unwrap()["body"],
                    if reads <= 2 || reads >= 9 {
                        "special"
                    } else {
                        "orient"
                    }
                );
                let previous = &events[index - 1];
                assert_eq!(previous["kind"], "read");
                assert_eq!(previous["known"], false);
                assert_eq!(event["reason"], case["expected_stop"]);
                let mut stop = event.clone();
                stop.as_object_mut().unwrap().remove("kind");
                assert_eq!(stop, call["stop"]);
                assert_eq!(number(&event["address"]), SOURCE + number(&event["offset"]));
            }
            _ => panic!("unknown native event {kind}"),
        }
    }
    assert!(expected_next.is_none());
    assert!(returned_child.is_none());
    let final_state = snapshot(&call["final"]);
    for name in ["source", "temporary_xyz", "basis", "process_null"] {
        assert_eq!(backing.regions[name], final_state[name]);
    }
    let old = &initial["stack"];
    let final_stack = &final_state["stack"];
    for (a, b) in old.known.iter().zip(&final_stack.known) {
        assert!(!a || *b);
    }
    assert_eq!(&old.bytes[0x800..], &final_stack.bytes[0x800..]);
    assert_eq!(&old.known[0x800..], &final_stack.known[0x800..]);
    backing.regions.insert("stack".into(), final_stack.clone());
    counts.stack_reads += number(&call["stack_reads"]);
    counts.stack_writes += number(&call["stack_writes"]);
    number(&call["raw_eax"]);
    assert_eq!(call["final_fpcw"], 0x027f);
    if call["completed"].as_bool().unwrap() {
        assert!(frames.is_empty());
        assert!(call["stop"].is_null());
        assert_eq!(entries, ["special", "orient", "yaw", "pitch"]);
        assert_eq!(call["final_esp"], STACK + 12);
        assert_eq!(number(&call["final_fpsw"]) & 0x3800, 0);
        assert_eq!(case["admission"], "complete");
    } else {
        assert!(!frames.is_empty());
        assert!(matches!(
            case["admission"].as_str().unwrap(),
            "prefix" | "diagnostic"
        ));
    }
}

fn genuine_vehicle(math_words: &[u16; 4096]) -> (ObjectArena, ObjectId) {
    let bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
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
    let terrain = TerrainRuntime::new(
        &source,
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_selector2_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([901, 902, 903]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: Some(ActorTerrainSurface {
                            terrain: &terrain,
                            plane_height_fixed: terrain.plane_base(),
                        }),
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: Some(0),
                        sine_quarter: math_words,
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut 17,
        )
        .unwrap();
    arena
        .initialize_context(
            actor,
            BaseContext {
                position_words: Some([701, 702, 703]),
                placement_index: 0,
            },
        )
        .unwrap();
    (arena, actor)
}
fn supply_source(arena: &mut ObjectArena, actor: ObjectId, input: &Value) {
    let state = arena.selector2_actor_mut(actor).unwrap();
    state.special_position_1ca =
        std::array::from_fn(|i| optional(&input["position"][i]).map(|v| u16::try_from(v).unwrap()));
    state.special_forward_1d0 = std::array::from_fn(|i| optional(&input["forward"][i]));
    state.special_up_1dc = std::array::from_fn(|i| optional(&input["up"][i]));
    let angles: [u16; 2] =
        std::array::from_fn(|i| u16::try_from(number(&input["angles"][i])).unwrap());
    state.words_1c2_1c8[2..4].copy_from_slice(&angles);
}
fn compare_typed(
    arena: &ObjectArena,
    actor: ObjectId,
    xyz: [Option<u16>; 3],
    basis: NativeBasis,
    native: &Value,
) {
    let state = snapshot(native);
    for i in 0..3 {
        assert_eq!(
            xyz[i].map(u32::from),
            state["temporary_xyz"].value(i * 2, 2)
        );
    }
    for i in 0..10 {
        assert_eq!(Some(basis.words[i]), state["basis"].value(i * 4, 4));
    }
    let vehicle = arena.selector2_actor(actor).unwrap();
    for i in 0..3 {
        assert_eq!(
            vehicle.special_position_1ca[i].map(u32::from),
            state["source"].value(0x1ca + i * 2, 2)
        );
        assert_eq!(
            vehicle.special_forward_1d0[i],
            state["source"].value(0x1d0 + i * 4, 4)
        );
        assert_eq!(
            vehicle.special_up_1dc[i],
            state["source"].value(0x1dc + i * 4, 4)
        );
    }
    for i in 0..2 {
        assert_eq!(
            Some(u32::from(vehicle.words_1c2_1c8[2 + i])),
            state["source"].value(0x1c6 + i * 2, 2)
        );
    }
}

#[test]
fn original_special_view_leaf_matches_genuine_masked_sources_and_basis_prefixes() {
    assert_eq!(digest(FIXTURE), FIXTURE_SHA);
    let value: Value = serde_json::from_slice(FIXTURE).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["boundary"], "selector2_special_view_leaf");
    assert_eq!(value["float_control_word"], 0x027f);
    assert_eq!(value["stack_scope"],"known accesses and saved-register/control restoration validated online; counts/full masked state; no serialized known-stack timeline");
    assert_eq!(
        value["exclusions"],
        json!([
            "full40FD10 eye/look publication",
            "constructor/normal initializer",
            "raw pointer aliases",
            "movement/toggle/global lifetime",
            "Rust cadence",
            "visual/retail acceptance"
        ])
    );
    let raw = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .unwrap();
    let original = Original::new(&raw);
    original.authenticate(&value["metadata"]);
    let sine: [u16; 4096] = std::array::from_fn(|i| {
        u16::from_le_bytes(
            original
                .bytes(0x53df00 + i as u32 * 2, 2)
                .try_into()
                .unwrap(),
        )
    });
    let math = NativeBasisMath::new(sine);
    let mut counts = Counts::default();
    let mut admitted = BTreeSet::new();
    let mut skipped = BTreeSet::new();
    let mut typed_calls = 0;
    let mut typed_successes = 0;
    let mut typed_prefixes = 0;
    let cases = value["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 20);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let classification = case["admission"].as_str().unwrap();
        let calls = case["calls"].as_array().unwrap();
        let steps = case["steps"].as_array().unwrap();
        assert_eq!(calls.len(), steps.len());
        let mut backing = Backing::new(&case["input"]);
        for (call, step) in calls.iter().zip(steps) {
            assert_eq!(&call["request"], step);
            audit_call(&original, case, call, &mut backing, &mut counts);
        }
        // Native angle holes cannot be represented by the genuine known-WORD
        // constructor. Exclude both before any Rust owner/source preparation.
        if DIAGNOSTICS.contains(&name) {
            assert_eq!(classification, "diagnostic");
            assert!(skipped.insert(name));
            continue;
        }
        assert!(admitted.insert(name));
        assert_eq!(
            classification,
            if COMPLETED.contains(&name) {
                "complete"
            } else {
                assert!(PREFIXES.contains(&name));
                "prefix"
            }
        );
        let (mut arena, actor) = genuine_vehicle(&sine);
        let mut current = case["input"].clone();
        supply_source(&mut arena, actor, &current);
        let mut xyz = std::array::from_fn(|i| {
            optional(&current["temporary_xyz"][i]).map(|v| u16::try_from(v).unwrap())
        });
        let mut basis = NativeBasis {
            words: std::array::from_fn(|i| number(&current["basis_words"][i])),
        };
        for call in calls {
            for (key, values) in call["request"]["patches"].as_object().unwrap() {
                current[key] = values.clone();
                if key == "temporary_xyz" {
                    xyz = std::array::from_fn(|i| {
                        optional(&values[i]).map(|v| u16::try_from(v).unwrap())
                    });
                }
                if key == "basis_words" {
                    basis.words = std::array::from_fn(|i| number(&values[i]));
                }
            }
            assert_eq!(current, call["current_inputs"]);
            supply_source(&mut arena, actor, &current);
            compare_typed(&arena, actor, xyz, basis, &call["initial"]);
            let before = format!("{arena:?}");
            let result = arena.special_view_421d50(actor, &mut xyz, &mut basis, &math);
            if call["completed"].as_bool().unwrap() {
                assert_eq!(result, Ok(()));
                typed_successes += 1;
            } else {
                assert_eq!(call["stop"]["reason"], "source_storage");
                assert_eq!(
                    result,
                    Err(CameraPoseError::UninitializedSpecialViewStorage {
                        actor,
                        offset: u16::try_from(number(&call["stop"]["offset"])).unwrap(),
                        bytes: u8::try_from(number(&call["stop"]["size"])).unwrap()
                    })
                );
                typed_prefixes += 1;
            }
            typed_calls += 1;
            compare_typed(&arena, actor, xyz, basis, &call["final"]);
            assert_eq!(
                format!("{arena:?}"),
                before,
                "leaf source/arena custody {name}"
            );
        }
    }
    counts.verify(&value["counts"]);
    assert_eq!(skipped, DIAGNOSTICS.into_iter().collect::<BTreeSet<_>>());
    assert_eq!(
        admitted,
        COMPLETED
            .into_iter()
            .chain(PREFIXES)
            .collect::<BTreeSet<_>>()
    );
    assert_eq!((typed_calls, typed_successes, typed_prefixes), (19, 10, 9));
}
