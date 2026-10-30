use super::*;
use serde_json::Value;

fn word(v: &Value) -> u32 {
    v.as_u64()
        .map(|v| v as u32)
        .unwrap_or_else(|| v.as_i64().unwrap() as u32)
}
#[test]
fn combat_auxiliary_box_dispatch_matches_native_alignment_slot_reset_and_contact() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/combat_auxiliary_box_dispatch_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let runtime = resident();
    let binding = source(&runtime, 223);
    let qualification = qualification(&binding);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let mut bytes = vec![0x8f, 0xa5, 0xa5, 0xa5];
        for i in 0..3 {
            bytes.extend_from_slice(&word(&input["extents"][i]).to_le_bytes());
        }
        bytes.extend_from_slice(&(word(&input["slot"]) as u16).to_le_bytes());
        bytes.push(0x88);
        let program = inf_formats::decode_model_auxiliary(&bytes).unwrap();
        assert_eq!(
            program.terminator.start,
            word(&case["next_offset"]) as usize
        );
        let mut model = binding.model.clone();
        model.header.vertex_slot_count = 32;
        model.auxiliary_program = Some(program);
        let (owners, mut process) = owners(true);
        //Native receipt starts with supplied current scratch vertices. Seed
        //retained pool bytes then retire normally; interpreter reallocates the
        //same slice without clearing it, exactly as the scratch owner promises.
        let scope = owners.scratch.begin_draw().unwrap();
        let allocation = owners.scratch.allocate(&scope, 32, 2).unwrap();
        for slot in 0..32 {
            let center: [i32; 3] = if slot == word(&input["slot"]) as u16 {
                std::array::from_fn(|i| word(&input["center"][i]) as i32)
            } else {
                [100000; 3]
            };
            owners
                .scratch
                .write_vertex(
                    &allocation,
                    slot,
                    0,
                    std::array::from_fn::<_, 12, _>(|i| Some(center[i / 4].to_le_bytes()[i % 4])),
                )
                .unwrap();
        }
        owners.scratch.retire(&scope, allocation.mark()).unwrap();
        owners
            .scratch
            .finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
            .unwrap();
        let mut contact = CollisionContact::from_native_words(std::array::from_fn(|i| {
            Some(word(&input["initial"][i]))
        }));
        query_auxiliary_direct(
            AuxiliaryDirectRequest {
                model: AuxiliaryModelSource {
                    model: &model,
                    qualification: &qualification,
                },
                basis: MenuBasis::INITIAL,
                relative_center: std::array::from_fn(|i| word(&input["query"][i]) as i32),
                shape: AuxiliaryQueryShape::Sphere {
                    radius: word(&input["radius"]) as i32,
                },
                values: AuxiliaryValueSource::Required,
                residency: Some(&runtime),
            },
            &mut process,
            &owners,
            &mut contact,
        )
        .unwrap();
        assert_eq!(
            contact.native_words(),
            std::array::from_fn(|i| Some(word(&case["output"][i]))),
            "case{index}"
        );
        assert_eq!(
            owners.scratch.state().unwrap(),
            ModelScratchDrawState::Ready
        );
    }
}
