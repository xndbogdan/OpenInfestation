//! Independent original-byte and recorded-protocol audit. This test never calls
//! the portable command kernel. Unlogged stack accesses/nonvolatile restoration
//! were checked online by the producing CPU harness; sparse masks and counts
//! are retained here without inventing a complete serialized stack timeline.

use std::{collections::BTreeMap, path::Path};

use inf_formats::{Preload, RequestKind};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/player_commands_native.json"
));
// The producing checker stays private; the fixture records its SHA-256.
const FIXTURE_HASH: &str = "bb86a8015881ccc85b98ff8065dd9cb56b39761dcf674dfd5946f715d94e8e3f";
const PRODUCER_HASH: &str = "f7b0badafdd7844fb011a9d2985f3ad0e8158ca2a56a60e276fae273440037d0";
const EXE_HASH: &str = "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956";
const TEXT_HASH: &str = "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2";
const PRELOAD_HASH: &str = "d41b9eb6aceae2df8c53e802ee27c6e5fcf5fbc9608c3df4758171ab6e8931d3";
const STACK: u32 = 0x730000;
const RETURN: u32 = 0xf00000;
const UNMAPPED: u32 = 0xdead1000;
const TABLE: u32 = 0x53a27c;
const H0_TABLE: u32 = 0x744000;
const CAPACITY: usize = 950;

