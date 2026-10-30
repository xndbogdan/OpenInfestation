//! Original-byte evidence for the bounded live-grant composition. This module
//! never calls the portable grant kernel. Native stack accesses/register restores
//! were checked online by the producer: only their counts, complete sparse mask,
//! authentic ABI frames and logged finite transitions are audited here.

use std::{collections::BTreeMap, path::Path};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/live_grants_native.json");
// The producing checker stays private; the fixture records its SHA-256.
const FIXTURE_HASH: &str = "97ebcc2368dc6de8e5abe94eddea3f88c0eb8754e227e9e358aa2de7aaed3894";
const PRODUCER_HASH: &str = "8abf19569bc33550e5278dd0aa892aebe1b7d39324ee6dc2476d3a1586aea967";
const EXE_HASH: &str = "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956";
const TEXT_HASH: &str = "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2";
const STACK: u32 = 0x730000;
const RETURN: u32 = 0xf00000;
const UNMAPPED: u32 = 0xdead1000;
const STACK_BASE: u32 = STACK - 0x2000;
const STACK_BYTES: usize = 0x2100;

const BODIES: [(&str, u32, usize, &str); 13] = [
    (
        "set",
        0x4750c0,
        530,
        "615f417bc2afb437601e544b1c485aa667480f1e9314abfade491ad56500e7ff",
    ),
    (
        "query",
        0x4752e0,
        58,
        "3c3c7175221a4064d194baeb7a48a0f34fd6ddb37c41db332b2cdef9cdf321c6",
    ),
    (
        "record",
        0x475320,
        40,
        "936513084d8287ee722d024b702bbd2b57a2e9814c637e3a918b080a3a9a6198",
    ),
    (
        "merge",
        0x475350,
        46,
        "6b0d572ee8834fcdfcd2724566d749c25012a322ad5f0b2222ee90cf90c0836b",
    ),
    (
        "copy_grants",
        0x475380,
        30,
        "79bf85f8e710ebb7dc48056b6dcf44b242e9b439226fd273e810ad4de83a2f98",
    ),
    (
        "h0_miss",
        0x4efbf0,
        5,
        "5fed5afb29946811bf02359627a94bc01d08d31b779528feaadde3866af9c855",
    ),
    (
        "message",
        0x4135d0,
        499,
        "7cee0612a13346c8b5ff7587c68c5a69f976b30cdcc755d11987c6695680a3d9",
    ),
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
const STATIC: [(&str, u32, usize, &str); 2] = [
    (
        "h0_wrapper",
        0x474020,
        10,
        "b120aaec63b75392c607f5a53633a4b7e91f1774e724fb28c6b735b8766c39b9",
    ),
    (
        "h0_constructor",
        0x4740a0,
        88,
        "e70df7fe5ca81c4071a7ca7a29fe7fabdf86741def51f4f6c318d50bc01becc3",
    ),
];
const DATA: [(&str, u32, usize, &str); 12] = [
    (
        "low_records",
        0x53abd8,
        1192,
        "0223d777aa604dced12d86c77c1699f2449d2dc0b6ee343019c7e6b3f1537626",
    ),
    (
        "high_records",
        0x53b088,
        416,
        "db026a463e21cec70304ba57f8c579b1d5b7d191bb7204b4e3eda733171e1b0b",
    ),
    (
        "h0_table",
        0x527598,
        20,
        "ed760b2862e7099d6fd6057ce45757fdd9dccdb2b51921f218a9284256de7b0c",
    ),
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
const INSTRUCTIONS: [(u32, &str); 20] = [
    (0x47510a, "897c8504"),
    (0x475113, "8b0d48f45500"),
    (0x475121, "8b4d1c"),
    (0x47512c, "668b08"),
    (0x475138, "8b4004"),
    (0x475270, "8b4d1c"),
    (0x475295, "668b06"),
    (0x4752b7, "e804feffff"),
    (0x47530f, "85748204"),
    (0x47538f, "8b3402"),
    (0x475392, "8930"),
    (0x4135ee, "385d01"),
    (0x4135f7, "385d3d"),
    (0x4137c0, "c20800"),
    (0x5145bd, "8b4df0"),
    (0x51460d, "834df0ff"),
    (0x511b91, "802000"),
    (0x514737, "8b0d745c5400"),
    (0x514743, "f644410180"),
    (0x5149ef, "8b0d00595400"),
];
const REGIONS: [(&str, u32, usize); 20] = [
    ("first", 0x740000, 0x1a0),
    ("second", 0x741000, 0x1a0),
    ("actor", 0x742000, 0x2a0),
    ("network", 0x55f448, 4),
    ("h0", 0x55ee50, 12),
    ("slots", 0x744000, 3800),
    ("null_pointer", 0x545900, 4),
    ("null_string", 0x5299f4, 7),
    ("ctype_pointer", 0x545c74, 4),
    ("colon_high", 0x545cf3, 1),
    ("space_high", 0x545cbf, 1),
    ("seh_chain", 0, 4),
    ("stack", STACK_BASE, STACK_BYTES),
    ("low_records", 0x53abd8, 1192),
    ("high_records", 0x53b088, 416),
    ("parser_jump_words", 0x514cb9, 32),
    ("parser_table", 0x529988, 89),
    ("single_format", 0x531d7c, 3),
    ("pair_format", 0x537ca4, 7),
    ("h0_vtable", 0x527598, 20),
];
const INVARIANTS: [&str; 12] = [
    "slots",
    "low_records",
    "high_records",
    "single_format",
    "pair_format",
    "parser_table",
    "parser_jump_words",
    "null_string",
    "h0_vtable",
    "0.ovl",
    "0l0.ovl",
    "2.ovl",
];
const CASES: [&str; 24] = [
    "low-word-zero-no-owner-or-context",
    "already-owned-raw-dword-bypasses-all-lower-data",
    "mask191-online-no-record-consumption",
    "selector12-null-binding-bypasses-record-and-residency",
    "text-zero-bypasses-flags-residency-formatter-message",
    "single-occupied-original-record-second-recipient",
    "single-canonical-hole-actual-null-callback",
    "pair13c-occupied-record-category",
    "pair13b-two-canonical-holes",
    "online-recursive-60-95-96-97",
    "online-multidependency-first-satisfied",
    "merge-empty-source-invalid-target-lazy",
    "merge-low-high-boundary-and-same-owner",
    "copy-six-words-and-same-owner",
    "current-network-and-suppression-between-requests",
    "prefix-bit-before-missing-process",
    "prefix-bit-before-missing-executable",
    "prefix-bit-before-missing-residency",
    "prefix-null-callback-before-missing-substitute",
    "prefix-pair-category-copy-before-missing-classification",
    "prefix-completed-format-before-enabled-message",
    "diagnostic-mask192-recipient-alias",
    "diagnostic-offline191-adjacent-record",
    "diagnostic-copy-late-unknown-source-word",
];
const PREFIXES: [(&str, &str, u32); 6] = [
    (CASES[15], "unknown_network", 0x475113),
    (CASES[16], "unknown_low_records", 0x47512c),
    (CASES[17], "unknown_h0", 0x475144),
    (CASES[18], "unknown_null_pointer", 0x5149ef),
    (CASES[19], "unknown_ctype_pointer", 0x514737),
    (CASES[20], "enabled_message", 0x4135f7),
];
const STACK_COUNTS: [(u32, u32); 28] = [
    (6, 4),
    (7, 5),
    (88, 86),
    (88, 86),
    (88, 86),
    (1253, 1079),
    (341, 335),
    (777, 693),
    (482, 456),
    (804, 802),
    (587, 585),
    (535, 533),
    (1549, 1547),
    (549, 547),
    (3, 1),
    (3, 1),
    (1253, 1079),
    (210, 208),
    (7, 5),
    (5, 8),
    (5, 8),
    (5, 8),
    (24, 45),
    (144, 142),
    (1044, 879),
    (210, 208),
    (5, 8),
    (1, 1),
];
const COUNTS: &str = r#"{"histories":24,"invocations":28,"original_body_histories":15,"original_body_invocations":19,"semantic_prefix_histories":6,"semantic_prefix_invocations":6,"source_diagnostic_histories":3,"source_diagnostic_invocations":3,"portable_histories":21,"portable_invocations":25,"entries":{"copy":18,"copy_grants":3,"emitter":336,"h0_miss":4,"merge":3,"message":6,"padding":9,"parser":8,"query":1711,"record":501,"set":37,"vararg":10,"wrapper":8},"returns":{"copy":18,"copy_grants":2,"emitter":336,"h0_miss":4,"merge":3,"message":5,"padding":9,"parser":6,"query":1711,"record":501,"set":30,"vararg":10,"wrapper":6},"events":{"finite_unknown_read":22,"format_write":342,"precision_reset":10,"raw_boundary":1,"read":4064,"required_boundary":7,"routine_entry":2654,"routine_return":2641,"supplied_effect":2,"support_write":11,"write":45},"outcomes":{"original_body_complete":19,"semantic_prefix_stop":6,"source_diagnostic":3},"unlisted_body_import_service_entries":0}"#;

fn array(v: &Value) -> &Vec<Value> {
    v.as_array().unwrap()
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap()
}
fn n(v: &Value) -> u32 {
    v.as_u64().unwrap().try_into().unwrap()
}
fn masked(v: &Value) -> Vec<Option<u8>> {
    array(v)
        .iter()
        .map(|v| {
            if v.is_null() {
                None
            } else {
                Some(n(v).try_into().unwrap())
            }
        })
        .collect()
}
fn word(v: Option<u32>) -> Vec<Option<u8>> {
    v.map_or_else(|| vec![None; 4], |v| v.to_le_bytes().map(Some).to_vec())
}
fn scalar(v: &[Option<u8>]) -> u32 {
    assert_eq!(v.len(), 4);
    u32::from_le_bytes(
        v.iter()
            .map(|b| b.unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
    )
}
fn digest(v: &[u8]) -> String {
    format!("{:x}", Sha256::digest(v))
}
fn hex(v: &str) -> Vec<u8> {
    assert_eq!(v.len() % 2, 0);
    (0..v.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&v[i..i + 2], 16).unwrap())
        .collect()
}
fn optional(v: &Value) -> Option<u32> {
    (!v.is_null()).then(|| n(v))
}
fn region(name: &str) -> (u32, usize) {
    REGIONS
        .iter()
        .find(|r| r.0 == name)
        .map(|r| (r.1, r.2))
        .unwrap()
}
fn owner(name: &str) -> u32 {
    match name {
        "first" => 0x74009c,
        "second" => 0x74109c,
        "actor" => 0x74224c,
        "unmapped" => UNMAPPED,
        _ => panic!("unknown owner"),
    }
}
fn body(name: &str) -> (u32, usize) {
    BODIES
        .iter()
        .find(|b| b.0 == name)
        .map(|b| (b.1, b.2))
        .unwrap()
}
fn record(id: u32) -> u32 {
    let id = id & 0xffff;
    if id < 149 {
        0x53abd8 + id * 8
    } else {
        0x53b088 + (id - 149) * 16
    }
}

struct Original {
    raw: Vec<u8>,
    sections: Vec<(u32, u32, usize)>,
}
impl Original {
    fn load() -> Option<Self> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe");
        let Ok(raw) = std::fs::read(path) else {
            eprintln!("retail Infest.exe missing; copy the game into retail/ to run this test");
            return None;
        };
        assert_eq!(digest(&raw), EXE_HASH);
        inf_formats::FrontendExecutable::parse(&raw).unwrap();
        let u16at = |i| u16::from_le_bytes(raw[i..i + 2].try_into().unwrap());
        let u32at = |i| u32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
        let pe = u32at(0x3c) as usize;
        assert_eq!(pe, 0xe8);
        assert_eq!(&raw[pe..pe + 4], b"PE\0\0");
        let table = pe + 24 + u16at(pe + 20) as usize;
        let sections = (0..u16at(pe + 6) as usize)
            .map(|i| {
                let p = table + i * 40;
                (
                    0x400000 + u32at(p + 12),
                    u32at(p + 16),
                    u32at(p + 20) as usize,
                )
            })
            .collect();
        assert_eq!(&raw[table..table + 8], b".text\0\0\0");
        assert_eq!(u32at(table + 12), 0x1000);
        assert_eq!(u32at(table + 16), 0x11e000);
        assert_eq!(u32at(table + 20), 0x1000);
        assert_eq!(digest(&raw[0x1000..0x11f000]), TEXT_HASH);
        Some(Self { raw, sections })
    }
    fn at(&self, address: u32, count: usize) -> &[u8] {
        let &(start, size, file) = self
            .sections
            .iter()
            .find(|(s, c, _)| {
                address >= *s && (address as u64 + count as u64) <= *s as u64 + *c as u64
            })
            .expect("guard must be fully file-backed");
        let offset = file + (address - start) as usize;
        assert!(count <= size as usize);
        assert_eq!(
            offset,
            (address - 0x400000) as usize,
            "finite supported guard mapping"
        );
        &self.raw[offset..offset + count]
    }
    fn guard(&self, v: &Value, address: u32, count: usize, hash: &str) {
        assert_eq!(v["address"], address);
        assert_eq!(v["bytes"], count);
        assert_eq!(v["sha256"], hash);
        assert_eq!(digest(self.at(address, count)), hash);
    }
    fn call(&self, ret: u32, target: u32) {
        let code = self.at(ret - 5, 5);
        assert_eq!(code[0], 0xe8);
        assert_eq!(
            ret.wrapping_add(u32::from_le_bytes(code[1..].try_into().unwrap())),
            target
        );
    }
}

fn provenance(m: &Value, o: &Original) {
    assert_eq!(m["schema_version"], 1);
    assert_eq!(m["executable_sha256"], EXE_HASH);
    assert_eq!(m["text_sha256"], TEXT_HASH);
    assert_eq!(m["text_address"], 0x401000);
    assert_eq!(m["fpcw"], 0x027f);
    assert_eq!(m["producing_script_sha256"], PRODUCER_HASH);
    assert_eq!(
        m["execution"],
        "13_unchanged_bodies_in_one_CPU_request_with_authentic_nested_calls"
    );
    for (key, profile) in [
        ("bodies", BODIES.as_slice()),
        ("static_bodies", STATIC.as_slice()),
        ("data_guards", DATA.as_slice()),
    ] {
        assert_eq!(array(&m[key]).len(), profile.len());
        for (v, &(name, start, count, hash)) in array(&m[key]).iter().zip(profile) {
            assert_eq!(v["name"], name);
            o.guard(v, start, count, hash);
        }
    }
    o.guard(
        &m["static_grant_constructor"],
        0x475020,
        27,
        "cfae372ea5e3ef4b6036b35902061f9776b997549efe9759f2e9ccbe537db1b7",
    );
    assert_eq!(array(&m["instruction_guards"]).len(), INSTRUCTIONS.len());
    for (v, (address, code)) in array(&m["instruction_guards"]).iter().zip(INSTRUCTIONS) {
        let bytes = hex(code);
        o.guard(v, address, bytes.len(), &digest(&bytes));
    }
    assert_eq!(
        m["native_regions"].as_object().unwrap().len(),
        REGIONS.len()
    );
    for (name, address, count) in REGIONS {
        assert_eq!(
            m["native_regions"][name],
            json!({"address":address,"bytes":count})
        );
    }
    assert_eq!(m["native_owners"].as_object().unwrap().len(), 3);
    for name in ["first", "second", "actor"] {
        assert_eq!(
            m["native_owners"][name],
            json!({"pointer":owner(name),"header":0x527818,"normal_recipient":if name=="actor"{0}else{region(name).0+0x88}})
        );
    }
    assert_eq!(m["counts"], serde_json::from_str::<Value>(COUNTS).unwrap());
    assert_eq!(
        m["abi"],
        json!({
            "set_merge_copy":"void_RET4_no_numeric_result",
            "query":"LOWWORD_id_AL_truth_RET4",
            "record":"ECX_LOWWORD_full_EAX_address_RET0",
            "h0_miss":"full_DWORD_argument_unused_constant_NULL_RET4",
            "formatter":"cdecl_original_RET0_full_emitted_DWORD_count_caller_cleans12_or16",
            "message":"successful_void_RET8_or_enabled_stop_before_plus3D"
        })
    );
    assert_eq!(m["diagnostics"], json!([CASES[23], CASES[21], CASES[22]]));
    assert_eq!(m["semantic_prefixes"].as_object().unwrap().len(), 6);
    for (name, reason, ip) in PREFIXES {
        assert_eq!(m["semantic_prefixes"][name], json!([reason, ip]));
    }
    assert_eq!(
        m["finite_unknown_reads"],
        json!([{"ip":0x5145bd,"policy":"dead_first_percent_precision_load"},{"ip":0x51460d,"policy":"exact_unknown_RMW_then_FFFFFFFF_reset"},{"ip":0x511b91,"policy":"exact_output_RMW_then_known_NUL"}])
    );
    assert_eq!(m["guard_file_mapping"],"VA-400000 equals file offset only for these authenticated file-backed spans of this exact supported PE; not a generic mapping contract");
    assert_eq!(m["stack_scope"],"all known reads/writes checked online; retained mask encoded as disjoint known spans with omitted bytes explicitly unknown; authentic frames/RET and finite unknown/reset/output events recorded; not a full stack-event trace");
    assert_eq!(m["output_scope"],"actual caller stack buffers untouched before execution; snapshot complete<=191 bytes+NUL at original CRT return before reuse; reused known tail retained; no portable partial-buffer result");
    assert_eq!(m["residency_scope"],"declared original H0 asset spans/registration sequence; canonical Rust ResidentRuntime/Boot and FINAL occupied registrations require independent comparison; no loader/constructor execution");
    assert_eq!(m["scopes"][2],"asset bytes installed but unbound in inactive/missing-pool histories are native-only unused backing; no portable allocation-presence parity");
    assert_eq!(
        m["exclusions"],
        json!([
            "raw_diagnostics_before_typed_construction",
            "H0_one_past_or_table_replacement",
            "arbitrary_native_pointer_aliases_or_notification_binding_retargeting",
            "native_loader_or_ctor_execution",
            "enabled_message_UI_or_imports",
            "generic_CRT_formats_or_truncation_or_locale_bit80",
            "native_PlayerId_ObjectId_slice_Arc_address_parity",
            "outer_callers_capture_event22_pickups_saves",
            "production_attachment_or_retail_acceptance"
        ])
    );
}

fn assets(capture: &Value) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let preload = std::fs::read(root.join("PreLoad.dat")).unwrap();
    let hash = "d41b9eb6aceae2df8c53e802ee27c6e5fcf5fbc9608c3df4758171ab6e8931d3";
    assert_eq!(digest(&preload), hash);
    assert_eq!(preload.len(), 18280);
    assert_eq!(capture["preload_sha256"], hash);
    assert_eq!(capture["preload_length"], preload.len());
    let parsed = inf_formats::Preload::parse(&preload).unwrap();
    assert_eq!(parsed.header.handler_capacities[0], 950);
    assert_eq!(capture["capacity"], 950);
    assert_eq!(
        capture["selection"],
        json!({"kind":"boot","difficulty":0,"language":0,"levels":[0,2]})
    );
    assert_eq!(
        capture["claim"],
        "declared_original_asset_backing_not_python_ResidentRuntime_or_loader_execution"
    );
    let profiles = [
        (
            0,
            "base",
            "0.ovl",
            0x720000,
            983065,
            53,
            6,
            "0d568d3aba640a144916276bc7fd2424497542cbfb98ac17858efd823a14d236",
        ),
        (
            0,
            "language",
            "0l0.ovl",
            0x724000,
            16091,
            16087,
            684,
            "39097d1188a9573e619591b9e2b99be1191e31943f5ff3f501fe63b20e7d2b26",
        ),
        (
            2,
            "base",
            "2.ovl",
            0x729000,
            64880,
            26,
            2,
            "5c8c45d856506d6c94a72012eccd21fe88ce8c19bf92e7e0aef85124a3a534be",
        ),
    ];
    assert_eq!(array(&capture["allocations"]).len(), 3);
    let mut slots = vec![0u32; 950];
    let mut overwrites = Vec::new();
    let mut records = 0;
    for (a, (level, phase, name, address, length, payload_length, count, hash)) in
        array(&capture["allocations"]).iter().zip(profiles)
    {
        let raw = std::fs::read(root.join("Overlay").join(name)).unwrap();
        assert_eq!(digest(&raw), hash);
        assert_eq!(raw.len(), length);
        assert_eq!(a["overlay_sha256"], hash);
        assert_eq!(a["overlay_length"], length);
        assert_eq!(a["level"], level);
        assert_eq!(a["phase"], phase);
        assert_eq!(a["filename"], name);
        assert_eq!(a["handler"], 0);
        assert_eq!(a["region_index"], 0);
        assert_eq!(a["native_address"], address);
        assert_eq!(
            u32::from_le_bytes(raw[..4].try_into().unwrap()) as usize,
            payload_length
        );
        assert_eq!(a["payload_span"], json!([4, 4 + payload_length]));
        let payload = &raw[4..4 + payload_length];
        assert_eq!(
            masked(&a["payload"]),
            payload.iter().copied().map(Some).collect::<Vec<_>>()
        );
        let leaf = parsed.leaf(level).unwrap();
        let request = if phase == "base" {
            &leaf.base.0[0]
        } else {
            &leaf.language.0[0]
        };
        assert_eq!(request.count, count as i16);
        assert_eq!(a["request"]["count"], count);
        assert_eq!(
            a["request"]["segments"],
            json!(request
                .segments
                .iter()
                .map(|s| s.packed())
                .collect::<Vec<_>>())
        );
        assert_eq!(array(&a["entries"]).len(), count);
        let mut cursor = 0;
        for (local, e) in array(&a["entries"]).iter().enumerate() {
            let end = cursor + payload[cursor..].iter().position(|b| *b == 0).unwrap() + 1;
            let global = usize::from(request.global_index(local.try_into().unwrap()).unwrap());
            assert!(global < 950);
            let pointer = address + cursor as u32;
            assert_eq!(
                *e,
                json!({"local_index":local,"global_index":global,"record_span":[4+cursor,4+end],"native_pointer":pointer})
            );
            if slots[global] != 0 {
                overwrites.push(json!({"global_index":global,"previous_pointer":slots[global],"next_pointer":pointer,"level":level,"phase":phase,"local_index":local}));
            }
            slots[global] = pointer;
            cursor = end;
            records += 1;
        }
        assert_eq!(cursor, payload.len());
    }
    assert_eq!(records, 692);
    assert_eq!(slots.iter().filter(|p| **p != 0).count(), 691);
    assert_eq!(capture["slots"], json!(slots));
    assert_eq!(capture["overwrites"], json!(overwrites));
    assert_eq!(overwrites,json!([{"global_index":355,"previous_pointer":7492576,"next_pointer":7492892,"level":0,"phase":"language","local_index":359}]).as_array().unwrap().clone());
}

