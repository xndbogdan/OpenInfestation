//! Actual event22 result/state parity, with a separate original-byte timeline audit.
//!
//! Native actor/player/list backing is supplied, not constructor execution. The
//! portable side uses real owners followed by declared inputs. The sibling
//! ordinary tests cover private owner error laziness. No Rust query observer is
//! installed: native query cadence and Rust result/state proof remain distinct.

use std::collections::{BTreeMap, BTreeSet};

use inf_formats::{FrontendExecutable, GrantTable, GRANT_TABLE_ADDRESS};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorExtension, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorClass, DerivedActorConstructionRequest, Family428650Class, Family429e20Class,
    ObjectClass, ObjectState, Selector12GrantContext, Selector12State,
};

const DIAGNOSTICS: [&str; 6] = [
    "diagnostic-bypassed-unknown-target-mask",
    "diagnostic-direct-stale-this-no-player",
    "diagnostic-raw-query-lowword-and-six-word-boundary",
    "diagnostic-selector13-static-body",
    "diagnostic-unknown-reached-source-word",
    "diagnostic-unknown-reached-target-word",
];
const TABLES: [(&str, u32, u32); 20] = [
    ("derived", 0x520e58, 0x4270b0),
    ("family", 0x520f34, 0x4270b0),
    ("scalar_base", 0x52104c, 0x4270b0),
    ("probe_base", 0x521164, 0x4270b0),
    ("family4236", 0x520b84, 0x4270b0),
    ("selector10", 0x520cc8, 0x4270b0),
    ("selector34", 0x521eac, 0x4270b0),
    ("selector35", 0x52273c, 0x4270b0),
    ("selector36", 0x527ee0, 0x4270b0),
    ("selector11", 0x51f080, 0x402420),
    ("selector2", 0x520860, 0x420be0),
    ("selector60", 0x527de8, 0x480f40),
    ("selector77", 0x51fbb0, 0x4270b0),
    ("selector12", 0x527058, 0x468be0),
    ("selector13", 0x5269c4, 0x468be0),
    ("shared", 0x527c00, 0x4045f0),
    ("selector16", 0x5204d0, 0x4045f0),
    ("selector30", 0x5289c0, 0x48c470),
    ("selector88", 0x524ac0, 0x4045f0),
    ("selector90", 0x526b50, 0x4045f0),
];
const ACTOR: u32 = 0x740000;
const GAME: u32 = 0x749000;
const HEAD: u32 = GAME + 0x60;
const TAIL: u32 = GAME + 0x6c;
const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn bytes(value: &Value) -> Vec<Option<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|byte| byte.as_u64().map(|byte| byte.try_into().unwrap()))
        .collect()
}

fn mask(value: &Value) -> [u32; 6] {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(number)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

fn known_word(raw: &[Option<u8>]) -> Option<u32> {
    assert_eq!(raw.len(), 4);
    Some(u32::from_le_bytes(
        raw.iter()
            .copied()
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .unwrap(),
    ))
}

fn player_address(name: &str) -> u32 {
    match name {
        "first" => 0x748800,
        "second" => 0x748a00,
        _ => panic!("unadmitted player {name}"),
    }
}

/// Test-only fixed supported PE profile. FrontendExecutable::mapped is a data
/// view, so instruction guards use the actual file-backed .text independently.
fn original_text(raw: &[u8]) -> &[u8] {
    let word = |offset| u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap());
    assert_eq!(word(0x3c), 0xe8);
    assert_eq!(&raw[0xe8..0xec], b"PE\0\0");
    assert_eq!(u16::from_le_bytes(raw[0xee..0xf0].try_into().unwrap()), 6);
    assert_eq!(
        u16::from_le_bytes(raw[0xfc..0xfe].try_into().unwrap()),
        0xe0
    );
    assert_eq!(word(0x11c), 0x400000);
    assert_eq!(&raw[0x1e0..0x1e8], b".text\0\0\0");
    assert_eq!(
        [word(0x1e8), word(0x1ec), word(0x1f0), word(0x1f4)],
        [0x11d475, 0x1000, 0x11e000, 0x1000]
    );
    &raw[0x1000..0x11f000]
}

fn hex_bytes(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
        .collect()
}