const BODIES: [(&str, u32, usize, &str); 4] = [
    (
        "set",
        0x46c760,
        185,
        "ad98f55bb693b85486e734fb22594acddc1ac00039d59a11e4f3b8e4ec500bc1",
    ),
    (
        "query",
        0x46c820,
        54,
        "8b53fc45286ed5325dbf799c0cf034531a4a7c88699f082fc22c23a4f93cf300",
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
];
const STATIC: [(&str, u32, usize, &str); 11] = [
    (
        "constructor_byte58",
        0x469d78,
        46,
        "267d528cc12a8562e3cc347aafe7348a2040ccb6d6a5bc005deb6b01821619e3",
    ),
    (
        "constructor_command_words",
        0x469f93,
        23,
        "cf6fa73f07dffbf7b4f889edcd3826a6c4df2798bf33d645c74fa69c059fe8a1",
    ),
    (
        "constructor_adjacent_constants",
        0x469ef4,
        32,
        "3348033a36bdc8065bf8e6d5a595b2f5f7b86b03e5b0085296ee367e6108a603",
    ),
    (
        "constructor_adjacent_byte_loop",
        0x469f33,
        25,
        "ebc60b86d5589bb507808678be230ac27e9250be3cf2ff084b107c320681de14",
    ),
    (
        "save_command_words",
        0x46c9b3,
        24,
        "a551b1ee60a445ac272381b8ee1be5451453d42f5a2db1a7980d6560edae8268",
    ),
    (
        "restore_command_words",
        0x46cb17,
        26,
        "7493fc6a7b854cd35fb2b8d42861f1278b96ec0a3de80918bef7048f3e7bac48",
    ),
    (
        "save_adjacent_words",
        0x46c97a,
        19,
        "fd9dd5c71af3987a3e1315ec4b40f4b23c298d55b23036f8c36e08b7b1026e01",
    ),
    (
        "restore_adjacent_words",
        0x46cad7,
        29,
        "a6594a92a702af3edbdac07ab211501532d80e7a90955303434957f85532dd95",
    ),
    (
        "bounded_table_caller",
        0x409e89,
        11,
        "855985dd8649baa87c8bb6fd3546a01fcceb443a9b40bf6803013438e76f6a0a",
    ),
    (
        "query_menu_decrement",
        0x41462c,
        16,
        "2f384c03610673e8731345e82eaf463ceab3f1c223c86fd702bc1941ffa3f194",
    ),
    (
        "query_menu_increment",
        0x414658,
        12,
        "6bb8ced85b20919af847c7f3a60696d01170d917c6d7a1dc0dc3005356f0daaa",
    ),
];
const INSTRUCTIONS: [(u32, &str); 23] = [
    (0x46c760, "a148f45500"),
    (0x46c771, "8b74240c"),
    (0x46c776, "e8a5000000"),
    (0x46c7a4, "8b8c8730010000"),
    (0x46c7ad, "898c8730010000"),
    (0x46c7b4, "8a442410"),
    (0x46c7bc, "668b34757ca25300"),
    (0x46c7c9, "8b0d58ee5500"),
    (0x46c7d4, "772e"),
    (0x46c7d6, "8b0d54ee5500"),
    (0x46c7dc, "8b0481"),
    (0x46c7e3, "8b1550ee5500"),
    (0x46c7ef, "ff12"),
    (0x46c7fa, "e8d16dfaff"),
    (0x46c80f, "e8bc6dfaff"),
    (0x46c801, "c20800"),
    (0x46c816, "c20800"),
    (0x46c847, "85bc8630010000"),
    (0x46c850, "0f95c0"),
    (0x46c853, "c20400"),
    (0x4135ee, "385d01"),
    (0x4135f7, "385d3d"),
    (0x4137c0, "c20800"),
];
const REGIONS: [(&str, u32, usize); 11] = [
    ("first", 0x740000, 0x1a0),
    ("second", 0x741000, 0x1a0),
    ("network", 0x55f448, 4),
    ("process_null", 0x54e894, 4),
    ("h0", 0x55ee50, 12),
    ("slots", H0_TABLE, CAPACITY * 4),
    ("command_table", TABLE, 146),
    ("adjacent_table", TABLE + 146, 2),
    ("h0_vtable", 0x527598, 20),
    ("seh_chain", 0, 4),
    ("stack", STACK - 0x1000, 0x1100),
];
const CASES: [&str; 25] = [
    "constructor-query-zero-is-not-owned",
    "silent-three-word-boundaries",
    "signed-minus-one-minus31-share-word-zero",
    "owned-bypasses-silent-exe-residency-message",
    "network100-bypasses-unmapped-player-and-raw-id",
    "lowbyte-silent100-versus101",
    "boot-holes-original-null-callback",
    "boot-retained-bank0-nonlinear-command41",
    "activate48-original-leaf1-occupied-records",
    "requested-first-and-second-player-independent",
    "current-network-and-suppression-reborrow",
    "prefix-missing-process-before-owned-query",
    "prefix-commit-before-missing-residency",
    "prefix-occupied-source-before-enabled-message",
    "prefix-null-source-before-enabled-message",
    "prefix-retained-commit-retry-missing-executable",
    "silent73-and-owned73-never-read-adjacent-table",
    "diagnostic-unknown-command-word",
    "diagnostic-negative32-aliases-player12c",
    "diagnostic-command96-aliases-adjacent-word",
    "diagnostic-full-dword-id-not-lowword",
    "diagnostic-nonsilent73-adjacent-table",
    "diagnostic-h0-count-equality-before-unknown-slot",
    "diagnostic-h0-count-greater-null-without-slot",
    "diagnostic-unknown-message-suppression",
];
const ASSETS: [(&str, u8, RequestKind, u32, usize, usize, usize, &str); 4] = [
    (
        "0.ovl",
        0,
        RequestKind::Base,
        0x720000,
        983065,
        53,
        6,
        "0d568d3aba640a144916276bc7fd2424497542cbfb98ac17858efd823a14d236",
    ),
    (
        "0l0.ovl",
        0,
        RequestKind::Language,
        0x724000,
        16091,
        16087,
        684,
        "39097d1188a9573e619591b9e2b99be1191e31943f5ff3f501fe63b20e7d2b26",
    ),
    (
        "2.ovl",
        2,
        RequestKind::Base,
        0x729000,
        64880,
        26,
        2,
        "5c8c45d856506d6c94a72012eccd21fe88ce8c19bf92e7e0aef85124a3a534be",
    ),
    (
        "1l0.ovl",
        1,
        RequestKind::Language,
        0x72c000,
        6536,
        6532,
        180,
        "eab6d4bdf70cf2851ac8843cfcd0144964fef7dcd9e1ce7ec08aca1ed0f81fdb",
    ),
];

fn arr(v: &Value) -> &[Value] {
    v.as_array().expect("native array")
}
fn txt(v: &Value) -> &str {
    v.as_str().expect("native string")
}
fn n(v: &Value) -> u32 {
    u32::try_from(v.as_u64().expect("native unsigned DWORD")).unwrap()
}
fn optional(v: &Value) -> Option<u32> {
    if v.is_null() {
        None
    } else {
        Some(n(v))
    }
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
fn masked(v: &Value) -> Vec<Option<u8>> {
    arr(v)
        .iter()
        .map(|b| optional(b).map(|b| u8::try_from(b).unwrap()))
        .collect()
}
fn word(v: Option<u32>) -> Vec<Option<u8>> {
    match v {
        Some(v) => v.to_le_bytes().into_iter().map(Some).collect(),
        None => vec![None; 4],
    }
}
fn scalar(v: &[Option<u8>]) -> u32 {
    assert!(!v.is_empty() && v.len() <= 4);
    v.iter().enumerate().fold(0, |v, (i, b)| {
        v | (u32::from(b.expect("reached whole known scalar")) << (i * 8))
    })
}
fn region(name: &str) -> (u32, usize) {
    let (_, base, count) = REGIONS.iter().find(|(name2, _, _)| *name2 == name).unwrap();
    (*base, *count)
}
fn receiver(name: &str) -> u32 {
    match name {
        "first" => 0x740000,
        "second" => 0x741000,
        "unmapped" => UNMAPPED,
        _ => panic!("unadmitted receiver"),
    }
}
fn body(name: &str) -> (u32, usize) {
    let (_, base, count, _) = BODIES.iter().find(|(n, _, _, _)| *n == name).unwrap();
    (*base, *count)
}
fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

struct Original {
    raw: Vec<u8>,
    sections: Vec<(u32, usize, usize)>,
    profile: Value,
}
impl Original {
    fn load() -> Option<Self> {
        let Ok(raw) = std::fs::read(root().join("Infest.exe")) else {
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
        assert_eq!(u16at(pe + 4), 0x14c);
        assert_eq!(u16at(pe + 6), 6);
        assert_eq!(u16at(pe + 20), 224);
        assert_eq!(u16at(pe + 24), 0x10b);
        assert_eq!(u32at(pe + 24 + 28), 0x400000);
        let table = pe + 24 + 224;
        let expected = [
            (".text", 1168501, 4096, 1171456, 4096),
            (".rdata", 57392, 1175552, 61440, 1175552),
            (".data", 1256504, 1236992, 102400, 1236992),
            (".idata", 4164, 2494464, 8192, 1339392),
            (".rsrc", 9768, 2502656, 12288, 1347584),
            (".reloc", 82228, 2514944, 86016, 1359872),
        ];
        let mut sections = Vec::new();
        let mut profile = Vec::new();
        for (i, (name, virtual_size, rva, raw_size, raw_offset)) in expected.into_iter().enumerate()
        {
            let p = table + i * 40;
            let actual_name = std::str::from_utf8(&raw[p..p + 8])
                .unwrap()
                .trim_end_matches('\0');
            assert_eq!(actual_name, name);
            assert_eq!(
                (u32at(p + 8), u32at(p + 12), u32at(p + 16), u32at(p + 20)),
                (virtual_size, rva, raw_size, raw_offset)
            );
            sections.push((0x400000 + rva, raw_size as usize, raw_offset as usize));
            profile.push(json!({"name":name,"virtual_size":virtual_size,"rva":rva,"raw_size":raw_size,"raw_offset":raw_offset}));
        }
        assert_eq!(digest(&raw[0x1000..0x11f000]), TEXT_HASH);
        Some(Self {
            raw,
            sections,
            profile: json!({"pe_offset":232,"image_base":0x400000,"optional_size":224,"sections":profile,"mapping_scope":"finite supported PE; code mapped by section RVA/raw offset"}),
        })
    }
    fn at(&self, address: u32, count: usize) -> &[u8] {
        let &(base, len, file) = self
            .sections
            .iter()
            .find(|(base, len, _)| {
                u64::from(address) >= u64::from(*base)
                    && u64::from(address) + count as u64 <= u64::from(*base) + *len as u64
            })
            .expect("fully file-backed guard");
        assert!(count <= len);
        let offset = file + (address - base) as usize;
        &self.raw[offset..offset + count]
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
    assert_eq!(m["producer_sha256"], PRODUCER_HASH);
    assert_eq!(m["text_address"], 0x401000);
    assert_eq!(m["text_sha256"], TEXT_HASH);
    assert_eq!(m["fpcw"], 0x027f);
    assert_eq!(m["pe_profile"], o.profile);
    for (key, guards, execution) in [
        ("body_guards", BODIES.as_slice(), "unchanged_armed_body"),
        (
            "static_guards",
            STATIC.as_slice(),
            "static_only_not_executed",
        ),
    ] {
        assert_eq!(arr(&m[key]).len(), guards.len());
        for (v, &(name, address, count, hash)) in arr(&m[key]).iter().zip(guards) {
            assert_eq!(v["name"], name);
            assert_eq!(v["start"], address);
            assert_eq!(v["end"], address + count as u32);
            assert_eq!(v["sha256"], hash);
            assert_eq!(v["execution"], execution);
            assert_eq!(digest(o.at(address, count)), hash);
        }
    }
    assert_eq!(arr(&m["instruction_guards"]).len(), INSTRUCTIONS.len());
    for (v, (address, code)) in arr(&m["instruction_guards"]).iter().zip(INSTRUCTIONS) {
        assert_eq!(v, &json!({"address":address,"sha256":digest(&hex(code))}));
        assert_eq!(hex(code), o.at(address, hex(code).len()));
    }
    let table = o.at(TABLE, 146);
    assert_eq!(
        digest(table),
        "e400339c64e0b775e6d68f3a0476cd56ce175da3b016948a76f17f3bf1bdca5e"
    );
    let words = table
        .chunks_exact(2)
        .map(|w| u16::from_le_bytes(w.try_into().unwrap()))
        .collect::<Vec<_>>();
    assert!(words.iter().all(|w| *w > 0 && *w < 0x8000));
    assert_eq!(
        (
            words[0],
            words[1],
            words[0x37],
            words[0x41],
            words[0x42],
            words[0x48]
        ),
        (0x2b4, 0x2b5, 0x2eb, 0x170, 0x2f5, 0x366)
    );
    assert_eq!(
        m["command_table"],
        json!({"address":TABLE,"width":2,"entries":73,"bytes":146,"sha256":digest(table),"words":words,"scope":"original_positive73WORD_table_static_guard_no_adjacent_domain_extension"})
    );
    assert_eq!(
        digest(o.at(0x527598, 20)),
        "ed760b2862e7099d6fd6057ce45757fdd9dccdb2b51921f218a9284256de7b0c"
    );
    assert_eq!(
        m["h0_vtable"],
        json!({"address":0x527598,"sha256":"ed760b2862e7099d6fd6057ce45757fdd9dccdb2b51921f218a9284256de7b0c"})
    );
    assert_eq!(m["regions"].as_object().unwrap().len(), REGIONS.len());
    for (name, address, count) in REGIONS {
        assert_eq!(m["regions"][name], json!({"address":address,"bytes":count}));
    }
    assert_eq!(
        m["abi"],
        json!({
        "set":{"arguments":["raw_id:u32","silent:u32"],"ret_pop_bytes":8,"result":"void_incidental_EAX"},
        "query":{"arguments":["raw_id:u32"],"ret_pop_bytes":4,"result":"AL_truth_only"},
        "h0_miss":{"arguments":["raw_H0_operand:u32"],"ret_pop_bytes":4,"result":"full_EAX_NULL"},
        "message":{"arguments":["unread_text_pointer:u32","unread_flag:u32"],"ret_pop_bytes":8,"result":"void_incidental_EAX"}})
    );
    assert_eq!(
        m["semantic_prefix_names"],
        json!([CASES[11], CASES[12], CASES[13], CASES[14], CASES[15]])
    );
    assert_eq!(m["diagnostic_names"], json!(&CASES[17..]));
    assert_eq!(
        m["asset_backing_scope"],
        json!({
        "boot":{"current_allocations":["0.ovl","0l0.ovl","2.ovl"],"occupied_slots":691,"original_records":692,"unused_physical_backing":["1l0.ovl"]},
        "activate48":{"current_allocations":["0.ovl","0l0.ovl","1l0.ovl"],"occupied_slots":869,"original_records":870,"unused_physical_backing":["2.ovl"]},
        "unavailable":{"unused_physical_backing":["0.ovl","0l0.ovl","2.ovl","1l0.ovl","slots"],"scope":"native physical backing only; no portable allocation presence or H0 ownership parity"},
        "capture":"source-authenticated PRELOAD/OVL declarations; future canonical Rust Boot/Activate48 verification required"})
    );
    assert_eq!(
        m["stack_scope"],
        json!({
        "caller_carriers":"each request supplies only return/arguments and nonvolatile test-register carriers",
        "masks":"all other stack bytes retain previous knownness, no unknown zeroing or numeric allowances",
        "serialization":"explicit argument reads only; remaining accesses validated online and counted, not serialized full timeline"})
    );
    assert_eq!(
        m["scopes"],
        json!([
        "Only four unchanged bodies while armed; no imports/services/CRT/RET injection",
        "Static constructor/save/table guards are not executed constructor/save proof",
        "Original declared asset/registration capture requires actual Rust residency verification",
        "Unknown reached data stops preserve actual earlier writes; no native fault claim",
        "Online stack/register/RET/FS protocol, sparse known stack state; no measured Rust cadence",
        "Native inherited particle pool preparation unarmed and excluded from request counts"])
    );
    assert_eq!(
        m["exclusions"],
        json!([
            "Exact raw diagnostics before future typed preparation",
            "native address/PlayerId/slice parity",
            "enabled message contents or UI",
            "arbitrary H0 callback/aliases",
            "enclosing intro/capture callers",
            "production attachment or retail acceptance"
        ])
    );
}

fn allocations(c: &Value) -> Vec<&Value> {
    arr(&c["allocations"])
        .iter()
        .chain(std::iter::once(&c["leaf1_language"]))
        .collect()
}

fn assets(c: &Value) {
    let raw = std::fs::read(root().join("PreLoad.dat")).unwrap();
    assert_eq!(raw.len(), 18280);
    assert_eq!(digest(&raw), PRELOAD_HASH);
    let preload = Preload::parse(&raw).unwrap();
    assert_eq!(preload.header.handler_capacities[0] as usize, CAPACITY);
    assert_eq!(c["preload_sha256"], PRELOAD_HASH);
    assert_eq!(c["preload_length"], raw.len());
    assert_eq!(c["capacity"], CAPACITY);
    assert_eq!(
        c["selection"],
        json!({"kind":"boot","difficulty":0,"language":0,"levels":[0,2]})
    );
    assert_eq!(
        c["claim"],
        "declared_original_asset_backing_not_python_ResidentRuntime_or_loader_execution"
    );
    assert_eq!(arr(&c["allocations"]).len(), 3);
    let mut slots = vec![0u32; CAPACITY];
    let mut overwrites = Vec::new();
    let mut boot_slots = Vec::new();
    for (i, (v, (filename, level, phase, address, file_bytes, payload_bytes, count, hash))) in
        allocations(c).into_iter().zip(ASSETS).enumerate()
    {
        let raw = std::fs::read(root().join("Overlay").join(filename)).unwrap();
        assert_eq!(raw.len(), file_bytes);
        assert_eq!(digest(&raw), hash);
        assert_eq!(
            u32::from_le_bytes(raw[..4].try_into().unwrap()) as usize,
            payload_bytes
        );
        let payload = &raw[4..4 + payload_bytes];
        assert_eq!(v["filename"], filename);
        assert_eq!(v["level"], level);
        assert_eq!(
            v["phase"],
            match phase {
                RequestKind::Base => "base",
                RequestKind::Language => "language",
                _ => unreachable!(),
            }
        );
        assert_eq!(v["handler"], 0);
        assert_eq!(v["region_index"], 0);
        assert_eq!(v["native_address"], address);
        assert_eq!(v["overlay_sha256"], hash);
        assert_eq!(v["overlay_length"], file_bytes);
        assert_eq!(v["payload_span"], json!([4, 4 + payload_bytes]));
        assert_eq!(
            masked(&v["payload"]),
            payload.iter().copied().map(Some).collect::<Vec<_>>()
        );
        if i == 3 {
            assert_eq!(v["payload_sha256"], digest(payload));
        }
        let request = &preload.leaf(level).unwrap().requests(phase).0[0];
        assert_eq!(request.count as usize, count);
        assert_eq!(
            v["request"],
            json!({"count":count,"segments":request.segments.iter().map(|s| s.packed()).collect::<Vec<_>>()})
        );
        assert_eq!(arr(&v["entries"]).len(), count);
        let mut start = 0usize;
        for (local, entry) in arr(&v["entries"]).iter().enumerate() {
            let end = start + payload[start..].iter().position(|b| *b == 0).unwrap() + 1;
            let global = usize::from(request.global_index(local as u16).unwrap());
            let pointer = address + start as u32;
            assert_eq!(
                entry,
                &json!({"local_index":local,"global_index":global,"record_span":[4 + start,4 + end],"native_pointer":pointer})
            );
            if slots[global] != 0 && i < 3 {
                overwrites.push(json!({"global_index":global,"previous_pointer":slots[global],"next_pointer":pointer,"level":level,"phase":txt(&v["phase"]),"local_index":local}));
            }
            slots[global] = pointer;
            start = end;
        }
        assert_eq!(start, payload.len());
        if i == 2 {
            assert_eq!(slots.iter().filter(|p| **p != 0).count(), 691);
            assert_eq!(c["slots"], json!(slots));
            assert_eq!(c["overwrites"], json!(overwrites));
            assert_eq!(overwrites.len(), 1);
            assert_eq!(overwrites[0]["global_index"], 355);
            boot_slots = slots.clone();
            // Original unconditional release clears these two registrations.
            for e in arr(&v["entries"]) {
                slots[n(&e["global_index"]) as usize] = 0;
            }
        }
    }
    assert_eq!(slots.iter().filter(|p| **p != 0).count(), 869);
    assert_eq!(c["activation48"]["slots"], json!(slots));
    for (raw_id, local) in [(0, 0), (1, 1), (0x37, 55), (0x42, 65), (0x48, 178)] {
        let e = &c["leaf1_language"]["entries"][local];
        assert_eq!(
            boot_slots[n(&e["global_index"]) as usize],
            0,
            "command {raw_id}"
        );
        assert_eq!(
            slots[n(&e["global_index"]) as usize],
            n(&e["native_pointer"])
        );
    }
    for (name, phase) in [
        ("base", RequestKind::Base),
        ("difficulty", RequestKind::Difficulty),
        ("language", RequestKind::Language),
        ("difficulty_language", RequestKind::DifficultyLanguage),
    ] {
        let req = &preload.leaf(48).unwrap().requests(phase).0[0];
        assert_eq!(req.count, 0);
        assert!(req.segments.is_empty());
        assert_eq!(
            c["activation48"]["leaf48_h0_requests"][name],
            json!({"count":0,"segments":[]})
        );
    }
    assert_eq!(
        c["activation48"]["leaf48_h0_requests"]
            .as_object()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(c["activation48"]["previous"], "Boot");
    assert_eq!(c["activation48"]["request_level"], 48);
    assert_eq!(
        c["activation48"]["operations"],
        json!([{"operation":"unload","level":2},{"operation":"load","level":1},{"operation":"load","level":48}])
    );
    assert_eq!(c["activation48"]["final_active_levels"], json!([0, 1, 48]));
    assert_eq!(c["activation48"]["released_h0_slots"], json!([872, 873]));
    assert_eq!(
        c["activation48"]["current_allocations"],
        json!(["0.ovl", "0l0.ovl", "1l0.ovl"])
    );
    assert_eq!(
        c["activation48"]["retired_unused_backing"],
        json!(["2.ovl"])
    );
    assert_eq!(c["activation48"]["expected_occupied_slots"], 869);
    assert_eq!(
        c["activation48"]["claim"],
        "declared source-authenticated tree/registration inputs, not Python residency execution"
    );
}

type Storage = BTreeMap<String, Vec<Option<u8>>>;
fn put(s: &mut Storage, name: &str, offset: usize, values: &[Option<u8>]) {
    s.get_mut(name).unwrap()[offset..offset + values.len()].copy_from_slice(values);
}
fn patch(s: &mut Storage, v: &Value) {
    put(
        s,
        txt(&v["region"]),
        n(&v["offset"]) as usize,
        &masked(&v["bytes"]),
    );
}
fn initial(input: &Value, o: &Original, c: &Value) -> Storage {
    let mut s: Storage = REGIONS
        .iter()
        .filter(|(name, _, _)| *name != "stack")
        .map(|(name, _, len)| ((*name).to_owned(), vec![None; *len]))
        .collect();
    for name in ["command_table", "adjacent_table", "h0_vtable"] {
        let (base, len) = region(name);
        put(
            &mut s,
            name,
            0,
            &o.at(base, len)
                .iter()
                .copied()
                .map(Some)
                .collect::<Vec<_>>(),
        );
    }
    for name in ["first", "second"] {
        assert_eq!(arr(&input[name]).len(), 3);
        let words = arr(&input[name])
            .iter()
            .flat_map(|v| word(Some(n(v))))
            .collect::<Vec<_>>();
        put(&mut s, name, 0x130, &words);
        put(
            &mut s,
            name,
            0x89,
            &[optional(&input["suppression"][name]).map(|b| u8::try_from(b).unwrap())],
        );
        put(&mut s, name, 0x12c, &word(Some(0)));
        for (offset, value) in [(0x13c, 0), (0x140, 0), (0x144, 1), (0x148, 0), (0x14c, 8)] {
            put(&mut s, name, offset, &word(Some(value)));
        }
    }
    let network = if input["process_owner_present"].as_bool().unwrap() {
        optional(&input["network"])
    } else {
        None
    };
    put(&mut s, "network", 0, &word(network));
    put(
        &mut s,
        "process_null",
        0,
        &word(optional(&input["process_null"])),
    );
    put(&mut s, "seh_chain", 0, &word(Some(0x12345678)));
    put(&mut s, "h0", 0, &word(Some(0x527598)));
    put(&mut s, "h0", 4, &word(Some(H0_TABLE)));
    put(
        &mut s,
        "h0",
        8,
        &word(if input["resident"].is_null() {
            None
        } else {
            Some(n(&input["capacity"]))
        }),
    );
    let slots = if input["resident"] == "activate48" {
        &c["activation48"]["slots"]
    } else {
        assert!(input["resident"].is_null() || input["resident"] == "boot");
        &c["slots"]
    };
    put(
        &mut s,
        "slots",
        0,
        &arr(slots)
            .iter()
            .flat_map(|v| word(Some(n(v))))
            .collect::<Vec<_>>(),
    );
    for allocation in allocations(c) {
        s.insert(
            txt(&allocation["filename"]).to_owned(),
            masked(&allocation["payload"]),
        );
    }
    if !input["executable"].as_bool().unwrap() {
        put(&mut s, "command_table", 0, &vec![None; 146]);
    }
    for v in arr(&input["patches"]) {
        patch(&mut s, v);
    }
    s
}

fn stack_mask(v: &Value) -> Vec<Option<u8>> {
    assert_eq!(v["length"], 0x1100);
    assert_eq!(
        v["omitted_bytes"],
        "explicitly_unknown_reused_native_stack_no_zero_defaults"
    );
    let mut values = vec![None; 0x1100];
    let mut end = 0;
    for span in arr(&v["known_spans"]) {
        let offset = n(&span["offset"]) as usize;
        let bytes = masked(&span["bytes"]);
        assert!(!bytes.is_empty() && bytes.iter().all(Option::is_some));
        assert!(offset >= end && offset + bytes.len() <= values.len());
        values[offset..offset + bytes.len()].copy_from_slice(&bytes);
        end = offset + bytes.len();
    }
    values
}
fn snapshot(v: &Value, s: &Storage) {
    assert_eq!(v.as_object().unwrap().len(), 7);
    for name in [
        "first",
        "second",
        "network",
        "process_null",
        "h0",
        "seh_chain",
    ] {
        assert_eq!(masked(&v[name]), s[name]);
    }
    stack_mask(&v["stack"]);
}

#[derive(Clone)]
struct Frame {
    function: &'static str,
    receiver: u32,
    arguments: Vec<u32>,
    stack: u32,
    ret: u32,
}
struct Protocol<'a> {
    events: &'a [Value],
    cursor: usize,
    frames: Vec<Frame>,
    receipt: &'a Value,
    storage: &'a mut Storage,
    capture: &'a Value,
    original: &'a Original,
}
type Reached<T> = Result<T, ()>;
impl Protocol<'_> {
    fn take(&mut self, kind: &str, fields: Value) -> Value {
        let event = self
            .events
            .get(self.cursor)
            .expect("ordered original event")
            .clone();
        self.cursor += 1;
        assert_eq!(event["kind"], kind, "event {}", self.cursor);
        for (key, value) in fields.as_object().unwrap() {
            assert_eq!(&event[key], value, "event {} field {key}", self.cursor);
        }
        event
    }
    fn enter(
        &mut self,
        function: &'static str,
        receiver: u32,
        arguments: Vec<u32>,
        stack: u32,
        ret: u32,
    ) {
        if !self.frames.is_empty() {
            match function {
                "query" | "message" => self.original.call(ret, body(function).0),
                "h0_miss" => assert_eq!(self.original.at(ret - 2, 2), [0xff, 0x12]),
                _ => panic!("unadmitted nested body"),
            }
        } else {
            assert_eq!(ret, RETURN);
            assert_eq!(stack, STACK);
        }
        self.take("routine_entry",json!({"function":function,"receiver":receiver,"arguments":arguments,"address":body(function).0,"entry_stack":stack,"return_ip":ret,"depth":self.frames.len()+1}));
        self.frames.push(Frame {
            function,
            receiver,
            arguments,
            stack,
            ret,
        });
    }
    fn leave(&mut self, ip: u32, low: Option<u8>, raw: Option<u32>) -> Value {
        let f = self.frames.last().unwrap().clone();
        let pop = if f.function == "query" || f.function == "h0_miss" {
            4
        } else {
            8
        };
        assert_eq!(self.original.at(ip, 3), [0xc2, pop as u8, 0]);
        let event=self.take("routine_return",json!({"function":f.function,"receiver":f.receiver,"arguments":f.arguments,"ip":ip,"return_ip":f.ret,"pop_bytes":pop,"stack_after":f.stack+4+pop}));
        assert_eq!(n(&event["low_al"]), n(&event["raw_eax"]) & 255);
        if let Some(low) = low {
            assert_eq!(n(&event["low_al"]), u32::from(low));
        }
        if let Some(raw) = raw {
            assert_eq!(n(&event["raw_eax"]), raw);
        }
        self.frames.pop();
        event
    }
    fn argument(&mut self, ip: u32, field: &str, value: u32, count: usize) {
        let f = self.frames.last().unwrap().clone();
        self.take("argument_read",json!({"function":f.function,"ip":ip,"field":field,"entry_stack":f.stack,"bytes":count,"data":word(Some(value))[..count]}));
    }
    fn stop(&mut self, reason: &str, ip: u32, fields: Value) -> Reached<u32> {
        let mut fields = fields;
        fields
            .as_object_mut()
            .unwrap()
            .insert("ip".into(), json!(ip));
        self.take("required_boundary", fields);
        assert_eq!(self.cursor, self.events.len());
        assert!(!self.frames.is_empty());
        assert_eq!(self.receipt["stop"], reason);
        assert_eq!(self.receipt["stop_ip"], ip);
        Err(())
    }
    fn read(&mut self, name: &str, offset: usize, count: usize, ip: u32) -> Reached<u32> {
        let values = self.storage[name][offset..offset + count].to_vec();
        self.take(
            "read",
            json!({"region":name,"offset":offset,"bytes":count,"data":values,"ip":ip}),
        );
        if values.iter().any(Option::is_none) {
            return self.stop(&format!("unknown_{name}"),ip,json!({"operation":format!("unavailable_{name}"),"region":name,"offset":offset,"bytes":count,"mask":values}));
        }
        Ok(scalar(&values))
    }
    fn write(&mut self, name: &str, offset: usize, value: u32, ip: u32, support: bool) {
        let values = word(Some(value));
        self.take(
            if support { "support_write" } else { "write" },
            json!({"region":name,"offset":offset,"bytes":4,"data":values,"ip":ip}),
        );
        put(self.storage, name, offset, &values);
    }
    fn player_word(&mut self, owner: u32, raw_id: u32, ip: u32) -> Reached<(String, usize, u32)> {
        let quotient = (raw_id as i32) / 32;
        let address = owner
            .wrapping_add((quotient as u32).wrapping_mul(4))
            .wrapping_add(0x130);
        let location = REGIONS.iter().find(|(_, base, count)| {
            u64::from(address) >= u64::from(*base)
                && u64::from(address) + 4 <= u64::from(*base) + *count as u64
        });
        let Some(&(name, base, _)) = location else {
            self.take("raw_boundary",json!({"ip":ip,"address":address,"bytes":4,"scope":"intentional_stop_before_unrepresented_effective_read_no_fault_claim"}));
            assert_eq!(ip, 0x46c847);
            assert_eq!(self.cursor, self.events.len());
            assert_eq!(self.receipt["stop"], "unrepresented_raw_read");
            assert_eq!(self.receipt["stop_ip"], ip);
            return Err(());
        };
        assert!(name == "first" || name == "second");
        let offset = (address - base) as usize;
        Ok((name.to_owned(), offset, self.read(name, offset, 4, ip)?))
    }
    fn query(&mut self, owner: u32, raw_id: u32, stack: u32, ret: u32) -> Reached<bool> {
        self.enter("query", owner, vec![raw_id], stack, ret);
        self.argument(0x46c820, "raw_id", raw_id, 4);
        let (_, _, value) = self.player_word(owner, raw_id, 0x46c847)?;
        let owned = value & (1u32 << (raw_id & 31)) != 0;
        self.leave(0x46c853, Some(u8::from(owned)), None);
        Ok(owned)
    }
    fn source_identity(&self, pointer: u32) -> Value {
        if pointer == 0 {
            return json!({"kind":"null"});
        }
        let matches = allocations(self.capture)
            .into_iter()
            .flat_map(|allocation| {
                arr(&allocation["entries"])
                    .iter()
                    .map(move |entry| (allocation, entry))
            })
            .filter(|(_, entry)| n(&entry["native_pointer"]) == pointer)
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
        let (allocation, entry) = matches[0];
        json!({"kind":"declared_original_record","allocation":allocation["filename"],"local_index":entry["local_index"],"global_index":entry["global_index"],"record_span":entry["record_span"]})
    }
    fn message(&mut self, owner: u32, source: u32, ret: u32) -> Reached<()> {
        self.enter("message", owner + 0x88, vec![source, 1], STACK - 20, ret);
        self.take("captured_source",json!({"ip":0x4135d0,"native_pointer":source,"identity":self.source_identity(source),"scope":"source_pointer_identity_only_no_record_byte_scan"}));
        let chain = self.read("seh_chain", 0, 4, 0x4135d0)?;
        self.write("seh_chain", 0, STACK - 32, 0x4135de, true);
        let name = if owner == receiver("first") {
            "first"
        } else {
            assert_eq!(owner, receiver("second"));
            "second"
        };
        let suppression = self.read(name, 0x89, 1, 0x4135ee)?;
        if suppression == 0 {
            self.stop(
                "enabled_message",
                0x4135f7,
                json!({"operation":"enabled_message_before_3d"}),
            )?;
            unreachable!();
        }
        self.write("seh_chain", 0, chain, 0x4137b6, true);
        self.leave(0x4137c0, None, Some(chain));
        Ok(())
    }
    fn set(&mut self, owner: u32, raw_id: u32, silent: u32) -> Reached<()> {
        self.enter("set", owner, vec![raw_id, silent], STACK, RETURN);
        if self.read("network", 0, 4, 0x46c760)? != 0 {
            self.leave(0x46c816, None, None);
            return Ok(());
        }
        self.argument(0x46c771, "raw_id", raw_id, 4);
        if self.query(owner, raw_id, STACK - 16, 0x46c77b)? {
            self.leave(0x46c816, None, None);
            return Ok(());
        }
        let (name, offset, value) = self.player_word(owner, raw_id, 0x46c7a4)?;
        self.write(
            &name,
            offset,
            value | (1u32 << (raw_id & 31)),
            0x46c7ad,
            false,
        );
        self.argument(0x46c7b4, "silent_low_byte", silent, 1);
        if silent as u8 != 0 {
            self.leave(0x46c816, None, None);
            return Ok(());
        }
        let address = TABLE.wrapping_add(raw_id.wrapping_mul(2));
        let text = if address >= TABLE && address + 2 <= TABLE + 146 {
            self.read("command_table", (address - TABLE) as usize, 2, 0x46c7bc)?
        } else {
            assert_eq!(address, TABLE + 146);
            self.read("adjacent_table", 0, 2, 0x46c7bc)?
        };
        let mut source = 0;
        let mut miss = false;
        if text < 0x8000 {
            let count = self.read("h0", 8, 4, 0x46c7c9)?;
            if text <= count {
                assert_eq!(self.read("h0", 4, 4, 0x46c7d6)?, H0_TABLE);
                source = self.read("slots", text as usize * 4, 4, 0x46c7dc)?;
                if source == 0 {
                    assert_eq!(self.read("h0", 0, 4, 0x46c7e3)?, 0x527598);
                    assert_eq!(self.read("h0_vtable", 0, 4, 0x46c7ef)?, body("h0_miss").0);
                    self.enter(
                        "h0_miss",
                        0x55ee50,
                        vec![(raw_id & 0xffff0000) | text],
                        STACK - 16,
                        0x46c7f1,
                    );
                    self.leave(0x4efbf2, None, Some(0));
                    miss = true;
                }
            }
        }
        self.message(owner, source, if miss { 0x46c7ff } else { 0x46c814 })?;
        self.leave(if miss { 0x46c801 } else { 0x46c816 }, None, None);
        Ok(())
    }
    fn audit(&mut self, request: &Value) {
        for v in arr(&request["patches"]) {
            self.take(
                "supplied_effect",
                json!({"patch":v,"scope":"between_requests_only"}),
            );
            patch(self.storage, v);
        }
        let owner = receiver(txt(&request["player"]));
        let reached = match txt(&request["operation"]) {
            "query" => self
                .query(owner, n(&request["raw_id"]), STACK, RETURN)
                .map(|owned| assert_eq!(self.receipt["result"], owned)),
            "set" => self
                .set(owner, n(&request["raw_id"]), n(&request["silent"]))
                .map(|()| assert!(self.receipt["result"].is_null())),
            _ => panic!("unadmitted operation"),
        };
        if reached.is_ok() {
            assert!(self.receipt["stop"].is_null());
            assert!(self.receipt["stop_ip"].is_null());
            assert!(self.frames.is_empty());
        } else {
            assert!(!self.frames.is_empty());
            assert!(self.receipt["result"].is_null());
        }
        assert_eq!(self.cursor, self.events.len());
        assert!(self.receipt["fault"].is_null());
        assert_eq!(self.receipt["native_completed"], self.frames.is_empty());
        if self.frames.is_empty() {
            assert_eq!(
                self.receipt["incidental_eax"],
                self.events.last().unwrap()["raw_eax"]
            );
        }
    }
}

fn increment(counts: &mut BTreeMap<String, u32>, name: &str) {
    *counts.entry(name.to_owned()).or_default() += 1;
}

pub(super) fn load_and_validate_fixture() -> Option<Value> {
    assert_eq!(digest(FIXTURE), FIXTURE_HASH);
    let fixture: Value = serde_json::from_slice(FIXTURE).unwrap();
    let original = Original::load()?;
    let m = &fixture["metadata"];
    let capture = &fixture["asset_capture"];
    provenance(m, &original);
    assets(capture);
    let cases = arr(&fixture["cases"]);
    assert_eq!(cases.len(), CASES.len());
    let mut events = BTreeMap::new();
    let mut entries = BTreeMap::new();
    let mut returns = BTreeMap::new();
    let mut outcomes = BTreeMap::new();
    let mut stops = BTreeMap::new();
    let mut requests = 0;
    let mut reads = 0;
    let mut writes = 0;
    let mut original_hist = 0;
    let mut original_steps = 0;
    let mut semantic_hist = 0;
    let mut semantic_steps = 0;
    let mut diagnostic_hist = 0;
    let mut diagnostic_steps = 0;
    let mut complete = 0;
    let mut prefix = 0;
    let mut diagnostic_complete = 0;
    for (case, expected_name) in cases.iter().zip(CASES) {
        let input = &case["input"];
        assert_eq!(input["name"], expected_name);
        assert_eq!(case["evidence"], input["evidence"]);
        assert_eq!(arr(&input["steps"]).len(), arr(&case["steps"]).len());
        let evidence = txt(&case["evidence"]);
        match evidence {
            "original_body" => {
                assert!(
                    !expected_name.starts_with("prefix-")
                        && !expected_name.starts_with("diagnostic-")
                );
                original_hist += 1;
                original_steps += arr(&case["steps"]).len();
            }
            "semantic_prefix" => {
                assert!(expected_name.starts_with("prefix-"));
                semantic_hist += 1;
                semantic_steps += arr(&case["steps"]).len();
            }
            "source_diagnostic" => {
                assert!(expected_name.starts_with("diagnostic-"));
                diagnostic_hist += 1;
                diagnostic_steps += arr(&case["steps"]).len();
            }
            _ => panic!("unadmitted evidence label"),
        }
        let mut storage = initial(input, &original, capture);
        snapshot(&case["initial_state"], &storage);
        assert!(stack_mask(&case["initial_state"]["stack"])
            .iter()
            .all(Option::is_none));
        for (request, receipt) in arr(&input["steps"]).iter().zip(arr(&case["steps"])) {
            requests += 1;
            Protocol {
                events: arr(&receipt["events"]),
                cursor: 0,
                frames: Vec::new(),
                receipt,
                storage: &mut storage,
                capture,
                original: &original,
            }
            .audit(request);
            snapshot(&receipt["state"], &storage);
            let done = receipt["native_completed"].as_bool().unwrap();
            let outcome = if evidence == "source_diagnostic" {
                diagnostic_complete += usize::from(done);
                "source_diagnostic"
            } else if done {
                complete += 1;
                "original_body_complete"
            } else {
                prefix += 1;
                "semantic_prefix_stop"
            };
            assert_eq!(receipt["outcome"], outcome);
            increment(&mut outcomes, outcome);
            if let Some(reason) = receipt["stop"].as_str() {
                increment(&mut stops, reason);
            }
            let r = n(&receipt["validated_stack_accesses"]["reads"]);
            let w = n(&receipt["validated_stack_accesses"]["writes"]);
            assert!(r > 0 || receipt["stop"] == "unknown_network");
            reads += r;
            writes += w;
            // Caller carriers remain backed; all other sparse stack knowledge
            // comes from online-validated original writers, not fabricated replay.
            let stack = stack_mask(&receipt["state"]["stack"]);
            assert_eq!(&stack[0x1000..0x1004], word(Some(RETURN)));
            assert_eq!(&stack[0x1004..0x1008], word(Some(n(&request["raw_id"]))));
            if request["operation"] == "set" {
                assert_eq!(&stack[0x1008..0x100c], word(Some(n(&request["silent"]))));
            }
            for event in arr(&receipt["events"]) {
                let kind = txt(&event["kind"]);
                increment(&mut events, kind);
                if kind == "routine_entry" {
                    increment(&mut entries, txt(&event["function"]));
                }
                if kind == "routine_return" {
                    increment(&mut returns, txt(&event["function"]));
                }
            }
        }
        assert_eq!(case["instruction_bytes_unchanged"], true);
    }
    assert_eq!(
        (requests, complete, prefix, diagnostic_complete),
        (62, 49, 5, 4)
    );
    assert_eq!(
        (
            original_hist,
            original_steps,
            semantic_hist,
            semantic_steps,
            diagnostic_hist,
            diagnostic_steps
        ),
        (12, 45, 5, 9, 8, 8)
    );
    assert_eq!((reads, writes), (490, 446));
    assert_eq!(
        m["counts"],
        json!({"histories":25,"invocations":62,"routine_entries":130,"routine_returns":118})
    );
    assert_eq!(
        m["admission_counts"],
        json!({"original_histories":original_hist,"original_history_steps":original_steps,"semantic_histories":semantic_hist,"semantic_history_steps":semantic_steps,"diagnostic_histories":diagnostic_hist,"diagnostic_steps":diagnostic_steps,"portable_histories":original_hist+semantic_hist,"portable_steps":original_steps+semantic_steps,"portable_completed_steps":complete,"portable_prefix_steps":prefix,"diagnostic_completed_steps":diagnostic_complete})
    );
    for (key, actual, expected) in [
        (
            "event_counts",
            events,
            json!({"argument_read":138,"captured_source":17,"raw_boundary":1,"read":263,"required_boundary":8,"routine_entry":130,"routine_return":118,"supplied_effect":3,"support_write":31,"write":36}),
        ),
        (
            "body_entry_counts",
            entries,
            json!({"h0_miss":8,"message":17,"query":59,"set":46}),
        ),
        (
            "body_return_counts",
            returns,
            json!({"h0_miss":8,"message":14,"query":57,"set":39}),
        ),
        (
            "outcome_counts",
            outcomes,
            json!({"original_body_complete":49,"semantic_prefix_stop":5,"source_diagnostic":8}),
        ),
        (
            "stop_counts",
            stops,
            json!({"enabled_message":2,"unknown_command_table":1,"unknown_h0":1,"unknown_network":1,"unknown_second":2,"unknown_slots":1,"unrepresented_raw_read":1}),
        ),
    ] {
        assert_eq!(json!(actual), expected);
        assert_eq!(m[key], expected);
    }
    assert_eq!(
        m["online_stack_access_counts"],
        json!({"reads":reads,"writes":writes})
    );
    Some(fixture)
}

#[test]
fn player_commands_original_byte_protocol_and_current_asset_backing() {
    load_and_validate_fixture();
}
