//! Original message-gate evidence and actual live-owner result/state parity.
//!
//! Native backing is supplied masked player/list storage, not constructor proof.
//! Its exact SEH/stack/RET protocol is audited independently of Rust operations.
//! Native raw text pointers, incidental EAX and single-instruction writer stops
//! have no portable pointer/register/whole-caller parity contract. Rust uses
//! genuine player constructors, declared complete initial suppression bytes and
//! the public message request. No observer measures Rust call/read cadence.

use std::{collections::BTreeMap, path::Path};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{PlayerMessage88, PlayerMessageError, PlayerMessageOperation, PlayerMessageRequest};
use crate::object_runtime::{
    ActorHandle, ActorRegistryConfig, ObjectArena, ObjectClassKind, PlayerId,
};

const EXE_HASH: &str = "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956";
const TEXT_HASH: &str = "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2";
const BODY_HASH: &str = "7cee0612a13346c8b5ff7587c68c5a69f976b30cdcc755d11987c6695680a3d9";
const ENTRY: u32 = 0x0041_35d0;
const FRONTIER: u32 = 0x0041_35f7;
const RET: u32 = 0x0041_37c0;
const STACK: u32 = 0x0073_0000;
const RETURN: u32 = 0x00f0_0000;
const UNMAPPED: u32 = 0xdead_1000;
const GAME: u32 = 0x0074_9000;
const HEAD: u32 = GAME + 0x60;
const TAIL: u32 = GAME + 0x6c;
const PLAYER_NAMES: [&str; 2] = ["first", "second"];
const PLAYER_ADDRESSES: [u32; 2] = [0x0074_0000, 0x0074_1000];
const DIAGNOSTICS: [&str; 2] = [
    "diagnostic-raw-unmapped-message",
    "diagnostic-unknown-suppression-byte",
];
const HISTORIES: [&str; 10] = [
    "suppressed-one-unmapped-text",
    "suppressed-high-byte-null-text",
    "suppressed-ff-high-flag",
    "selected-second-first-enabled",
    "repeated-same-message-live-carrier",
    "live-suppress-send-enable-frontier",
    "two-independent-message-writers",
    "enabled-frontier-before-known-replay-byte",
    "diagnostic-unknown-suppression-byte",
    "diagnostic-raw-unmapped-message",
];

const CODE_GUARDS: [(u32, &str, &str); 10] = [
    (
        0x4135d0,
        "64a100000000",
        "be63273f1e503bf064cc49dada11214d645270055abbffdfc6f1e5432700f98d",
    ),
    (
        0x4135d8,
        "6866c15100",
        "8059ccf20040b6d9844b4890caa389c5d8ac938abf2bb29b551e41e795befba5",
    ),
    (
        0x4135de,
        "64892500000000",
        "d461a4367e2ba7959de7159b91a2675d5e9e6e012d3750e42d4dd74bb417456f",
    ),
    (
        0x4135ee,
        "385d01",
        "806965cb84882a6a796a64aa32da3fe1ae16ed6936df431e7df1cc37b2753bf7",
    ),
    (
        0x4135f1,
        "0f85b9010000",
        "2450a74657a78e1498df461f528c5e4db3be685699d00e68a0761bd290a7ace6",
    ),
    (
        0x4135f7,
        "385d3d",
        "a0a87871b4633012aaa420474064e343ef5ac29864aa10a617ab77dceb29fc73",
    ),
    (
        0x4137b0,
        "8b4c2430",
        "a6a303ce49b1abec2c67a9d3708128afea6abf4e318bf3dccef4ee5aff3ab22b",
    ),
    (
        0x4137b6,
        "64890d00000000",
        "abfc2e5808656f2897610be3e6f32164afebce4617b7f868b5386720810fbfc5",
    ),
    (
        0x4137bd,
        "83c434",
        "870ec3fcea14a5143fee715dbae53cc4af99fd000801152e67ed2496a95f139c",
    ),
    (
        0x4137c0,
        "c20800",
        "1d10894625976bca9d8906f3ecf1e766b94e9219ec3bf1c30e02feea5e18e45a",
    ),
];
const EXECUTED_WRITERS: [(u32, &str, &str); 2] = [
    (
        0x40a947,
        "c6400101",
        "fcff70ba9f694bc9bb8f68619a048a5fa910846f6721fd11ede8f3a03df03cc5",
    ),
    (
        0x40aa79,
        "c6400100",
        "bb752fdc04283a02da66e906dbf3eb87a2b97a1daa3c785fbecb0a124e2206a8",
    ),
];
const STATIC_WRITERS: [(u32, &str, &str); 3] = [
    (
        0x413153,
        "c6470101",
        "e25d2225b7d5a568e73de421b94b5b2252713f918b747fcfb78953370e757380",
    ),
    (
        0x40631a,
        "c6878900000001",
        "48863a37a5c634633e44708d9f95f063e9125d0d466cce931fc274afdf91bbdd",
    ),
    (
        0x40632a,
        "c6878900000000",
        "f0db53e2c4b476d049856755c59d2b5d20200e5b7f264e9f0cf531525371a875",
    ),
];
const COUNTS: &str = r#"{
    "histories":10,"invocations":19,"admitted_histories":8,"admitted_invocations":17,
    "completed_send":9,"writer_fragment_completions":5,"bounded_enabled_prefixes":3,
    "diagnostic_histories":2,"diagnostic_invocations":2,"original_routine_entries":14,
    "authentic_ret8":9,
    "outcomes":{"original_body":9,"original_writer_fragment":5,"semantic_prefix":3,"source_diagnostic":2},
    "events":{"original_routine_entry":14,"original_routine_return":9,
    "original_writer_fragment_complete":5,"original_writer_fragment_entry":5,
    "payload_read":13,"payload_write":5,"required_enabled_boundary":3,
    "runtime_support_read":50,"runtime_support_write":93,"source_diagnostic_boundary":1,
    "supplied_invocation_carrier":19}
}"#;

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn bytes(value: &Value) -> Vec<Option<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|byte| (!byte.is_null()).then(|| number(byte).try_into().unwrap()))
        .collect()
}

