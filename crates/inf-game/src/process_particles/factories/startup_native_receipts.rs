//! Static original-PE startup custody, compared with the named Rust initializer.
//!
//! The independent checker authenticates the complete supported image, parses
//! every base relocation and retains finite original source windows. No native
//! body is executed by this fixture. Initial loader zero is not a constant read
//! fallback, a global immutability proof or complete game startup acceptance.
//! SessionState owns the live phase independently of this carrier.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::ParticleFactoryProcessState;

const STARTUP: ParticleFactoryProcessState = ParticleFactoryProcessState::native_startup();

// Exact original source guards; these assert provenance, not executed paths.
const CODE_GUARDS: [(&str, u32, u32, &str); 15] = [
    (
        "network-reset-prefix",
        0x493650,
        0x493669,
        "a8f21f8d1b9ec15b1ee70ab6987a62da2d4f23e9f4568f24488b58de7539ff2d",
    ),
    (
        "network-init-calls-reset",
        0x493f90,
        0x493f9f,
        "d9a8a93bbf1ce88d87dfc285d739e8c3dd766333d04ad7252652caa221e370f6",
    ),
    (
        "network-publish-one",
        0x493495,
        0x49349f,
        "b0ea70035064824b343b08606adf3647373ff0f22da1d8e503f9ae3d74fc32d9",
    ),
    (
        "settings-copy-leaf",
        0x463a90,
        0x463aa6,
        "d4e20b03e3fb4d908f06945f16744ff6883a8bf7ab219284a1f9f7bcb2829bde",
    ),
    (
        "settings-restore-copy-prefix",
        0x463c7b,
        0x463ca2,
        "3bfa44af085b10611f7000c88edcce498670b759b4af11847cd35647d344d24e",
    ),
    (
        "registry-config-window",
        0x462390,
        0x4623bb,
        "b6d13e6106998a66d0e5f27f2f60c4ac6350ab34b2b5c67bf4b19655a5fa0c80",
    ),
    (
        "allocator-pointer-leaf",
        0x495940,
        0x495965,
        "6d4d2fa7277300a40090fff122e16731083b1a683382345e5dc7aa07d7439958",
    ),
    (
        "null-value-fallback0",
        0x443990,
        0x4439a2,
        "c2dc3fd4d589ac716258550f571de0db287a14f2b8466d5dd30267f84de49b00",
    ),
    (
        "null-value-fallback1",
        0x4439ce,
        0x4439e3,
        "d69988784c788c07f5c338e01552d7126b327ca83b9e3c960f4fef9bd9f51f13",
    ),
    (
        "null-value-fallback2",
        0x443a1c,
        0x443a31,
        "0555692d94b2f87d995a199c9ed80e3389c7ef4f05ef764ce1855087140f526e",
    ),
    (
        "null-value-fallback3",
        0x443a6a,
        0x443a7f,
        "4d3dab4c01668aec23531b529224a93622611c3a86acb9cb06d507f93de692ce",
    ),
    (
        "null-value-to-actor16c",
        0x476500,
        0x47650d,
        "2f85b382ad1e700046073075e92ff20e14ccaf8d46b0a18d88bb90423fbd990e",
    ),
    (
        "session-owner-getter",
        0x4391b0,
        0x4391b6,
        "1214fde4371880e5b9ef3d887378d1907f5d0d34a321544615a7840419c65700",
    ),
    (
        "allocator-network-phase-prefix",
        0x464ed0,
        0x464eec,
        "21c620558b7ea77cdd535de316db11f0b79620aea4cdeca955c95555a5a43b20",
    ),
    (
        "resolver-cache-leaf",
        0x462190,
        0x46220f,
        "2b2aca8473232d0d8f7ca4c7d9f080a1cbadddda0c26b210f4a397e37cbcc207",
    ),
];