type Storage = BTreeMap<String, Vec<Option<u8>>>;
fn put(s: &mut Storage, name: &str, offset: usize, bytes: &[Option<u8>]) {
    s.get_mut(name).unwrap()[offset..offset + bytes.len()].copy_from_slice(bytes);
}
fn apply(s: &mut Storage, patch: &Value) {
    put(
        s,
        text(&patch["region"]),
        n(&patch["offset"]) as usize,
        &masked(&patch["bytes"]),
    );
}
fn initial(input: &Value, o: &Original, capture: &Value) -> Storage {
    let mut s: Storage = REGIONS
        .iter()
        .filter(|r| r.0 != "stack")
        .map(|r| (r.0.to_owned(), vec![None; r.2]))
        .collect();
    for name in [
        "low_records",
        "high_records",
        "parser_table",
        "parser_jump_words",
        "single_format",
        "pair_format",
        "null_string",
        "h0_vtable",
    ] {
        let (base, count) = region(name);
        put(
            &mut s,
            name,
            0,
            &o.at(base, count)
                .iter()
                .copied()
                .map(Some)
                .collect::<Vec<_>>(),
        );
    }
    for name in ["first", "second", "actor"] {
        assert_eq!(array(&input[name]).len(), 6);
        let mut bytes = word(Some(0x527818));
        for v in array(&input[name]) {
            bytes.extend(word(Some(n(v))));
        }
        bytes.extend(word(Some(if name == "actor" {
            0
        } else {
            region(name).0 + 0x88
        })));
        put(
            &mut s,
            name,
            (owner(name) - region(name).0) as usize,
            &bytes,
        );
    }
    for name in ["first", "second"] {
        put(
            &mut s,
            name,
            0x89,
            &[Some(n(&input["suppression"][name]).try_into().unwrap())],
        );
    }
    put(&mut s, "network", 0, &word(optional(&input["network"])));
    put(&mut s, "seh_chain", 0, &word(Some(0x12345678)));
    let mut h0 = word(Some(0x527598));
    h0.extend(word(Some(0x744000)));
    h0.extend(word((!input["resident"].is_null()).then_some(950)));
    put(&mut s, "h0", 0, &h0);
    let slots: Vec<_> = array(&capture["slots"])
        .iter()
        .flat_map(|v| word(Some(if input["resident"] == "boot" { n(v) } else { 0 })))
        .collect();
    put(&mut s, "slots", 0, &slots);
    for a in array(&capture["allocations"]) {
        s.insert(text(&a["filename"]).to_owned(), masked(&a["payload"]));
    }
    for name in ["null_pointer", "ctype_pointer"] {
        put(&mut s, name, 0, &word(optional(&input[name])));
    }
    for name in ["colon_high", "space_high"] {
        put(
            &mut s,
            name,
            0,
            &[optional(&input[name]).map(|v| v.try_into().unwrap())],
        );
    }
    if input["executable"] == false {
        for name in ["low_records", "high_records"] {
            let count = s[name].len();
            put(&mut s, name, 0, &vec![None; count]);
        }
    }
    if let Some(patches) = input.get("patches") {
        for p in array(patches) {
            apply(&mut s, p);
        }
    }
    s
}
fn stack_mask(v: &Value) -> Vec<Option<u8>> {
    assert_eq!(v["length"], STACK_BYTES);
    assert_eq!(v["omitted_bytes"], "explicitly_unknown_no_scalar_defaults");
    let mut bytes = vec![None; STACK_BYTES];
    let mut end = None;
    for span in array(&v["known_spans"]) {
        let offset = n(&span["offset"]) as usize;
        let values = masked(&span["bytes"]);
        assert!(!values.is_empty() && values.iter().all(Option::is_some));
        assert!(end.is_none_or(|e| offset > e));
        assert!(offset + values.len() <= STACK_BYTES);
        bytes[offset..offset + values.len()].copy_from_slice(&values);
        end = Some(offset + values.len());
    }
    bytes
}
fn snapshot(v: &Value, s: &Storage) {
    let names: Vec<_> = s
        .keys()
        .filter(|name| !INVARIANTS.contains(&name.as_str()))
        .collect();
    assert_eq!(v.as_object().unwrap().len(), names.len() + 1);
    for name in names {
        assert_eq!(masked(&v[name]), s[name]);
    }
    stack_mask(&v["stack"]);
}
fn absolute(name: &str, offset: usize, capture: &Value) -> u32 {
    let base = REGIONS
        .iter()
        .find(|r| r.0 == name)
        .map(|r| r.1)
        .unwrap_or_else(|| {
            n(&array(&capture["allocations"])
                .iter()
                .find(|a| a["filename"] == name)
                .unwrap()["native_address"])
        });
    base + offset as u32
}