fn word(value: u32) -> Vec<Option<u8>> {
    value.to_le_bytes().into_iter().map(Some).collect()
}

fn hex_bytes(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    (0..value.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&value[offset..offset + 2], 16).unwrap())
        .collect()
}

fn player_address(name: &str) -> u32 {
    PLAYER_ADDRESSES[PLAYER_NAMES
        .iter()
        .position(|&candidate| candidate == name)
        .unwrap()]
}

fn writer(operation: &str) -> (u32, u8) {
    match operation {
        "suppress" => (0x40a947, 1),
        "enable" => (0x40aa79, 0),
        _ => panic!("not an original writer operation {operation}"),
    }
}

/// Fixed supported .text backing; FrontendExecutable::mapped is data-only.
struct OriginalText(Vec<u8>);

impl OriginalText {
    fn load() -> Option<Self> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe");
        let Ok(raw) = std::fs::read(path) else {
            eprintln!("retail Infest.exe missing; copy the game into retail/ to run this test");
            return None;
        };
        inf_formats::FrontendExecutable::parse(&raw).unwrap();
        let dword = |offset| u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap());
        assert_eq!(dword(0x3c), 0xe8);
        assert_eq!(&raw[0xe8..0xec], b"PE\0\0");
        assert_eq!(&raw[0xee..0xf0], &6u16.to_le_bytes());
        assert_eq!(&raw[0xfc..0xfe], &0xe0u16.to_le_bytes());
        assert_eq!(dword(0x11c), 0x400000);
        assert_eq!(&raw[0x1e0..0x1e8], b".text\0\0\0");
        assert_eq!(
            (dword(0x1e8), dword(0x1ec), dword(0x1f0), dword(0x1f4)),
            (0x11d475, 0x1000, 0x11e000, 0x1000)
        );
        Some(Self(raw[0x1000..0x11f000].to_vec()))
    }

    fn at(&self, address: u32, count: usize) -> &[u8] {
        let offset = usize::try_from(address.checked_sub(0x401000).unwrap()).unwrap();
        &self.0[offset..offset + count]
    }

    fn guard(&self, receipt: &Value, address: u32, code: &str, hash: &str) {
        let expected = hex_bytes(code);
        assert_eq!(receipt["address"], address);
        assert_eq!(receipt["bytes"], expected.len());
        assert_eq!(receipt["sha256"], hash);
        assert_eq!(self.at(address, expected.len()), expected);
    }

    fn metadata(&self, metadata: &Value) {
        assert_eq!(metadata["executable_sha256"], EXE_HASH);
        assert_eq!(metadata["text_sha256"], TEXT_HASH);
        assert_eq!(metadata["text_address"], 0x401000);
        assert_eq!(metadata["fpcw"], 0x027f);
        assert_eq!(
            metadata["execution"],
            "original_message_gate_and_single_instruction_writers"
        );
        let routine = &metadata["routine"];
        assert_eq!(routine["address"], ENTRY);
        assert_eq!(routine["bytes"], 499);
        assert_eq!(routine["sha256"], BODY_HASH);
        let digest: String = Sha256::digest(self.at(ENTRY, 499))
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(digest, BODY_HASH);
        for (key, guards) in [
            ("instruction_guards", CODE_GUARDS.as_slice()),
            ("executed_writer_fragments", EXECUTED_WRITERS.as_slice()),
            ("static_only_writers", STATIC_WRITERS.as_slice()),
        ] {
            let recorded = metadata[key].as_array().unwrap();
            assert_eq!(recorded.len(), guards.len());
            for (receipt, &(address, code, hash)) in recorded.iter().zip(guards) {
                self.guard(receipt, address, code, hash);
            }
        }
        for (receipt, operation) in metadata["executed_writer_fragments"]
            .as_array()
            .unwrap()
            .iter()
            .zip(["suppress", "enable"])
        {
            assert_eq!(receipt["operation"], operation);
            assert_eq!(number(&receipt["byte"]), u32::from(writer(operation).1));
        }
        assert_eq!(
            metadata["counts"],
            serde_json::from_str::<Value>(COUNTS).unwrap()
        );
        assert_eq!(metadata["diagnostics"], json!(DIAGNOSTICS));
        assert_eq!(
            metadata["enabled_frontier"],
            json!({
                "ip":FRONTIER,"offset":0x3d,
                "scope":"required_enabled_message_before_known_or_unknown_storage_read_no_fault_or_missing_field_claim"
            })
        );
        assert_eq!(
            metadata["abi"],
            json!({
                "send_arguments":["unconsumed_text_pointer","unconsumed_full_flag"],
                "pop_bytes":8,"stack_delta":12,"return_contract":"void_incidental_EAX_retained_native_only",
                "writer":"one original instruction then explicit next-instruction stop; no RET claim"
            })
        );
        assert_eq!(metadata["text_operand_bound"], "native NULL/unmapped raw operands remain unread within suppression/frontier scope; portable absent byte backing is not native pointer identity or bad-pointer parity");
        assert_eq!(
            metadata["scope_exclusions"],
            json!([
                "player_or_message_constructor_execution",
                "enclosing_screen_or_pickup_writer_execution",
                "native_PlayerId_or_arena_number_parity",
                "enabled_message_storage_or_UI",
                "resource_loading",
                "CRT_format_execution",
                "grant_setter_or_merge",
                "production_attachment",
                "foreign_arena_or_missing_owner_native_parity",
                "hardware_fault_or_raw_pointer_alias_parity",
                "inherited_particle_pool_preparation"
            ])
        );
        assert_eq!(
            metadata["native_support"],
            json!({
                "players":{"first":PLAYER_ADDRESSES[0],"second":PLAYER_ADDRESSES[1]},
                "message_offset":0x88,"suppression_offset":0x89,
                "payload_regions":{"first":{"address":PLAYER_ADDRESSES[0],"bytes":0x1a0},
                    "second":{"address":PLAYER_ADDRESSES[1],"bytes":0x1a0},
                    "game":{"address":GAME,"bytes":0x78},"game_pointer":{"address":0x55f160,"bytes":4},
                    "process_null":{"address":0x54e894,"bytes":4},"network":{"address":0x55f448,"bytes":4}},
                "runtime_regions":{"seh_chain":{"address":0,"bytes":4},
                    "invocation_stack":{"address":STACK-0x40,"bytes":0x50}},
                "preparation":"supplied masked player/list/process backing; no constructor, screen, pickup or lookup execution; inherited Native preparation excluded from counts",
                "frame_policy":"explicit independent invocation reset before each entry; original stopped SEH writes retained in that receipt; completed RET restores supplied FS:0 chain",
                "seh_carrier":"flat supplied FS:0 chain and original51C166 handler address on native stack; no exception, handler, Windows TIB or dispatcher execution claimed",
                "unknown_bytes":"available raw bytes use poisonD3 only in CPU; every saved unknown mask remains None and stops before mandatory numeric consumption"
            })
        );
    }
}

