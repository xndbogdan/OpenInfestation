//! Native instruction receipts with explicitly supplied lower services.
//! Heap sound pointers project to presence; the owning sound record is real.
//! Infallible Rust helper release/disable cannot consume supplied native faults.

use super::*;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs,
        DerivedActorConstructionRequest, Selector10Helper,
    },
    platform_audio::ResidentSoundResource,
    process_sound::{
        ProcessSoundRecordConstructionRequest, ProcessSoundRecordKind, ProcessSoundRef,
        ProcessSoundRuntime,
    },
};
use serde_json::{json, Value};
use std::cell::Cell;

fn n(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
fn xyz(v: &Value) -> [u16; 3] {
    serde_json::from_value(v.clone()).unwrap()
}
fn project(mut value: Value) -> Value {
    value["sound"] = json!(n(&value["sound"]) != 0);
    if value["helper"] == false {
        value["helper_byte"] = Value::Null;
    }
    value
}

fn snapshot(arena: &ObjectArena, id: ObjectId, rng: u32, process_null: u32) -> Value {
    let base = &arena.object(id).unwrap().base;
    let actor = arena.actor(id).unwrap();
    let derived = arena.derived_actor(id).unwrap();
    let family = arena.family4236d0_actor(id).unwrap();
    let helper = arena.selector10_actor(id).unwrap().helper_26c.as_ref();
    json!({"flags":base.flags,"source":actor.handle.raw(),"kind":actor.word_b4,
        "state":actor.word_c4,"sound":derived.sound_168.is_some(),
        "scale0":derived.word_16c.unwrap(),"scale1":derived.word_170.unwrap(),
        "mode":derived.word_174.unwrap(),
        "elapsed":arena.family428650_actor(id).unwrap().words_188_19c[0].unwrap(),
        "status":arena.family428650_actor(id).unwrap().word_228,"stage":family.word_240,
        "size":base.word_28,"dirty":base.dirty_word,"model":actor.rotating.visual.model_word,
        "word74":actor.word_74,"death_sound":derived.sound_words_17c_182.unwrap()[3],
        "death_model":family.word_262,"death_flags":family.word_264,
        "position":base.position_words.unwrap(),"helper":helper.is_some(),
        "helper_byte":helper.map(|h|h.byte_0c),"rng":rng,"process_null":process_null})
}

fn supply(
    arena: &mut ObjectArena,
    actor: ObjectId,
    values: &Value,
    rng: &mut u32,
    null: &Cell<u32>,
    sound: ProcessSoundRef,
) {
    for (field, v) in values.as_object().unwrap() {
        match field.as_str() {
            "flags" => arena.owned_mut(actor).unwrap().state.base.flags = n(v),
            "source" => arena.actor_mut(actor).unwrap().handle = ActorHandle::from_raw(n(v)),
            "kind" => arena.actor_mut(actor).unwrap().word_b4 = n(v),
            "state" => arena.actor_mut(actor).unwrap().word_c4 = n(v),
            "sound" => {
                arena.derived_actor_mut(actor).unwrap().sound_168 = (n(v) != 0).then_some(sound)
            }
            "scale0" => arena.derived_actor_mut(actor).unwrap().word_16c = Some(n(v)),
            "scale1" => arena.derived_actor_mut(actor).unwrap().word_170 = Some(n(v)),
            "mode" => arena.derived_actor_mut(actor).unwrap().word_174 = Some(n(v)),
            "elapsed" => arena.family428650_actor_mut(actor).unwrap().words_188_19c[0] = Some(n(v)),
            "status" => arena.family428650_actor_mut(actor).unwrap().word_228 = n(v),
            "stage" => arena.family4236d0_actor_mut(actor).unwrap().word_240 = n(v),
            "size" => arena.owned_mut(actor).unwrap().state.base.word_28 = n(v) as u16,
            "dirty" => arena.owned_mut(actor).unwrap().state.base.dirty_word = n(v) as u16,
            "model" => arena.actor_mut(actor).unwrap().rotating.visual.model_word = n(v) as u16,
            "word74" => arena.actor_mut(actor).unwrap().word_74 = n(v) as u16,
            "death_sound" => {
                arena.derived_actor_mut(actor).unwrap().sound_words_17c_182 =
                    Some([0, 0, 0, n(v) as u16])
            }
            "death_model" => arena.family4236d0_actor_mut(actor).unwrap().word_262 = n(v) as u16,
            "death_flags" => arena.family4236d0_actor_mut(actor).unwrap().word_264 = n(v) as u16,
            "position" => arena.owned_mut(actor).unwrap().state.base.position_words = Some(xyz(v)),
            "helper" => {
                let target = &mut arena.selector10_actor_mut(actor).unwrap().helper_26c;
                if v == false {
                    *target = None;
                } else if target.is_none() {
                    *target = Selector10Helper::for_selector(1);
                }
            }
            "helper_byte" => {
                if let Some(h) = &mut arena.selector10_actor_mut(actor).unwrap().helper_26c {
                    h.byte_0c = n(v) as u8;
                }
            }
            "rng" => *rng = n(v),
            "process_null" => null.set(n(v)),
            // Input metadata is deliberately separate from mutable actor state.
            "timers" | "before_timer" | "globals" | "microseconds" | "entering" | "after"
            | "stop" | "sound_present" | "player" | "factory_returns" | "basis_vector" => {}
            other => panic!("unknown receipt field {other}"),
        }
    }
}

fn fixture() -> (ObjectArena, ObjectId, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let request = || DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0; 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &[0; 4096],
            },
        },
        world_mask_limits: [3, 3],
    };
    let actor = arena.allocate_selector10_actor(request(), &mut 0).unwrap();
    let child = arena.allocate_selector10_actor(request(), &mut 0).unwrap();
    arena.owned_mut(child).unwrap().state.base.dirty_word = 0x8000;
    arena.actor_mut(child).unwrap().word_b4 = 0xa5a5_a5a5;
    arena.family4236d0_actor_mut(actor).unwrap().table_244 = 0x0400_c000;
    (arena, actor, child)
}

