//! Original-byte provenance, supplied masked backing and complete native timelines.
//! This independent proof phase never creates or calls a portable actor owner.

use inf_formats::FrontendExecutable;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

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
const GAME: u32 = 0x749000;
const RETURN: u32 = 0xf00000;
const STACK: u32 = 0x730000;
pub(super) const DIAGNOSTICS: [&str; 7] = [
    "diagnostic-partial-process-null",
    "diagnostic-raw-game-and-list-storage",
    "diagnostic-scalar-base-binding",
    "diagnostic-selector13-binding",
    "diagnostic-stale-constant-body-receiver",
    "diagnostic-unknown-mandatory-actor84",
    "diagnostic-unknown-reached-grant-masks",
];
pub(super) const PREFIXES: [&str; 3] = [
    "fifo-prior-selector12-write-before-missing-null",
    "selector2-missing-process-owner",
    "selector2-owned-missing-null",
];

fn region_address(name: &str) -> u32 {
    match name {
        "a" => 0x740000,
        "b" => 0x741000,
        "c" => 0x742000,
        "d" => 0x743000,
        "first" => 0x748800,
        "second" => 0x748a00,
        "game" => GAME,
        "game_pointer" => 0x55f160,
        "process_null" => 0x54e894,
        other => panic!("unadmitted region {other}"),
    }
}

/// Only physical receipt storage: no arena, query implementation or generated
/// expected grant scan. All byte masks and declared setup effects are retained.
struct Backing {
    regions: BTreeMap<String, Vec<Option<u8>>>,
    live: BTreeMap<String, bool>,
    order: Vec<String>,
    player_order: Vec<String>,
    bindings: BTreeMap<String, String>,
    process_owner_present: bool,
    registry_available: bool,
    registry_null: u32,
}

impl Backing {
    fn new(input: &Value) -> Self {
        let mut backing = Self {
            regions: [
                ("a", 0x2a0),
                ("b", 0x2a0),
                ("c", 0x2a0),
                ("d", 0x2a0),
                ("first", 0x1a0),
                ("second", 0x1a0),
                ("game", 0x78),
                ("game_pointer", 4),
                ("process_null", 4),
            ]
            .into_iter()
            .map(|(name, count)| (name.to_owned(), vec![None; count]))
            .collect(),
            live: BTreeMap::new(),
            order: input["actor_order"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect(),
            player_order: Vec::new(),
            bindings: BTreeMap::new(),
            process_owner_present: input["process_owner_present"].as_bool().unwrap(),
            registry_available: input["registry_available"].as_bool().unwrap(),
            registry_null: number(&input["registry_null"]),
        };
        for record in input["actors"].as_array().unwrap() {
            let name = record["identity"].as_str().unwrap();
            let binding = record["binding"].as_str().unwrap();
            backing
                .live
                .insert(name.to_owned(), record["live"].as_bool().unwrap());
            backing.bindings.insert(name.to_owned(), binding.to_owned());
            backing.word(name, 0, Some(table(binding).0));
            for (offset, key) in [
                (0x84, "handle84"),
                (0xb4, "classification_b4"),
                (0x2c, "flags2c"),
                (0xc4, "lifecycle_c4"),
            ] {
                backing.word(name, offset, Some(number(&record[key])));
            }
            backing.regions.get_mut(name).unwrap()[0x38..0x3a]
                .copy_from_slice(&(number(&record["dirty38"]) as u16).to_le_bytes().map(Some));
            backing.regions.get_mut(name).unwrap()[0x32..0x38]
                .copy_from_slice(&bytes(&record["position6"]));
            if !record["grants6"].is_null() {
                backing.words(name, 0x250, mask(&record["grants6"]));
            }
            backing.regions.get_mut(name).unwrap()[0x29d] = record["byte29d"]
                .as_u64()
                .map(|value| value.try_into().unwrap());
        }
        backing.word("game_pointer", 0, Some(GAME));
        backing.word(
            "process_null",
            0,
            input["process_null"]
                .as_u64()
                .map(|value| value.try_into().unwrap()),
        );
        backing.bind_actor_list();
        backing.bind_player_list();
        for record in input["players"].as_array().unwrap() {
            backing.append_player(record);
        }
        for patch in input["patches"].as_array().unwrap() {
            backing.patch(patch);
        }
        backing
    }

    fn word(&mut self, region: &str, offset: usize, word: Option<u32>) {
        let bytes = word.map(u32::to_le_bytes);
        for i in 0..4 {
            self.regions.get_mut(region).unwrap()[offset + i] = bytes.map(|value| value[i]);
        }
    }

    fn words(&mut self, region: &str, offset: usize, words: [u32; 6]) {
        for (i, word) in words.into_iter().enumerate() {
            self.word(region, offset + 4 * i, Some(word));
        }
    }

    fn bind_list(&mut self, order: &[String], offset: usize, head: usize) {
        let chain: Vec<_> = std::iter::once(GAME + head as u32)
            .chain(
                order
                    .iter()
                    .map(|name| region_address(name) + offset as u32),
            )
            .chain(std::iter::once(GAME + head as u32 + 12))
            .collect();
        for (i, &pointer) in chain.iter().enumerate() {
            let (name, at, payload) = if i == 0 || i + 1 == chain.len() {
                ("game", (pointer - GAME) as usize, 0)
            } else {
                (order[i - 1].as_str(), offset, region_address(&order[i - 1]))
            };
            self.word(name, at, Some(chain.get(i + 1).copied().unwrap_or(0)));
            self.word(
                name,
                at + 4,
                Some(i.checked_sub(1).map(|i| chain[i]).unwrap_or(0)),
            );
            self.word(name, at + 8, Some(payload));
        }
    }

    fn bind_actor_list(&mut self) {
        self.bind_list(&self.order.clone(), 0xa8, 0x48);
    }
    fn bind_player_list(&mut self) {
        self.bind_list(&self.player_order.clone(), 0xc8, 0x60);
    }

    fn append_player(&mut self, record: &Value) {
        let name = record["identity"].as_str().unwrap();
        assert!(
            matches!(name, "first" | "second")
                && !self.player_order.iter().any(|present| present == name)
        );
        self.player_order.push(name.to_owned());
        self.word(name, 0xd4, Some(number(&record["handle_d4"])));
        self.word(name, 0xd8, Some(number(&record["handle_d8"])));
        self.words(name, 0xa0, mask(&record["grants6"]));
        self.bind_player_list();
    }

    fn patch(&mut self, patch: &Value) {
        let data = bytes(&patch["bytes"]);
        let offset = number(&patch["offset"]) as usize;
        self.regions
            .get_mut(patch["region"].as_str().unwrap())
            .unwrap()[offset..offset + data.len()]
            .copy_from_slice(&data);
    }

    fn effect(&mut self, effect: &Value) {
        match effect["operation"].as_str() {
            Some("append_player") => self.append_player(&effect["player"]),
            Some("destroy_shared") => {
                let name = effect["actor"].as_str().unwrap();
                assert_eq!(self.bindings[name], "shared");
                assert!(self.live[name]);
                self.live.insert(name.to_owned(), false);
                self.order.retain(|present| present != name);
                self.bind_actor_list();
            }
            Some(operation @ ("set_flag0" | "set_flag1" | "dispose")) => {
                let name = effect["actor"].as_str().unwrap();
                let old = known_word(&self.regions[name][0x2c..0x30]).unwrap();
                let bit = if operation == "set_flag0" { 1 } else { 2 };
                let new = if operation == "dispose" {
                    old & 0xffff_fe98
                } else {
                    (old & !bit)
                        | if effect["enabled"].as_bool().unwrap() {
                            bit
                        } else {
                            0
                        }
                };
                let dirty = u16::from_le_bytes(
                    self.regions[name][0x38..0x3a]
                        .iter()
                        .copied()
                        .collect::<Option<Vec<_>>>()
                        .unwrap()
                        .try_into()
                        .unwrap(),
                ) | if (old ^ new) & 0x1ff != 0 { 2 } else { 0 };
                self.word(name, 0x2c, Some(new));
                self.regions.get_mut(name).unwrap()[0x38..0x3a]
                    .copy_from_slice(&dirty.to_le_bytes().map(Some));
            }
            None => self.patch(effect),
            other => panic!("unadmitted effect {other:?}"),
        }
    }

    fn compare(&self, state: &Value) {
        assert_eq!(state["actor_live"], json!(self.live));
        assert_eq!(state["actor_order"], json!(self.order));
        assert_eq!(state["player_order"], json!(self.player_order));
        assert_eq!(state["process_owner_present"], self.process_owner_present);
        assert_eq!(
            state["registry"],
            json!({"available":self.registry_available,"null":self.registry_null})
        );
        assert_eq!(state["storage"], json!(self.regions));
    }
}

pub(super) fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

pub(super) fn bytes(value: &Value) -> Vec<Option<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|byte| byte.as_u64().map(|byte| byte.try_into().unwrap()))
        .collect()
}