/// Supplied native storage, independent of actual portable constructor defaults.
struct NativeBacking {
    payload: BTreeMap<String, Vec<Option<u8>>>,
    support: BTreeMap<String, Vec<Option<u8>>>,
    player_order: Value,
}

impl NativeBacking {
    fn new(input: &Value) -> Self {
        assert_eq!(input["player_order"], json!(PLAYER_NAMES));
        let mut payload: BTreeMap<_, _> = [
            ("first", 0x1a0),
            ("second", 0x1a0),
            ("game", 0x78),
            ("game_pointer", 4),
            ("process_null", 4),
            ("network", 4),
        ]
        .into_iter()
        .map(|(name, count)| (name.to_owned(), vec![None; count]))
        .collect();
        payload.insert("game_pointer".into(), word(GAME));
        for name in ["process_null", "network"] {
            let supplied = bytes(&input["sentinels"][name]);
            assert_eq!(supplied.len(), 4);
            payload.insert(name.to_owned(), supplied);
        }
        let players = input["players"].as_array().unwrap();
        assert_eq!(players.len(), 2);
        for (player, name) in players.iter().zip(PLAYER_NAMES) {
            assert_eq!(player["identity"], name);
            payload.get_mut(name).unwrap()[0x89] = (!player["suppression_byte"].is_null())
                .then(|| number(&player["suppression_byte"]).try_into().unwrap());
        }
        let mut backing = Self {
            payload,
            support: BTreeMap::from([
                (
                    "seh_chain".into(),
                    word(number(&input["initial_seh_chain"])),
                ),
                ("invocation_stack".into(), vec![None; 0x50]),
            ]),
            player_order: input["player_order"].clone(),
        };
        let chain = [
            HEAD,
            PLAYER_ADDRESSES[0] + 0xc8,
            PLAYER_ADDRESSES[1] + 0xc8,
            TAIL,
        ];
        for (index, &pointer) in chain.iter().enumerate() {
            let (region, offset) = if index == 0 || index == chain.len() - 1 {
                ("game", usize::try_from(pointer - GAME).unwrap())
            } else {
                (PLAYER_NAMES[index - 1], 0xc8)
            };
            let mut node = word(chain.get(index + 1).copied().unwrap_or(0));
            node.extend(word(
                index
                    .checked_sub(1)
                    .map(|previous| chain[previous])
                    .unwrap_or(0),
            ));
            node.extend(word(if index == 0 || index == chain.len() - 1 {
                0
            } else {
                PLAYER_ADDRESSES[index - 1]
            }));
            backing.store(false, region, offset, &node);
        }
        let mut actor_head = word(GAME + 0x54);
        actor_head.extend(word(0));
        actor_head.extend(word(0));
        backing.store(false, "game", 0x48, &actor_head);
        let mut actor_tail = word(0);
        actor_tail.extend(word(GAME + 0x48));
        actor_tail.extend(word(0));
        backing.store(false, "game", 0x54, &actor_tail);
        let patches = input["patches"].as_array().unwrap();
        if !patches.is_empty() {
            assert_eq!(input["name"], "enabled-frontier-before-known-replay-byte");
            assert_eq!(
                input["patches"],
                json!([{"region":"first","offset":0xc5,"bytes":[1]}])
            );
            backing.store(false, "first", 0xc5, &[Some(1)]);
        }
        backing
    }

