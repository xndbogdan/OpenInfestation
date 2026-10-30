//! Original-body receipts. The host supplies only the documented lower
//! placement/factory/virtual boundaries; it does not emulate row arithmetic.
use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, DerivedActorConstructionRequest, InstanceCommandAllocation, Selector60Row,
};
use inf_formats::{native_basis::NativeBasis, AssetRelocation, ByteSpan};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

const SINE: [u16; 4096] = [0; 4096];

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/selector60_update_native.json"
    ))
    .unwrap()
}
fn d(v: &Value) -> u32 {
    v.as_i64().unwrap() as u32
}
fn words(v: &Value) -> [u16; 3] {
    std::array::from_fn(|i| d(&v[i]) as u16)
}
fn basis(v: &Value) -> NativeBasis {
    NativeBasis {
        words: std::array::from_fn(|i| d(&v[i])),
    }
}
fn allocation_request() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        },
        world_mask_limits: [3, 3],
    }
}

struct Host {
    input: Value,
    child: ObjectId,
    source: Arc<InstanceCommandAllocation>,
    events: Vec<String>,
    basis_count: usize,
}
impl Host {
    fn stop(&self, name: &str) -> Result<(), String> {
        if self.input["stop"].as_str() == Some(name) {
            Err(name.to_owned())
        } else {
            Ok(())
        }
    }
}
impl TargetNullHandleSource for Host {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0))
    }
}
impl Selector60UpdateHost for Host {
    type Error = String;
    fn delayed_physics(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        microseconds: u32,
    ) -> Result<(), String> {
        assert_eq!(microseconds, d(&self.input["delta"]));
        self.events.push("physics".into());
        self.stop("physics")
    }
    fn spawn_placement(
        &mut self,
        _: &mut ObjectArena,
        row: &Selector60SpawnRow,
        _: &mut u32,
    ) -> Result<Selector60SpawnResult, String> {
        self.events.push("placement".into());
        if row.reference().unwrap() == 0 {
            return Ok(Selector60SpawnResult::NoPlacement);
        }
        self.stop("height")?;
        self.stop("factory")?;
        Ok(if self.input["factory"] == "rejected" {
            Selector60SpawnResult::Rejected
        } else {
            Selector60SpawnResult::Created(self.child)
        })
    }
    fn actor_component(
        &mut self,
        _: &mut ObjectArena,
        raw: ObjectId,
    ) -> Result<Option<ObjectId>, String> {
        assert_eq!(raw, self.child);
        self.events.push("component".into());
        self.stop("component")?;
        Ok(self.input["component"]
            .as_bool()
            .unwrap()
            .then_some(self.child))
    }
    fn initialize_spawned(
        &mut self,
        _: &mut ObjectArena,
        raw: ObjectId,
        rng: &mut u32,
    ) -> Result<(), String> {
        assert_eq!(raw, self.child);
        self.events.push("initialize".into());
        if self.input["mutation"] == "initialize" {
            let image = self.source.runtime_allocation();
            image.write_word(32 + 0x0c, 32767).unwrap();
            image.write_word(32 + 0x0e, 32768).unwrap();
            image.write_word(32 + 0x10, 4).unwrap();
            *rng = 0xa55a55aa;
        }
        self.stop("initialize")
    }
    fn source_basis(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
    ) -> Result<Option<ObjectId>, String> {
        self.basis_count += 1;
        let name = format!("basis{}", self.basis_count);
        self.events.push(name.clone());
        if self.input["mutation"] == "basis2" && self.basis_count == 2 {
            arena.actor_mut(parent).unwrap().rotating.basis = NativeBasis {
                words: [
                    91, 0x3f000000, 0, 0, 0, 0x3f800000, 0, 0x3f000000, 0x3f000000, 0xbf000000,
                ],
            };
            arena.owned_mut(parent).unwrap().state.base.position_words =
                Some([0xffff, 0x8000, 0x7fff]);
        }
        self.stop(&name)?;
        Ok(self.input["basis_present"]
            .as_bool()
            .unwrap()
            .then_some(parent))
    }
}