pub(super) fn mask(value: &Value) -> [u32; 6] {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(number)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

pub(super) fn known_word(raw: &[Option<u8>]) -> Option<u32> {
    assert_eq!(raw.len(), 4);
    Some(u32::from_le_bytes(
        raw.iter()
            .copied()
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .unwrap(),
    ))
}

pub(super) fn table(binding: &str) -> (u32, u32) {
    let &(_, table, body) = TABLES
        .iter()
        .find(|&&(name, _, _)| name == binding)
        .unwrap();
    (table, body)
}

/// One evidence context owns the original-byte views and aggregate audit.
/// Histories own only their supplied backing; no actual arena is stored here.
pub(super) struct NativeEvidence<'a> {
    text: &'a [u8],
    executable: FrontendExecutable<'a>,
    expected: &'a Value,
    counts: Counts,
}

impl<'a> NativeEvidence<'a> {
    pub(super) fn new(
        metadata: &'a Value,
        raw: &'a [u8],
        executable: FrontendExecutable<'a>,
    ) -> Self {
        let text = original_text(raw);
        validate_metadata(metadata, text, executable);
        Self {
            text,
            executable,
            expected: &metadata["counts"],
            counts: Counts::default(),
        }
    }

    pub(super) fn audit_history(&mut self, case: &Value) {
        assert_eq!(case["instruction_bytes_unchanged"], true);
        let input = &case["input"];
        let mut backing = Backing::new(input);
        backing.compare(&case["initial_state"]);
        let requests = input["steps"].as_array().unwrap();
        let receipts = case["steps"].as_array().unwrap();
        assert_eq!(requests.len(), receipts.len());
        for (request, receipt) in requests.iter().zip(receipts) {
            self.step(request, receipt, &mut backing);
        }
    }

    pub(super) fn finish(&self) {
        assert_eq!(json!(self.counts.entries), self.expected["routine_entries"]);
        assert_eq!(json!(self.counts.returns), self.expected["routine_returns"]);
        assert_eq!(json!(self.counts.events), self.expected["events"]);
        assert_eq!(json!(self.counts.outcomes), self.expected["outcomes"]);
    }
}
#[derive(Default)]
struct Counts {
    entries: BTreeMap<String, u32>,
    returns: BTreeMap<String, u32>,
    events: BTreeMap<String, u32>,
    outcomes: BTreeMap<String, u32>,
}

fn bump(map: &mut BTreeMap<String, u32>, key: impl ToString) {
    *map.entry(key.to_string()).or_default() += 1;
}

type ReadSpan = (String, usize, u32);

fn first_player_reads(backing: &Backing) -> Vec<ReadSpan> {
    let mut reads = vec![
        ("game_pointer".into(), 0, 0x488aa0),
        ("game".into(), 0x60, 0x488aa5),
    ];
    if let Some(first) = backing.player_order.first() {
        reads.extend([
            (first.clone(), 0xc8, 0x488aa8),
            (first.clone(), 0xd0, 0x488ab4),
        ]);
    } else {
        reads.push(("game".into(), 0x6c, 0x488aa8));
    }
    reads
}