    fn store(&mut self, support: bool, region: &str, offset: usize, values: &[Option<u8>]) {
        let regions = if support {
            &mut self.support
        } else {
            &mut self.payload
        };
        regions.get_mut(region).unwrap()[offset..offset + values.len()].copy_from_slice(values);
    }

    fn state(&self, recorded: &Value) {
        assert_eq!(
            recorded,
            &json!({
                "player_order":self.player_order,"payload":self.payload,"runtime_support":self.support
            })
        );
    }

    fn invocation(&mut self, request: &Value) {
        self.support.insert(
            "seh_chain".into(),
            word(number(&request["invocation_carrier"]["seh_chain"])),
        );
        self.support
            .insert("invocation_stack".into(), vec![None; 0x50]);
        let mut arguments = word(RETURN);
        arguments.extend(word(number(&request["text_pointer"])));
        arguments.extend(word(number(&request["flag"])));
        self.store(true, "invocation_stack", 0x40, &arguments);
    }
}

struct Events<'a> {
    values: &'a [Value],
    next: usize,
}

impl<'a> Events<'a> {
    fn new(value: &'a Value) -> Self {
        Self {
            values: value.as_array().unwrap(),
            next: 0,
        }
    }

    fn exact(&mut self, expected: Value) {
        assert_eq!(
            self.values.get(self.next),
            Some(&expected),
            "native event {}",
            self.next
        );
        self.next += 1;
    }

