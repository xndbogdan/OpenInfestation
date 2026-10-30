//! Original recorded cadence and actual mutable-phase parity are separate.
//! Supplied native registry capacity/cache/free-chain bytes are not constructor
//! parity. Genuine arena registrations supply referent identities; only their
//! semantic resolution/cache effects and full typed custody are compared.
//! Known stack accesses/restoration were checked online by the producer, not
//! serialized as a full instruction trace. No enclosing Tick or Draw claim.

use std::{collections::BTreeMap, collections::BTreeSet, path::Path};

use inf_formats::{native_basis::NativeBasisMath, FrontendExecutable};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    ActorTerrainSurface, BaseContext, CameraDerivedComponent, CameraParameterState,
    DerivedActorConstructionRequest, FollowTargetPoint, FollowTargetRequest, NativeTarget,
    ObjectId, ObjectState,
};
use crate::terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_ENTRY_FIXED_BYTES,
};

const FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/vehicle_camera_targets_native.json"
));
// The producing checker stays private; the fixture records its SHA-256.
const FIXTURE_SHA: &str = "2e132d7ab088a825e14fe03adb42804146f75b46d4891c6b76affa0510487415";
const PRODUCER_SHA: &str = "6172b7687bfe0326aa6a24a5e292fb7bb748a59c5ece3c848e328c355a0ea4ed";
const HARNESS_SHA: &str = "fcd6591f3adca0cc5c8b3a6e5f1357cb496ed7bb6492d2aed3fb43564ed5e8c1";
const EXE_SHA: &str = "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956";
const TEXT_SHA: &str = "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2";
const VEHICLE: u32 = 0x724000;
const REFERENT: u32 = 0x725000;
const TARGET: u32 = 0x726000;
const CAMERA: u32 = 0x728000;
const EYE: u32 = CAMERA + 0x138;
const LOOK: u32 = CAMERA + 0x15c;
const STACK: u32 = 0x730000;
const RETURN: u32 = 0xf00000;
const NATIVE_HANDLE: u32 = 0x08010001;
const SEEDS: [u32; 4] = [0x13579bdf, 0x2468ace0, 0x10203040, 0x50607080];
const DIAGNOSTICS: [&str; 4] = [
    "late-mode-unavailable",
    "late-angular-unavailable",
    "referent-partial-z-after-focus-dword",
    "settings-unavailable-after-refresh-writes",
];
const PREFIXES: [(&str, &str); 5] = [
    ("process-null-unavailable", "process_null"),
    ("released-current-target", "released_target"),
    ("follow-missing-registry", "registry"),
    ("follow-stale-unwritten-focus", "target_focus"),
    ("referent-position-unavailable", "referent_position"),
];
const BODIES: [(&str, u32, usize, u32, &str); 12] = [
    (
        "chase",
        0x4218e0,
        1095,
        8,
        "0c0f18760b1422402a1baf77cc8b7ad83d255203bbb52e91631842c3e68c827e",
    ),
    (
        "projection",
        0x421d30,
        32,
        12,
        "c88d57fedfd82cd15f31e11bf47078cc29cf9a77606e549bedca9ceb6c23bbeb",
    ),
    (
        "basis_query",
        0x4026b0,
        4,
        0,
        "b185eed3e098fd3dc404ff66db8be1121a2a2610cf0814ec915e002304c9bccf",
    ),
    (
        "parameter",
        0x40f190,
        43,
        8,
        "f68a3a9e176110b850978b0df2efed54b0db072494aea0c46aa2ec50078da833",
    ),
    (
        "vector_copy",
        0x40d1b0,
        25,
        4,
        "92782247fa786f1cbc0e78e43c76ba006a4daac83765e2ad52f6c71d41b9cea3",
    ),
    (
        "yaw",
        0x491530,
        672,
        4,
        "c2b2049b34fb459f4468d624bbd959eb891836ae30f0e86c21a7883a4ffed0d3",
    ),
    (
        "normalize_words",
        0x417da0,
        146,
        12,
        "8628949746c03273f9661ba840983d96a848c441e196b3d1da38c84b9cb69039",
    ),
    (
        "ftol",
        0x511ba8,
        39,
        0,
        "d0a2aca68648a0ae9d5cbfae1b276d4a64d4f797fc0c60f28b64096cdfa3148c",
    ),
    (
        "follow_refresh",
        0x482690,
        169,
        0,
        "6c73508648f687eb8daeb9fedcbf8417a57f2d35541083df8e1e409d56d748cc",
    ),
    (
        "resolver_wrapper",
        0x47ad90,
        13,
        4,
        "5bc4fc44fba4c6b1945c38f61570c479687c1b23e23f691c7e4d89c7db860271",
    ),
    (
        "resolver",
        0x462190,
        127,
        4,
        "2b2aca8473232d0d8f7ca4c7d9f080a1cbadddda0c26b210f4a397e37cbcc207",
    ),
    (
        "settings",
        0x463ab0,
        6,
        0,
        "466ca2f0114e8bc38cf6c67095b637b45624ce39c27e6e1540ecc18d9bea3e24",
    ),
];