/// The bounded pure lookup's reached read contract, independent of Rust owners.
/// It validates native D8/current-NULL branch reads and physical return identity;
/// the enclosing typed receiver ignores that return, as its original suffix does.
fn lookup_reads(backing: &Backing, request: u32) -> (Vec<ReadSpan>, Option<u32>) {
    let mut reads = vec![("process_null".into(), 0, 0x46b096)];
    let Some(null) = known_word(&backing.regions["process_null"]) else {
        return (reads, None);
    };
    assert!(backing.process_owner_present);
    if request == null {
        return (reads, Some(0));
    }
    reads.extend([
        ("game_pointer".into(), 0, 0x46b0a0),
        ("game".into(), 0x60, 0x46b0a9),
    ]);
    if backing.player_order.is_empty() {
        reads.push(("game".into(), 0x6c, 0x46b0ac));
    }
    for (i, name) in backing.player_order.iter().enumerate() {
        reads.extend([
            (name.clone(), 0xc8, if i == 0 { 0x46b0ac } else { 0x46b0d2 }),
            (name.clone(), 0xd0, 0x46b0b5),
            (name.clone(), 0xd8, 0x46b0b8),
        ]);
        let d8 = known_word(&backing.regions[name][0xd8..0xdc]).unwrap();
        let selected = if d8 == null {
            reads.push((name.clone(), 0xd4, 0x46b0c8));
            known_word(&backing.regions[name][0xd4..0xd8]).unwrap()
        } else {
            d8
        };
        if selected == request {
            return (reads, Some(region_address(name)));
        }
        reads.push((name.clone(), 0xc8, 0x46b0d0));
        if i + 1 == backing.player_order.len() {
            reads.push(("game".into(), 0x6c, 0x46b0d2));
        }
    }
    (reads, Some(0))
}