    fn memory(&mut self, kind: &str, region: &str, offset: usize, values: &[Option<u8>], ip: u32) {
        self.exact(json!({"kind":kind,"region":region,"offset":offset,"bytes":values.len(),
            "data":values,"known_mask":values.iter().map(Option::is_some).collect::<Vec<_>>(),"ip":ip}));
    }

    fn complete(&self) {
        assert_eq!(
            self.next,
            self.values.len(),
            "no reordered, duplicate or unadmitted native events"
        );
    }
}

#[derive(Default)]
struct NativeTotals {
    invocations: usize,
    outcomes: BTreeMap<String, usize>,
    events: BTreeMap<String, usize>,
}

impl NativeTotals {
    fn audit(&mut self, backing: &mut NativeBacking, request: &Value, receipt: &Value) {
        self.invocations += 1;
        for event in receipt["events"].as_array().unwrap() {
            *self
                .events
                .entry(event["kind"].as_str().unwrap().to_owned())
                .or_default() += 1;
        }
        let recorded_kind = receipt["outcome"]["kind"].as_str().unwrap();
        *self.outcomes.entry(recorded_kind.to_owned()).or_default() += 1;
        assert!(
            receipt["invalid"].is_null(),
            "no native hardware-fault claim"
        );
        backing.invocation(request);
        let carrier = &request["invocation_carrier"];
        let operation = request["operation"].as_str().unwrap();
        let recipient = request["recipient"].as_str().unwrap();
        let receiver = if PLAYER_NAMES.contains(&recipient) {
            player_address(recipient) + 0x88
        } else {
            assert_eq!(recipient, "raw_unmapped");
            UNMAPPED
        };
        let mut events = Events::new(&receipt["events"]);
        events.exact(
            json!({"kind":"supplied_invocation_carrier","carrier":carrier,
            "stack_policy":"independent_request_unknown_frame_except_return_and_two_arguments",
            "scope":"explicit_setup_before_native_entry_not_prefix_rollback"}),
        );
        let expected_kind = if operation != "send" {
            let (address, byte) = writer(operation);
            events.exact(
                json!({"kind":"original_writer_fragment_entry","address":address,
                "message":recipient,"next_instruction":address+4,
                "execution":"unchanged_single_store_instruction_not_enclosing_body"}),
            );
            events.memory("payload_write", recipient, 0x89, &[Some(byte)], address);
            backing.store(false, recipient, 0x89, &[Some(byte)]);
            events.exact(
                json!({"kind":"original_writer_fragment_complete","address":address,
                "next_instruction":address+4,"stack_delta":0,
                "scope":"no_enclosing_screen_constructor_or_pickup_execution_or_RET"}),
            );
            assert_eq!(
                receipt["registers"],
                json!({"eax":receiver,"ecx":receiver,
                "ebx":carrier["ebx"],"ebp":carrier["ebp"],"esp":STACK,"eip":address+4})
            );
            assert!(receipt["stop_ip"].is_null());
            "original_writer_fragment"
        } else {
            events.exact(json!({"kind":"original_routine_entry","address":ENTRY,
                "message":recipient,"receiver":receiver,"arguments":[request["text_pointer"],request["flag"]],
                "entry_stack":STACK,"return_ip":RETURN,"execution":"unchanged_native_body"}));
            // These are original instructions, not a portable SEH implementation.
            let chain = number(&carrier["seh_chain"]);
            events.memory("runtime_support_read", "seh_chain", 0, &word(chain), ENTRY);
            for (region, offset, value, ip) in [
                ("invocation_stack", 0x3c, u32::MAX, 0x4135d6),
                ("invocation_stack", 0x38, 0x51c166, 0x4135d8),
                ("invocation_stack", 0x34, chain, 0x4135dd),
                ("seh_chain", 0, STACK - 12, 0x4135de),
                ("invocation_stack", 8, number(&carrier["ebx"]), 0x4135e8),
                ("invocation_stack", 4, number(&carrier["ebp"]), 0x4135e9),
            ] {
                events.memory("runtime_support_write", region, offset, &word(value), ip);
                backing.store(true, region, offset, &word(value));
            }
            let byte = if PLAYER_NAMES.contains(&recipient) {
                let value = backing.payload[recipient][0x89];
                events.memory("payload_read", recipient, 0x89, &[value], 0x4135ee);
                value
            } else {
                events.exact(json!({"kind":"source_diagnostic_boundary","address":UNMAPPED+1,
                    "bytes":1,"ip":0x4135ee,"scope":"preempted_unmapped_receiver_read_no_fault_claim"}));
                None
            };
            if byte.is_some_and(|byte| byte != 0) {
                for (region, offset, value, ip) in [
                    ("invocation_stack", 0x34, chain, 0x4137b0),
                    ("invocation_stack", 4, number(&carrier["ebp"]), 0x4137b4),
                    ("invocation_stack", 8, number(&carrier["ebx"]), 0x4137b5),
                ] {
                    events.memory("runtime_support_read", region, offset, &word(value), ip);
                }
                events.memory(
                    "runtime_support_write",
                    "seh_chain",
                    0,
                    &word(chain),
                    0x4137b6,
                );
                backing.store(true, "seh_chain", 0, &word(chain));
                events.memory(
                    "runtime_support_read",
                    "invocation_stack",
                    0x40,
                    &word(RETURN),
                    RET,
                );
                events.exact(json!({"kind":"original_routine_return","ip":RET,
                    "message":recipient,"pop_bytes":8,"entry_stack":STACK,"return_ip":RETURN,
                    "raw_eax":chain,"scope":"void_incidental_EAX_authentic_RET_checked_after_execution",
                    "stack_after":STACK+12}));
                assert_eq!(
                    receipt["registers"],
                    json!({"eax":chain,"ecx":chain,
                    "ebx":carrier["ebx"],"ebp":carrier["ebp"],"esp":STACK+12,"eip":RETURN})
                );
                assert!(receipt["stop_ip"].is_null());
                "original_body"
            } else {
                let (kind, stop, ip) = if byte == Some(0) {
                    events.exact(
                        json!({"kind":"required_enabled_boundary","message":recipient,
                        "offset":0x3d,"ip":FRONTIER,
                        "scope":"deliberate_stop_before_unowned_enabled_storage_consumer"}),
                    );
                    ("semantic_prefix", "required_enabled_message", FRONTIER)
                } else {
                    (
                        "source_diagnostic",
                        if recipient == "raw_unmapped" {
                            "raw_unmapped_message"
                        } else {
                            "unknown_suppression_byte"
                        },
                        0x4135ee,
                    )
                };
                assert_eq!(receipt["outcome"]["stop"], stop);
                assert_eq!(receipt["stop_ip"], ip);
                assert_eq!(
                    receipt["registers"],
                    json!({"eax":chain,"ecx":receiver,
                    "ebx":0,"ebp":receiver,"esp":STACK-0x3c,"eip":ip})
                );
                kind
            }
        };
        assert_eq!(recorded_kind, expected_kind);
        if matches!(expected_kind, "original_body" | "original_writer_fragment") {
            assert!(receipt["outcome"]["stop"].is_null());
        }
        assert_eq!(
            receipt["return_stack_delta"],
            if expected_kind == "original_body" {
                json!(12)
            } else {
                Value::Null
            }
        );
        events.complete();
        backing.state(&receipt["state"]);
    }