#[derive(Clone)]
struct Frame {
    entry: Value,
    index: usize,
    reads: Vec<usize>,
    writes: Vec<usize>,
    children: Vec<(usize, usize)>,
    pending_byte: Option<u8>,
    emits: usize,
    output: Vec<Option<u8>>,
    resets: usize,
    varargs: usize,
}
impl Frame {
    fn name(&self) -> &str {
        text(&self.entry["function"])
    }
    fn args(&self) -> Vec<u32> {
        array(&self.entry["arguments"]).iter().map(n).collect()
    }
    fn receiver(&self) -> u32 {
        n(&self.entry["receiver"])
    }
    fn sp(&self) -> u32 {
        n(&self.entry["entry_stack"])
    }
}
fn wrapper(frames: &[Frame]) -> &Frame {
    frames.iter().rev().find(|f| f.name() == "wrapper").unwrap()
}
fn selected<'a>(frame: &Frame, events: &'a [Value], kind: &str, ip: u32) -> Vec<&'a Value> {
    let indices = if kind == "read" {
        &frame.reads
    } else {
        &frame.writes
    };
    indices
        .iter()
        .filter_map(|i| (events[*i]["ip"] == ip).then_some(&events[*i]))
        .collect()
}
fn field_read(
    event: &Value,
    s: &Storage,
    expected_name: &str,
    address: u32,
    count: usize,
    capture: &Value,
) {
    let name = text(&event["region"]);
    let offset = n(&event["offset"]) as usize;
    assert_eq!(name, expected_name);
    assert_eq!(absolute(name, offset, capture), address);
    assert_eq!(event["bytes"], count);
    assert_eq!(masked(&event["data"]), s[name][offset..offset + count]);
}
fn receiver_region(receiver: u32) -> &'static str {
    ["first", "second", "actor"]
        .into_iter()
        .find(|name| owner(name) == receiver)
        .unwrap()
}

