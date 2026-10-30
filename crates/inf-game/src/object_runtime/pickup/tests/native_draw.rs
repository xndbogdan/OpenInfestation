use super::*;
use crate::{
    frontend_scene::SceneView,
    menu::MenuBasis,
    object_runtime::{ActorHandle, ObjectDrawContext, ObjectModelPacket, PlayerId},
};
use serde_json::{json, Value};
struct Host<'a> {
    input: &'a Value,
    events: Vec<Value>,
}
fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
impl PickupDrawHost for Host<'_> {
    type Error = &'static str;
    fn network_flag(&mut self) -> u32 {
        0
    }
    fn player_for_handle(
        &mut self,
        _: &ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<PlayerId>, Self::Error> {
        panic!("empty actual player list")
    }
    fn grant_pickup(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: PlayerId,
        _: &mut u32,
    ) -> Result<bool, Self::Error> {
        panic!()
    }
    fn collection_effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: PickupCollectionEffect,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        panic!()
    }
    fn base_color(&mut self) -> u32 {
        u(&self.input["color"])
    }
    fn animation_clock(&mut self) -> u32 {
        u(&self.input["clock"])
    }
    fn emit_trail(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        basis: &MenuBasis,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push(json!({"kind":"trail","position":arena.object(object).unwrap().base.position_words.unwrap(),"velocity":[0,65504,0],"basis":basis.words,"strength":200}));
        if self.input["stop"] == "trail" {
            return Err("trail");
        }
        if self.input["mutate"] == true {
            arena.owned_mut(object).unwrap().state.base.position_words = Some([8, 9, 10]);
            arena.timed_pickup_mut(object).unwrap().visual.model_word = 777;
        }
        Ok(())
    }
    fn model(
        &mut self,
        _: &mut ObjectArena,
        packet: ObjectModelPacket,
        color: u32,
        _: &ObjectDrawContext,
    ) -> Result<(), Self::Error> {
        self.events.push(json!({"kind":"color","color":color}));
        self.events.push(json!({"kind":"model","position":packet.position,"model":packet.model_slot as u16,"basis":packet.basis.words}));
        if self.input["stop"] == "model" {
            Err("model")
        } else {
            Ok(())
        }
    }
}
#[test]
fn pickup_draw_matches_native_pulse_basis_captured_packets_and_failure_order() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/pickup_object_draw_native.json"
    ))
    .unwrap();
    let bytes = raw();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let table = exe.mapped(0x53df00, 8192).unwrap().bytes;
    let sine = std::array::from_fn(|i| u16::from_le_bytes([table[i * 2], table[i * 2 + 1]]));
    for (index, row) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &row["input"];
        let mut arena = ObjectArena::default();
        let object = arena.allocate(ObjectClassKind::TimedPickup);
        arena
            .initialize_context(
                object,
                BaseContext {
                    position_words: Some(std::array::from_fn(|i| u(&input["position"][i]) as u16)),
                    placement_index: u32::MAX,
                },
            )
            .unwrap();
        arena.set_flag0(object, true).unwrap();
        let pickup = arena.timed_pickup_mut(object).unwrap();
        pickup.kind_44 = Some(u(&input["kind"]) as u16);
        pickup.visual.model_word = u(&input["model"]) as u16;
        let mut process = ObjectDrawProcessState::native_startup(exe).unwrap();
        process.pickup_basis.words = std::array::from_fn(|i| u(&input["basis"][i]));
        let mut context = ObjectDrawContext {
            view: SceneView {
                eye: [0; 3],
                basis: MenuBasis::INITIAL,
            },
            near: 0,
            far: 1000,
            far_color: 0,
        };
        let mut host = Host {
            input,
            events: vec![],
        };
        let result = arena.draw_timed_pickup(
            object,
            PickupDrawRequest {
                context: &mut context,
                process: &mut process,
                sine_quarter: &sine,
            },
            &mut host,
            &mut 7,
        );
        if u(&input["model"]) == 0xffff {
            host.events.push(json!({"kind":"flag0","value":0}));
            assert_eq!(arena.object(object).unwrap().base.flags & 1, 0);
        }
        assert_eq!(
            result.is_ok(),
            row["normal_return"].as_bool().unwrap(),
            "case{index}: {result:?}"
        );
        assert_eq!(json!(host.events), row["events"], "case{index}");
        assert_eq!(
            json!(arena.object(object).unwrap().base.position_words.unwrap()),
            row["position"],
            "case{index}"
        );
        assert_eq!(
            arena.timed_pickup(object).unwrap().visual.model_word,
            u(&row["model"]) as u16,
            "case{index}"
        );
    }
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 13);
}