    fn finish(&self, counts: &Value) {
        assert_eq!(self.invocations, 19);
        assert_eq!(
            serde_json::to_value(&self.events).unwrap(),
            counts["events"]
        );
        assert_eq!(
            serde_json::to_value(&self.outcomes).unwrap(),
            counts["outcomes"]
        );
    }
}

// Complete actual arena custody other than player records, which are compared
// as whole typed records with only the literal requested post-call byte changed.
fn other_owners(arena: &ObjectArena) -> String {
    format!(
        "{:?}{:?}{:?}",
        (
            &arena.slots,
            &arena.free,
            arena.live_count,
            &arena.lists,
            &arena.list_owners,
            &arena.routes,
            &arena.actors,
            &arena.map_annotations
        ),
        (
            &arena.actor_handles,
            &arena.actor_payloads,
            &arena.constructor_allocations,
            &arena.selector60_rows,
            arena.world_load
        ),
        (
            arena.identity,
            arena.players.next_id,
            arena.players.camera_74
        )
    )
}

struct ActualOwners {
    arena: ObjectArena,
    players: BTreeMap<String, PlayerId>,
    messages: BTreeMap<String, PlayerMessage88>,
}

impl ActualOwners {
    fn new(input: &Value) -> Self {
        // These are deliberately independent portable construction sentinels,
        // not known native grant/handle/actor fields or native constructor proof.
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0x7163_4925),
        })
        .unwrap();
        let base = arena.allocate(ObjectClassKind::Base);
        arena.set_flag0(base, true).unwrap();
        arena.allocate(ObjectClassKind::VisualBase);
        let mut players = BTreeMap::new();
        let mut messages = BTreeMap::new();
        for (index, record) in input["players"].as_array().unwrap().iter().enumerate() {
            let name = record["identity"].as_str().unwrap();
            assert_eq!(name, PLAYER_NAMES[index]);
            let seed = if index == 0 { 0x81e6_2495 } else { 0xd783_6b14 };
            let id = arena.add_world_player([seed; 6]);
            assert_eq!(arena.player_message(id).unwrap().suppression_byte(), 1);
            let token = arena.player_message(id).unwrap().identity();
            assert_eq!(token.arena, arena.identity);
            assert_eq!(token.player, id);
            // Declared complete source state, before the first actual operation.
            // No expected native post-call writes enter actual live owners.
            arena
                .world_player_mut(id)
                .unwrap()
                .message_88
                .suppression_01 = number(&record["suppression_byte"]).try_into().unwrap();
            assert!(players.insert(name.to_owned(), id).is_none());
            assert!(messages.insert(name.to_owned(), token).is_none());
        }
        assert_eq!(input["player_order"], json!(PLAYER_NAMES));
        Self {
            arena,
            players,
            messages,
        }
    }

    fn compare_state(&self, state: &Value) {
        let order: Vec<_> = self
            .arena
            .world_players()
            .iter()
            .map(|player| {
                self.players
                    .iter()
                    .find(|(_, id)| **id == player.id)
                    .unwrap()
                    .0
                    .as_str()
            })
            .collect();
        assert_eq!(json!(order), state["player_order"]);
        for name in PLAYER_NAMES {
            let id = self.players[name];
            let message = self.arena.player_message(id).unwrap();
            assert_eq!(message.identity(), self.messages[name]);
            assert_eq!(state["payload"][name][0x89], message.suppression_byte());
        }
        assert_eq!(self.arena.first_player(), Some(self.players["first"]));
    }

    fn invoke(&mut self, request: &Value, receipt: &Value) -> &'static str {
        let name = request["recipient"].as_str().unwrap();
        let recipient = self.messages[name];
        let operation = match request["operation"].as_str().unwrap() {
            "suppress" => PlayerMessageOperation::Suppress,
            "enable" => PlayerMessageOperation::Enable,
            "send" => PlayerMessageOperation::Send {
                // All admitted native raw pointers are unread in this scope.
                // None is absent portable byte backing, not a native NULL map.
                text: None,
                flag: number(&request["flag"]),
            },
            unknown => panic!("unowned operation {unknown}"),
        };
        let before = self.arena.world_players().to_vec();
        let other_before = other_owners(&self.arena);
        let actual = self.arena.apply_player_message(PlayerMessageRequest {
            recipient,
            operation,
        });
        let mut expected_players = before;
        match operation {
            PlayerMessageOperation::CleanupWorld => {
                unreachable!("this historical message fixture parser excludes world cleanup")
            }
            PlayerMessageOperation::Suppress | PlayerMessageOperation::Enable => {
                let byte = match operation {
                    PlayerMessageOperation::Suppress => 1,
                    _ => 0,
                };
                assert_eq!(
                    actual,
                    Ok(()),
                    "original store has a void portable contract"
                );
                // Expected clone is changed after the actual call, using only
                // the literal requested store rather than native output data.
                expected_players
                    .iter_mut()
                    .find(|player| player.id == self.players[name])
                    .unwrap()
                    .message_88
                    .suppression_01 = byte;
                assert_eq!(receipt["outcome"]["kind"], "original_writer_fragment");
            }
            PlayerMessageOperation::Send { .. } => {
                let old_byte = expected_players
                    .iter()
                    .find(|player| player.id == self.players[name])
                    .unwrap()
                    .message_88
                    .suppression_01;
                if old_byte == 0 {
                    assert_eq!(
                        actual,
                        Err(PlayerMessageError::EnabledMessageBoundary(recipient))
                    );
                    assert_eq!(receipt["outcome"]["kind"], "semantic_prefix");
                } else {
                    assert_eq!(
                        actual,
                        Ok(()),
                        "incidental native EAX is not a Rust return value"
                    );
                    assert_eq!(receipt["outcome"]["kind"], "original_body");
                }
            }
        }
        assert_eq!(self.arena.world_players(), expected_players);
        assert_eq!(other_owners(&self.arena), other_before);
        self.compare_state(&receipt["state"]);
        match receipt["outcome"]["kind"].as_str().unwrap() {
            "original_body" => "send",
            "original_writer_fragment" => "writer",
            "semantic_prefix" => "prefix",
            _ => unreachable!("diagnostics never construct actual owners"),
        }
    }
}