fn validate_metadata(metadata: &Value, raw: &[u8], executable: &FrontendExecutable<'_>) {
    assert_eq!(
        metadata["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(
        metadata["text_sha256"],
        "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2"
    );
    assert_eq!(metadata["fpcw"], 0x027f);
    let text = original_text(raw);
    let code = |address: u32, length| {
        let offset = address.checked_sub(0x401000).unwrap() as usize;
        &text[offset..offset + length]
    };
    let bodies = [
        (
            0x468be0,
            0x468c3d,
            "0b3c47cbe75c73c98cedbf743ec3fb87127820941f7a961c371a0458bad79a59",
        ),
        (
            0x488aa0,
            0x488ab8,
            "9b594118f5712139882504b45ab023d976170685230e7d3358ba064a2981f1a5",
        ),
        (
            0x4753a0,
            0x4753dc,
            "86448e1f28705dd0b7d640d6adf719eacd20405350860a6aa3cb413e3964d3b4",
        ),
        (
            0x4752e0,
            0x47531a,
            "3c3c7175221a4064d194baeb7a48a0f34fd6ddb37c41db332b2cdef9cdf321c6",
        ),
    ];
    assert_eq!(metadata["body_guards"].as_array().unwrap().len(), 4);
    for (guard, (start, end, hash)) in metadata["body_guards"]
        .as_array()
        .unwrap()
        .iter()
        .zip(bodies)
    {
        assert_eq!(number(&guard["address"]), start);
        assert_eq!(number(&guard["end"]), end);
        assert_eq!(guard["sha256"], hash);
        let digest: String = Sha256::digest(code(start, (end - start) as usize))
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(digest, hash);
    }
    let instructions = [
        (0x468be0, "8b44240c"),
        (0x468bfa, "8d8e4c020000"),
        (0x468c01, "8d889c000000"),
        (0x468c10, "c6869d02000001"),
        (0x468c1d, "c6869d02000000"),
        (0x4752e6, "6685c0"),
        (0x4752f0, "25ffff0000"),
        (0x47530f, "85748204"),
        (0x4753c4, "81feaf000000"),
        (0x488aa0, "a160f15500"),
        (0x488aa5, "8b4060"),
        (0x488aa8, "833800"),
        (0x488ab4, "8b4008"),
    ];
    assert_eq!(
        metadata["instruction_guards"].as_array().unwrap().len(),
        instructions.len()
    );
    for (guard, (address, data)) in metadata["instruction_guards"]
        .as_array()
        .unwrap()
        .iter()
        .zip(instructions)
    {
        assert_eq!(number(&guard["address"]), address);
        assert_eq!(guard["bytes"], data);
        assert_eq!(code(address, data.len() / 2), hex_bytes(data));
    }
    assert_eq!(
        metadata["caller_probe"],
        json!({"start":0x47dbbb,"end":0x47dbc8,"bytes":"55508b116a0053ff9298000000","operation":"event22","forced_distance_squared":0,"completion":"before_next_fanout_instruction","scope":"original_fragment_not_full_fanout"})
    );
    assert_eq!(code(0x47dbbb, 13), hex_bytes("55508b116a0053ff9298000000"));
    assert_eq!(
        metadata["table98_matrix"].as_array().unwrap().len(),
        TABLES.len()
    );
    for (row, (name, table, callback)) in metadata["table98_matrix"]
        .as_array()
        .unwrap()
        .iter()
        .zip(TABLES)
    {
        assert_eq!(
            *row,
            json!({"name":name,"table":table,"slot":0x98,"callback":callback})
        );
        assert_eq!(
            executable.mapped(table + 0x98, 4).unwrap().bytes,
            callback.to_le_bytes()
        );
    }
    assert_eq!(
        metadata["abi"],
        json!({
            "event22":{"argument_count":4,"event_code_argument_index":2,"event_code":22,"ret_pop_bytes":16,"direct_stack_delta":20,"probe_stack_delta":0,"ignored_argument_indices":[0,1,3],"return_policy":"full_eax_zero"},
            "raw_query":{"argument_count":1,"ret_pop_bytes":4,"stack_delta":8,"return_policy":"low_al_only","scalar_id_policy":"unsigned_low_word"}
        })
    );
    assert_eq!(metadata["diagnostic_names"], json!(DIAGNOSTICS));
    assert_eq!(metadata["supplied_services"], json!([]));
    assert_eq!(
        metadata["counts"],
        json!({
            "histories":15,"steps":24,"original_histories":9,"original_steps":14,"diagnostic_histories":6,"diagnostic_steps":10,
            "routine_entries":{"4623328":19,"4754080":19,"4674464":17,"4674272":1627},
            "routine_returns":{"4754080":19,"4623328":17,"4674272":1624,"4674464":15},
            "events":{"caller_probe_entry":17,"read":1682,"executable_read":17,"routine_entry":55,"routine_return":51,"caller_probe_stop":15,"supplied_effect":4,"difference_entry":17,"query_entry":1627,"query_return":1624,"difference_return":15,"write":15},
            "outcomes":{"completed":21,"numeric_unknown_read_stop":3}
        })
    );
    assert_eq!(metadata["admission"]["rust_cadence"], "original native ordered query receipts; no Rust query observer or measured Rust call-cadence claim");
    assert_eq!(
        metadata["admission"]["native_read_stop"],
        "intentional pre-consumer stop at47530F; no native bounds-check or hardware-fault claim"
    );
    assert_eq!(
        metadata["scope_exclusions"],
        json!([
            "full47DAF0_fanout",
            "outer468220_capture",
            "production_attachment",
            "retail_acceptance",
            "actor_or_player_constructors",
            "selector13_portable_owner",
            "grant_setter4750C0",
            "merge475350",
            "notification_pointer_or_message_owner",
            "actor_registry_or_resolver",
            "network_process_null_rng_clock",
            "unknown_numeric_storage_portable_defaults",
            "raw_pointer_alias_or_oob_portable_admission"
        ])
    );
    assert_eq!(
        metadata["native_support"],
        json!({
            "regions":{"actor":{"address":ACTOR,"length":0x2a0},"first":{"address":0x748800,"length":0x1a0},"second":{"address":0x748a00,"length":0x1a0},"game":{"address":GAME,"length":0x78},"game_pointer":{"address":0x55f160,"length":4}},
            "source_owner_offset":0x24c,"source_words_offset":0x250,"target_owner_offset":0x9c,"target_words_offset":0xa0,"result_byte_offset":0x29d,"player_node_offset":0xc8,"head":HEAD,"tail":TAIL,
            "preparation":"supplied actor/player/list fields; no constructors; inherited Native unused particle preparation excluded from receipt counts"
        })
    );
}

/// Declared physical backing for mask/read audits only. It is not an arena or
/// a query implementation, and none of its expected output is fed into Rust.
struct Backing {
    regions: BTreeMap<String, Vec<Option<u8>>>,
    order: Vec<String>,
    actor_live: bool,
}

impl Backing {
    fn new(input: &Value) -> Self {
        let mut result = Self {
            regions: [
                ("actor", 0x2a0),
                ("first", 0x1a0),
                ("second", 0x1a0),
                ("game", 0x78),
                ("game_pointer", 4),
            ]
            .into_iter()
            .map(|(name, length)| (name.to_owned(), vec![None; length]))
            .collect(),
            order: Vec::new(),
            actor_live: input["actor"]["live"].as_bool().unwrap(),
        };
        let table = match input["actor"]["binding"].as_str().unwrap() {
            "selector12" => 0x527058,
            "selector13" => 0x5269c4,
            other => panic!("binding {other}"),
        };
        result.word("actor", 0, table);
        result.words("actor", 0x250, mask(&input["actor"]["grants6"]));
        result.regions.get_mut("actor").unwrap()[0x29d] =
            Some(number(&input["actor"]["byte29d"]).try_into().unwrap());
        result.word("game_pointer", 0, GAME);
        result.bind_list();
        for player in input["players"].as_array().unwrap() {
            result.append(player);
        }
        for patch in input["patches"].as_array().unwrap() {
            result.patch(patch);
        }
        result
    }

    fn word(&mut self, region: &str, offset: usize, word: u32) {
        self.regions.get_mut(region).unwrap()[offset..offset + 4]
            .copy_from_slice(&word.to_le_bytes().map(Some));
    }

    fn words(&mut self, region: &str, offset: usize, words: [u32; 6]) {
        for (index, word) in words.into_iter().enumerate() {
            self.word(region, offset + 4 * index, word);
        }
    }

    fn bind_list(&mut self) {
        let chain: Vec<_> = std::iter::once(HEAD)
            .chain(self.order.iter().map(|name| player_address(name) + 0xc8))
            .chain(std::iter::once(TAIL))
            .collect();
        for (index, &pointer) in chain.iter().enumerate() {
            let (name, offset, payload) = if pointer == HEAD || pointer == TAIL {
                ("game".to_owned(), (pointer - GAME) as usize, 0)
            } else {
                (self.order[index - 1].clone(), 0xc8, pointer - 0xc8)
            };
            self.word(&name, offset, chain.get(index + 1).copied().unwrap_or(0));
            self.word(
                &name,
                offset + 4,
                index.checked_sub(1).map(|index| chain[index]).unwrap_or(0),
            );
            self.word(&name, offset + 8, payload);
        }
    }

    fn append(&mut self, player: &Value) {
        let name = player["identity"].as_str().unwrap();
        player_address(name);
        assert!(!self.order.iter().any(|present| present == name));
        self.order.push(name.to_owned());
        self.words(name, 0xa0, mask(&player["grants6"]));
        self.bind_list();
    }

    fn patch(&mut self, patch: &Value) {
        let name = patch["region"].as_str().unwrap();
        let offset = number(&patch["offset"]) as usize;
        let data = bytes(&patch["bytes"]);
        self.regions.get_mut(name).unwrap()[offset..offset + data.len()].copy_from_slice(&data);
    }

    fn effects(&mut self, input: &Value) {
        for effect in input["setup_effects"].as_array().unwrap() {
            if effect["operation"] == "append_player" {
                self.append(&effect["player"]);
            } else {
                assert!(effect["operation"].is_null());
                self.patch(effect);
            }
        }
    }

    fn compare(&self, state: &Value) {
        assert_eq!(state["actor_live"], self.actor_live);
        assert_eq!(state["player_order"], json!(self.order));
        assert_eq!(state["storage"], json!(self.regions));
    }
}

#[derive(Default)]
struct Counts {
    events: BTreeMap<String, u32>,
    entries: BTreeMap<String, u32>,
    returns: BTreeMap<String, u32>,
    outcomes: BTreeMap<String, u32>,
}

fn bump(map: &mut BTreeMap<String, u32>, key: impl ToString) {
    *map.entry(key.to_string()).or_default() += 1;
}

/// Validate the original emitted timeline, rather than inventing a detached
/// expected grant scan. Query entries determine each reached word; authentic
/// returns determine whether a target or the next source may follow.
fn validate_native_step(
    input: &Value,
    receipt: &Value,
    backing: &mut Backing,
    counts: &mut Counts,
    executable: &FrontendExecutable<'_>,
) {
    backing.effects(input);
    let effects = input["setup_effects"].as_array().unwrap();
    let events = receipt["events"].as_array().unwrap();
    for (event, effect) in events.iter().zip(effects) {
        assert_eq!(event["kind"], "supplied_effect");
        assert_eq!(event["effect"], *effect);
        let execution = if effect["operation"] == "append_player" {
            "supplied_between_requests_owner_append_no_native_ctor"
        } else {
            "supplied_between_requests_field_write"
        };
        assert_eq!(event["execution"], execution);
    }
    let raw_query = input["operation"] == "raw_query";
    let probe = input["dispatch"] == "probe";
    let mut frames = Vec::new();
    let mut query: Option<(&Value, Option<bool>, usize)> = None;
    let mut expected_id = 0;
    let mut expected_owner = "source";
    let mut difference = None;
    let mut selected_player = None;
    let mut first_reads = 0;
    let mut difference_started = false;
    let mut callback_returned = false;
    let mut writes = 0;
    for (index, event) in events.iter().enumerate() {
        let kind = event["kind"].as_str().unwrap();
        bump(&mut counts.events, kind);
        match kind {
            "supplied_effect" => assert!(index < effects.len()),
            "caller_probe_entry" => {
                assert!(probe);
                assert_eq!(number(&event["address"]), 0x47dbbb);
                assert_eq!(event["execution"], "original_fragment");
            }
            "executable_read" => {
                assert!(probe);
                assert_eq!(number(&event["ip"]), 0x47dbc2);
                let data: Vec<u8> = bytes(&event["data"])
                    .into_iter()
                    .map(Option::unwrap)
                    .collect();
                assert_eq!(
                    data,
                    executable
                        .mapped(number(&event["address"]), number(&event["bytes"]) as usize)
                        .unwrap()
                        .bytes
                );
                assert_eq!(u32::from_le_bytes(data.try_into().unwrap()), 0x468be0);
            }
            "routine_entry" => {
                let body = number(&event["address"]);
                assert_eq!(event["execution"], "unchanged_native_body");
                bump(&mut counts.entries, body);
                match body {
                    0x468be0 => {
                        assert!(!raw_query);
                        assert!(frames.is_empty());
                        assert_eq!(
                            event["arguments"],
                            json!([
                                number(&input["args"]["source_pointer"]),
                                number(&input["args"]["distance_squared"]),
                                22,
                                number(&input["args"]["fourth_operand"])
                            ])
                        );
                        if backing.actor_live {
                            assert_eq!(event["receiver"], "actor");
                        } else {
                            assert_eq!(event["receiver"], json!({"native_address":0xdead1000_u32}));
                        }
                    }
                    0x488aa0 => {
                        assert_eq!(frames.last(), Some(&0x468be0));
                        assert_eq!(event["arguments"], json!([]));
                        first_reads = 0;
                    }
                    0x4753a0 => {
                        assert_eq!(frames.last(), Some(&0x468be0));
                        assert_eq!(event["receiver"], "first_target");
                        assert_eq!(event["arguments"], json!([ACTOR + 0x24c]));
                        assert_eq!(selected_player.as_deref(), Some("first"));
                    }
                    _ => panic!("unexpected body {body:08x}"),
                }
                frames.push(body);
            }
            "difference_entry" => {
                assert_eq!(frames.last(), Some(&0x4753a0));
                assert_eq!(event["source"], "source");
                assert_eq!(event["target"], "first_target");
                assert_eq!(number(&event["ip"]), 0x4753a0);
                difference_started = true;
            }
            "query_entry" => {
                assert!(query.is_none());
                assert_eq!(number(&event["address"]), 0x4752e0);
                assert_eq!(number(&event["ip"]), 0x4752e0);
                let id = number(&event["raw_id"]);
                assert_eq!(number(&event["low_word"]), id & 0xffff);
                if raw_query {
                    assert_eq!(id, number(&input["raw_query"]["raw_id"]));
                    assert_eq!(number(&event["return_ip"]), 0xf00000);
                    let owner = match input["raw_query"]["owner"].as_str().unwrap() {
                        "source" => json!("source"),
                        "null" => Value::Null,
                        "unmapped" => json!({"native_address":0xdead1000_u32}),
                        other => panic!("raw owner {other}"),
                    };
                    assert_eq!(event["owner"], owner);
                } else {
                    assert!(difference_started && difference.is_none());
                    assert_eq!(id, expected_id);
                    assert!(id < 175);
                    assert_eq!(event["owner"], expected_owner);
                    assert_eq!(
                        number(&event["return_ip"]),
                        if expected_owner == "source" {
                            0x4753b3
                        } else {
                            0x4753bf
                        }
                    );
                }
                bump(&mut counts.entries, 0x4752e0);
                query = Some((event, None, 0));
            }
            "read" => {
                let region = event["region"].as_str().unwrap();
                let offset = number(&event["offset"]) as usize;
                let data = bytes(&event["data"]);
                assert_eq!(data.len(), number(&event["bytes"]) as usize);
                assert_eq!(
                    event["known_mask"],
                    json!(data.iter().map(Option::is_some).collect::<Vec<_>>())
                );
                assert_eq!(data, backing.regions[region][offset..offset + data.len()]);
                let ip = number(&event["ip"]);
                if frames.last() == Some(&0x488aa0) {
                    assert_eq!(ip, [0x488aa0, 0x488aa5, 0x488aa8, 0x488ab4][first_reads]);
                    first_reads += 1;
                }
                match ip {
                    0x47530f => {
                        let (entry, result, reads) = query.as_mut().unwrap();
                        let id = number(&entry["low_word"]);
                        assert_ne!(id, 0);
                        assert_eq!(*reads, 0);
                        assert_eq!(data.len(), 4);
                        let (wanted_region, base) = match entry["owner"].as_str().unwrap() {
                            "source" => ("actor", 0x250),
                            "first_target" => ("first", 0xa0),
                            other => panic!("owner {other}"),
                        };
                        assert_eq!(region, wanted_region);
                        assert_eq!(offset, base + 4 * (id >> 5) as usize);
                        *result = known_word(&data).map(|word| word & (1 << (id & 31)) != 0);
                        *reads += 1;
                    }
                    0x47dbbd => {
                        assert!(probe);
                        assert_eq!((region, offset, data.len()), ("actor", 0, 4));
                    }
                    0x488aa0 => assert_eq!((region, offset, data.len()), ("game_pointer", 0, 4)),
                    0x488aa5 => assert_eq!((region, offset, data.len()), ("game", 0x60, 4)),
                    0x488aa8 => assert_eq!(
                        (region, offset, data.len()),
                        if backing.order.is_empty() {
                            ("game", 0x6c, 4)
                        } else {
                            ("first", 0xc8, 4)
                        }
                    ),
                    0x488ab4 => assert_eq!((region, offset, data.len()), ("first", 0xd0, 4)),
                    _ => panic!("unexpected read IP {ip:08x}"),
                }
            }
            "query_return" => {
                let (entry, read_result, reads) = query.take().unwrap();
                for key in ["owner", "raw_id", "low_word", "return_ip"] {
                    assert_eq!(event[key], entry[key]);
                }
                let id = number(&entry["low_word"]);
                let result = if id == 0 {
                    assert_eq!(reads, 0);
                    true
                } else {
                    assert_eq!(reads, 1);
                    read_result.unwrap()
                };
                assert_eq!(number(&event["low_al"]), u32::from(result));
                let raw_eax = if id == 0 {
                    (number(&entry["raw_id"]) & !255) | 1
                } else {
                    ((id >> 5) & !255) | u32::from(result)
                };
                assert_eq!(number(&event["raw_eax"]), raw_eax);
                assert_eq!(
                    number(&event["ip"]),
                    if id == 0 { 0x4752ed } else { 0x475317 }
                );
                assert_eq!(event["pop_bytes"], 4);
                assert_eq!(event["execution"], "authentic_ret_checked_at_next_boundary");
                bump(&mut counts.returns, 0x4752e0);
                if !raw_query {
                    if expected_owner == "source" && result {
                        expected_owner = "first_target";
                    } else if expected_owner == "first_target" && !result {
                        difference = Some(true);
                    } else {
                        expected_owner = "source";
                        expected_id += 1;
                    }
                }
            }
            "routine_return" => {
                let body = number(&event["body"]);
                assert_eq!(frames.pop(), Some(body));
                assert_eq!(event["execution"], "authentic_ret_checked_at_next_boundary");
                assert_eq!(number(&event["low_al"]), number(&event["raw_eax"]) & 255);
                let address = number(&event["address"]);
                match body {
                    0x488aa0 => {
                        assert_eq!(event["pop_bytes"], 0);
                        assert_eq!(number(&event["return_ip"]), 0x468bf6);
                        let first = backing.order.first().cloned();
                        assert_eq!(first_reads, if first.is_some() { 4 } else { 3 });
                        assert_eq!(
                            number(&event["raw_eax"]),
                            first.as_deref().map(player_address).unwrap_or(0)
                        );
                        assert_eq!(address, if first.is_some() { 0x488ab7 } else { 0x488ab3 });
                        selected_player = first;
                    }
                    0x4753a0 => {
                        assert!(query.is_none());
                        let result = difference.unwrap_or_else(|| {
                            assert_eq!(expected_id, 175);
                            false
                        });
                        assert_eq!(number(&event["low_al"]), u32::from(result));
                        assert_eq!(address, if result { 0x4753d9 } else { 0x4753d1 });
                        assert_eq!(event["pop_bytes"], 4);
                        assert_eq!(number(&event["return_ip"]), 0x468c0c);
                        difference = Some(result);
                    }
                    0x468be0 => {
                        assert_eq!(event["raw_eax"], 0);
                        assert_eq!(event["pop_bytes"], 16);
                        assert_eq!(
                            number(&event["return_ip"]),
                            if probe { 0x47dbc8 } else { 0xf00000 }
                        );
                        assert_eq!(
                            address,
                            match difference {
                                Some(true) => 0x468c1a,
                                Some(false) => 0x468c27,
                                None => 0x468c3a,
                            }
                        );
                        callback_returned = true;
                    }
                    _ => panic!("unexpected return {body:08x}"),
                }
                bump(&mut counts.returns, body);
            }
            "difference_return" => {
                assert_eq!(number(&event["low_al"]), u32::from(difference.unwrap()));
                assert_eq!(number(&event["raw_eax"]) & 255, number(&event["low_al"]));
            }
            "write" => {
                assert_eq!(event["region"], "actor");
                assert_eq!(event["offset"], 0x29d);
                assert_eq!(event["bytes"], 1);
                let result = u8::from(difference.unwrap());
                assert_eq!(event["data"], json!([result]));
                assert_eq!(event["known_mask"], json!([true]));
                assert_eq!(
                    number(&event["ip"]),
                    if result == 1 { 0x468c10 } else { 0x468c1d }
                );
                backing.regions.get_mut("actor").unwrap()[0x29d] = Some(result);
                writes += 1;
            }
            "caller_probe_stop" => {
                assert!(probe && callback_returned);
                assert_eq!(number(&event["address"]), 0x47dbc8);
                assert_eq!(event["stack_restored"], true);
                assert_eq!(event["execution"], "before_next_fanout_instruction");
            }
            _ => panic!("unexpected timeline event {kind}"),
        }
    }
    assert!(receipt["invalid"].is_null());
    if receipt["outcome"]["stop"].is_null() {
        assert!(query.is_none() && frames.is_empty());
        assert!(receipt["stop_ip"].is_null());
        assert_eq!(
            receipt["return_stack_delta"],
            if raw_query {
                8
            } else if probe {
                0
            } else {
                20
            }
        );
        if !raw_query {
            assert!(callback_returned);
            assert_eq!(receipt["result"], 0);
            assert_eq!(receipt["low_al"], 0);
            assert_eq!(writes, usize::from(selected_player.is_some()));
        }
        bump(&mut counts.outcomes, "completed");
    } else {
        assert_eq!(number(&receipt["stop_ip"]), 0x47530f);
        assert!(receipt["result"].is_null() && receipt["return_stack_delta"].is_null());
        assert_eq!(writes, 0);
        let (entry, result, reads) = query.unwrap();
        assert_eq!(reads, 1);
        assert!(result.is_none());
        let (region, offset) = if entry["owner"] == "source" {
            ("actor", 0x250 + 4 * (number(&entry["low_word"]) >> 5))
        } else {
            ("first", 0xa0 + 4 * (number(&entry["low_word"]) >> 5))
        };
        assert_eq!(
            receipt["outcome"]["stop"],
            format!("unknown_{region}_{offset:x}")
        );
        bump(&mut counts.outcomes, "numeric_unknown_read_stop");
    }
    backing.compare(&receipt["state"]);
}

fn selector_in_state(state: &mut ObjectState) -> &mut Selector12State {
    let ObjectClass::Actor(actor) = &mut state.class else {
        panic!("actor")
    };
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("derived")
    };
    let DerivedActorClass::Family428650(family) = &mut derived.class else {
        panic!("family")
    };
    let Family428650Class::Family429e20(probe) = &mut family.class else {
        panic!("probe")
    };
    let Family429e20Class::Selector12(selector) = &mut probe.class else {
        panic!("selector12")
    };
    selector
}