// Read ownership is derived from the original instruction, not from aggregate
// event counts. Source scans and copies are also checked in their vararg phase.
fn audit_read(event: &Value, frames: &mut [Frame], s: &Storage, capture: &Value) {
    let top = frames.last().unwrap();
    let ip = n(&event["ip"]);
    let name = text(&event["region"]);
    let offset = n(&event["offset"]) as usize;
    let count = n(&event["bytes"]) as usize;
    assert_eq!(masked(&event["data"]).len(), count);
    assert_eq!(masked(&event["data"]), s[name][offset..offset + count]);
    let address = absolute(name, offset, capture);
    match ip {
        0x47530f => {
            assert_eq!(top.name(), "query");
            let id = top.args()[0] & 0xffff;
            assert_ne!(id, 0);
            assert_eq!(address, top.receiver() + 4 + (id >> 5) * 4);
            assert_eq!(count, 4);
            assert_eq!(name, receiver_region(top.receiver()));
        }
        0x4750fb => {
            assert_eq!(top.name(), "set");
            assert_eq!(
                address,
                top.receiver() + 4 + ((top.args()[0] & 0xffff) >> 5) * 4
            );
            assert_eq!(count, 4);
        }
        0x475121 | 0x475270 => {
            assert_eq!(top.name(), "set");
            assert_eq!(address, top.receiver() + 28);
            assert_eq!(count, 4);
        }
        0x475113 => {
            assert_eq!(top.name(), "set");
            field_read(event, s, "network", 0x55f448, 4, capture);
        }
        0x47512c | 0x475138 => {
            assert_eq!(top.name(), "set");
            let expected = record(top.args()[0]) + if ip == 0x475138 { 4 } else { 0 };
            assert_eq!(address, expected);
            assert_eq!(count, if ip == 0x47512c { 2 } else { 4 });
            assert!(["low_records", "high_records"].contains(&name));
        }
        0x475295 => {
            assert_eq!(top.name(), "set");
            assert_eq!(name, "high_records");
            assert_eq!(count, 2);
            assert!((8..=12).contains(&(offset % 16)) && offset % 2 == 0);
        }
        0x475144 | 0x475190 | 0x4751bd | 0x4751f6 | 0x475223 => {
            assert_eq!(top.name(), "set");
            field_read(event, s, "h0", 0x55ee58, 4, capture);
        }
        0x475151 | 0x47519d | 0x4751cd | 0x475203 | 0x47522f => {
            assert_eq!(top.name(), "set");
            field_read(event, s, "h0", 0x55ee54, 4, capture);
        }
        0x47515e | 0x4751aa | 0x4751dd => {
            assert_eq!(top.name(), "set");
            field_read(event, s, "h0", 0x55ee50, 4, capture);
        }
        0x475157 | 0x4751a3 | 0x4751d3 | 0x475209 | 0x475235 => {
            assert_eq!(top.name(), "set");
            assert_eq!(name, "slots");
            assert_eq!(count, 4);
            assert_eq!(offset % 4, 0);
        }
        0x475169 | 0x4751b5 | 0x4751ed => {
            assert_eq!(top.name(), "set");
            field_read(event, s, "h0_vtable", 0x527598, 4, capture);
            assert_eq!(scalar(&masked(&event["data"])), body("h0_miss").0);
        }
        0x47538f => {
            assert_eq!(top.name(), "copy_grants");
            assert_eq!(count, 4);
            assert_eq!(address, top.args()[0] + 4 + (top.reads.len() as u32) * 4);
        }
        0x4135d0 => {
            assert_eq!(top.name(), "message");
            field_read(event, s, "seh_chain", 0, 4, capture);
        }
        0x4135ee => {
            assert_eq!(top.name(), "message");
            assert_eq!(address, top.receiver() + 1);
            assert_eq!(count, 1);
            assert!(["first", "second"].contains(&name));
        }
        0x5145a9 | 0x514ca3 => {
            assert_eq!(top.name(), "parser");
            assert!(["single_format", "pair_format"].contains(&name));
            assert_eq!(count, 1);
        }
        0x5145e2 | 0x5145ef => {
            assert_eq!(top.name(), "parser");
            assert_eq!(name, "parser_table");
            assert_eq!(count, 1);
        }
        0x514606 => {
            assert_eq!(top.name(), "parser");
            assert_eq!(name, "parser_jump_words");
            assert_eq!(count, 4);
            assert_eq!(offset % 4, 0);
        }
        0x514737 => {
            assert_eq!(top.name(), "parser");
            field_read(event, s, "ctype_pointer", 0x545c74, 4, capture);
        }
        0x514743 => {
            assert_eq!(top.name(), "parser");
            assert!(["colon_high", "space_high"].contains(&name));
            assert_eq!(offset, 0);
            assert_eq!(count, 1);
            if let Some(byte) = masked(&event["data"])[0] {
                assert_eq!(byte & 0x80, 0);
            }
        }
        0x5149ef => {
            assert_eq!(top.name(), "parser");
            field_read(event, s, "null_pointer", 0x545900, 4, capture);
        }
        0x514a01 => {
            assert_eq!(top.name(), "parser");
            assert_eq!(count, 1);
            assert!(
                name == "null_string"
                    || array(&capture["allocations"])
                        .iter()
                        .any(|a| a["filename"] == name)
            );
        }
        0x514d55 => {
            assert_eq!(top.name(), "copy");
            assert_eq!(count, 1);
            assert_eq!(address, top.args()[0] + top.reads.len() as u32);
            assert!(top.reads.len() < (top.args()[1] as usize));
            assert!(top.pending_byte.is_none());
            frames.last_mut().unwrap().pending_byte = masked(&event["data"])[0];
        }
        _ => panic!("unadmitted read {ip:08x}"),
    }
}

