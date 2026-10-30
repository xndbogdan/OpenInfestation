use super::*;
use crate::object_runtime::{
    actor::ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorClass, DerivedActorConstructionRequest,
};
use serde_json::{json, Value};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn request(position: [u16; 3]) -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some(position),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &UNUSED_SINE,
        },
    }
}

fn retail_math() -> NativeBasisMath {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    NativeBasisMath::new(
        inf_formats::parse_frontend_sine_table(executable.data(), 0x53df00)
            .unwrap()
            .values,
    )
}

/// The receipt's recorded services.
struct Recorder<'a> {
    math: &'a NativeBasisMath,
    stalk: Option<u16>,
    child: bool,
    events: Vec<Value>,
}

impl EyeDeathHost for Recorder<'_> {
    type Error = std::convert::Infallible;

    fn burst(
        &mut self,
        kind: u32,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), Self::Error> {
        self.events
            .push(json!({"kind": "burst", "type": kind, "position": position,
            "velocity": velocity, "flags": 0}));
        Ok(())
    }

    fn stalk_length(&mut self) -> Result<Option<u16>, Self::Error> {
        Ok(self.stalk)
    }

    fn sound(&mut self, position: [u16; 3]) -> Result<(), Self::Error> {
        self.events
            .push(json!({"kind": "sound", "sound": 0x28, "position": position,
            "args": [0x10000, 0x10000]}));
        self.events
            .push(json!({"kind": "sound_done", "record": true}));
        Ok(())
    }

    fn child(
        &mut self,
        arena: &mut ObjectArena,
        _: ObjectId,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, Self::Error> {
        self.events.push(
            json!({"kind": "child", "parent": true, "resource": EYE_DEATH_CHILD,
            "position": position}),
        );
        Ok(self
            .child
            .then(|| arena.allocate_actor(request(position)).unwrap()))
    }

    fn basis_math(&self) -> &NativeBasisMath {
        self.math
    }
}

/// `00408560` against the 48 eyes of `eye_death_native.json`, run on the
/// original instructions with the eye's own `+D8` and `+4C`, the real
/// square root, rotations, transform and sine table: every burst down the
/// stalk, the sound and the child at the tip, the child's turned basis and
/// flag, and the disposal.
#[test]
fn the_eyes_death_matches_the_native_receipt() {
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../../../../tests/fixtures/eye_death_native.json"
    ))
    .unwrap();
    let math = retail_math();
    let number = |value: &Value| value.as_u64().unwrap();
    let words = |value: &Value| -> Vec<u32> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|word| number(word) as u32)
            .collect()
    };
    let cases = fixture["death"].as_array().unwrap();
    assert_eq!(cases.len(), 48);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0x1234),
        })
        .unwrap();
        let position = words(&input["position"]);
        let position = [position[0] as u16, position[1] as u16, position[2] as u16];
        let eye = arena
            .allocate_selector79_actor(
                DerivedActorConstructionRequest {
                    actor: request(position),
                    world_mask_limits: [3, 3],
                },
                &mut 1,
            )
            .unwrap();
        let basis = words(&input["basis"]);
        arena.actor_mut(eye).unwrap().rotating.basis.words =
            std::array::from_fn(|word| basis[word]);
        let angles = words(&input["angles"]);
        let root = words(&input["root"]);
        let DerivedActorClass::Selector77(state) = &mut arena.derived_actor_mut(eye).unwrap().class
        else {
            unreachable!()
        };
        state.words_184_190 = std::array::from_fn(|word| angles[word] as u16);
        state.position_192 = Some([root[0] as u16, root[1] as u16, root[2] as u16]);
        let mut host = Recorder {
            math: &math,
            stalk: input["stalk"]
                .as_bool()
                .unwrap()
                .then(|| number(&input["stalk_length"]) as u16),
            child: input["child"].as_bool().unwrap(),
            events: Vec::new(),
        };
        let children = arena.actor_list().len();
        arena.retire_selector77(eye, &mut host).unwrap();
        let mut expected: Vec<Value> = case["events"].as_array().unwrap().clone();
        assert_eq!(expected.pop().unwrap()["kind"], "dispose", "case {index}");
        assert_eq!(host.events, expected, "case {index}");
        assert_eq!(
            arena.object(eye).unwrap().base.flags & 0x0000_0004,
            0,
            "case {index}"
        );
        let output = &case["output"];
        if host.child {
            let child = arena.actor_list()[children];
            assert_eq!(
                arena.actor(child).unwrap().rotating.basis.words.to_vec(),
                words(&output["child_basis"]),
                "case {index}"
            );
            assert_ne!(
                arena.object(child).unwrap().base.flags & 0x40_0000,
                0,
                "case {index}"
            );
        }
    }
}