#[test]
fn player_message_gate_matches_native_suppression_histories() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/player_message_gate_native.json"
    ))
    .unwrap();
    let Some(original) = OriginalText::load() else {
        return;
    };
    original.metadata(&fixture["metadata"]);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), HISTORIES.len());
    let mut totals = NativeTotals::default();
    let mut diagnostics = Vec::new();
    let (mut admitted, mut calls, mut sends, mut writers, mut prefixes) = (0, 0, 0, 0, 0);
    for (case, name) in cases.iter().zip(HISTORIES) {
        let input = &case["input"];
        assert_eq!(input["name"], name);
        assert_eq!(case["instruction_bytes_unchanged"], true);
        let diagnostic = DIAGNOSTICS.contains(&name);
        assert_eq!(
            case["evidence"],
            if diagnostic {
                "source_diagnostic"
            } else {
                "portable_kernel_and_bounded_frontier"
            }
        );
        let requests = input["steps"].as_array().unwrap();
        let receipts = case["steps"].as_array().unwrap();
        assert_eq!(requests.len(), receipts.len());
        let mut backing = NativeBacking::new(input);
        backing.state(&case["initial_state"]);
        for (request, receipt) in requests.iter().zip(receipts) {
            totals.audit(&mut backing, request, receipt);
        }
        if diagnostic {
            // Audit all raw masks/stopped prefixes, but never create typed
            // owners for unknown suppression or an unmapped raw receiver.
            diagnostics.push(name);
            continue;
        }
        admitted += 1;
        let mut actual = ActualOwners::new(input);
        actual.compare_state(&case["initial_state"]);
        for (request, receipt) in requests.iter().zip(receipts) {
            calls += 1;
            match actual.invoke(request, receipt) {
                "send" => sends += 1,
                "writer" => writers += 1,
                "prefix" => prefixes += 1,
                _ => unreachable!(),
            }
        }
    }
    diagnostics.sort_unstable();
    assert_eq!(diagnostics, DIAGNOSTICS);
    assert_eq!(
        (admitted, calls, sends, writers, prefixes),
        (8, 17, 9, 5, 3)
    );
    totals.finish(&fixture["metadata"]["counts"]);
}