struct Actual {
    arena: ObjectArena,
    actor: ObjectId,
    players: BTreeMap<String, PlayerId>,
}

impl Actual {
    fn new(input: &Value) -> Self {
        assert_eq!(input["actor"]["binding"], "selector12");
        assert_eq!(input["actor"]["live"], true);
        assert_eq!(input["patches"], json!([]));
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0x08010000),
        })
        .unwrap();
        // Explicit constructor-only backing; this no-first-player arm reads
        // neither grant records nor terrain/sine. No native constructor claim.
        let table = GrantTable {
            virtual_address: GRANT_TABLE_ADDRESS,
            records: Vec::new(),
        };
        let actor = arena
            .allocate_selector12_actor(
                DerivedActorConstructionRequest {
                    actor: ActorConstructionRequest {
                        constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: None,
                        owner: 1,
                        terrain: ActorTerrainInputs {
                            terrain: None,
                            coordinate_shift: 8,
                            clock: 0,
                            world_flags: None,
                            sine_quarter: &UNUSED_SINE,
                        },
                    },
                    world_mask_limits: [3, 3],
                },
                &mut 0,
                Selector12GrantContext {
                    table: &table,
                    first_player_grants: None,
                },
            )
            .unwrap();
        let selector = arena.selector12_actor_mut(actor).unwrap();
        selector.grants_24c.mask_04 = mask(&input["actor"]["grants6"]);
        selector.byte_29d = number(&input["actor"]["byte29d"]).try_into().unwrap();
        let mut actual = Self {
            arena,
            actor,
            players: BTreeMap::new(),
        };
        for player in input["players"].as_array().unwrap() {
            actual.append(player);
        }
        actual
    }

    fn append(&mut self, input: &Value) {
        let name = input["identity"].as_str().unwrap();
        player_address(name);
        let player = self.arena.add_world_player(mask(&input["grants6"]));
        assert!(self.players.insert(name.to_owned(), player).is_none());
    }

    fn effects(&mut self, input: &Value) {
        for effect in input["setup_effects"].as_array().unwrap() {
            if effect["operation"] == "append_player" {
                self.append(&effect["player"]);
                continue;
            }
            assert!(effect["operation"].is_null());
            let data: Vec<u8> = bytes(&effect["bytes"])
                .into_iter()
                .map(Option::unwrap)
                .collect();
            assert_eq!(data.len(), 24);
            let words: [u32; 6] = std::array::from_fn(|index| {
                u32::from_le_bytes(data[4 * index..4 * index + 4].try_into().unwrap())
            });
            match effect["region"].as_str().unwrap() {
                "actor" => {
                    assert_eq!(effect["offset"], 0x250);
                    self.arena
                        .selector12_actor_mut(self.actor)
                        .unwrap()
                        .grants_24c
                        .mask_04 = words;
                }
                name @ ("first" | "second") => {
                    assert_eq!(effect["offset"], 0xa0);
                    let id = self.players[name];
                    self.arena
                        .players
                        .list
                        .iter_mut()
                        .find(|player| player.id == id)
                        .unwrap()
                        .grants_9c = words;
                }
                other => panic!("typed patch region {other}"),
            }
        }
    }

    fn compare(&self, native: &Value) {
        assert_eq!(native["actor_live"], true);
        let selector = self.arena.selector12_actor(self.actor).unwrap();
        let raw = bytes(&native["storage"]["actor"]);
        assert_eq!(known_word(&raw[..4]), Some(0x527058));
        assert_eq!(raw[0x29d], Some(selector.byte_29d));
        for (index, word) in selector.grants_24c.mask_04.iter().enumerate() {
            assert_eq!(
                known_word(&raw[0x250 + index * 4..0x254 + index * 4]),
                Some(*word)
            );
        }
        let names: Vec<_> = self
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
        assert_eq!(native["player_order"], json!(names));
        assert_eq!(
            self.arena.first_player(),
            self.arena.world_players().first().map(|player| player.id)
        );
        for (name, id) in &self.players {
            let player = self.arena.world_player(*id).unwrap();
            let raw = bytes(&native["storage"][name]);
            for (index, word) in player.grants_9c.iter().enumerate() {
                assert_eq!(
                    known_word(&raw[0xa0 + index * 4..0xa4 + index * 4]),
                    Some(*word)
                );
            }
        }
    }

    /// All arena custody outside object payloads, including player allocator
    /// state and registry/cache/free chain, must survive the pure callback.
    fn custody_snapshot(&self) -> String {
        let arena = &self.arena;
        let allocations: Vec<_> = arena
            .slots
            .iter()
            .map(|slot| {
                (
                    slot.generation,
                    slot.value.as_ref().map(|owned| owned.memberships),
                )
            })
            .collect();
        format!(
            "{:?}",
            (
                (
                    &arena.identity,
                    &arena.free,
                    &arena.live_count,
                    &arena.lists,
                    &arena.list_owners,
                    &arena.routes,
                    &arena.actors,
                    &arena.map_annotations,
                    &arena.actor_handles
                ),
                (
                    &arena.actor_payloads,
                    &arena.constructor_allocations,
                    &arena.selector60_rows,
                    &arena.players,
                    &arena.world_load,
                    allocations
                )
            )
        )
    }

    fn invoke(&mut self, receipt: &Value) {
        let before = self.custody_snapshot();
        let mut expected = self.arena.object(self.actor).unwrap().clone();
        // Expected native state enters only the post-call assertion, never the
        // request, live owners, or the predicate's inputs.
        let actual_result =
            self.arena
                .notify_selector12_grant_difference(Selector12GrantDifferenceRequest {
                    actor: self.actor,
                });
        match actual_result {
            Ok(value) => assert_eq!(receipt["result"], value),
            Err(error) => panic!("admitted event22 stopped: {error}"),
        }
        selector_in_state(&mut expected).byte_29d =
            bytes(&receipt["state"]["storage"]["actor"])[0x29d].unwrap();
        assert_eq!(self.arena.object(self.actor).unwrap(), &expected);
        assert_eq!(self.custody_snapshot(), before);
        self.compare(&receipt["state"]);
    }
}