fn audit_entry(
    entry: &Value,
    frames: &mut [Frame],
    events: &[Value],
    index: usize,
    request: &Value,
    o: &Original,
) {
    let name = text(&entry["function"]);
    let args: Vec<_> = array(&entry["arguments"]).iter().map(n).collect();
    let sp = n(&entry["entry_stack"]);
    let ret = n(&entry["return_ip"]);
    let receiver = n(&entry["receiver"]);
    assert_eq!(entry["address"], body(name).0);
    assert_eq!(entry["depth"], frames.len() + 1);
    assert!(sp >= STACK_BASE && sp + 4 <= STACK + 0x100);
    if let Some(parent) = frames.last() {
        assert!(sp < parent.sp());
        let (base, count) = body(parent.name());
        assert!((base..base + count as u32).contains(&ret));
        if name == "h0_miss" {
            assert_eq!(parent.name(), "set");
            assert_eq!(
                o.at(ret - 2, 2),
                [0xff, if ret == 0x4751ef { 0x12 } else { 0x10 }]
            );
            assert_eq!(events[index - 1]["kind"], "read");
            assert_eq!(events[index - 1]["region"], "h0_vtable");
        } else {
            o.call(ret, body(name).0);
        }
    } else {
        assert_eq!(sp, STACK);
        assert_eq!(ret, RETURN);
        assert_eq!(receiver, owner(text(&request["target"])));
        assert_eq!(
            name,
            match text(&request["operation"]) {
                "set" => "set",
                "merge" => "merge",
                "copy" => "copy_grants",
                _ => panic!("operation"),
            }
        );
        assert_eq!(
            args,
            [if name == "set" {
                n(&request["raw_id"])
            } else {
                owner(text(&request["source"]))
            }]
        );
    }
    match name {
        "set" | "query" | "merge" | "copy_grants" => assert_eq!(args.len(), 1),
        "record" => assert!(args.is_empty()),
        "h0_miss" => {
            assert_eq!(receiver, 0x55ee50);
            assert_eq!(args.len(), 1);
            assert!(args[0] == 0x13b || args[0] == 0x13c || args[0] >> 16 == 0x74);
        }
        "message" => {
            assert_eq!(frames.last().unwrap().name(), "set");
            assert_eq!(args.len(), 2);
            assert_eq!(args[1], 0);
            assert!([0x740088, 0x741088].contains(&receiver));
            let late = &events[index - 1];
            assert_eq!(late["kind"], "read");
            assert_eq!(late["ip"], 0x475270);
            assert_eq!(scalar(&masked(&late["data"])), receiver);
            let returned = events[..index]
                .iter()
                .rev()
                .find(|v| v["kind"] == "routine_return" && v["function"] == "wrapper")
                .unwrap();
            assert_eq!(args[0], n(&returned["arguments"][0]));
        }
        "wrapper" => {
            let parent = frames.last().unwrap();
            assert_eq!(parent.name(), "set");
            assert_eq!(args[0], parent.sp() - 0xc0);
            let single = args[1] == 0x531d7c;
            assert!(single || args[1] == 0x537ca4);
            assert_eq!(args.len(), if single { 3 } else { 4 });
            assert_eq!(ret, if single { 0x47517f } else { 0x475266 });
            assert_eq!(sp, parent.sp() - if single { 0xe0 } else { 0xe4 });
            assert_eq!(masked(&entry["output_before"]).len(), 192);
        }
        "parser" => {
            let w = wrapper(frames);
            assert_eq!(args, [w.sp() - 0x24, w.args()[1], w.sp() + 12]);
            assert_eq!(sp, w.sp() - 0x38);
        }
        "vararg" => {
            let w = wrapper(frames);
            assert_eq!(args, [w.sp() - 0x2c]);
            let parser = frames.last_mut().unwrap();
            assert_eq!(parser.name(), "parser");
            assert_eq!(parser.resets, parser.varargs + 1);
            parser.varargs += 1;
        }
        "padding" => {
            let w = wrapper(frames);
            assert_eq!(args.len(), 4);
            assert_eq!(args[0], 32);
            assert!((args[1] as i32) <= 0);
            assert_eq!(args[2..], [w.sp() - 0x24, w.sp() - 0x50]);
        }
        "copy" => {
            let w = wrapper(frames);
            assert_eq!(args.len(), 4);
            assert_eq!(args[2..], [w.sp() - 0x24, w.sp() - 0x50]);
            assert!(args[1] <= 191);
            if args[0] == w.sp() - 0x52 {
                assert_eq!(args[1], 0);
            }
        }
        "emitter" => {
            let w = wrapper(frames);
            assert_eq!(args.len(), 3);
            assert_eq!(args[1..], [w.sp() - 0x24, w.sp() - 0x50]);
            let parent = frames.last_mut().unwrap();
            assert!(["copy", "parser"].contains(&parent.name()));
            if parent.name() == "copy" {
                let b = parent.pending_byte.take().unwrap();
                assert_eq!(args[0], i32::from(b as i8) as u32);
            } else {
                assert!([58, 32].contains(&args[0]));
                let prior = &events[index - 1];
                assert_eq!(prior["kind"], "read");
                assert_eq!(prior["ip"], 0x514743);
                assert_eq!(
                    prior["region"],
                    if args[0] == 58 {
                        "colon_high"
                    } else {
                        "space_high"
                    }
                );
            }
        }
        _ => unreachable!(),
    }
}

// Checks direct children of the actual grant frame. It never emits an expected
// grant mask by running a second closure algorithm.
fn audit_grant_frame(f: &Frame, events: &[Value], completed: bool) {
    let children = &f.children;
    let args = f.args();
    match f.name() {
        "query" => {
            if !completed {
                return;
            }
            let id = args[0] & 0xffff;
            assert_eq!(f.reads.len(), if id == 0 { 0 } else { 1 });
            assert!(f.writes.is_empty() && children.is_empty());
        }
        "copy_grants" => {
            assert_eq!(f.reads.len(), f.writes.len() + usize::from(!completed));
            assert_eq!(f.writes.len(), if completed { 6 } else { 4 });
            assert!(children.is_empty());
            for (k, (r, w)) in f.reads.iter().zip(&f.writes).enumerate() {
                assert_eq!(*w, *r + 1);
                assert_eq!(events[*r]["ip"], 0x47538f);
                assert_eq!(events[*w]["ip"], 0x475392);
                assert_eq!(events[*r]["data"], events[*w]["data"]);
                assert_eq!(
                    n(&events[*w]["offset"]),
                    (f.receiver() - region(text(&events[*w]["region"])).0) + 4 + k as u32 * 4
                );
            }
        }
        "merge" => {
            assert!(completed);
            let mut cursor = 0;
            for id in 0..175 {
                let (i, r) = children[cursor];
                let query = &events[i];
                assert_eq!(query["function"], "query");
                assert_eq!(query["receiver"], args[0]);
                assert_eq!(query["arguments"], json!([id]));
                assert_eq!(query["return_ip"], 0x475363);
                cursor += 1;
                if n(&events[r]["raw_eax"]) as u8 != 0 {
                    let (j, s) = children[cursor];
                    assert_eq!(events[j]["function"], "set");
                    assert_eq!(events[j]["receiver"], f.receiver());
                    assert_eq!(events[j]["arguments"], json!([id]));
                    assert_eq!(events[j]["return_ip"], 0x47536f);
                    assert!(r < j && j < s);
                    cursor += 1;
                }
            }
            assert_eq!(cursor, children.len());
            assert!(f.reads.is_empty() && f.writes.is_empty());
        }
        "set" => {
            let (i, r) = children[0];
            assert_eq!(events[i]["function"], "query");
            assert_eq!(events[i]["receiver"], f.receiver());
            assert_eq!(events[i]["arguments"], json!(args));
            assert_eq!(events[i]["return_ip"], 0x4750d7);
            let owned = n(&events[r]["raw_eax"]) as u8 != 0;
            let commits = selected(f, events, "write", 0x47510a);
            if owned {
                assert!(f.reads.is_empty() && commits.is_empty());
                assert_eq!(children.len(), 1);
                return;
            }
            assert_eq!(commits.len(), 1);
            let old = selected(f, events, "read", 0x4750fb);
            assert_eq!(old.len(), 1);
            let commit = commits[0];
            assert_eq!(commit["region"], old[0]["region"]);
            assert_eq!(commit["offset"], old[0]["offset"]);
            assert_eq!(
                scalar(&masked(&commit["data"])),
                scalar(&masked(&old[0]["data"])) | 1 << ((args[0] & 0xffff) & 31)
            );
            let write_index = f.writes[0];
            assert!(r < f.reads[0] && f.reads[0] + 1 == write_index);
            assert_eq!(events[write_index + 1]["kind"], "routine_entry");
            assert_eq!(events[write_index + 1]["function"], "record");
            let record_return = children
                .iter()
                .find(|(i, _)| events[*i]["function"] == "record")
                .unwrap()
                .1;
            let network = selected(f, events, "read", 0x475113);
            assert_eq!(network.len(), 1);
            assert_eq!(events[record_return + 1], *network[0]);
            let queries: Vec<_> = children
                .iter()
                .copied()
                .filter(|(i, _)| {
                    events[*i]["function"] == "query" && events[*i]["return_ip"] == 0x475285
                })
                .collect();
            if !completed {
                assert!(queries.is_empty());
                return;
            }
            assert_eq!(queries.len(), 26);
            for (ordinal, &(i, r)) in queries.iter().enumerate() {
                let candidate = 149 + ordinal as u32;
                let later = queries.get(ordinal + 1).map_or(events.len(), |p| p.0);
                assert_eq!(events[i]["receiver"], f.receiver());
                assert_eq!(events[i]["arguments"], json!([candidate]));
                let next = children.iter().position(|p| p.0 == i).unwrap() + 1;
                if n(&events[r]["raw_eax"]) as u8 != 0 {
                    continue;
                }
                let (record_entry, record_return) = children[next];
                assert_eq!(events[record_entry]["function"], "record");
                assert_eq!(n(&events[record_entry]["receiver"]) & 0xffff, candidate);
                assert_eq!(events[record_entry]["return_ip"], 0x475290);
                let dependencies: Vec<_> = f
                    .reads
                    .iter()
                    .copied()
                    .filter(|k| *k > record_return && *k < later && events[*k]["ip"] == 0x475295)
                    .collect();
                assert!(!dependencies.is_empty() && dependencies.len() <= 3);
                let mut position = next + 1;
                let mut satisfied = false;
                for (slot, &dep_index) in dependencies.iter().enumerate() {
                    let previous = &events[dep_index];
                    assert_eq!(previous["region"], "high_records");
                    assert_eq!(
                        region("high_records").0 + n(&previous["offset"]),
                        record(candidate) + 8 + slot as u32 * 2
                    );
                    let bytes = masked(&previous["data"]);
                    let dep = u16::from_le_bytes([bytes[0].unwrap(), bytes[1].unwrap()]);
                    if dep == 0 {
                        continue;
                    }
                    let (q, qr) = children[position];
                    assert_eq!(q, dep_index + 1);
                    assert_eq!(events[q]["function"], "query");
                    assert_eq!(events[q]["return_ip"], 0x4752a5);
                    assert_eq!(n(&events[q]["arguments"][0]) & 0xffff, u32::from(dep));
                    assert_eq!(events[q]["receiver"], f.receiver());
                    position += 1;
                    if n(&events[qr]["raw_eax"]) as u8 != 0 {
                        let (s, sr) = children[position];
                        assert_eq!(s, qr + 1);
                        assert_eq!(events[s]["function"], "set");
                        assert_eq!(events[s]["receiver"], f.receiver());
                        assert_eq!(events[s]["arguments"], json!([candidate]));
                        assert_eq!(events[s]["return_ip"], 0x4752bc);
                        assert!(sr < later);
                        assert_eq!(slot + 1, dependencies.len());
                        satisfied = true;
                        break;
                    }
                }
                assert!(satisfied || dependencies.len() == 3);
            }
        }
        _ => {}
    }
}