struct GrantFlow {
    source: String,
    id: u32,
    target_next: bool,
    difference: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListPhase {
    Owner,
    Head,
    FirstNext,
    Payload,
    Table,
    Slot,
    Call,
    InCallback,
    Successor,
    Next,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Selector2Phase {
    Handle,
    Lookup,
    InLookup,
    Byte,
    Dword,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Selector30Phase {
    Delegate,
    InDelegate,
    Complete,
}

/// Audit emitted original entries/reads/RETs directly. Native query returns
/// determine subsequent query cadence; this is not a second grant scanner and
/// it never supplies an expected answer or fake service to the portable API.
impl NativeEvidence<'_> {
    fn step(&mut self, input: &Value, receipt: &Value, backing: &mut Backing) {
        let counts = &mut self.counts;
        let raw_text = self.text;
        let executable = self.executable;
        let effects = input["setup_effects"].as_array().unwrap();
        for effect in effects {
            backing.effect(effect);
        }
        let events = receipt["events"].as_array().unwrap();
        for (event, effect) in events.iter().zip(effects) {
            assert_eq!(event["kind"], "supplied_effect");
            assert_eq!(event["effect"], *effect);
            assert_eq!(
                event["execution"],
                "between_requests_supplied_owner_effect_no_native_constructor_or_destructor"
            );
        }
        let mut frames: Vec<&Value> = Vec::new();
        let mut grant: Option<GrantFlow> = None;
        let mut query_result: Option<Option<bool>> = None;
        let mut query_reads = 0;
        let mut first_reads: Option<(Vec<ReadSpan>, usize)> = None;
        let mut lookup: Option<(Vec<ReadSpan>, usize, Option<u32>)> = None;
        let mut delivered: Vec<String> = Vec::new();
        let mut completed: Vec<String> = Vec::new();
        let mut list_reads: Vec<ReadSpan> = Vec::new();
        let mut table_reads: Vec<(u32, u32)> = Vec::new();
        let mut wrote: BTreeSet<String> = BTreeSet::new();
        let mut callback_grant: BTreeMap<String, bool> = BTreeMap::new();
        let mut first_results: BTreeMap<String, Option<String>> = BTreeMap::new();
        let mut handles_read = BTreeSet::new();
        let mut lookup_completed = BTreeSet::new();
        let mut list_phase = ListPhase::Owner;
        let mut selector2_phase = None;
        let mut selector30_phase = None;
        let mut semantic = false;
        for (index, event) in events.iter().enumerate() {
            let kind = event["kind"].as_str().unwrap();
            bump(&mut counts.events, kind);
            match kind {
                "supplied_effect" => assert!(index < effects.len()),
                "routine_entry" | "query_entry" => {
                    let body = number(&event["address"]);
                    assert_eq!(event["execution"], "unchanged_native_body");
                    assert_eq!(event["call_depth"], frames.len() + 1);
                    let mut stack: Vec<_> = frames
                        .iter()
                        .map(|frame| number(&frame["address"]))
                        .collect();
                    stack.push(body);
                    assert_eq!(event["call_stack"], json!(stack));
                    assert_eq!(kind == "query_entry", body == 0x4752e0);
                    bump(&mut counts.entries, body);
                    if frames.is_empty() {
                        assert_eq!(event["entry_stack"], STACK);
                        assert_eq!(event["return_ip"], RETURN);
                        if input["delivery"] == "actor_list" {
                            assert_eq!(list_phase, ListPhase::Owner);
                            assert_eq!(body, 0x47daf0);
                            let source = number(&input["args"]["source_pointer"]);
                            assert_eq!(
                                event["receiver"],
                                if source == 0 {
                                    Value::Null
                                } else {
                                    json!({"native_address":source})
                                }
                            );
                            assert_eq!(
                                event["arguments"],
                                json!([
                                    22,
                                    number(&input["args"]["fourth_operand"]),
                                    number(&input["args"]["origin_pointer"])
                                ])
                            );
                        } else {
                            let name = input["actor"].as_str().unwrap();
                            assert_eq!(body, table(&backing.bindings[name]).1);
                        }
                    }
                    if TABLES.iter().any(|&(_, _, callback)| callback == body) {
                        let name = if backing
                            .live
                            .get(event["receiver"].as_str().unwrap_or(""))
                            .copied()
                            .unwrap_or(false)
                        {
                            event["receiver"].as_str().unwrap()
                        } else {
                            assert_eq!(event["receiver"], json!({"native_address":0xdead1000_u32}));
                            input["actor"].as_str().unwrap()
                        };
                        assert_eq!(body, table(&backing.bindings[name]).1);
                        let distance = if input["delivery"] == "actor_list" {
                            0
                        } else {
                            number(&input["args"]["distance_squared"])
                        };
                        assert_eq!(
                            event["arguments"],
                            json!([
                                number(&input["args"]["source_pointer"]),
                                distance,
                                22,
                                number(&input["args"]["fourth_operand"])
                            ])
                        );
                        if input["delivery"] == "actor_list" {
                            assert_eq!(list_phase, ListPhase::Call);
                            list_phase = ListPhase::InCallback;
                            assert_eq!(frames.last().unwrap()["address"], 0x47daf0);
                            assert_eq!(event["return_ip"], 0x47dbc8);
                            assert_eq!(name, backing.order[delivered.len()]);
                        }
                        delivered.push(name.to_owned());
                        callback_grant.remove(name);
                        first_results.remove(name);
                        handles_read.remove(name);
                        lookup_completed.remove(name);
                        selector2_phase = (body == 0x420be0).then_some(Selector2Phase::Handle);
                        selector30_phase = (body == 0x48c470).then_some(Selector30Phase::Delegate);
                    } else {
                        match body {
                            0x47daf0 => assert!(frames.is_empty()),
                            0x40de70 => {
                                assert_eq!(selector30_phase, Some(Selector30Phase::Delegate));
                                selector30_phase = Some(Selector30Phase::InDelegate);
                                let parent = frames.last().unwrap();
                                assert_eq!(parent["address"], 0x48c470);
                                assert_eq!(event["receiver"], parent["receiver"]);
                                assert_eq!(event["arguments"], parent["arguments"]);
                                assert_eq!(event["return_ip"], 0x48c498);
                            }
                            0x46b090 => {
                                assert_eq!(selector2_phase, Some(Selector2Phase::Lookup));
                                selector2_phase = Some(Selector2Phase::InLookup);
                                let parent = frames.last().unwrap();
                                assert_eq!(parent["address"], 0x420be0);
                                let actor = parent["receiver"].as_str().unwrap();
                                assert!(handles_read.contains(actor));
                                let request =
                                    known_word(&backing.regions[actor][0x84..0x88]).unwrap();
                                assert_eq!(event["arguments"], json!([request]));
                                assert_eq!(event["return_ip"], 0x420bf0);
                                let (reads, result) = lookup_reads(backing, request);
                                lookup = Some((reads, 0, result));
                            }
                            0x488aa0 => {
                                assert_eq!(frames.last().unwrap()["address"], 0x468be0);
                                assert_eq!(event["arguments"], json!([]));
                                assert_eq!(event["return_ip"], 0x468bf6);
                                first_reads = Some((first_player_reads(backing), 0));
                            }
                            0x4753a0 => {
                                let parent = frames.last().unwrap();
                                assert_eq!(parent["address"], 0x468be0);
                                let source = parent["receiver"].as_str().unwrap();
                                assert_eq!(first_results[source].as_deref(), Some("first"));
                                assert!(grant.is_none() && !callback_grant.contains_key(source));
                                assert_eq!(
                                    backing.player_order.first().map(String::as_str),
                                    Some("first")
                                );
                                assert_eq!(event["receiver"], "first_grants");
                                assert_eq!(
                                    event["arguments"],
                                    json!([region_address(source) + 0x24c])
                                );
                                assert_eq!(event["return_ip"], 0x468c0c);
                                grant = Some(GrantFlow {
                                    source: source.to_owned(),
                                    id: 0,
                                    target_next: false,
                                    difference: None,
                                });
                            }
                            0x4752e0 => {
                                let flow = grant.as_ref().unwrap();
                                assert!(flow.difference.is_none());
                                assert_eq!(event["arguments"], json!([flow.id]));
                                assert!(flow.id < 175);
                                assert_eq!(
                                    event["receiver"],
                                    if flow.target_next {
                                        "first_grants".to_owned()
                                    } else {
                                        format!("{}_grants", flow.source)
                                    }
                                );
                                assert_eq!(
                                    event["return_ip"],
                                    if flow.target_next { 0x4753bf } else { 0x4753b3 }
                                );
                                query_result = Some(None);
                                query_reads = 0;
                            }
                            other => panic!("unadmitted original body {other:08x}"),
                        }
                    }
                    frames.push(event);
                }
                "read" => {
                    let region = event["region"].as_str().unwrap();
                    let offset = number(&event["offset"]) as usize;
                    let data = bytes(&event["data"]);
                    let ip = number(&event["ip"]);
                    assert_eq!(data.len(), number(&event["bytes"]) as usize);
                    assert_eq!(
                        event["known_mask"],
                        json!(data.iter().map(Option::is_some).collect::<Vec<_>>())
                    );
                    assert_eq!(data, backing.regions[region][offset..offset + data.len()]);
                    let span = (region.to_owned(), offset, ip);
                    match number(&frames.last().unwrap()["address"]) {
                        0x47daf0 => {
                            assert!(matches!(
                                ip,
                                0x47dafb
                                    | 0x47dba4
                                    | 0x47dba7
                                    | 0x47dbb4
                                    | 0x47dbbd
                                    | 0x47dbc8
                                    | 0x47dbcc
                            ));
                            assert_eq!(data.len(), 4);
                            let wanted = match ip {
                                0x47dafb => ListPhase::Owner,
                                0x47dba4 => ListPhase::Head,
                                0x47dba7 => ListPhase::FirstNext,
                                0x47dbb4 => ListPhase::Payload,
                                0x47dbbd => ListPhase::Table,
                                0x47dbc8 => ListPhase::Successor,
                                0x47dbcc => ListPhase::Next,
                                _ => unreachable!(),
                            };
                            assert_eq!(list_phase, wanted);
                            list_phase = match ip {
                                0x47dafb => ListPhase::Head,
                                0x47dba4 => ListPhase::FirstNext,
                                0x47dba7 | 0x47dbcc => {
                                    if known_word(&data) == Some(0) {
                                        ListPhase::Complete
                                    } else {
                                        ListPhase::Payload
                                    }
                                }
                                0x47dbb4 => ListPhase::Table,
                                0x47dbbd => ListPhase::Slot,
                                0x47dbc8 => ListPhase::Next,
                                _ => unreachable!(),
                            };
                            list_reads.push(span);
                        }
                        0x420be0 => {
                            assert_eq!(selector2_phase, Some(Selector2Phase::Handle));
                            selector2_phase = Some(Selector2Phase::Lookup);
                            assert_eq!((offset, ip, data.len()), (0x84, 0x420be4, 4));
                            assert_eq!(event["region"], frames.last().unwrap()["receiver"]);
                            assert!(handles_read.insert(region.to_owned()));
                        }
                        0x46b090 => {
                            let (reads, next, _) = lookup.as_mut().unwrap();
                            assert_eq!(span, reads[*next]);
                            *next += 1;
                            assert_eq!(data.len(), 4);
                        }
                        0x488aa0 => {
                            let (reads, next) = first_reads.as_mut().unwrap();
                            assert_eq!(span, reads[*next]);
                            *next += 1;
                            assert_eq!(data.len(), 4);
                        }
                        0x4752e0 => {
                            let flow = grant.as_ref().unwrap();
                            assert_ne!(flow.id, 0);
                            assert_eq!(query_reads, 0);
                            query_reads += 1;
                            assert_eq!(ip, 0x47530f);
                            assert_eq!(data.len(), 4);
                            assert_eq!(
                                region,
                                if flow.target_next {
                                    "first"
                                } else {
                                    &flow.source
                                }
                            );
                            assert_eq!(
                                offset,
                                if flow.target_next { 0xa0 } else { 0x250 }
                                    + 4 * (flow.id >> 5) as usize
                            );
                            query_result = Some(
                                known_word(&data).map(|word| word & (1 << (flow.id & 31)) != 0),
                            );
                        }
                        other => panic!("unexpected payload read in {other:08x}"),
                    }
                }
                "executable_read" => {
                    let address = number(&event["address"]);
                    let data: Vec<_> = bytes(&event["data"])
                        .into_iter()
                        .map(Option::unwrap)
                        .collect();
                    let ip = number(&event["ip"]);
                    assert_eq!(data.len(), number(&event["bytes"]) as usize);
                    if address < 0x51f000 {
                        let at = (address - 0x401000) as usize;
                        assert_eq!(data, raw_text[at..at + data.len()]);
                    } else {
                        assert_eq!(data, executable.mapped(address, data.len()).unwrap().bytes);
                    }
                    match ip {
                        0x47dbc2 => {
                            assert_eq!(list_phase, ListPhase::Slot);
                            list_phase = ListPhase::Call;
                            let name = delivered
                                .get(completed.len())
                                .map(String::as_str)
                                .unwrap_or_else(|| backing.order[completed.len()].as_str());
                            assert_eq!(address, table(&backing.bindings[name]).0 + 0x98);
                            assert_eq!(data.len(), 4);
                            table_reads.push((
                                address,
                                known_word(&data.iter().copied().map(Some).collect::<Vec<_>>())
                                    .unwrap(),
                            ));
                        }
                        0x420c04 => {
                            let frame = frames.last().unwrap();
                            assert_eq!(frame["address"], 0x420be0);
                            assert!(lookup_completed.contains(frame["receiver"].as_str().unwrap()));
                            assert_eq!(selector2_phase, Some(Selector2Phase::Byte));
                            selector2_phase = Some(Selector2Phase::Dword);
                            assert_eq!(address, 0x420d9a);
                            assert_eq!(data, vec![8]);
                        }
                        0x420c0a => {
                            let frame = frames.last().unwrap();
                            assert_eq!(frame["address"], 0x420be0);
                            assert!(lookup_completed.contains(frame["receiver"].as_str().unwrap()));
                            assert_eq!(selector2_phase, Some(Selector2Phase::Dword));
                            selector2_phase = Some(Selector2Phase::Complete);
                            assert_eq!(address, 0x420d84);
                            assert_eq!(data, 0x420d5c_u32.to_le_bytes());
                        }
                        other => panic!("unexpected executable read IP {other:08x}"),
                    }
                }
                "query_return" | "routine_return" => {
                    let entry = frames.pop().unwrap();
                    let body = number(&event["body"]);
                    let ip = number(&event["ip"]);
                    let eax = number(&event["raw_eax"]);
                    let pop = number(&event["pop_bytes"]);
                    assert_eq!(body, number(&entry["address"]));
                    assert_eq!(kind == "query_return", body == 0x4752e0);
                    for key in ["receiver", "arguments", "return_ip", "entry_stack"] {
                        assert_eq!(event[key], entry[key]);
                    }
                    assert_eq!(event["call_depth"], frames.len() + 1);
                    assert_eq!(
                        event["stack_after"],
                        number(&entry["entry_stack"]) + 4 + pop
                    );
                    assert_eq!(event["execution"], "authentic_ret_checked_at_next_boundary");
                    assert_eq!(event["low_al"], eax & 255);
                    bump(&mut counts.returns, body);
                    match body {
                        0x4752e0 => {
                            let flow = grant.as_mut().unwrap();
                            let result = if flow.id == 0 {
                                assert_eq!(query_reads, 0);
                                true
                            } else {
                                assert_eq!(query_reads, 1);
                                query_result.unwrap().unwrap()
                            };
                            assert_eq!(
                                eax,
                                if flow.id == 0 {
                                    1
                                } else {
                                    ((flow.id >> 5) & !255) | u32::from(result)
                                }
                            );
                            assert_eq!(pop, 4);
                            assert_eq!(ip, if flow.id == 0 { 0x4752ed } else { 0x475317 });
                            query_result = None;
                            if !flow.target_next && result {
                                flow.target_next = true;
                            } else if flow.target_next && !result {
                                flow.difference = Some(true);
                            } else {
                                flow.target_next = false;
                                flow.id += 1;
                            }
                        }
                        0x4753a0 => {
                            let flow = grant.take().unwrap();
                            let difference = flow.difference.unwrap_or_else(|| {
                                assert_eq!(flow.id, 175);
                                false
                            });
                            assert_eq!(eax & 255, u32::from(difference));
                            assert_eq!(pop, 4);
                            assert_eq!(ip, if difference { 0x4753d9 } else { 0x4753d1 });
                            callback_grant.insert(flow.source, difference);
                        }
                        0x488aa0 => {
                            let (reads, count) = first_reads.take().unwrap();
                            assert_eq!(count, reads.len());
                            let first = backing.player_order.first();
                            assert_eq!(eax, first.map(|name| region_address(name)).unwrap_or(0));
                            assert_eq!(pop, 0);
                            let source = frames.last().unwrap()["receiver"].as_str().unwrap();
                            assert!(first_results
                                .insert(source.to_owned(), first.cloned())
                                .is_none());
                            assert_eq!(ip, if first.is_some() { 0x488ab7 } else { 0x488ab3 });
                        }
                        0x46b090 => {
                            assert_eq!(selector2_phase, Some(Selector2Phase::InLookup));
                            selector2_phase = Some(Selector2Phase::Byte);
                            let (reads, count, result) = lookup.take().unwrap();
                            assert_eq!(count, reads.len());
                            assert_eq!(eax, result.unwrap());
                            assert_eq!((pop, ip), (4, 0x46b0df));
                            let source = frames.last().unwrap()["receiver"].as_str().unwrap();
                            assert!(lookup_completed.insert(source.to_owned()));
                        }
                        0x47daf0 => {
                            assert_eq!(list_phase, ListPhase::Complete);
                            assert_eq!((eax, pop, ip), (0, 12, 0x47dbde));
                        }
                        0x40de70 => {
                            assert_eq!(selector30_phase, Some(Selector30Phase::InDelegate));
                            selector30_phase = Some(Selector30Phase::Complete);
                            assert_eq!((eax, pop, ip), (0, 16, 0x40deec));
                        }
                        _ => {
                            assert_eq!((eax, pop), (0, 16));
                            let name = delivered.last().unwrap();
                            let wanted = match body {
                                0x4270b0 => 0x4270ce,
                                0x402420 => 0x4024fa,
                                0x4045f0 => 0x4045f2,
                                0x420be0 => 0x420d60,
                                0x480f40 => 0x480fba,
                                0x48c470 => 0x48c498,
                                0x468be0 => match callback_grant.get(name) {
                                    Some(true) => 0x468c1a,
                                    Some(false) => 0x468c27,
                                    None => 0x468c3a,
                                },
                                _ => panic!("return {body:08x}"),
                            };
                            assert_eq!(ip, wanted);
                            if body == 0x420be0 {
                                assert_eq!(selector2_phase, Some(Selector2Phase::Complete));
                                assert!(lookup_completed.contains(name));
                            }
                            if body == 0x48c470 {
                                assert_eq!(selector30_phase, Some(Selector30Phase::Complete));
                            }
                            if body == 0x468be0 {
                                let first = first_results.get(name).unwrap();
                                assert_eq!(wrote.contains(name), first.is_some());
                                assert_eq!(callback_grant.contains_key(name), first.is_some());
                            }
                            if input["delivery"] == "actor_list" {
                                assert_eq!(list_phase, ListPhase::InCallback);
                                list_phase = ListPhase::Successor;
                            }
                            completed.push(name.clone());
                        }
                    }
                }
                "write" => {
                    let name = event["region"].as_str().unwrap();
                    let frame = frames.last().unwrap();
                    assert_eq!(frame["address"], 0x468be0);
                    assert_eq!(frame["receiver"], name);
                    assert!(grant.is_none() && query_result.is_none());
                    let difference = callback_grant[name];
                    assert_eq!(event["offset"], 0x29d);
                    assert_eq!(event["bytes"], 1);
                    assert_eq!(event["data"], json!([u8::from(difference)]));
                    assert_eq!(event["known_mask"], json!([true]));
                    assert_eq!(
                        number(&event["ip"]),
                        if difference { 0x468c10 } else { 0x468c1d }
                    );
                    assert!(wrote.insert(name.to_owned()));
                    backing.regions.get_mut(name).unwrap()[0x29d] = Some(u8::from(difference));
                }
                "semantic_boundary" => {
                    assert!(!semantic);
                    assert_eq!(frames.last().unwrap()["address"], 0x46b090);
                    assert_eq!(selector2_phase, Some(Selector2Phase::InLookup));
                    let (reads, read_count, result) = lookup.as_ref().unwrap();
                    assert_eq!(reads.len(), 1);
                    assert_eq!(*read_count, 1);
                    assert_eq!(*result, None);
                    let preceding = &events[index.checked_sub(1).unwrap()];
                    assert_eq!(preceding["kind"], "read");
                    assert_eq!(preceding["region"], "process_null");
                    assert_eq!(preceding["offset"], 0);
                    assert_eq!(preceding["bytes"], 4);
                    assert_eq!(preceding["ip"], 0x46b096);
                    assert_eq!(index + 1, events.len());
                    semantic = true;
                    assert_eq!(event["operation"], "process_null_read");
                    assert_eq!(event["owner_present"], backing.process_owner_present);
                    assert_eq!(event["ip"], 0x46b096);
                    assert_eq!(receipt["outcome"]["stop"], "missing_process_null");
                    assert_eq!(backing.regions["process_null"], vec![None; 4]);
                }
                "outside_storage_read" => {
                    assert_eq!(input["delivery"], "actor_list");
                    assert_eq!(
                        (
                            number(&event["address"]),
                            number(&event["bytes"]),
                            number(&event["ip"])
                        ),
                        (0x48, 4, 0x47dba4)
                    );
                    assert_eq!(known_word(&backing.regions["game_pointer"]), Some(0));
                }
                other => panic!("unadmitted native event {other}"),
            }
        }
        if input["delivery"] == "actor_list" {
            let mut expected = vec![
                ("game_pointer".into(), 0, 0x47dafb),
                ("game".into(), 0x48, 0x47dba4),
                backing
                    .order
                    .first()
                    .map(|name| (name.clone(), 0xa8, 0x47dba7))
                    .unwrap_or(("game".into(), 0x54, 0x47dba7)),
            ];
            for (i, name) in delivered.iter().enumerate() {
                expected.extend([(name.clone(), 0xb0, 0x47dbb4), (name.clone(), 0, 0x47dbbd)]);
                if completed.contains(name) {
                    expected.extend([
                        (name.clone(), 0xa8, 0x47dbc8),
                        backing
                            .order
                            .get(i + 1)
                            .map(|name| (name.clone(), 0xa8, 0x47dbcc))
                            .unwrap_or(("game".into(), 0x54, 0x47dbcc)),
                    ]);
                }
            }
            assert_eq!(list_reads, expected[..list_reads.len()]);
            assert_eq!(table_reads.len(), delivered.len());
            for (i, (address, body)) in table_reads.iter().enumerate() {
                assert_eq!(
                    (*address, *body),
                    (
                        table(&backing.bindings[&delivered[i]]).0 + 0x98,
                        table(&backing.bindings[&delivered[i]]).1
                    )
                );
            }
            if receipt["outcome"]["stop"].is_null() {
                assert_eq!(delivered, backing.order);
                assert_eq!(list_reads, expected);
            }
        } else {
            assert!(list_reads.is_empty() && table_reads.is_empty());
            assert_eq!(delivered, vec![input["actor"].as_str().unwrap().to_owned()]);
        }
        assert!(receipt["invalid"].is_null());
        if receipt["outcome"]["stop"].is_null() {
            assert!(
                frames.is_empty()
                    && grant.is_none()
                    && query_result.is_none()
                    && lookup.is_none()
                    && first_reads.is_none()
            );
            assert_eq!(receipt["result"], 0);
            assert!(receipt["stop_ip"].is_null());
            assert_eq!(
                receipt["return_stack_delta"],
                if input["delivery"] == "actor_list" {
                    16
                } else {
                    20
                }
            );
            assert_eq!(completed, delivered);
            bump(&mut counts.outcomes, "completed");
        } else {
            assert!(receipt["result"].is_null() && receipt["return_stack_delta"].is_null());
            let stop = receipt["outcome"]["stop"].as_str().unwrap();
            bump(&mut counts.outcomes, stop);
            if semantic {
                assert_eq!(receipt["stop_ip"], 0x46b096);
                assert_eq!(receipt["outcome"]["kind"], "semantic_prefix");
            } else if stop == "outside_storage_read" {
                assert_eq!(receipt["stop_ip"], 0x47dba4);
                assert_eq!(receipt["outcome"]["kind"], "source_diagnostic");
            } else {
                let read = events
                    .iter()
                    .rev()
                    .find(|event| event["kind"] == "read")
                    .unwrap();
                assert_eq!(receipt["stop_ip"], read["ip"]);
                assert!(bytes(&read["data"]).iter().any(Option::is_none));
                assert_eq!(
                    stop,
                    format!(
                        "unknown_{}_{:x}",
                        read["region"].as_str().unwrap(),
                        number(&read["offset"])
                    )
                );
                assert_eq!(receipt["outcome"]["kind"], "source_diagnostic");
            }
        }
        backing.compare(&receipt["state"]);
    }
}

/// Fixed supported original PE profile, confined to this test. Runtime mapped
/// ranges expose authored data; complete .text guards use its file bytes.
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

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hex_bytes(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}

fn validate_metadata(metadata: &Value, text: &[u8], executable: FrontendExecutable<'_>) {
    assert_eq!(
        metadata["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(
        metadata["text_sha256"],
        "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2"
    );
    assert_eq!(metadata["fpcw"], 0x027f);
    let profiles = [
        (
            0x47daf0,
            0x47dbe1,
            "981ff9301101b67148ba7cc952102a2ad96a08e45a213c33dafdcaaa5cb85c56",
        ),
        (
            0x4270b0,
            0x4270d1,
            "6c992f3c1768bb5d84792fa8fe2475861ad33a33333f7db81f1d027fe72fdb5c",
        ),
        (
            0x402420,
            0x4024fd,
            "264e8b4a46571536a8b9562d85d1fffb5950330a6cb44264a66b7ff6050de4cc",
        ),
        (
            0x4045f0,
            0x4045f5,
            "743e1bd88d09aaee11fa6bad94803b7e31241e4196b32ff1abc9aba5bdd924c5",
        ),
        (
            0x420be0,
            0x420d63,
            "573361033631b2ea51299e28f94d005ef1e34d72c926b9d783b0ba03b6e03531",
        ),
        (
            0x480f40,
            0x480fbd,
            "7173498717f59c7d52a391397fc6d6f5751254b969ca57ebbf9d4f2ac30a7a79",
        ),
        (
            0x468be0,
            0x468c3d,
            "0b3c47cbe75c73c98cedbf743ec3fb87127820941f7a961c371a0458bad79a59",
        ),
        (
            0x48c470,
            0x48c4d7,
            "10ceffe66041d63538f10d64db87983b2b126b49f0f081285a88f8ee7b42f17a",
        ),
        (
            0x40de70,
            0x40deef,
            "030b5a1fea27e1d4f3cabf4e67b5588fa21d2e221c58e53aabb2c4658a69fa40",
        ),
        (
            0x46b090,
            0x46b0e2,
            "18dfb4b22c6657b39616026564af78c321ee059ad2c8a25da2b6c0c2ac975ee8",
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
    let guards = metadata["body_guards"].as_array().unwrap();
    assert_eq!(guards.len(), profiles.len());
    for (guard, (address, end, hash)) in guards.iter().zip(profiles) {
        assert_eq!(guard["address"], address);
        assert_eq!(guard["end"], end);
        assert_eq!(guard["sha256"], hash);
        assert_eq!(
            digest(&text[(address - 0x401000) as usize..(end - 0x401000) as usize]),
            hash
        );
    }
    let instructions = [
        (0x47dafb, "8b0d60f15500"),
        (0x47dba4, "8b7148"),
        (0x47dba7, "833e00"),
        (0x47dbb4, "8b4e08"),
        (0x47dbbb, "55508b116a0053ff9298000000"),
        (0x47dbc8, "8b36"),
        (0x47dbca, "03f8"),
        (0x47dbcc, "833e00"),
        (0x47dbde, "c20c00"),
        (0x420be4, "8b8784000000"),
        (0x420beb, "e8a0a40400"),
        (0x420bf2, "8b442414"),
        (0x46b096, "8b3d94e85400"),
        (0x46b0b8, "8b90d8000000"),
        (0x46b0c8, "39b0d4000000"),
        (0x48c493, "e8d819f8ff"),
        (0x468bfa, "8d8e4c020000"),
        (0x468c01, "8d889c000000"),
        (0x468c10, "c6869d02000001"),
        (0x468c1d, "c6869d02000000"),
        (0x4752e6, "6685c0"),
        (0x47530f, "85748204"),
        (0x4753c4, "81feaf000000"),
        (0x488aa0, "a160f15500"),
        (0x488aa5, "8b4060"),
        (0x488aa8, "833800"),
        (0x488ab4, "8b4008"),
    ];
    let guards = metadata["instruction_guards"].as_array().unwrap();
    assert_eq!(guards.len(), instructions.len());
    for (guard, (address, hex)) in guards.iter().zip(instructions) {
        let data = hex_bytes(hex);
        assert_eq!(guard, &json!({"address":address,"sha256":digest(&data)}));
        let at = (address - 0x401000) as usize;
        assert_eq!(data, text[at..at + data.len()]);
    }
    let switches = [
        (
            0x420d64,
            "110c42004b0c4200700c4200ec0c4200fd0c4200410d42001f0d42003d0c42005c0d4200",
            "a16771f7ca6674522940d096dd97c197b702bc958ce40a73ecca23a60228979c",
        ),
        (
            0x420d88,
            "0001080808080802020308080804050608080807",
            "c133b4f4d818d8f44bbc515f48e0738f8451dece08f87d949126ba0f8c9190c1",
        ),
        (
            0x402500,
            "89244000ba2440005d2440007b2440003f244000f3244000f8244000",
            "4b72d8e51f1a33f12d4597aad2a166ffbfa28237fd232766546c24ea31c2eddd",
        ),
        (
            0x40251c,
            "000106060203040606060606060605",
            "bf4a44a100a8b98415524aacac8662073e1f2e45c9eb7fbfa2bc3aa6cd886814",
        ),
    ];
    let guards = metadata["switch_tables"].as_array().unwrap();
    assert_eq!(guards.len(), switches.len());
    for (guard, (address, hex, hash)) in guards.iter().zip(switches) {
        assert_eq!(guard, &json!({"address":address,"sha256":hash}));
        let data = hex_bytes(hex);
        let at = (address - 0x401000) as usize;
        assert_eq!(data, text[at..at + data.len()]);
    }
    let matrix = metadata["table98_matrix"].as_array().unwrap();
    assert_eq!(matrix.len(), TABLES.len());
    for (entry, (name, table, body)) in matrix.iter().zip(TABLES) {
        assert_eq!(
            entry,
            &json!({"name":name,"table":table,"slot":0x98,"callback":body})
        );
        assert_eq!(
            executable.mapped(table + 0x98, 4).unwrap().bytes,
            body.to_le_bytes()
        );
    }
    assert_eq!(
        metadata["construction_carriers"],
        json!({"owner_count":2,"initial_capacity":8,"actor_owner":1,"registered_handles_in_construction_order":[0x08070001,0x08060001,0x08050001,0x08040001],"scope":"portable comparer construction operands only; original constructor instructions not executed","known_masks":"full native storage retains every supplied known/unknown byte; only literal29D writes gain knownness during calls"})
    );
    assert_eq!(
        metadata["abi"],
        json!({"direct":{"argument_count":4,"event_index":2,"event":22,"pop_bytes":16,"stack_delta":20,"ignored_argument_indices":[0,1,3]},"actor_list":{"fastcall_source":"ECX","radius":"EDX_literal_zero","argument_count":3,"stack_arguments":["event22","fourth_operand","unread_origin_pointer"],"pop_bytes":12,"stack_delta":16},"callback_return":"full_DWORD_zero","grant_query_return":"low_AL_only","caller_stack":"every authentic RET checked on next original instruction or final sentinel"})
    );
    assert_eq!(metadata["counts"],serde_json::from_str::<Value>(r#"{"histories":34,"steps":66,"admitted_histories":27,"admitted_steps":56,"original_histories":24,"original_steps":53,"prefix_histories":3,"prefix_steps":3,"diagnostic_histories":7,"diagnostic_steps":10,"routine_entries":{"4354224":21,"4709104":45,"4203552":4,"4722496":4,"4623328":18,"4754080":18,"4212208":13,"4768880":4,"4251248":4,"4328416":13,"4632720":12,"4674464":14,"4674272":1093},"routine_returns":{"4354224":21,"4709104":36,"4203552":4,"4722496":4,"4754080":18,"4623328":16,"4212208":13,"4251248":4,"4768880":4,"4632720":8,"4328416":8,"4674272":1091,"4674464":12},"events":{"routine_entry":170,"routine_return":148,"read":1546,"executable_read":72,"semantic_boundary":3,"query_entry":1093,"query_return":1091,"write":12,"supplied_effect":15,"outside_storage_read":1},"outcomes":{"completed":56,"missing_process_null":3,"unknown_a_84":1,"unknown_a_250":1,"unknown_first_a0":1,"unknown_game_pointer_0":1,"outside_storage_read":1,"unknown_game_48":1,"unknown_process_null_0":1}}"#).unwrap());
    assert_eq!(metadata["diagnostic_names"], json!(DIAGNOSTICS));
    assert_eq!(metadata["prefix_names"], json!(PREFIXES));
    assert_eq!(metadata["supplied_services"], json!([]));
    assert_eq!(metadata["admission"]["cadence"],"native entry/read/query/RET measurements separately validated; no Rust observer or measured Rust call-count claim");
    assert_eq!(
        metadata["scope_exclusions"],
        json!([
            "nonzero_radius",
            "other_events",
            "fake_virtual_override",
            "raw_pointer_alias_or_ABA",
            "actor_or_player_constructors",
            "native_owner_effect_bodies",
            "live_grant_setter_or_merge_or_copy",
            "message_and_notification_owners",
            "outer_capture",
            "production_attachment",
            "retail_acceptance",
            "unknown_bytes_as_port_defaults",
            "registry_resolver_or_cache_reads",
            "unrelated_inherited_preparation"
        ])
    );
    let support = &metadata["native_support"];
    let lengths = [
        ("a", 0x2a0),
        ("b", 0x2a0),
        ("c", 0x2a0),
        ("d", 0x2a0),
        ("first", 0x1a0),
        ("second", 0x1a0),
        ("game", 0x78),
        ("game_pointer", 4),
        ("process_null", 4),
    ];
    assert_eq!(support["regions"].as_object().unwrap().len(), lengths.len());
    for (name, length) in lengths {
        assert_eq!(
            support["regions"][name],
            json!({"address":region_address(name),"length":length})
        );
    }
    for (key, value) in [
        ("actor_node_offset", 0xa8),
        ("actor_head", GAME + 0x48),
        ("actor_tail", GAME + 0x54),
        ("player_node_offset", 0xc8),
        ("player_head", GAME + 0x60),
        ("player_tail", GAME + 0x6c),
        ("actor_grant_owner", 0x24c),
        ("actor_grant_words", 0x250),
        ("player_grant_owner", 0x9c),
        ("player_grant_words", 0xa0),
        ("result_byte", 0x29d),
    ] {
        assert_eq!(support[key], value);
    }
    assert_eq!(support["registry"],"independent portable constructor carrier; no native registry bytes or resolver consumed or registry capacity numeric parity");
}