#[test]
fn selector12_grant_difference_matches_native_callback_histories() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/selector12_grant_difference_native.json"
    ))
    .unwrap();
    let Ok(raw) = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    ) else {
        eprintln!("retail Infest.exe missing; copy the game into retail/ to run this test");
        return;
    };
    let executable = FrontendExecutable::parse(&raw).unwrap();
    validate_metadata(&fixture["metadata"], &raw, &executable);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    let mut counts = Counts::default();
    let mut skipped = BTreeSet::new();
    let (mut admitted, mut invocations, mut diagnostic_steps) = (0, 0, 0);
    for case in cases {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        assert_eq!(case["instruction_bytes_unchanged"], true);
        let mut backing = Backing::new(input);
        backing.compare(&case["initial_state"]);
        let steps = input["steps"].as_array().unwrap();
        let receipts = case["steps"].as_array().unwrap();
        assert_eq!(steps.len(), receipts.len());
        for (step, receipt) in steps.iter().zip(receipts) {
            assert_eq!(receipt["outcome"]["kind"], case["evidence"]);
            validate_native_step(step, receipt, &mut backing, &mut counts, &executable);
        }
        match case["evidence"].as_str().unwrap() {
            "source_diagnostic" => {
                assert!(DIAGNOSTICS.contains(&name));
                assert!(skipped.insert(name));
                diagnostic_steps += steps.len();
                // No typed preparation of unknown masks, raw IDs/owners or
                // selector13, even when original raw storage completes.
                continue;
            }
            "original_body" => assert!(!DIAGNOSTICS.contains(&name)),
            other => panic!("implicit admission {other}"),
        }
        admitted += 1;
        let mut actual = Actual::new(input);
        actual.compare(&case["initial_state"]);
        for (step, receipt) in steps.iter().zip(receipts) {
            assert_eq!(step["operation"], "event22");
            assert!(step["raw_query"].is_null());
            assert!(receipt["outcome"]["stop"].is_null());
            actual.effects(step);
            actual.invoke(receipt);
            invocations += 1;
        }
    }
    assert_eq!((admitted, invocations, diagnostic_steps), (9, 14, 10));
    assert_eq!(skipped, DIAGNOSTICS.into_iter().collect());
    assert_eq!(
        json!(counts.entries),
        fixture["metadata"]["counts"]["routine_entries"]
    );
    assert_eq!(
        json!(counts.returns),
        fixture["metadata"]["counts"]["routine_returns"]
    );
    assert_eq!(
        json!(counts.events),
        fixture["metadata"]["counts"]["events"]
    );
    assert_eq!(
        json!(counts.outcomes),
        fixture["metadata"]["counts"]["outcomes"]
    );
}