fn source_at(pointer: u32, s: &Storage, capture: &Value) -> (String, usize, Vec<u8>) {
    let names = ["null_string", "0.ovl", "0l0.ovl", "2.ovl"];
    let name = names
        .into_iter()
        .find(|name| {
            let base = absolute(name, 0, capture);
            pointer >= base && (pointer - base) < s[*name].len() as u32
        })
        .unwrap();
    let offset = (pointer - absolute(name, 0, capture)) as usize;
    let end = offset
        + s[name][offset..]
            .iter()
            .position(|b| *b == Some(0))
            .unwrap();
    let bytes = s[name][offset..end].iter().map(|b| b.unwrap()).collect();
    (name.to_owned(), offset, bytes)
}

// Reached native sources are read anew for each vararg, including identical
// NULL operands. The complete scan must finish before the first source copy.
fn audit_formatter(
    f: &Frame,
    end: usize,
    events: &[Value],
    s: &Storage,
    capture: &Value,
    returned: Option<&Value>,
) {
    let args = f.args();
    let pair = args[1] == 0x537ca4;
    let window = &events[f.index..end];
    let varargs: Vec<_> = (f.index..end)
        .filter(|i| events[*i]["kind"] == "routine_return" && events[*i]["function"] == "vararg")
        .collect();
    assert!(varargs.len() <= args.len() - 2);
    let mut contents = Vec::new();
    let mut finishes = Vec::new();
    for (operand, &start) in varargs.iter().enumerate() {
        assert_eq!(events[start]["raw_eax"], args[2 + operand]);
        let upper = varargs.get(operand + 1).copied().unwrap_or(end);
        let null_reads: Vec<_> = (start + 1..upper)
            .filter(|i| events[*i]["kind"] == "read" && events[*i]["ip"] == 0x5149ef)
            .collect();
        let pointer = if args[2 + operand] == 0 {
            assert_eq!(null_reads.len(), 1);
            let read = &events[null_reads[0]];
            assert_eq!(read["region"], "null_pointer");
            assert_eq!(read["offset"], 0);
            let values = masked(&read["data"]);
            if values.iter().any(Option::is_none) {
                assert!(returned.is_none());
                assert_eq!(null_reads[0] + 2, end);
                continue;
            }
            scalar(&values)
        } else {
            assert!(null_reads.is_empty());
            args[2 + operand]
        };
        let (name, offset, bytes) = source_at(pointer, s, capture);
        let scans: Vec<_> = (start + 1..upper)
            .filter(|i| events[*i]["kind"] == "read" && events[*i]["ip"] == 0x514a01)
            .collect();
        assert_eq!(scans.len(), bytes.len() + 1);
        for (k, &i) in scans.iter().enumerate() {
            assert_eq!(events[i]["region"], name);
            assert_eq!(events[i]["offset"], offset + k);
            assert_eq!(
                events[i]["data"],
                json!([if k == bytes.len() { 0 } else { bytes[k] }])
            );
        }
        if let Some(&i) = null_reads.first() {
            assert!(i < scans[0]);
        }
        let copies: Vec<_> = (start + 1..upper)
            .filter(|i| {
                events[*i]["kind"] == "routine_entry"
                    && events[*i]["function"] == "copy"
                    && n(&events[*i]["arguments"][0]) == pointer
            })
            .collect();
        assert_eq!(copies.len(), 1);
        let copy = copies[0];
        assert_eq!(
            events[copy]["arguments"],
            json!([pointer, bytes.len(), f.sp() - 0x24, f.sp() - 0x50])
        );
        assert!(*scans.last().unwrap() < copy);
        let finish = (copy + 1..upper)
            .find(|i| events[*i]["kind"] == "routine_return" && events[*i]["function"] == "copy")
            .unwrap();
        let reads: Vec<_> = (copy + 1..finish)
            .filter(|i| events[*i]["kind"] == "read" && events[*i]["ip"] == 0x514d55)
            .collect();
        assert_eq!(reads.len(), bytes.len());
        for (k, &i) in reads.iter().enumerate() {
            assert_eq!(events[i]["region"], name);
            assert_eq!(events[i]["offset"], offset + k);
            assert_eq!(events[i]["data"], json!([bytes[k]]));
        }
        finishes.push(finish);
        contents.push(bytes);
    }
    let pointers: Vec<_> = (f.index..end)
        .filter(|i| events[*i]["kind"] == "read" && events[*i]["ip"] == 0x514737)
        .collect();
    let highs: Vec<_> = (f.index..end)
        .filter(|i| events[*i]["kind"] == "read" && events[*i]["ip"] == 0x514743)
        .collect();
    let writes: Vec<_> = (f.index..end)
        .filter(|i| events[*i]["kind"] == "format_write")
        .collect();
    for (k, &i) in writes.iter().enumerate() {
        assert_eq!(events[i]["output"], args[0]);
        assert_eq!(events[i]["offset"], k);
        assert_eq!(events[i]["bytes"], 1);
    }
    let formats: Vec<_> = window
        .iter()
        .filter(|v| {
            v["kind"] == "read" && ["single_format", "pair_format"].contains(&text(&v["region"]))
        })
        .collect();
    for (k, v) in formats.iter().enumerate() {
        assert_eq!(
            v["region"],
            if pair { "pair_format" } else { "single_format" }
        );
        assert_eq!(v["offset"], k);
        assert_eq!(v["ip"], if k == 0 { 0x5145a9 } else { 0x514ca3 });
    }
    if pair {
        if returned.is_some() {
            assert_eq!(contents.len(), 2);
            assert_eq!(pointers.len(), 2);
            assert_eq!(highs.len(), 2);
            let mut previous = finishes[0];
            for (k, name) in ["colon_high", "space_high"].into_iter().enumerate() {
                assert!(previous < pointers[k] && pointers[k] + 1 == highs[k]);
                assert_eq!(events[pointers[k]]["data"], json!([0x7e, 0x5c, 0x54, 0]));
                assert_eq!(events[highs[k]]["region"], name);
                let w = writes[contents[0].len() + k];
                assert!(highs[k] < w && w < varargs[1]);
                assert_eq!(events[w]["data"], json!([if k == 0 { 58 } else { 32 }]));
                previous = w;
            }
            assert!(previous < varargs[1] && varargs[1] < finishes[1]);
        } else if !pointers.is_empty() {
            assert_eq!(pointers.len(), 1);
            assert!(highs.is_empty());
            assert_eq!(contents.len(), 1);
            assert!(finishes[0] < pointers[0]);
            assert_eq!(events[pointers[0]]["data"], json!([null, null, null, null]));
        }
    } else {
        assert!(pointers.is_empty() && highs.is_empty());
    }
    if let Some(ret) = returned {
        assert_eq!(varargs.len(), args.len() - 2);
        assert_eq!(formats.len(), if pair { 7 } else { 3 });
        let mut expected = contents[0].clone();
        if pair {
            expected.extend(b": ");
            expected.extend(&contents[1]);
        }
        assert!(expected.len() <= 191);
        assert_eq!(ret["raw_eax"], expected.len());
        expected.push(0);
        assert_eq!(writes.len(), expected.len());
        for (&i, &byte) in writes.iter().zip(&expected) {
            assert_eq!(events[i]["data"], json!([byte]));
        }
        assert_eq!(events[*writes.last().unwrap()]["ip"], 0x511b91);
        let result = masked(&ret["formatted"]);
        let before = masked(&f.entry["output_before"]);
        assert_eq!(result.len(), 192);
        assert_eq!(
            result[..expected.len()],
            expected.iter().copied().map(Some).collect::<Vec<_>>()
        );
        assert_eq!(result[expected.len()..], before[expected.len()..]);
    } else {
        // A stopped format has no complete portable buffer result. Its actual
        // reached source bytes/output writes and finite native transitions remain.
        assert!(!writes.iter().any(|i| events[*i]["ip"] == 0x511b91));
    }
}

