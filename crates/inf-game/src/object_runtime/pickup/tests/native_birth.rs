use super::*;
use crate::object_runtime::{ActorHandle, TargetNullHandleSource};
use serde_json::{json, Value};
struct Host<'a> {
    input: &'a Value,
    events: Vec<Value>,
    object: Option<ObjectId>,
    ground: usize,
}
fn u(v: &Value) -> u32 {
    v.as_i64().unwrap() as u32
}
fn xyz(v: &Value) -> [u16; 3] {
    std::array::from_fn(|i| u(&v[i]) as u16)
}
impl TargetNullHandleSource for Host<'_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(u(&self.input["source"])))
    }
}
impl PickupBirthHost for Host<'_> {
    type Error = &'static str;
    fn create_pickup(
        &mut self,
        arena: &mut ObjectArena,
        resource: u32,
        position: &mut [u16; 3],
        source: ActorHandle,
        _: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error> {
        self.events.push(json!({"kind":"factory","resource":resource,"source":source.raw(),"position":*position}));
        if self.input["stop"] == "factory" {
            return Err("factory");
        }
        if self.input["factory_null"] == true {
            if !self.input["caller_after"].is_null() {
                *position = xyz(&self.input["caller_after"]);
            }
            return Ok(None);
        }
        let object = arena.allocate(ObjectClassKind::TimedPickup);
        arena
            .initialize_context(
                object,
                BaseContext {
                    position_words: Some(*position),
                    placement_index: u32::MAX,
                },
            )
            .unwrap();
        arena.owned_mut(object).unwrap().state.base.dirty_word = 0;
        if !self.input["caller_after"].is_null() {
            *position = xyz(&self.input["caller_after"]);
        }
        if !self.input["object_after"].is_null() {
            arena.owned_mut(object).unwrap().state.base.position_words =
                Some(xyz(&self.input["object_after"]));
        }
        self.object = Some(object);
        Ok(Some(object))
    }
    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
    ) -> Result<(), Self::Error> {
        self.events.push(
            json!({"kind":"refresh","model":arena.timed_pickup(object).unwrap().visual.model_word}),
        );
        if self.input["stop"] == "refresh" {
            Err("refresh")
        } else {
            Ok(())
        }
    }
    fn ground_height(&mut self, _: &mut ObjectArena, xz: [u16; 2]) -> Result<u16, Self::Error> {
        self.events.push(json!({"kind":"ground","xz":xz}));
        self.ground += 1;
        if self.input["stop"] == format!("ground{}", self.ground) {
            Err("ground")
        } else {
            Ok(u(&self.input["ground"][(self.ground - 1).min(1)]) as u16)
        }
    }
}
#[test]
fn pickup_object_birth_matches_native_callback_order_fields_and_failure_prefixes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/pickup_object_birth_native.json"
    ))
    .unwrap();
    for (index, row) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &row["input"];
        let mut arena = ObjectArena::default();
        let mut host = Host {
            input,
            events: vec![],
            object: None,
            ground: 0,
        };
        let mut position = xyz(&input["position"]);
        let result = arena.birth_timed_pickup(
            PickupBirthRequest {
                position: &mut position,
                kind: u(&input["kind"]),
                value: u(&input["value"]) as u16,
            },
            &mut host,
            &mut 7,
        );
        assert_eq!(
            result.is_ok(),
            row["normal_return"].as_bool().unwrap(),
            "case{index}: {result:?}"
        );
        assert_eq!(json!(host.events), row["events"], "case{index}");
        assert_eq!(json!(position), row["output"]["caller"], "case{index}");
        if let Some(object) = host.object {
            let pickup = arena.timed_pickup(object).unwrap();
            let out = &row["output"];
            assert_eq!(
                json!(arena.object(object).unwrap().base.position_words.unwrap()),
                out["position"],
                "case{index}"
            );
            assert_eq!(pickup.kind_44, Some(u(&out["kind"]) as u16), "case{index}");
            assert_eq!(pickup.value_46, u(&out["value"]) as u16, "case{index}");
            assert_eq!(
                pickup.visual.model_word,
                u(&out["model"]) as u16,
                "case{index}"
            );
            assert_eq!(
                arena.object(object).unwrap().base.dirty_word,
                u(&out["dirty"]) as u16,
                "case{index}"
            );
        }
    }
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 19);
}