struct Host<'a> {
    input: &'a Value,
    events: &'a [Value],
    cursor: usize,
    actor: ObjectId,
    child: ObjectId,
    null: Cell<u32>,
    sound: ProcessSoundRef,
    last_rng: u32,
    executable: inf_formats::FrontendExecutable<'a>,
    sine: [u16; 4096],
}
impl Host<'_> {
    fn next(&mut self, name: &str) -> Value {
        while self.cursor < self.events.len()
            && matches!(
                self.events[self.cursor]["kind"].as_str(),
                Some("rng" | "helper_destroy" | "helper_disable")
            )
        {
            self.cursor += 1;
        }
        let event = self
            .events
            .get(self.cursor)
            .expect("unexpected portable service")
            .clone();
        self.cursor += 1;
        assert_eq!(event["kind"], name, "event index{}", self.cursor - 1);
        event
    }
    fn stop(&self, event: &Value) -> Result<(), &'static str> {
        let name = event["kind"].as_str().unwrap();
        let label = format!("{name}:{}", event["ordinal"].as_u64().unwrap_or(0));
        if self.input["stop"]
            .as_str()
            .is_some_and(|s| s == name || s == label)
        {
            Err("supplied native boundary")
        } else {
            Ok(())
        }
    }
    fn event(
        &mut self,
        name: &str,
        arena: &mut ObjectArena,
        rng: &mut u32,
    ) -> Result<Value, &'static str> {
        let event = self.next(name);
        assert_eq!(
            snapshot(arena, self.actor, *rng, self.null.get()),
            project(event["actor"].clone()),
            "at {name}"
        );
        self.last_rng = *rng;
        self.stop(&event)?;
        let label = format!("{name}:{}", event["ordinal"].as_u64().unwrap_or(0));
        let mutation = self.input["after"]
            .get(&label)
            .or_else(|| self.input["after"].get(name));
        if let Some(values) = mutation {
            supply(arena, self.actor, values, rng, &self.null, self.sound);
        }
        self.last_rng = *rng;
        Ok(event)
    }
}
impl TargetNullHandleSource for Host<'_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(self.null.get()))
    }
}
impl FamilyDyingHost for Host<'_> {
    type Error = &'static str;
    fn read_dword(&mut self, address: u32) -> Result<u32, Self::Error> {
        if address == 0x0400_bffc {
            return Ok(n(&self.input["before_timer"]));
        }
        if (0x0400_c000..0x0400_c010).contains(&address) {
            return Ok(n(
                &self.input["timers"][((address - 0x0400_c000) / 4) as usize]
            ));
        }
        if (0x5307e4..=0x5307f0).contains(&address) {
            return Ok(n(
                &self.input["globals"][((address - 0x5307e4) / 4) as usize]
            ));
        }
        Ok(self.executable.word(address).unwrap())
    }
    fn stage_change(
        &mut self,
        arena: &mut ObjectArena,
        _: ObjectId,
        entering: u32,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        let e = self.event("stage", arena, rng)?;
        assert_eq!(n(&e["entering"]), entering);
        Ok(())
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        _: ObjectId,
        effect: FamilyDyingEffect,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        match effect {
            FamilyDyingEffect::RefreshModel => {
                self.event("refresh", arena, rng)?;
            }
            FamilyDyingEffect::Burst(b) => {
                let e = self.event("burst", arena, rng)?;
                assert_eq!(
                    e["packet"],
                    json!({"count":b.debris_count,"position":b.position,
                    "radius":b.placement_radius,"central_type":b.central_type,"debris_types":b.debris_types,
                    "speed":b.speed,"flags":b.flags})
                );
            }
            FamilyDyingEffect::Radius {
                block_address,
                source,
            } => {
                let e = self.event("radius", arena, rng)?;
                assert_eq!(
                    (n(&e["block"]), n(&e["source"]), n(&e["replicate"])),
                    (block_address, source.raw(), 1)
                );
            }
            FamilyDyingEffect::CameraEffect { strength, argument } => {
                let e = self.event("camera", arena, rng)?;
                assert_eq!(e["arguments"], json!([strength, argument]));
            }
            FamilyDyingEffect::Audio(audio) => match audio {
                FamilyDyingAudio::OneShot {
                    sound,
                    volume,
                    pitch,
                } => {
                    let e = self.event("one_shot", arena, rng)?;
                    assert_eq!(n(&e["sound"]), sound as u32);
                    assert_eq!(e["arguments"], json!([volume, pitch]));
                    self.event("sound_submit", arena, rng)?;
                }
                FamilyDyingAudio::CreatePersistent {
                    sound,
                    volume,
                    pitch,
                } => {
                    let e = self.event("sound_create", arena, rng)?;
                    assert_eq!(n(&e["sound"]), sound);
                    assert_eq!(e["arguments"], json!([volume, pitch, 0]));
                }
                FamilyDyingAudio::UpdatePersistent { volume, pitch } => {
                    let e = self.event("sound_update", arena, rng)?;
                    assert_eq!(e["arguments"], json!([volume, pitch]));
                }
                FamilyDyingAudio::StopPersistent => {
                    self.event("sound_stop", arena, rng)?;
                }
            },
            FamilyDyingEffect::StatusVisual { microseconds } => {
                let e = self.event("physics", arena, rng)?;
                assert_eq!(n(&e["microseconds"]), microseconds);
            }
            FamilyDyingEffect::ChangeState(r) => {
                let e = self.event("state_zero", arena, rng)?;
                assert_eq!(
                    e["arguments"],
                    json!([r.argument, r.source.raw(), r.target_state])
                );
            }
            FamilyDyingEffect::Pickup {
                position,
                kind,
                value,
            } => {
                let e = self.event("pickup", arena, rng)?;
                assert_eq!(e["position"], json!(position));
                assert_eq!(
                    (n(&e["pickup_kind"]), n(&e["value"])),
                    (kind, u32::from(value))
                );
            }
            FamilyDyingEffect::AttachSpawnedActor { child } => {
                let e = self.event("attach", arena, rng)?;
                assert_eq!(child, self.child);
                assert_eq!(arena.actor(child).unwrap().word_b4, n(&e["child_kind"]));
                assert_eq!(
                    u32::from(arena.object(child).unwrap().base.dirty_word),
                    n(&e["child_dirty"])
                );
            }
            other => panic!("unexpected class10 service {other:?}"),
        }
        Ok(())
    }
    fn sound_present(&mut self, sound: i16) -> Result<bool, Self::Error> {
        let e = self.next("sound_lookup");
        assert_eq!(n(&e["sound"]), u32::from(sound as u16));
        self.stop(&e)?;
        Ok(self.input["sound_present"].as_bool().unwrap())
    }
    fn player_position(
        &mut self,
        arena: &mut ObjectArena,
    ) -> Result<Option<[u16; 3]>, Self::Error> {
        let mut rng = self.last_rng;
        let e = self.event("player", arena, &mut rng)?;
        Ok((!e["position"].is_null()).then(|| xyz(&e["position"])))
    }
    fn create_death_object(
        &mut self,
        arena: &mut ObjectArena,
        r: FamilyDyingSpawnRequest,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error> {
        let e = self.event("factory", arena, rng)?;
        assert_eq!(
            (n(&e["resource"]), xyz(&e["position"]), n(&e["source"])),
            (r.resource, r.position, r.source.raw())
        );
        assert_eq!(
            (
                n(&e["suffix"]),
                n(&e["rotation"]),
                n(&e["placement"]),
                n(&e["mode"])
            ),
            (0, 0, u32::MAX, 0)
        );
        Ok(e["returns_actor"].as_bool().unwrap().then_some(self.child))
    }
    fn resolve_spawned_actor(
        &mut self,
        _: &mut ObjectArena,
        raw: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error> {
        self.next("actor_query_return");
        assert_eq!(raw, self.child);
        Ok(Some(raw))
    }
    fn actor_basis(
        &mut self,
        arena: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<NativeBasis, Self::Error> {
        let mut rng = self.last_rng;
        let e = self.event("basis", arena, &mut rng)?;
        let mut result = NativeBasis::INITIAL;
        for i in 0..3 {
            result.words[7 + i] = n(&e["vector"][i]);
        }
        Ok(result)
    }
    fn rotate_pickup_vector(
        &mut self,
        vector: &mut [u32; 3],
        axis: u32,
        angle: u32,
    ) -> Result<(), Self::Error> {
        let e = self.next("rotate");
        assert_eq!(e["before"], json!(vector));
        assert_eq!((n(&e["axis"]), n(&e["angle"])), (axis, angle));
        self.stop(&e)?;
        *vector = inf_formats::native_basis::rotate_axis_vector(
            vector.map(f32::from_bits),
            [0., 1., 0.],
            angle as i32,
            &self.sine,
        )
        .map(f32::to_bits);
        assert_eq!(e["after"], json!(vector));
        Ok(())
    }
}

#[test]
fn family_dying_matches_native_entry_update_and_pickup_stage_receipts() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = inf_formats::parse_frontend_sine_table(executable.data(), 0x53df00)
        .unwrap()
        .values;
    let mut resident =
        crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    resident.apply(crate::ResidencyRequest::Boot).unwrap();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let sound = sounds
        .records_mut()
        .construct(ProcessSoundRecordConstructionRequest {
            resource: ResidentSoundResource::capture(&resident, 2),
            kind: ProcessSoundRecordKind::Persistent,
            volume: 65536,
            pitch: 32768,
            control_flag: 0,
            position: || Ok::<_, &'static str>([0; 3]),
        })
        .unwrap();
    let mut compared = 0;
    let mut unavailable_helper_faults = 0;
    for (kind, text, count) in [
        (
            "entry",
            include_str!("../../../../../tests/fixtures/family_dying_entry_native.json"),
            42,
        ),
        (
            "update",
            include_str!("../../../../../tests/fixtures/family_dying_update_native.json"),
            39,
        ),
        (
            "stage",
            include_str!("../../../../../tests/fixtures/family_dying_stage_native.json"),
            35,
        ),
    ] {
        let document: Value = serde_json::from_str(text).unwrap();
        assert_eq!(document["provenance"]["fpcw"], json!(0x027f));
        let cases = document["cases"].as_array().unwrap();
        assert_eq!(cases.len(), count);
        for (index, case) in cases.iter().enumerate() {
            let input = &case["input"];
            // Native helper allocator/destructor interruption and arbitrary
            // destructor mutations have no counterpart in Box-owned release.
            if matches!(
                input["stop"].as_str(),
                Some("helper_destroy" | "helper_disable")
            ) || input["after"].get("helper_destroy").is_some()
            {
                assert!(case["events"].as_array().unwrap().iter().any(|e| matches!(
                    e["kind"].as_str(),
                    Some("helper_destroy" | "helper_disable")
                )));
                unavailable_helper_faults += 1;
                continue;
            }
            let (mut arena, actor, child) = fixture();
            let null = Cell::new(n(&input["process_null"]));
            let mut rng = n(&input["rng"]);
            supply(&mut arena, actor, input, &mut rng, &null, sound);
            let mut host = Host {
                input,
                events: case["events"].as_array().unwrap(),
                cursor: 0,
                actor,
                child,
                null,
                sound,
                last_rng: rng,
                executable,
                sine,
            };
            assert_eq!(
                snapshot(&arena, actor, rng, host.null.get()),
                project(case["before"].clone()),
                "{kind}{index} initial"
            );
            let request = match kind {
                "entry" => FamilyDyingRequest::Change {
                    entering: n(&input["entering"]),
                },
                "stage" => FamilyDyingRequest::Stage {
                    entering: n(&input["entering"]),
                },
                _ => FamilyDyingRequest::Update {
                    microseconds: n(&input["microseconds"]),
                },
            };
            let result = arena.run_family_dying(actor, request, &mut host, &mut rng);
            assert_eq!(
                result.is_ok(),
                case["normal_return"].as_bool().unwrap(),
                "{kind}{index}: {result:?}"
            );
            assert_eq!(
                snapshot(&arena, actor, rng, host.null.get()),
                project(case["output"].clone()),
                "{kind}{index} output"
            );
            assert!(
                host.events[host.cursor..].iter().all(|e| matches!(
                    e["kind"].as_str(),
                    Some("rng" | "helper_destroy" | "helper_disable")
                )),
                "{kind}{index} unconsumed callbacks"
            );
            compared += 1;
        }
    }
    assert_eq!((compared, unavailable_helper_faults), (111, 5));
}

#[test]
fn family_dying_complete_native_bodies_and_selector10_dispatch_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let bytes = assets.executable_bytes();
    let executable = inf_formats::FrontendExecutable::parse(bytes).unwrap();
    for (offset, target) in [(0x120, 0x425840), (0x11c, 0x424590), (0x138, 0x424080)] {
        assert_eq!(executable.word(0x520cc8 + offset).unwrap(), target);
    }
    for (start, end, digest) in [
        (
            0x24080,
            0x2427c,
            "244e45fe1eb75c636c61f8a64db6a488c04c08e97d56a0c2552403f1e540e49c",
        ),
        (
            0x24280,
            0x2458d,
            "42a0b01ac163bc3887bd1dfe7f166c59b68821b81e3cfcb4bafec7a53177798c",
        ),
        (
            0x24590,
            0x24783,
            "383d5aa22c815b6d9164f04a26c42c5d0cc7fdfbf85c97df3e857e285018d2d1",
        ),
        (
            0x25840,
            0x25868,
            "2f8254fa7694a28831ceee2f1cc8a62ccb7398e9d251bfce1efa48991da58162",
        ),
    ] {
        assert_eq!(format!("{:x}", Sha256::digest(&bytes[start..end])), digest);
    }
}