fn return_frame(
    f: &Frame,
    event: &Value,
    events: &[Value],
    end: usize,
    s: &Storage,
    capture: &Value,
    o: &Original,
) {
    assert_eq!(event["function"], f.entry["function"]);
    for key in ["receiver", "arguments", "return_ip"] {
        assert_eq!(event[key], f.entry[key]);
    }
    let pop = match f.name() {
        "message" => 8,
        "set" | "query" | "merge" | "copy_grants" | "h0_miss" => 4,
        _ => 0,
    };
    assert_eq!(event["pop_bytes"], pop);
    assert_eq!(event["stack_after"], f.sp() + 4 + pop);
    let ip = n(&event["ip"]);
    let (start, count) = body(f.name());
    assert!((start..start + count as u32).contains(&ip));
    if pop == 0 {
        assert_eq!(o.at(ip, 1), [0xc3]);
    } else {
        assert_eq!(o.at(ip, 3), [0xc2, pop as u8, 0]);
    }
    let args = f.args();
    let raw = n(&event["raw_eax"]);
    match f.name() {
        "query" => {
            let id = args[0] & 0xffff;
            let expected = if id == 0 {
                true
            } else {
                scalar(&masked(&events[f.reads[0]]["data"])) & 1 << (id & 31) != 0
            };
            assert_eq!(raw as u8, u8::from(expected));
        }
        "record" => {
            assert_eq!(raw, record(f.receiver()));
            assert!(f.reads.is_empty() && f.writes.is_empty() && f.children.is_empty());
        }
        "h0_miss" => {
            assert_eq!(raw, 0);
            assert!(f.reads.is_empty() && f.writes.is_empty() && f.children.is_empty());
        }
        "emitter" => {
            assert_eq!(f.emits, 1);
            assert_eq!(raw, args[2]);
            assert!(f.pending_byte.is_none());
        }
        "padding" => {
            assert_eq!(raw, args[1]);
            assert!(f.reads.is_empty() && f.children.is_empty());
        }
        "copy" => {
            assert_eq!(raw, 0);
            assert_eq!(f.reads.len(), args[1] as usize);
            assert_eq!(f.children.len(), args[1] as usize);
            assert!(f.pending_byte.is_none());
        }
        "vararg" => {
            assert!(f.reads.is_empty() && f.writes.is_empty() && f.children.is_empty());
        }
        "parser" => {
            assert_eq!(f.resets, f.varargs);
            let writes: Vec<_> = events[f.index..end]
                .iter()
                .filter(|v| v["kind"] == "format_write")
                .collect();
            assert_eq!(raw, writes.len() as u32);
        }
        "wrapper" => audit_formatter(f, end, events, s, capture, Some(event)),
        "message" => {
            assert_eq!(f.reads.len(), 2);
            assert_eq!(f.writes.len(), 2);
            assert!(f.children.is_empty());
            let old = &events[f.reads[0]];
            assert_eq!(old["ip"], 0x4135d0);
            assert_eq!(events[f.reads[1]]["ip"], 0x4135ee);
            assert_ne!(masked(&events[f.reads[1]]["data"])[0].unwrap(), 0);
            assert!(
                f.reads[0] < f.writes[0] && f.writes[0] < f.reads[1] && f.reads[1] < f.writes[1]
            );
            assert_eq!(events[f.writes[0]]["ip"], 0x4135de);
            assert_eq!(events[f.writes[1]]["ip"], 0x4137b6);
            assert_eq!(events[f.writes[1]]["data"], old["data"]);
            assert_eq!(raw, scalar(&masked(&old["data"])));
        }
        _ => {}
    }
    audit_grant_frame(f, events, true);
}

fn timeline(request: &Value, receipt: &Value, s: &mut Storage, capture: &Value, o: &Original) {
    let events = array(&receipt["events"]);
    let mut frames: Vec<Frame> = Vec::new();
    let mut formatted = Vec::new();
    let mut effect = 0;
    for (index, event) in events.iter().enumerate() {
        match text(&event["kind"]) {
            "supplied_effect" => {
                assert!(frames.is_empty());
                assert_eq!(index, effect);
                assert_eq!(event["scope"], "between_requests_only");
                assert_eq!(event["patch"], request["patches"][effect]);
                assert!(["network", "first", "second"].contains(&text(&event["patch"]["region"])));
                apply(s, &event["patch"]);
                effect += 1;
            }
            "routine_entry" => {
                if frames.is_empty() {
                    assert_eq!(index, effect);
                    assert_eq!(effect, array(&request["patches"]).len());
                }
                audit_entry(event, &mut frames, events, index, request, o);
                frames.push(Frame {
                    entry: event.clone(),
                    index,
                    reads: Vec::new(),
                    writes: Vec::new(),
                    children: Vec::new(),
                    pending_byte: None,
                    emits: 0,
                    output: Vec::new(),
                    resets: 0,
                    varargs: 0,
                });
            }
            "routine_return" => {
                let f = frames.pop().unwrap();
                return_frame(&f, event, events, index, s, capture, o);
                if f.name() == "wrapper" {
                    let mut record = event.clone();
                    record.as_object_mut().unwrap().remove("kind");
                    formatted.push(record);
                }
                if let Some(parent) = frames.last_mut() {
                    parent.children.push((f.index, index));
                } else {
                    assert_eq!(index + 1, events.len());
                    assert_eq!(event["stack_after"], STACK + 8);
                    assert_eq!(receipt["incidental_eax"], event["raw_eax"]);
                }
            }
            "read" => {
                audit_read(event, &mut frames, s, capture);
                frames.last_mut().unwrap().reads.push(index);
            }
            "write" => {
                let f = frames.last_mut().unwrap();
                assert!(["set", "copy_grants"].contains(&f.name()));
                let name = text(&event["region"]);
                let offset = n(&event["offset"]) as usize;
                let bytes = masked(&event["data"]);
                assert_eq!(event["bytes"], 4);
                assert_eq!(bytes.len(), 4);
                assert!(bytes.iter().all(Option::is_some));
                assert_eq!(name, receiver_region(f.receiver()));
                if f.name() == "set" {
                    assert_eq!(event["ip"], 0x47510a);
                    assert_eq!(
                        absolute(name, offset, capture),
                        f.receiver() + 4 + ((f.args()[0] & 0xffff) >> 5) * 4
                    );
                    assert_eq!(events[index - 1]["kind"], "read");
                    assert_eq!(events[index - 1]["ip"], 0x4750fb);
                } else {
                    assert_eq!(event["ip"], 0x475392);
                    assert_eq!(
                        absolute(name, offset, capture),
                        f.receiver() + 4 + f.writes.len() as u32 * 4
                    );
                    assert_eq!(events[index - 1]["kind"], "read");
                    assert_eq!(events[index - 1]["ip"], 0x47538f);
                    assert_eq!(events[index - 1]["data"], event["data"]);
                }
                put(s, name, offset, &bytes);
                f.writes.push(index);
            }
            "support_write" => {
                let f = frames.last_mut().unwrap();
                assert_eq!(f.name(), "message");
                assert_eq!(event["region"], "seh_chain");
                assert_eq!(event["offset"], 0);
                assert_eq!(event["bytes"], 4);
                let bytes = masked(&event["data"]);
                assert_eq!(bytes.len(), 4);
                assert!(bytes.iter().all(Option::is_some));
                match n(&event["ip"]) {
                    0x4135de => {
                        assert!(f.writes.is_empty());
                        assert_eq!(bytes, word(Some(f.sp() - 12)));
                    }
                    0x4137b6 => {
                        assert_eq!(f.writes.len(), 1);
                        assert_eq!(bytes, masked(&events[f.reads[0]]["data"]));
                    }
                    _ => panic!("SEH write"),
                }
                put(s, "seh_chain", 0, &bytes);
                f.writes.push(index);
            }
            "format_write" => {
                assert_eq!(event["bytes"], 1);
                let bytes = masked(&event["data"]);
                assert_eq!(bytes.len(), 1);
                let byte = bytes[0].unwrap();
                let top = frames.last_mut().unwrap();
                match n(&event["ip"]) {
                    0x514ce9 => {
                        assert_eq!(top.name(), "emitter");
                        assert_eq!(byte, top.args()[0] as u8);
                        top.emits += 1;
                    }
                    0x511b91 => {
                        assert_eq!(top.name(), "wrapper");
                        assert_eq!(byte, 0);
                    }
                    _ => panic!("output write"),
                }
                let w = frames
                    .iter_mut()
                    .rev()
                    .find(|f| f.name() == "wrapper")
                    .unwrap();
                assert_eq!(event["output"], w.args()[0]);
                assert_eq!(event["offset"], w.output.len());
                assert!(w.output.len() < 192);
                w.output.extend(bytes);
            }
            "precision_reset" => {
                let w = wrapper(&frames);
                let address = w.sp() - 0x4c;
                assert_eq!(event["offset"], address - STACK_BASE);
                assert_eq!(event["bytes"], 4);
                assert_eq!(event["ip"], 0x51460d);
                assert_eq!(event["data"], json!([255, 255, 255, 255]));
                let f = frames.last_mut().unwrap();
                assert_eq!(f.name(), "parser");
                assert_eq!(f.resets, f.varargs);
                f.resets += 1;
            }
            "finite_unknown_read" => {
                assert_eq!(event["region"], "stack");
                let bytes = masked(&event["data"]);
                assert!(bytes.iter().any(Option::is_none));
                assert_eq!(bytes.len(), n(&event["bytes"]) as usize);
                let w = wrapper(&frames);
                let ip = n(&event["ip"]);
                let (address, count, label) = match ip {
                    0x5145bd => (w.sp() - 0x4c, 4, "dead_initial_precision_load"),
                    0x51460d => (w.sp() - 0x4c, 4, "deterministic_precision_or_read"),
                    0x511b91 => (
                        w.args()[0] + w.output.len() as u32,
                        1,
                        "deterministic_final_nul_and_read",
                    ),
                    _ => panic!("finite unknown"),
                };
                assert_eq!(event["offset"], address - STACK_BASE);
                assert_eq!(event["bytes"], count);
                assert_eq!(event["allowance"], label);
                assert_eq!(
                    frames.last().unwrap().name(),
                    if ip == 0x511b91 { "wrapper" } else { "parser" }
                );
                if ip == 0x51460d {
                    assert_eq!(events[index + 1]["kind"], "precision_reset");
                }
                if ip == 0x511b91 {
                    assert_eq!(events[index + 1]["kind"], "format_write");
                    assert_eq!(events[index + 1]["ip"], ip);
                }
            }
            "required_boundary" => {
                assert_eq!(index + 1, events.len());
                assert!(!frames.is_empty());
                assert_eq!(receipt["stop_ip"], event["ip"]);
                assert!(!receipt["stop"].is_null());
                if event["operation"] == "enabled_message_before_3d" {
                    let f = frames.last().unwrap();
                    assert_eq!(f.name(), "message");
                    assert_eq!(event["ip"], 0x4135f7);
                    assert_eq!(f.reads.len(), 2);
                    assert_eq!(f.writes.len(), 1);
                    assert_eq!(events[f.reads[1]]["data"], json!([0]));
                } else {
                    let read = &events[index - 1];
                    assert_eq!(read["kind"], "read");
                    for key in ["region", "offset", "bytes", "ip"] {
                        assert_eq!(event[key], read[key]);
                    }
                    assert_eq!(event["mask"], read["data"]);
                    assert!(masked(&read["data"]).iter().any(Option::is_none));
                    assert_eq!(
                        text(&event["operation"]),
                        format!("unavailable_{}", text(&read["region"]))
                    );
                }
            }
            "raw_boundary" => {
                assert_eq!(index + 1, events.len());
                assert_eq!(receipt["stop"], "unrepresented_raw_read");
                assert_eq!(receipt["stop_ip"], event["ip"]);
                assert_eq!(event["ip"], 0x47512c);
                assert_eq!(event["address"], record(191));
                assert_eq!(event["bytes"], 2);
            }
            _ => panic!("unadmitted event"),
        }
    }
    assert_eq!(receipt["fault"], Value::Null);
    assert_eq!(receipt["result"], Value::Null);
    assert_eq!(receipt["native_completed"], frames.is_empty());
    assert_eq!(frames.is_empty(), receipt["stop"].is_null());
    assert_eq!(receipt["formatted"], json!(formatted));
    for f in &frames {
        if f.name() == "wrapper" {
            audit_formatter(f, events.len(), events, s, capture, None);
        }
        if ["set", "copy_grants"].contains(&f.name()) {
            audit_grant_frame(f, events, false);
        }
    }
    snapshot(&receipt["state"], s);
    let stack = stack_mask(&receipt["state"]["stack"]);
    let arg = if request["operation"] == "set" {
        n(&request["raw_id"])
    } else {
        owner(text(&request["source"]))
    };
    let mut carrier = word(Some(RETURN));
    carrier.extend(word(Some(arg)));
    assert_eq!(stack[0x2000..0x2008], carrier);
}

