use super::*;
use crate::object_runtime::{
    actor::ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};
use inf_formats::native_basis::NativeBasisMath;
use serde_json::Value;

const UNUSED_SINE: [u16; 4096] = [0; 4096];
const NULL: u32 = 0x1234;

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

fn words(value: &Value) -> Vec<u32> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|word| word.as_u64().unwrap() as u32)
        .collect()
}

fn triple(value: &Value) -> [u16; 3] {
    let words = words(value);
    [words[0] as u16, words[1] as u16, words[2] as u16]
}

/// `check_eye_strike.py`: `00407BB0` over 96 eyes and partners, the tip
/// (`+D8`) and the partner's frame (`0047A660`) run natively. The contact
/// pass admits only a partner whose `+68` answers and an eye from state 2
/// on; the rest is `selector77_struck` over the port's tip.
#[test]
fn a_thrown_weapon_at_the_tip_marks_the_eye_as_00407bb0() {
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../../../../tests/fixtures/eye_strike_native.json"
    ))
    .unwrap();
    let math = retail_math();
    let cases = fixture["strike"].as_array().unwrap();
    assert_eq!(cases.len(), 96);
    let mut marked = 0;
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(NULL),
        })
        .unwrap();
        let eye = arena
            .allocate_selector79_actor(
                DerivedActorConstructionRequest {
                    actor: request(triple(&input["position"])),
                    world_mask_limits: [3, 3],
                },
                &mut 1,
            )
            .unwrap();
        let basis = words(&input["basis"]);
        arena.actor_mut(eye).unwrap().rotating.basis.words = std::array::from_fn(|w| basis[w]);
        arena.actor_mut(eye).unwrap().word_c4 = input["state"].as_u64().unwrap() as u32;
        let angles = words(&input["angles"]);
        let root = triple(&input["root"]);
        let DerivedActorClass::Selector77(state) = &mut arena.derived_actor_mut(eye).unwrap().class
        else {
            unreachable!()
        };
        state.words_184_190 = std::array::from_fn(|word| angles[word] as u16);
        state.position_192 = Some(root);
        let partner = arena
            .allocate_actor(request(triple(&input["partner_position"])))
            .unwrap();
        let partner_basis = words(&input["partner_basis"]);
        let partner_state = arena.actor_mut(partner).unwrap();
        partner_state.rotating.basis.words = std::array::from_fn(|w| partner_basis[w]);
        partner_state.owner_handle =
            ActorHandle::from_raw(input["partner_owner"].as_u64().unwrap() as u32);
        let null = arena.selector77_actor(eye).unwrap().handle_1b0;
        if input["answers"].as_bool().unwrap() && arena.actor(eye).unwrap().word_c4 >= 2 {
            let length = input["stalk"]
                .as_bool()
                .unwrap()
                .then(|| input["stalk_length"].as_u64().unwrap() as u16);
            let tip = arena.selector77_tip(eye, root, length, &math).unwrap();
            arena.selector77_struck(eye, partner, tip).unwrap();
        }
        let after = arena.selector77_actor(eye).unwrap().handle_1b0;
        let expected = case["output"]["marked"].as_u64().map(|raw| raw as u32);
        assert_eq!(
            (after != null).then_some(after.raw()),
            expected,
            "case {index}"
        );
        marked += usize::from(expected.is_some());
    }
    assert_eq!(marked, 19);
}