#[test]
fn selector60_update_and_row_native_receipts_preserve_arithmetic_and_service_order() {
    for case in fixture()["cases"].as_array().unwrap() {
        let input = &case["input"];
        let expected = &case["output"];
        let name = input["name"].as_str().unwrap();
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 64,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let actor = arena
            .allocate_selector60_actor(allocation_request(), &mut 17)
            .unwrap();
        let child = arena
            .allocate_derived_actor(allocation_request(), &mut 17)
            .unwrap();
        arena.owned_mut(child).unwrap().state.base.position_words = Some([0xaaa1, 0xaaa2, 0xaaa3]);
        arena.actor_mut(child).unwrap().rotating.basis = NativeBasis::INITIAL;
        arena.owned_mut(actor).unwrap().state.base.position_words = Some(words(&input["position"]));
        arena.owned_mut(actor).unwrap().state.base.word_28 = d(&input["radius"]) as u16;
        arena.actor_mut(actor).unwrap().rotating.basis = basis(&input["basis"]);
        arena.derived_actor_mut(actor).unwrap().word_144 = d(&input["delay"]) as i32;
        let rows = input["rows"].as_array().unwrap();
        let mut bytes = vec![0; 32 + 24 * rows.len()];
        bytes[2..4].copy_from_slice(&(d(&input["header_delay"]) as u16).to_le_bytes());
        let count = input.get("count").map(d).unwrap_or(rows.len() as u32);
        bytes[4..8].copy_from_slice(&count.to_le_bytes());
        bytes[8..12].copy_from_slice(&32_u32.to_le_bytes());
        for (i, row) in rows.iter().enumerate() {
            let at = 32 + i * 24;
            bytes[at..at + 4].copy_from_slice(&d(&row["reference"]).to_le_bytes());
            for (off, key) in [
                (4, "threshold"),
                (6, "capacity"),
                (8, "limit"),
                (12, "lower"),
                (14, "upper"),
            ] {
                bytes[at + off..at + off + 2].copy_from_slice(&(d(&row[key]) as u16).to_le_bytes());
            }
            bytes[at + 16] = d(&row["flags"]) as u8;
        }
        let main = ByteSpan::new(0, bytes.len());
        let source = Arc::new(
            InstanceCommandAllocation::new(
                bytes.into(),
                main,
                vec![AssetRelocation {
                    table_span: ByteSpan::new(0, 0),
                    field_offset: 8,
                    field_span: ByteSpan::new(8, 12),
                    target_offset: 32,
                }]
                .into(),
            )
            .unwrap(),
        );
        let mut tags = BTreeMap::new();
        tags.insert(0, Value::Null);
        tags.insert(arena.actor(child).unwrap().handle.raw(), json!("child"));
        let mut row_ids = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let mut handles = Vec::new();
            for (j, state) in row["handles"].as_array().unwrap().iter().enumerate() {
                if state.is_null() {
                    handles.push(ActorHandle::from_raw(0));
                    continue;
                }
                let handle = if state == "missing" {
                    ActorHandle::from_raw(0xf100_0000 + (i * 16 + j) as u32)
                } else {
                    let object = arena
                        .allocate_derived_actor(allocation_request(), &mut 17)
                        .unwrap();
                    arena.actor_mut(object).unwrap().word_c4 = d(state);
                    arena.actor(object).unwrap().handle
                };
                tags.insert(handle.raw(), json!(1 + i * 16 + j));
                handles.push(handle);
            }
            let id = Selector60RowId {
                arena: arena.identity,
                index: arena.selector60_rows.0.len(),
            };
            arena.selector60_rows.0.push(Some(Selector60Row {
                byte_00: d(&row["complete"]) as u8,
                byte_01: d(&row["active"]) as u8,
                word_04: d(&row["timer"]),
                handles_08: handles,
                word_0c: d(&row["total"]),
                word_10: d(&row["priority"]),
            }));
            source
                .runtime_allocation()
                .set_runtime_pointer((32 + i * 24 + 20) as u32, RuntimePointer::Selector60Row(id))
                .unwrap();
            row_ids.push(id);
        }
        let retained = RetainedSpawnList {
            allocation: source.clone(),
            allocation_offset: 0,
        };
        let state = arena.selector60_actor_mut(actor).unwrap();
        state.retained_190 = input
            .get("retained")
            .and_then(Value::as_bool)
            .unwrap_or(true)
            .then_some(retained.clone());
        state.word_1a4 = d(&input["gate_count"]);
        state.byte_1a8 = d(&input["gate_seen"]) as u8;
        state.word_194 = d(&input["cooldown"]);
        state.word_19e = d(&input["opening"]) as u16;
        state.word_1a0 = d(&input["indicator"]) as u16;
        state.byte_184 = d(&input["cached"]) as u8;
        state.position_198 = Some(words(&input["cached_position"]));
        state.byte_188 = d(&input["aggregate"][0]) as u8;
        state.byte_189 = d(&input["aggregate"][1]) as u8;
        state.word_18a = d(&input["aggregate"][2]) as u16;
        state.word_18c = d(&input["aggregate"][3]);
        let mut host = Host {
            input: input.clone(),
            child,
            source,
            events: Vec::new(),
            basis_count: 0,
        };
        let mut rng = d(&input["rng"]);
        let row = Selector60SpawnRow {
            retained,
            offset: 32,
        };
        let result = match input["mode"].as_str().unwrap() {
            "update" => arena.run_selector60_update(actor, d(&input["delta"]), &mut host, &mut rng),
            "row" => arena
                .census_spawn_row(actor, row_ids[0], &row, d(&input["delta"]), &host)
                .map(|_| ()),
            "spawn" => arena.execute_spawn_row(actor, row_ids[0], &row, &mut host, &mut rng),
            "position" => arena
                .selector60_spawn_position(actor, &mut host)
                .map(|_| ()),
            other => panic!("{other}"),
        };
        assert_eq!(
            result.is_err(),
            !expected["failed"].is_null(),
            "{name}: {result:?}"
        );
        let state = arena.selector60_actor(actor).unwrap();
        let actual_rows:Vec<_>=row_ids.iter().map(|&id| { let r=arena.selector60_row(id).unwrap(); json!({"complete":r.byte_00,"active":r.byte_01,"timer":r.word_04,"total":r.word_0c,"priority":r.word_10,"handles":r.handles_08.iter().map(|h|tags[&h.raw()].clone()).collect::<Vec<_>>()}) }).collect();
        let actual = json!({"delay":arena.derived_actor(actor).unwrap().word_144 as u32,
            "aggregate":[u32::from(state.byte_188),u32::from(state.byte_189),u32::from(state.word_18a),state.word_18c],
            "cooldown":state.word_194,"opening":state.word_19e,"indicator":state.word_1a0,"cached_position":state.position_198,
            "rows":actual_rows,"rng":rng,"events":host.events,"child_position":arena.object(child).unwrap().base.position_words,
            "child_basis":arena.actor(child).unwrap().rotating.basis.words,"parent_basis":arena.actor(actor).unwrap().rotating.basis.words});
        for (key, value) in actual.as_object().unwrap() {
            assert_eq!(value, &expected[key], "{name}: {key}");
        }
        if input["mode"] == "update" && result.is_err() {
            let before = host.events.clone();
            let before_rng = rng;
            assert!(
                matches!(arena.run_selector60_update(actor,1000,&mut host,&mut rng),Err(Selector60UpdateError::PreviouslyStopped(id)) if id==actor)
            );
            assert_eq!(host.events, before);
            assert_eq!(rng, before_rng);
        }
    }
}

#[test]
fn selector60_update_receipt_source_guards_cover_actual_vtable_and_whole_bodies() {
    use sha2::{Digest, Sha256};
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .expect("required original Infest.exe");
    let receipts = fixture();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        receipts["provenance"]["exe_sha256"].as_str().unwrap()
    );
    for guard in receipts["guards"].as_array().unwrap() {
        let at = (d(&guard["address"]) - 0x400000) as usize;
        let count = d(&guard["length"]) as usize;
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw[at..at + count])),
            guard["sha256"].as_str().unwrap()
        );
    }
    let at = (0x527de8 + 0x24 - 0x400000) as usize;
    assert_eq!(
        u32::from_le_bytes(raw[at..at + 4].try_into().unwrap()),
        0x480910
    );
}