/// The player's own executable in `retail/`, if present; the guards hash it.
fn retail_exe() -> Option<Vec<u8>> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .ok()
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn assert_loader_custody(fixture: &Value) {
    let pe = &fixture["pe_profile"];
    assert_eq!(pe["machine"], 0x14c);
    assert_eq!(pe["optional_magic"], 0x10b);
    assert_eq!(pe["image_base"], 0x400000);
    assert_eq!(pe["image_size"], 0x27b000);
    assert_eq!(pe["section_count"], 6);
    assert_eq!(
        pe["header_guard"],
        json!({
            "file_offset": 0xe8, "bytes_count": 488,
            "sha256": "10031f2fe3d057eca09c7ae5cb9766191c38447a1a8dd46a0a9982fb567ffe85"
        })
    );
    let sections = pe["sections"].as_array().unwrap();
    let expected_sections = [
        (".text", 0x401000, 0x1000, 1168501, 1171456, 0x1000),
        (".rdata", 0x51f000, 0x11f000, 57392, 61440, 0x11f000),
        (".data", 0x52e000, 0x12e000, 0x132c38, 0x19000, 0x12e000),
        (".idata", 0x661000, 0x261000, 4164, 8192, 1339392),
        (".rsrc", 0x663000, 0x263000, 9768, 12288, 1347584),
        (".reloc", 0x666000, 0x266000, 82228, 86016, 1359872),
    ];
    assert_eq!(sections.len(), expected_sections.len());
    for (section, (name, address, rva, virtual_size, raw_size, file_offset)) in
        sections.iter().zip(expected_sections)
    {
        assert_eq!(
            section,
            &json!({
                "name": name, "virtual_address": address, "rva": rva,
                "virtual_size": virtual_size, "raw_size": raw_size, "file_offset": file_offset
            })
        );
    }

    assert_eq!(fixture["field_count"], 2);
    let fields = fixture["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 2);
    for (field, (name, address)) in fields.iter().zip([
        ("process_null_handle", 0x54e894_u32),
        ("network_flag", 0x55f448_u32),
    ]) {
        assert_eq!(field["name"], name);
        assert_eq!(field["address"], address);
        assert_eq!(field["width"], 4);
        assert_eq!(field["section"], ".data");
        assert_eq!(field["section_address"], 0x52e000);
        assert_eq!(field["raw_size"], 0x19000);
        assert_eq!(field["virtual_size"], 0x132c38);
        let relative = address - number(&field["section_address"]);
        assert_eq!(field["section_relative"], relative);
        assert_eq!(field["section_relative_end"], relative + 4);
        assert!(relative >= number(&field["raw_size"]));
        assert!(relative + 4 <= number(&field["virtual_size"]));
        assert_eq!(field["raw_backing_end"], 0x52e000 + 0x19000);
        assert_eq!(field["virtual_end"], 0x52e000 + 0x132c38);
        assert_eq!(field["provenance"], "loader_virtual_zero_tail");
        assert_eq!(field["initial_value"], 0);
        assert_eq!(field["overlapping_relocations"], json!([]));
    }

    let relocations = &fixture["relocation_directory"];
    assert_eq!(
        relocations,
        &json!({
            "address": 0x666000, "rva": 0x266000, "file_offset": 1359872,
            "bytes_count": 73836, "parsed_bytes": 73836, "block_count": 323,
            "sha256": "4f57621480944f5f4da2d781a1a024cdd73b8e27edd781e8083ab76a471fa593",
            "entry_type_counts": {"0": 179, "3": 35447},
            "supported_types": [
                {"kind": 0, "name": "ABSOLUTE", "write_width": 0},
                {"kind": 3, "name": "HIGHLOW", "write_width": 4}
            ]
        })
    );
    // Every block header and every two-byte entry is accounted for. Widths
    // are explicit; unowned relocation types cannot support a no-overlap claim.
    assert_eq!(323 * 8 + 2 * (179 + 35447), 73836);
}

fn assert_bounded_sources(fixture: &Value) {
    assert_eq!(fixture["code_guard_count"], 15);
    assert_eq!(fixture["code_guard_bytes"], 446);
    let guards = fixture["code_guards"].as_array().unwrap();
    assert_eq!(guards.len(), CODE_GUARDS.len());
    let exe = retail_exe();
    if exe.is_none() {
        eprintln!("retail Infest.exe missing; the source guards' bytes are not checked");
    }
    let mut total = 0;
    for (guard, (name, start, end, hash)) in guards.iter().zip(CODE_GUARDS) {
        assert_eq!(guard["name"], name);
        assert_eq!(guard["start"], start);
        assert_eq!(guard["end"], end);
        assert_eq!(guard["bytes_count"], end - start);
        assert_eq!(guard["file_offset"], start - 0x400000);
        assert_eq!(guard["sha256"], hash);
        if let Some(exe) = &exe {
            let at = (start - 0x400000) as usize;
            assert_eq!(
                digest(&exe[at..at + (end - start) as usize]),
                hash,
                "{name}"
            );
        }
        assert!(!guard["policy"].as_str().unwrap().is_empty());
        total += end - start;
    }
    assert_eq!(total, 446);
    assert_eq!(fixture["source_guard_count"], 1);
    let source = fixture["source_guards"].as_array().unwrap();
    assert_eq!(source.len(), 1);
    assert_eq!(
        source[0],
        json!({
            "name": "settings-source", "address": 0x538cd0, "end": 0x538d58,
            "file_offset": 1281232, "bytes_count": 136,
            "sha256": "6d9d09b985793c0f448c2036f95f1280fe642c2db4032ed35f05f468765e4122"
        })
    );
    if let Some(exe) = &exe {
        assert_eq!(
            digest(&exe[1281232..1281232 + 136]),
            "6d9d09b985793c0f448c2036f95f1280fe642c2db4032ed35f05f468765e4122"
        );
    }

    let intervals = &fixture["bounded_storage_intervals"];
    assert_eq!(
        intervals,
        &json!({
            "settings_copy": {"start": 0x54e8a8, "end": 0x54e930, "requires": "clearDF"},
            "registry_pointer_write": {"start": 0x54e888, "end": 0x54e88c},
            "registry_config_writes": [
                {"start": 0x54e88c, "end": 0x54e890},
                {"start": 0x54e890, "end": 0x54e894}
            ],
            "resolver_cache_writes": [
                {"start": 0x54e880, "end": 0x54e884},
                {"start": 0x54e884, "end": 0x54e888}
            ]
        })
    );
    for interval in std::iter::once(&intervals["settings_copy"])
        .chain(std::iter::once(&intervals["registry_pointer_write"]))
        .chain(intervals["registry_config_writes"].as_array().unwrap())
        .chain(intervals["resolver_cache_writes"].as_array().unwrap())
    {
        let start = number(&interval["start"]);
        let end = number(&interval["end"]);
        assert!(start < end);
        assert!(end <= 0x54e894 || start >= 0x54e898);
    }
    assert_eq!(fixture["phase_provenance"], "phaseNone is deliberately unattached; native offline allocator reads actual SessionState+64C through004391B0, not a BSS phase field");
}

#[test]
fn particle_process_native_startup_matches_original_pe_evidence() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/particle_process_startup_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(fixture["execution"], "static_original_bytes");
    assert_eq!(fixture["body_execution_count"], 0);
    assert_eq!(
        fixture["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(
        fixture["text_sha256"],
        "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2"
    );
    assert_eq!(
        fixture["scope"],
        "initial loader DWORD custody and fifteen bounded original source guards only"
    );
    assert_eq!(
        fixture["exclusions"],
        json!([
            "no native instruction/body/constructor execution",
            "no global immutability or exhaustive247-reference proof",
            "no arbitrary computed pointer aliases or DF1 bulk-copy admission",
            "no complete network/CRT/game startup acceptance",
            "no virtual-zero mapping added to file-backed authored-data API",
            "no production particle birth registry-null source migration"
        ])
    );
    assert_loader_custody(&fixture);
    assert_bounded_sources(&fixture);
    assert_eq!(
        fixture["expected_portable"],
        json!({
            "network_flag": 0, "process_null_handle": 0, "phase": null
        })
    );
    assert_eq!(
        json!({
            "network_flag": STARTUP.network_flag,
            "process_null_handle": STARTUP.process_null_handle.map(|handle| handle.raw()),
            "phase": crate::process_particles::factories::phase_fixture::FixtureSessionPhase::supplied(None).snapshot()
        }),
        fixture["expected_portable"]
    );
}