fn number(v: &Value) -> u32 {
    u32::try_from(v.as_u64().unwrap()).unwrap()
}
fn optional(v: &Value) -> Option<u32> {
    (!v.is_null()).then(|| number(v))
}
fn word_array(v: &Value) -> Option<[u16; 3]> {
    v.as_array().map(|a| {
        assert_eq!(a.len(), 3);
        std::array::from_fn(|i| u16::try_from(number(&a[i])).unwrap())
    })
}
fn basis_words(v: &Value) -> NativeBasis {
    assert_eq!(v.as_array().unwrap().len(), 10);
    NativeBasis {
        words: std::array::from_fn(|i| number(&v[i])),
    }
}
fn hex(v: &str) -> Vec<u8> {
    assert_eq!(v.len() % 2, 0);
    v.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn digest(v: &[u8]) -> String {
    format!("{:x}", Sha256::digest(v))
}

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
    fn metadata(&self, v: &Value) {
        assert_eq!(v["executable_sha256"], EXE_SHA);
        assert_eq!(v["text_sha256"], TEXT_SHA);
        assert_eq!(v["text_address"], 0x401000);
        assert_eq!(v["text_bytes"], 0x11e000);
        assert_eq!(v["producer_sha256"], PRODUCER_SHA);
        assert_eq!(v["harness_sha256"], HARNESS_SHA);
        assert_eq!(v["bodies"].as_object().unwrap().len(), BODIES.len());
        for (name, address, width, cleanup, sha) in BODIES {
            assert_eq!(
                v["bodies"][name],
                json!({"address":address,"end":address+width as u32,"bytes":width,"return_cleanup":cleanup,"sha256":sha})
            );
            assert_eq!(digest(self.bytes(address, width)), sha);
        }
        for (name, address, width, sha) in [
            (
                "selector2_table",
                0x520860,
                76,
                "8e311e5e648c8da048a2f4c56796f404667a51e6a75429faed0d05cedd438dd6",
            ),
            (
                "follow_table",
                0x528038,
                28,
                "02d992b8c56c2b1699941ce5a50eafd3c6021ac20d8f274341a1c9537d7f51c5",
            ),
            (
                "fixed_table",
                0x51f154,
                28,
                "2344be6ce88b3ef7c4b37cae6c624978d2057797e92756f5e2bb6e2abc74adcd",
            ),
            (
                "sine_words",
                0x53df00,
                8192,
                "4bb036e0600ba85ef736d572cc19fb28082d2ca81ba4c8995b69ffc893f9b3d8",
            ),
        ] {
            assert_eq!(
                v["data"][name],
                json!({"address":address,"bytes":width,"sha256":sha})
            );
            assert_eq!(digest(self.bytes(address, width)), sha);
        }
        assert_eq!(v["data"].as_object().unwrap().len(), 4);
        assert_eq!(self.bytes(0x5208a8, 4), 0x4026b0_u32.to_le_bytes());
        assert_eq!(self.bytes(0x528050, 4), 0x482690_u32.to_le_bytes());
        for (address, bits) in [
            (0x5205b0, 0x45800000_u32),
            (0x5205ac, 0x43c80000),
            (0x520a6c, 0x43480000),
            (0x520a70, 0x3f800000),
            (0x51f198, 0),
            (0x528ce0, 0x38000100),
        ] {
            assert_eq!(v["constants"][address.to_string()], bits);
            assert_eq!(self.bytes(address, 4), bits.to_le_bytes());
        }
        assert_eq!(v["constants"].as_object().unwrap().len(), 6);
        // The original consumes the eye pointer before reusing precisely its
        // caller slot for the rotated horizontal length (entry ESP + 4).
        assert_eq!(self.bytes(0x421b0e, 4), hex("8b4c2474"));
        assert_eq!(self.bytes(0x421b96, 4), hex("8944246c"));
        assert_eq!(self.bytes(0x421bad, 4), hex("8944246c"));
        assert_eq!(self.bytes(0x421bc5, 4), hex("d954246c"));
        // Both genuine normalize_words tails convert their retained length.
        assert_eq!(self.bytes(0x417e14, 5), hex("e88f9d0f00"));
        assert_eq!(self.bytes(0x417e2a, 5), hex("e8799d0f00"));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Span {
    address: u32,
    bytes: Vec<u8>,
    known: Vec<bool>,
}
impl Span {
    fn unknown(address: u32, width: usize) -> Self {
        Self {
            address,
            bytes: vec![0xd3; width],
            known: vec![false; width],
        }
    }
    fn parse(v: &Value) -> Self {
        let bytes = hex(v["bytes"].as_str().unwrap());
        let known = v["known"]
            .as_str()
            .unwrap()
            .bytes()
            .map(|b| match b {
                b'0' => false,
                b'1' => true,
                _ => panic!("mask"),
            })
            .collect::<Vec<_>>();
        assert_eq!(bytes.len(), known.len());
        Self {
            address: number(&v["address"]),
            bytes,
            known,
        }
    }
    fn bind(&mut self, offset: usize, data: &[u8]) {
        self.bytes[offset..offset + data.len()].copy_from_slice(data);
        self.known[offset..offset + data.len()].fill(true);
    }
    fn scalar(&mut self, offset: usize, v: &Value, width: usize) {
        if let Some(value) = optional(v) {
            self.bind(offset, &value.to_le_bytes()[..width]);
        } else {
            self.known[offset..offset + width].fill(false);
        }
    }
    fn vector(&mut self, offset: usize, v: &Value, width: usize) {
        if let Some(a) = v.as_array() {
            for (i, value) in a.iter().enumerate() {
                self.scalar(offset + i * width, value, width);
            }
        } else {
            self.known[offset..offset + 3 * width].fill(false);
        }
    }
    fn complete(&self, offset: usize, width: usize) -> &[u8] {
        assert!(self.known[offset..offset + width].iter().all(|v| *v));
        &self.bytes[offset..offset + width]
    }
}
type Backing = BTreeMap<String, Span>;
fn parse_backing(v: &Value) -> Backing {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), Span::parse(v)))
        .collect()
}
fn apply_backing(state: &mut Backing, input: &Value) {
    if let Some(value) = input.get("process_null") {
        state.get_mut("registry").unwrap().scalar(20, value, 4);
    }
    if let Some(v) = input.get("vehicle") {
        let state = state.get_mut("vehicle").unwrap();
        for (key, offset) in [
            ("handle_1b4", 0x1b4),
            ("mode_1e8", 0x1e8),
            ("angular_98", 0x98),
        ] {
            if let Some(value) = v.get(key) {
                state.scalar(offset, value, 4);
            }
        }
        for (key, offset, width) in [
            ("position", 0x32, 2),
            ("velocity", 0x8e, 2),
            ("basis_words", 0x44, 4),
        ] {
            if let Some(value) = v.get(key) {
                state.vector(offset, value, width);
            }
        }
    }
    if let Some(v) = input.get("referent") {
        let state = state.get_mut("referent").unwrap();
        for (key, offset) in [("position", 0x32), ("velocity", 0x8e)] {
            if let Some(value) = v.get(key) {
                state.vector(offset, value, 2);
            }
        }
        for (key, offset, width) in [
            ("raise_word", 0x74, 2),
            ("flags", 0x2c, 4),
            ("lifecycle", 0xc4, 4),
        ] {
            if let Some(value) = v.get(key) {
                state.scalar(offset, value, width);
            }
        }
    }
    if let Some(v) = input.get("settings_7c") {
        state.get_mut("settings").unwrap().scalar(0, v, 4);
    }
    if let Some(v) = input.get("target") {
        state
            .get_mut("vehicle")
            .unwrap()
            .bind(0x148, &(if v.is_null() { 0 } else { TARGET }).to_le_bytes());
        if !v.is_null() {
            let state = state.get_mut("target").unwrap();
            state.bind(
                0,
                &(if v["target_class"] == "follow" {
                    0x528038_u32
                } else {
                    assert_eq!(v["target_class"], "fixed");
                    0x51f154
                })
                .to_le_bytes(),
            );
            state.scalar(4, &v["handle_04"], 4);
            state.scalar(16, &v["kind_10"], 4);
            state.vector(8, &v["raw_focus_08"], 2);
            if v["target_class"] == "follow" {
                state.vector(20, &v["virtual_focus_14"], 2);
                state.vector(26, &v["velocity_1a"], 2);
            }
        }
    }
}
fn initial_backing(input: &Value) -> Backing {
    let mut state = Backing::new();
    for (name, address, width) in [
        ("vehicle", VEHICLE, 0x300),
        ("referent", REFERENT, 0x100),
        ("target", TARGET, 0x20),
        ("camera", CAMERA, 0x180),
        ("registry", 0x54e880, 0x18),
        ("groups", 0x740000, 72),
        ("entries", 0x741000, 48),
        ("settings", 0x54e924, 4),
        ("stack", STACK - 0x800, 0x900),
    ] {
        state.insert(name.to_owned(), Span::unknown(address, width));
    }
    state
        .get_mut("vehicle")
        .unwrap()
        .bind(0, &0x520860_u32.to_le_bytes());
    state
        .get_mut("vehicle")
        .unwrap()
        .bind(0xb4, &2_u32.to_le_bytes());
    state
        .get_mut("camera")
        .unwrap()
        .bind(0x138, &hex(input["eye"].as_str().unwrap()));
    state
        .get_mut("camera")
        .unwrap()
        .bind(0x15c, &hex(input["look"].as_str().unwrap()));
    apply_backing(&mut state, input);
    if input["registry"]["present"] == true {
        for (offset, value) in [(0, 0_u32), (4, NATIVE_HANDLE + 7), (8, 0x740000), (12, 2)] {
            state
                .get_mut("registry")
                .unwrap()
                .bind(offset, &value.to_le_bytes());
        }
        state.get_mut("groups").unwrap().bind(0, &[0; 72]);
        state
            .get_mut("groups")
            .unwrap()
            .bind(40, &4_u32.to_le_bytes());
        state
            .get_mut("groups")
            .unwrap()
            .bind(44, &0x741000_u32.to_le_bytes());
        state.get_mut("entries").unwrap().bind(0, &[0; 48]);
        let handle = if input["registry"]["resolves"] == true {
            NATIVE_HANDLE
        } else {
            NATIVE_HANDLE + 1
        };
        state
            .get_mut("entries")
            .unwrap()
            .bind(12, &handle.to_le_bytes());
        state
            .get_mut("entries")
            .unwrap()
            .bind(16, &REFERENT.to_le_bytes());
    }
    state
}
fn merge_inputs(current: &mut Value, patch: &Value) {
    for (key, value) in patch.as_object().unwrap() {
        if ["vehicle", "referent", "registry"].contains(&key.as_str()) {
            for (k, v) in value.as_object().unwrap() {
                current[key][k] = v.clone();
            }
        } else {
            current[key] = value.clone();
        }
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
    completed: u32,
    calls: u32,
}
#[derive(Clone)]
struct Frame {
    body: String,
    entry: u32,
    sp: u32,
    return_ip: u32,
    ecx: u32,
    writes: usize,
}
fn body(name: &str) -> (u32, usize, u32) {
    let (_, a, w, c, _) = BODIES.iter().copied().find(|b| b.0 == name).unwrap();
    (a, w, c)
}

fn audit_call(
    original: &Original<'_>,
    case: &Value,
    call: &Value,
    state: &mut Backing,
    counts: &mut Counts,
) {
    let request = &call["request"];
    let projection = request["operation"] == "projection";
    let arguments = if projection {
        request["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(number)
            .collect::<Vec<_>>()
    } else {
        vec![EYE, LOOK]
    };
    let mut caller = RETURN.to_le_bytes().to_vec();
    for value in &arguments {
        caller.extend_from_slice(&value.to_le_bytes());
    }
    state.get_mut("stack").unwrap().bind(0x800, &caller);
    assert_eq!(
        parse_backing(&call["initial"]),
        *state,
        "{} initial",
        case["name"]
    );
    let initial = state.clone();
    let mut frames = Vec::<Frame>::new();
    let mut eye_done = false;
    let mut look_done = false;
    let mut mode_read = false;
    let mut angular_read = false;
    let mut refresh_done = false;
    let mut target_reread = false;
    let mut focus_reads = Vec::new();
    let mut basis_returns = Vec::new();
    let mut parameter_order = Vec::new();
    let mut rotated_quantized = Vec::new();
    let mut caller_eye_scratch = None;
    let events = call["events"].as_array().unwrap();
    if !projection {
        assert_eq!(events[1]["kind"], "read");
        assert_eq!(events[1]["ip"], 0x4218e4);
        assert_eq!(events[1]["region"], "registry");
        assert_eq!(events[1]["offset"], 20);
        assert_eq!(events[1]["size"], 4);
    }
    for (index, event) in events.iter().enumerate() {
        let kind = event["kind"].as_str().unwrap();
        match kind {
            "entry" => {
                let name = event["body"].as_str().unwrap();
                let (address, _, cleanup) = body(name);
                assert_eq!(event["entry"], address);
                assert_eq!(event["fpcw"], 0x27f);
                assert_eq!(
                    event["arguments"].as_array().unwrap().len(),
                    (cleanup / 4) as usize
                );
                if frames.is_empty() {
                    assert_eq!(index, 0);
                    assert_eq!(name, if projection { "projection" } else { "chase" });
                    assert_eq!(
                        event["ecx"],
                        if projection { VEHICLE + 0x60 } else { VEHICLE }
                    );
                    assert_eq!(event["return_ip"], RETURN);
                    assert_eq!(event["sp"], STACK);
                    assert_eq!(event["arguments"], json!(arguments));
                    assert_eq!(event["saved"], json!(SEEDS));
                } else {
                    let parent = frames.last().unwrap();
                    let return_ip = number(&event["return_ip"]);
                    assert!((parent.entry..parent.entry + body(&parent.body).1 as u32)
                        .contains(&return_ip));
                    match name {
                        "basis_query" => {
                            assert_eq!(parent.body, "chase");
                            assert_eq!(event["ecx"], VEHICLE);
                        }
                        "projection" => {
                            assert_eq!(parent.body, "chase");
                            assert_eq!(return_ip, 0x4219c6);
                            assert_eq!(event["ecx"], VEHICLE + 0x60);
                            let v = &call["current_inputs"]["vehicle"]["velocity"];
                            assert_eq!(
                                event["arguments"],
                                json!(std::array::from_fn::<_, 3, _>(
                                    |i| number(&v[i]) as u16 as i16 as i32 as u32
                                ))
                            );
                        }
                        "parameter" => {
                            assert_eq!(parent.body, "chase");
                            assert_eq!(event["arguments"][1], VEHICLE + 0x8e);
                            let receiver = number(&event["ecx"]);
                            assert_eq!(
                                receiver,
                                if parameter_order.is_empty() {
                                    EYE
                                } else {
                                    LOOK
                                }
                            );
                            if receiver == EYE {
                                assert_eq!(return_ip, 0x421b1c);
                                assert!(!mode_read);
                            } else {
                                assert!(eye_done && mode_read);
                                assert!(!look_done);
                                if call["current_inputs"]["target"].get("kind_10")
                                    == Some(&json!(2))
                                {
                                    assert!(refresh_done && target_reread);
                                    assert_eq!(focus_reads, vec![12, 10, 8]);
                                }
                            }
                            parameter_order.push(receiver);
                        }
                        "follow_refresh" => {
                            assert_eq!(parent.body, "chase");
                            assert!(eye_done);
                            assert_eq!(event["ecx"], TARGET);
                            assert_eq!(return_ip, 0x421c33);
                        }
                        "resolver_wrapper" => assert_eq!(parent.body, "follow_refresh"),
                        "resolver" => assert_eq!(parent.body, "resolver_wrapper"),
                        "settings" => assert_eq!(parent.body, "follow_refresh"),
                        "yaw" => {
                            assert_eq!(parent.body, "chase");
                            assert!(eye_done && angular_read);
                            let angle =
                                number(&call["current_inputs"]["vehicle"]["angular_98"]) as i32 / 4;
                            assert_ne!(angle, 0);
                            assert_eq!(event["arguments"], json!([angle as u32]));
                        }
                        "vector_copy" => assert_eq!(parent.body, "chase"),
                        "normalize_words" => {
                            assert!(["chase", "yaw"].contains(&parent.body.as_str()))
                        }
                        "ftol" => match parent.body.as_str() {
                            "chase" | "projection" => {}
                            "normalize_words" => {
                                assert!([0x417e19, 0x417e2f].contains(&return_ip));
                            }
                            other => panic!("unowned ftol parent {other}"),
                        },
                        _ => panic!("unowned frame {name}"),
                    }
                }
                frames.push(Frame {
                    body: name.to_owned(),
                    entry: address,
                    sp: number(&event["sp"]),
                    return_ip: number(&event["return_ip"]),
                    ecx: number(&event["ecx"]),
                    writes: 0,
                });
                *counts.entries.entry(name.to_owned()).or_default() += 1;
            }
            "return" => {
                let frame = frames.pop().unwrap();
                let (_, width, cleanup) = body(&frame.body);
                let ip = number(&event["ip"]);
                assert_eq!(event["body"], frame.body);
                assert_eq!(event["sp"], frame.sp);
                assert_eq!(event["return_ip"], frame.return_ip);
                assert_eq!(event["cleanup"], cleanup);
                assert!((frame.entry..frame.entry + width as u32).contains(&ip));
                let opcode = if cleanup == 0 {
                    vec![0xc3]
                } else {
                    vec![0xc2, cleanup as u8, 0]
                };
                assert_eq!(original.bytes(ip, opcode.len()), opcode);
                if frame.body == "basis_query" {
                    assert_eq!(event["raw_eax"], VEHICLE + 0x44);
                    basis_returns.push(frame.return_ip);
                }
                if frame.body == "parameter" {
                    assert_eq!(frame.writes, 4);
                    if frame.ecx == EYE {
                        eye_done = true;
                    } else {
                        look_done = true;
                    }
                }
                if frame.body == "follow_refresh" {
                    refresh_done = true;
                }
                if frame.body == "ftol" && [0x421b96, 0x421bad].contains(&frame.return_ip) {
                    assert!(eye_done && angular_read);
                    assert_eq!(
                        frame.return_ip,
                        [0x421b96, 0x421bad][rotated_quantized.len()]
                    );
                    rotated_quantized.push(number(&event["raw_eax"]) as i32);
                }
                if frame.return_ip == RETURN {
                    assert_eq!(index + 1, events.len());
                    assert!(call["completed"] == true);
                    assert_eq!(event["raw_eax"], call["raw_eax"]);
                }
                *counts.returns.entry(frame.body).or_default() += 1;
            }
            "read" | "pe_read" | "write" => {
                let active = frames.last_mut().unwrap();
                let ip = number(&event["ip"]);
                assert!((active.entry..active.entry + body(&active.body).1 as u32).contains(&ip));
                let width = number(&event["size"]) as usize;
                if kind == "pe_read" {
                    assert_eq!(
                        original.bytes(number(&event["address"]), width),
                        hex(event["bytes"].as_str().unwrap())
                    );
                    counts.reads += 1;
                    continue;
                }
                let region = event["region"].as_str().unwrap();
                let offset = number(&event["offset"]) as usize;
                let span = state.get_mut(region).unwrap();
                if kind == "read" {
                    assert_eq!(
                        &span.bytes[offset..offset + width],
                        hex(event["bytes"].as_str().unwrap())
                    );
                    let known = span.known[offset..offset + width].iter().all(|v| *v);
                    assert_eq!(event["known"], known);
                    counts.reads += 1;
                    if !known {
                        assert_eq!(events[index + 1]["kind"], "stop");
                        assert_eq!(index + 2, events.len());
                    }
                    if region == "vehicle" && [0x1e8, 0x98, 0x148].contains(&offset) {
                        assert!(eye_done);
                        assert_eq!(active.body, "chase");
                        if offset == 0x1e8 {
                            assert!(!mode_read);
                            mode_read = true;
                        }
                        if offset == 0x98 {
                            assert!(mode_read);
                            assert!(!angular_read);
                            assert!([1, 2]
                                .contains(&number(&call["current_inputs"]["vehicle"]["mode_1e8"])));
                            angular_read = true;
                        }
                        if offset == 0x148 && refresh_done {
                            assert!(!target_reread);
                            target_reread = true;
                        }
                    }
                    if region == "target" && [8, 10, 12].contains(&offset) && active.body == "chase"
                    {
                        assert!(target_reread);
                        focus_reads.push(offset);
                        assert_eq!(focus_reads, &[12, 10, 8][..focus_reads.len()]);
                    }
                } else {
                    assert!(["camera", "target", "registry"].contains(&region));
                    assert_eq!(
                        &span.bytes[offset..offset + width],
                        hex(event["before"].as_str().unwrap())
                    );
                    if region == "camera" {
                        assert_eq!(active.body, "parameter");
                        let writes = [
                            (0x12, 4, 0x40f19d),
                            (0x16, 2, 0x40f1a4),
                            (0x0c, 4, 0x40f1ae),
                            (0x10, 2, 0x40f1b4),
                        ];
                        let (off, size, write_ip) = writes[active.writes];
                        assert_eq!(offset, active.ecx as usize - CAMERA as usize + off);
                        assert_eq!(width, size);
                        assert_eq!(ip, write_ip);
                        active.writes += 1;
                    }
                    if region == "target" {
                        assert_eq!(active.body, "follow_refresh");
                        assert!(eye_done && !look_done);
                    }
                    if region == "registry" {
                        assert_eq!(active.body, "resolver");
                        assert!([0, 4].contains(&offset));
                        assert_eq!(width, 4);
                    }
                    let data = hex(event["bytes"].as_str().unwrap());
                    assert_eq!(data.len(), width);
                    span.bind(offset, &data);
                    counts.writes += 1;
                }
            }
            "phase" => {
                let active = frames.last().unwrap();
                assert_eq!(active.body, "chase");
                let (ip, width) = match event["name"].as_str().unwrap() {
                    "first_normal" => (0x4219a0, Some(12)),
                    "clamped_stand_off" => (0x4219e7, None),
                    "dead_200_words" => (0x421afe, Some(6)),
                    "eye_published" => (0x421b1c, Some(6)),
                    "rotated_basis" => (0x421b87, Some(40)),
                    "late_target_phase" => (0x421c14, Some(12)),
                    "after_refresh" => (0x421c33, None),
                    other => panic!("unowned phase {other}"),
                };
                assert_eq!(event["ip"], ip);
                if let Some(width) = width {
                    assert_eq!(hex(event["bytes"].as_str().unwrap()).len(), width);
                }
                if event["name"] == "eye_published" {
                    assert!(eye_done);
                    assert_eq!(
                        hex(event["bytes"].as_str().unwrap()),
                        state["camera"].complete(0x14a, 6)
                    );
                }
                if event["name"] == "rotated_basis" {
                    assert!(eye_done && angular_read);
                }
                if event["name"] == "late_target_phase" {
                    assert!(eye_done && mode_read);
                    if angular_read
                        && number(&call["current_inputs"]["vehicle"]["angular_98"]) as i32 / 4 != 0
                    {
                        assert_eq!(rotated_quantized.len(), 2);
                        let x = f64::from(rotated_quantized[0]);
                        let z = f64::from(rotated_quantized[1]);
                        caller_eye_scratch =
                            Some(((f64::from(z as f32) * z + x * x).sqrt() as f32).to_le_bytes());
                    } else {
                        assert!(rotated_quantized.is_empty());
                    }
                }
                if event["name"] == "after_refresh" {
                    assert!(refresh_done);
                }
            }
            "stop" => {
                assert_eq!(index + 1, events.len());
                let mut expected = call["stop"].clone();
                expected["kind"] = json!("stop");
                assert_eq!(event, &expected);
                assert_eq!(event["reason"], case["expected_stop"]);
                assert!(call["completed"] == false);
                if event["reason"] == "released_target" {
                    assert_eq!(event["ip"], 0x421c22);
                    assert!(eye_done && !look_done);
                }
            }
            _ => panic!("unowned recorded event {kind}"),
        }
    }
    let final_state = parse_backing(&call["final"]);
    for (name, span) in state.iter() {
        if name != "stack" {
            assert_eq!(span, &final_state[name], "{} final {name}", case["name"]);
        }
    }
    let old_stack = &initial["stack"];
    let new_stack = &final_state["stack"];
    for (old, new) in old_stack.known.iter().zip(&new_stack.known) {
        assert!(!*old || *new);
    }
    if projection {
        assert_eq!(
            old_stack.complete(0x800, caller.len()),
            new_stack.complete(0x800, caller.len())
        );
    } else {
        assert_eq!(old_stack.complete(0x800, 4), new_stack.complete(0x800, 4));
        assert_eq!(old_stack.complete(0x808, 4), new_stack.complete(0x808, 4));
        // Known stack accesses are online-only; this checks the surviving
        // caller-slot value from the source-proved stores and recorded ftol
        // returns, without inventing a serialized internal-write timeline.
        let expected = caller_eye_scratch
            .as_ref()
            .map(|v| &v[..])
            .unwrap_or_else(|| old_stack.complete(0x804, 4));
        assert_eq!(new_stack.complete(0x804, 4), expected);
    }
    if call["completed"] == true {
        assert!(frames.is_empty());
        assert!(projection || (eye_done && look_done));
        assert_eq!(call["final_esp"], STACK + 4 + arguments.len() as u32 * 4);
        assert_eq!(call["final_fpcw"], 0x27f);
        assert_eq!(number(&call["final_fpsw"]) & 0x3800, 0);
        counts.completed += 1;
        if !projection {
            assert_eq!(
                &basis_returns[..4],
                &[0x42190c, 0x421915, 0x421a27, 0x421a30]
            );
            if angular_read
                && number(&call["current_inputs"]["vehicle"]["angular_98"]) as i32 / 4 != 0
            {
                assert_eq!(basis_returns[4], 0x421b4e);
                assert_eq!(basis_returns.len(), 5);
            } else {
                assert_eq!(basis_returns.len(), 4);
            }
        }
    } else {
        assert!(!frames.is_empty());
        assert!(!look_done);
    }
    *state = final_state;
    counts.calls += 1;
    counts.stack_reads += number(&call["stack_reads"]);
    counts.stack_writes += number(&call["stack_writes"]);
}

// The actual phase consumes genuine owners. Input supply is independent from
// the receipt outputs; no expected vector/target/cache is fed to that phase.
static SINE_CONSTRUCTION: [u16; 4096] = [0; 4096];
fn allocation(position: Option<[u16; 3]>) -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: position,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: Some(0),
            sine_quarter: &SINE_CONSTRUCTION,
        },
    }
}
fn constructor_arena(input: &Value) -> (ObjectArena, ObjectId, ObjectId, u32) {
    let bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    let span = ByteSpan::new(0, bytes.len());
    let terrain = parse_terrain_bank(
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
        &terrain,
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
    let mut request = allocation(word_array(&input["vehicle"]["position"]));
    request.terrain.terrain = Some(ActorTerrainSurface {
        terrain: &terrain,
        plane_height_fixed: terrain.plane_base(),
    });
    let mut rng = 17;
    let vehicle = arena
        .allocate_selector2_actor(
            DerivedActorConstructionRequest {
                actor: request,
                world_mask_limits: [3, 3],
            },
            &mut rng,
        )
        .unwrap();
    let referent = arena
        .allocate_actor(allocation(word_array(&input["referent"]["position"])))
        .unwrap();
    (arena, vehicle, referent, rng)
}
fn target_handle(arena: &ObjectArena, referent: ObjectId, input: &Value) -> ActorHandle {
    assert_eq!(input["target"]["handle_04"], NATIVE_HANDLE);
    let handle = arena.actor(referent).unwrap().handle;
    // Same live allocation with a mismatching generation represents the supplied
    // stale registry entry. Neither object lifetime nor cache is fabricated.
    if input["registry"]["resolves"] == true {
        handle
    } else {
        ActorHandle::from_raw(
            (handle.raw() & 0xffff0000) | u32::from(handle.generation().wrapping_add(1)),
        )
    }
}
fn supply_target(arena: &mut ObjectArena, vehicle: ObjectId, referent: ObjectId, input: &Value) {
    let v = &input["target"];
    let target = if v.is_null() {
        None
    } else if v["target_class"] == "follow" {
        let mut target = arena
            .follow_target(FollowTargetRequest {
                handle: target_handle(arena, referent, input),
                point: FollowTargetPoint::ExplicitPoint(word_array(&v["virtual_focus_14"])),
            })
            .unwrap();
        target.focus_08 = word_array(&v["raw_focus_08"]);
        target.velocity_1a = word_array(&v["velocity_1a"]);
        target.word_10 = number(&v["kind_10"]);
        Some(NativeTarget::ActorFollow(target))
    } else {
        assert_eq!(v["target_class"], "fixed");
        assert_eq!(v["kind_10"], 8);
        let mut target = arena
            .fixed_position_target(word_array(&v["raw_focus_08"]))
            .unwrap();
        target.handle_04 = ActorHandle::from_raw(number(&v["handle_04"]));
        Some(NativeTarget::FixedPosition(target))
    };
    let target = target.map(|data| arena.import_fixture_target(data));
    if !v.is_null() && v["custody"] == "released" {
        arena.release_actor_service(target.unwrap()).unwrap();
    }
    arena.derived_actor_mut(vehicle).unwrap().target_148 = target;
}
fn supply_fields(arena: &mut ObjectArena, vehicle: ObjectId, referent: ObjectId, patch: &Value) {
    if let Some(v) = patch.get("vehicle") {
        if let Some(value) = v.get("position") {
            arena
                .initialize_context(
                    vehicle,
                    BaseContext {
                        position_words: word_array(value),
                        placement_index: 0,
                    },
                )
                .unwrap();
        }
        if let Some(value) = v.get("velocity") {
            arena.actor_mut(vehicle).unwrap().words_8e_92 = word_array(value).unwrap();
        }
        if let Some(value) = v.get("basis_words") {
            arena.actor_mut(vehicle).unwrap().rotating.basis = basis_words(value);
        }
        if let Some(value) = v.get("angular_98").and_then(optional) {
            arena.actor_mut(vehicle).unwrap().words_94_9c[1] = value;
        }
        if let Some(value) = v.get("handle_1b4") {
            arena.selector2_actor_mut(vehicle).unwrap().bytes_1b4_1bf[..4]
                .copy_from_slice(&number(value).to_le_bytes());
        }
        if let Some(value) = v.get("mode_1e8").and_then(optional) {
            arena.selector2_actor_mut(vehicle).unwrap().dwords_1e8_1f4[0] = value;
        }
    }
    if let Some(v) = patch.get("referent") {
        if let Some(value) = v.get("position") {
            arena
                .initialize_context(
                    referent,
                    BaseContext {
                        position_words: word_array(value),
                        placement_index: 0,
                    },
                )
                .unwrap();
        }
        if let Some(value) = v.get("velocity") {
            arena.actor_mut(referent).unwrap().words_8e_92 = word_array(value).unwrap();
        }
        if let Some(value) = v.get("raise_word") {
            arena.actor_mut(referent).unwrap().word_74 = u16::try_from(number(value)).unwrap();
        }
        if let Some(value) = v.get("lifecycle") {
            arena.actor_mut(referent).unwrap().word_c4 = number(value);
        }
        if let Some(value) = v.get("flags") {
            arena.owned_mut(referent).unwrap().state.base.flags = number(value);
        }
    }
}
fn parameter(bytes: &[u8]) -> CameraParameterState {
    assert_eq!(bytes.len(), 24);
    let vector = |offset| {
        Some(std::array::from_fn(|i| {
            u16::from_le_bytes(
                bytes[offset + i * 2..offset + i * 2 + 2]
                    .try_into()
                    .unwrap(),
            )
        }))
    };
    CameraParameterState {
        current_velocity: vector(0),
        current_position: vector(6),
        desired_velocity: vector(12),
        desired_position: vector(18),
    }
}
fn parameter_bytes(v: CameraParameterState) -> Vec<u8> {
    [
        v.current_velocity,
        v.current_position,
        v.desired_velocity,
        v.desired_position,
    ]
    .into_iter()
    .flat_map(|words| words.unwrap().into_iter().flat_map(u16::to_le_bytes))
    .collect()
}
fn objects(arena: &ObjectArena) -> Vec<Option<ObjectState>> {
    arena
        .slots
        .iter()
        .map(|slot| slot.value.as_ref().map(|v| v.state.clone()))
        .collect()
}
fn custody(arena: &ObjectArena) -> String {
    // Object contents and resolver cache have their own exact comparisons.
    format!(
        "{:?}",
        (
            (
                arena.identity,
                &arena.free,
                arena.live_count,
                &arena.lists,
                &arena.list_owners,
                &arena.routes,
                &arena.actors,
                &arena.map_annotations
            ),
            (
                &arena.actor_payloads,
                &arena.constructor_allocations,
                &arena.selector60_rows,
                &arena.players,
                &arena.world_load
            ),
            arena
                .slots
                .iter()
                .map(|slot| (slot.generation, slot.value.as_ref().map(|v| v.memberships)))
                .collect::<Vec<_>>(),
            arena.actor_handles.as_ref().map(|r| (
                r.initial_capacity(),
                r.null_handle(),
                r.owners()
            ))
        )
    )
}
fn span_words(span: &Span, offset: usize) -> Option<[u16; 3]> {
    let mask = &span.known[offset..offset + 6];
    if mask.iter().all(|v| !*v) {
        None
    } else {
        assert!(mask.iter().all(|v| *v));
        Some(std::array::from_fn(|i| {
            u16::from_le_bytes(
                span.bytes[offset + 2 * i..offset + 2 * i + 2]
                    .try_into()
                    .unwrap(),
            )
        }))
    }
}
fn compare_declared_fields(
    arena: &ObjectArena,
    vehicle: ObjectId,
    referent: ObjectId,
    state: &Backing,
) {
    let v = &state["vehicle"];
    let r = &state["referent"];
    assert_eq!(
        arena.object(vehicle).unwrap().base.position_words,
        span_words(v, 0x32)
    );
    assert_eq!(
        arena.actor(vehicle).unwrap().words_8e_92,
        span_words(v, 0x8e).unwrap()
    );
    let basis = arena.actor(vehicle).unwrap().rotating.basis;
    assert_eq!(
        basis
            .words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>(),
        v.complete(0x44, 40)
    );
    assert_eq!(
        &arena.selector2_actor(vehicle).unwrap().bytes_1b4_1bf[..4],
        v.complete(0x1b4, 4)
    );
    assert_eq!(
        arena.selector2_actor(vehicle).unwrap().dwords_1e8_1f4[0].to_le_bytes(),
        v.complete(0x1e8, 4)
    );
    if v.known[0x98..0x9c].iter().all(|x| *x) {
        assert_eq!(
            arena.actor(vehicle).unwrap().words_94_9c[1].to_le_bytes(),
            v.complete(0x98, 4)
        );
    }
    assert_eq!(
        arena.object(referent).unwrap().base.position_words,
        span_words(r, 0x32)
    );
    assert_eq!(
        arena.actor(referent).unwrap().words_8e_92,
        span_words(r, 0x8e).unwrap()
    );
    assert_eq!(
        arena.actor(referent).unwrap().word_74.to_le_bytes(),
        r.complete(0x74, 2)
    );
    assert_eq!(
        arena.actor(referent).unwrap().word_c4.to_le_bytes(),
        r.complete(0xc4, 4)
    );
    assert_eq!(
        arena.object(referent).unwrap().base.flags.to_le_bytes(),
        r.complete(0x2c, 4)
    );
}
fn expected_target(before: Option<NativeTarget>, span: &Span) -> Option<NativeTarget> {
    before.map(|mut target| {
        match &mut target {
            NativeTarget::ActorFollow(v) => {
                v.focus_08 = span_words(span, 8);
                v.focus_14 = span_words(span, 20);
                v.velocity_1a = span_words(span, 26);
                assert_eq!(v.word_10.to_le_bytes(), span.complete(16, 4));
            }
            NativeTarget::FixedPosition(v) => {
                v.focus_08 = span_words(span, 8);
                assert_eq!(v.word_10.to_le_bytes(), span.complete(16, 4));
            }
            NativeTarget::ObjectPosition(_) | NativeTarget::FurniturePosition(_) => {
                panic!("unadmitted target class")
            }
        }
        target
    })
}
fn target_data(
    arena: &ObjectArena,
    reference: Option<crate::object_runtime::NativeHeapRef>,
) -> Option<NativeTarget> {
    reference.and_then(
        |reference| match arena.actor_service_heap().state(reference).unwrap() {
            crate::object_runtime::ActorServiceResourceState::Released => None,
            crate::object_runtime::ActorServiceResourceState::Target(data) => Some(data),
            state => panic!("unexpected supplied target state {state:?}"),
        },
    )
}

#[test]
fn mutable_vehicle_targets_match_original_native_receipts() {
    assert_eq!(digest(FIXTURE), FIXTURE_SHA);
    let fixture: Value = serde_json::from_slice(FIXTURE).unwrap();
    assert_eq!(fixture["schema"], "vehicle_camera_targets_native_v1");
    assert_eq!(fixture["float_control_word"], 0x27f);
    assert_eq!(
        fixture["projection_domain"],
        "direct signed DWORD; Chase caller signextends WORD"
    );
    assert_eq!(fixture["stack_scope"],"all known reads validated online; counts and full masked state; no serialized known-stack timeline");
    assert_eq!(fixture["boundary"],"supplied canonical selector2 and target storage; original callbacks/math; stopped masks; no mode4, constructor, aliases, global immutability, enclosing tick or visual acceptance");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let raw = std::fs::read(root.join("Infest.exe")).unwrap();
    let original = Original::new(&raw);
    original.metadata(&fixture["metadata"]);
    let math = NativeBasisMath::new(assets.sine_table().values);
    let mut counts = Counts::default();
    let mut names = BTreeSet::new();
    let mut skipped = BTreeSet::new();
    let mut prefix_names = BTreeSet::new();
    let (mut typed_histories, mut chase_calls, mut projection_calls, mut prefix_calls) =
        (0, 0, 0, 0);
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        assert!(names.insert(name));
        let mut current = case["input"].clone();
        let mut native = initial_backing(&current);
        let calls = case["calls"].as_array().unwrap();
        let steps = case["steps"].as_array().unwrap();
        assert_eq!(steps.len(), calls.len());
        for (step, call) in steps.iter().zip(calls) {
            assert_eq!(step, &call["request"]);
            merge_inputs(&mut current, &step["patches"]);
            assert_eq!(current, call["current_inputs"]);
            apply_backing(&mut native, &step["patches"]);
            audit_call(&original, case, call, &mut native, &mut counts);
        }
        // Exclusion precedes all genuine construction or unknown-scalar supply.
        if DIAGNOSTICS.contains(&name) {
            assert_eq!(case["admission"], "diagnostic");
            assert!(skipped.insert(name));
            continue;
        }
        assert_ne!(case["admission"], "diagnostic");
        typed_histories += 1;
        if case["admission"] == "prefix" {
            let reason = PREFIXES.iter().find(|p| p.0 == name).unwrap().1;
            assert_eq!(case["expected_stop"], reason);
            prefix_names.insert(name);
        } else {
            assert_eq!(case["admission"], "complete");
            assert!(case["expected_stop"].is_null());
        }
        let mut current = case["input"].clone();
        let (mut arena, vehicle, referent, _construction_rng) = constructor_arena(&current);
        supply_fields(&mut arena, vehicle, referent, &current);
        supply_target(&mut arena, vehicle, referent, &current);
        if current["registry"]["present"] == false {
            arena.actor_handles = None;
        }
        let mut process = ParticleFactoryProcessState {
            network_flag: 0xface0001,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: optional(&current["process_null"]).map(ActorHandle::from_raw),
        };
        let mut eye = parameter(&hex(current["eye"].as_str().unwrap()));
        let mut look = parameter(&hex(current["look"].as_str().unwrap()));
        let mut camera_basis = NativeBasis::INITIAL;
        for (step, call) in steps.iter().zip(calls) {
            let patch = &step["patches"];
            merge_inputs(&mut current, patch);
            supply_fields(&mut arena, vehicle, referent, patch);
            if patch.get("process_null").is_some() {
                process.process_null_handle =
                    optional(&current["process_null"]).map(ActorHandle::from_raw);
            }
            assert!(patch.get("target").is_none());
            assert!(patch.get("registry").is_none());
            let initial = parse_backing(&call["initial"]);
            compare_declared_fields(&arena, vehicle, referent, &initial);
            assert_eq!(parameter_bytes(eye), initial["camera"].complete(0x138, 24));
            assert_eq!(parameter_bytes(look), initial["camera"].complete(0x15c, 24));
            let before_objects = objects(&arena);
            let before_custody = custody(&arena);
            let before_process = format!("{process:?}");
            let before_basis = camera_basis;
            let before_cache = arena.actor_handles.as_ref().map(|r| r.cache());
            let before_reference = arena.derived_actor(vehicle).unwrap().target_148;
            let before_target = target_data(&arena, before_reference);
            let heap_attempts = arena.actor_service_heap().attempts().len();
            assert_eq!(
                before_target,
                expected_target(before_target, &initial["target"])
            );
            if step["operation"] == "projection" {
                assert_eq!(case["admission"], "complete");
                projection_calls += 1;
                let args = std::array::from_fn(|i| number(&step["arguments"][i]) as i32);
                assert_eq!(
                    projection_421d30(basis_z(arena.actor(vehicle).unwrap().rotating.basis), args)
                        as u32,
                    number(&call["raw_eax"])
                );
            } else {
                assert_eq!(step["operation"], "targets_mode3");
                chase_calls += 1;
                let result = arena.update_selector2_camera_targets(
                    CameraDerivedTargetRequest {
                        component: CameraDerivedComponent {
                            object: vehicle,
                            special_view_active: false,
                        },
                        mode: CameraDerivedTargetMode::Chase,
                        primary_target: None,
                        eye: &mut eye,
                        look: &mut look,
                        basis: &mut camera_basis,
                        settings_7c: optional(&current["settings_7c"]).unwrap(),
                        math: &math,
                    },
                    Some(&process),
                );
                if call["completed"] == true {
                    result.unwrap();
                } else {
                    prefix_calls += 1;
                    let error = match case["expected_stop"].as_str().unwrap() {
                        "process_null" => CameraPoseError::MissingVehicleProcessNullHandle,
                        "released_target" => ObjectError::ActorServiceHeap(
                            crate::object_runtime::ActorServiceHeapError::Released(
                                before_reference.unwrap(),
                            ),
                        )
                        .into(),
                        "registry" => ObjectError::MissingActorRegistry.into(),
                        "target_focus" => CameraPoseError::UnwrittenTarget("vehicle +148 raw +08"),
                        "referent_position" => ObjectError::ActorPositionUnknown(referent).into(),
                        other => panic!("unadmitted prefix {other}"),
                    };
                    assert_eq!(result, Err(error));
                }
            }
            let final_state = parse_backing(&call["final"]);
            assert_eq!(
                parameter_bytes(eye),
                final_state["camera"].complete(0x138, 24)
            );
            assert_eq!(
                parameter_bytes(look),
                final_state["camera"].complete(0x15c, 24)
            );
            assert_eq!(objects(&arena), before_objects);
            assert_eq!(
                arena.derived_actor(vehicle).unwrap().target_148,
                before_reference
            );
            assert_eq!(
                target_data(&arena, before_reference),
                expected_target(before_target, &final_state["target"])
            );
            assert_eq!(arena.actor_service_heap().attempts().len(), heap_attempts);
            compare_declared_fields(&arena, vehicle, referent, &final_state);
            assert_eq!(custody(&arena), before_custody);
            assert_eq!(camera_basis, before_basis);
            assert_eq!(format!("{process:?}"), before_process);
            let resolution_return = call["events"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["kind"] == "return" && e["body"] == "resolver");
            if let Some(event) = resolution_return.filter(|e| e["raw_eax"] == REFERENT) {
                assert_eq!(event["raw_eax"], REFERENT);
                let cache = arena.actor_handles.as_ref().unwrap().cache();
                assert_eq!(cache.object, Some(referent));
                assert_eq!(cache.handle, arena.actor(referent).unwrap().handle);
            } else {
                assert_eq!(
                    arena.actor_handles.as_ref().map(|r| r.cache()),
                    before_cache
                );
            }
        }
    }
    assert_eq!(names.len(), 45);
    assert_eq!(skipped, DIAGNOSTICS.into_iter().collect::<BTreeSet<_>>());
    assert_eq!(
        prefix_names,
        PREFIXES.into_iter().map(|p| p.0).collect::<BTreeSet<_>>()
    );
    assert_eq!(
        (typed_histories, chase_calls, projection_calls, prefix_calls),
        (41, 38, 6, 5)
    );
    let expected_entries:Value=serde_json::from_str(r#"{"basis_query":173,"chase":42,"follow_refresh":12,"ftol":583,"normalize_words":9,"parameter":74,"projection":47,"resolver":12,"resolver_wrapper":12,"settings":6,"vector_copy":27,"yaw":9}"#).unwrap();
    let expected_returns:Value=serde_json::from_str(r#"{"basis_query":173,"chase":33,"follow_refresh":8,"ftol":583,"normalize_words":9,"parameter":74,"projection":47,"resolver":11,"resolver_wrapper":11,"settings":6,"vector_copy":27,"yaw":9}"#).unwrap();
    assert_eq!(
        serde_json::to_value(&counts.entries).unwrap(),
        expected_entries
    );
    assert_eq!(
        serde_json::to_value(&counts.returns).unwrap(),
        expected_returns
    );
    assert_eq!(
        (
            counts.calls,
            counts.completed,
            counts.reads,
            counts.writes,
            counts.stack_reads,
            counts.stack_writes
        ),
        (48, 39, 2281, 344, 6717, 5337)
    );
    assert_eq!(
        fixture["counts"],
        json!({"body_entries":expected_entries,"body_returns":expected_returns,"calls":48,"completed":39,"diagnostic_histories":4,"histories":45,"nonstack_reads":2281,"nonstack_writes":344,"prefix_histories":5,"stack_reads":6717,"stack_writes":5337})
    );
}