fn increment(map: &mut BTreeMap<String, u32>, key: &str) {
    *map.entry(key.to_owned()).or_default() += 1;
}
fn discriminators(cases: &[Value]) {
    let events = |index: usize| array(&cases[index]["steps"][0]["events"]);
    let has_read = |index: usize, ip: u32| {
        events(index)
            .iter()
            .any(|v| v["kind"] == "read" && v["ip"] == ip)
    };
    assert!(!has_read(0, 0x47530f) && !has_read(0, 0x475113));
    assert!(!has_read(1, 0x475113));
    for index in [2, 3] {
        assert!(!events(index).iter().any(|v| v["kind"] == "read"
            && ["low_records", "high_records"].contains(&text(&v["region"]))));
    }
    assert!(!events(3)
        .iter()
        .any(|v| v["kind"] == "read" && v["region"] == "h0"));
    assert!(!has_read(4, 0x475138));
    assert!(!events(4)
        .iter()
        .any(|v| v["kind"] == "routine_entry"
            && ["wrapper", "message"].contains(&text(&v["function"]))));
    for index in [5, 7] {
        assert!(!events(index)
            .iter()
            .any(|v| v["kind"] == "routine_entry" && v["function"] == "h0_miss"));
        assert_eq!(
            events(index)
                .iter()
                .filter(|v| v["kind"] == "routine_entry" && v["function"] == "message")
                .map(|v| n(&v["receiver"]))
                .collect::<Vec<_>>(),
            [0x741088]
        );
    }
    for (index, count) in [(6, 1), (8, 2)] {
        assert_eq!(
            events(index)
                .iter()
                .filter(|v| v["kind"] == "routine_entry" && v["function"] == "h0_miss")
                .count(),
            count
        );
    }
    assert_eq!(
        events(9)
            .iter()
            .filter(|v| v["kind"] == "routine_entry" && v["function"] == "set")
            .map(|v| n(&v["arguments"][0]))
            .collect::<Vec<_>>(),
        [0x60, 0x95, 0x96, 0x97]
    );
    let changed = array(&cases[14]["steps"]);
    assert_eq!(array(&changed[0]["formatted"]).len(), 1);
    assert!(
        array(&changed[1]["formatted"]).is_empty() && array(&changed[2]["formatted"]).is_empty()
    );
    assert!(!array(&changed[2]["events"])
        .iter()
        .any(|v| v["kind"] == "read" && v["region"] == "network"));
}

pub(super) fn load_and_validate_fixture() -> Option<Value> {
    assert_eq!(digest(FIXTURE), FIXTURE_HASH);
    assert_eq!(FIXTURE.len(), 1829374);
    assert!(!FIXTURE.contains(&b'\r'));
    let value: Value = serde_json::from_slice(FIXTURE).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 3);
    let o = Original::load()?;
    provenance(&value["metadata"], &o);
    assets(&value["asset_capture"]);
    let cases = array(&value["cases"]);
    assert_eq!(cases.len(), CASES.len());
    let mut event_counts = BTreeMap::new();
    let mut entry_counts = BTreeMap::new();
    let mut return_counts = BTreeMap::new();
    let mut outcomes = BTreeMap::new();
    let mut step_index = 0;
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        assert_eq!(input["name"], CASES[index]);
        assert_eq!(case["instruction_bytes_unchanged"], true);
        let evidence = if index < 15 {
            "original_body"
        } else if index < 21 {
            "semantic_prefix"
        } else {
            "source_diagnostic"
        };
        assert_eq!(case["evidence"], evidence);
        assert_eq!(input["evidence"], evidence);
        let mut storage = initial(input, &o, &value["asset_capture"]);
        snapshot(&case["initial_state"], &storage);
        assert!(array(&case["initial_state"]["stack"]["known_spans"]).is_empty());
        assert_eq!(array(&input["steps"]).len(), array(&case["steps"]).len());
        for (request, receipt) in array(&input["steps"]).iter().zip(array(&case["steps"])) {
            timeline(request, receipt, &mut storage, &value["asset_capture"], &o);
            let (reads, writes) = STACK_COUNTS[step_index];
            assert_eq!(
                receipt["validated_stack_accesses"],
                json!({"reads":reads,"writes":writes})
            );
            step_index += 1;
            let outcome = if evidence == "source_diagnostic" {
                "source_diagnostic"
            } else if evidence == "semantic_prefix" {
                "semantic_prefix_stop"
            } else {
                "original_body_complete"
            };
            assert_eq!(receipt["outcome"], outcome);
            increment(&mut outcomes, outcome);
            if index < 15 {
                assert_eq!(receipt["native_completed"], true);
                assert_eq!(receipt["stop_ip"], Value::Null);
            } else if index < 21 {
                let (_, reason, ip) = PREFIXES[index - 15];
                assert_eq!(receipt["stop"], reason);
                assert_eq!(receipt["stop_ip"], ip);
                assert!(array(&receipt["events"])
                    .iter()
                    .any(|v| v["kind"] == "write" && v["ip"] == 0x47510a));
            }
            for event in array(&receipt["events"]) {
                let kind = text(&event["kind"]);
                increment(&mut event_counts, kind);
                if kind == "routine_entry" {
                    increment(&mut entry_counts, text(&event["function"]));
                }
                if kind == "routine_return" {
                    increment(&mut return_counts, text(&event["function"]));
                }
            }
        }
    }
    assert_eq!(step_index, 28);
    let counts = &value["metadata"]["counts"];
    assert_eq!(json!(event_counts), counts["events"]);
    assert_eq!(json!(entry_counts), counts["entries"]);
    assert_eq!(json!(return_counts), counts["returns"]);
    assert_eq!(json!(outcomes), counts["outcomes"]);
    discriminators(cases);
    Some(value)
}

#[test]
fn live_grant_original_byte_protocol_and_finite_asset_backing() {
    load_and_validate_fixture();
}
