use super::*;
use serde_json::Value;

#[test]
fn combat_auxiliary_actor_getter_matches_native_wrappers_clock_and_captured_source() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/combat_auxiliary_actor_getter_native.json"
    ))
    .unwrap();
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let mut objects = arena();
        let mut actors = Vec::new();
        for field in ["words", "alternate_words"] {
            let actor = objects
                .allocate_selector36_actor(derived_request(), &mut 17)
                .unwrap();
            let words: [u16; 7] = std::array::from_fn(|i| input[field][i].as_u64().unwrap() as u16);
            family423_mut(&mut objects, actor).word_266 = words[0];
            let state = selector36_mut(&mut objects, actor);
            state.word_268 = u32::from(words[1]) | (u32::from(words[2]) << 16);
            state.word_26c = u32::from(words[3]) | (u32::from(words[4]) << 16);
            state.word_270 = u32::from(words[5]) | (u32::from(words[6]) << 16);
            actors.push(actor);
        }
        let selector = input["selector"].as_u64().unwrap() as u32;
        let source = if selector == 0 {
            arena().allocate(ObjectClassKind::Base)
        } else {
            actors[usize::from(input["alternate"].as_bool().unwrap())]
        };
        let mut clock = AnimationClock::default();
        clock.advance(input["clock"].as_u64().unwrap() as u32 * 20_000);
        let callbacks = ObjectModelCallbacks {
            arena: &objects,
            source,
            animation: &clock,
            executable,
        };
        // Both original wrappers use this same captured actor through children.
        // The +4/+3C ABI distinction is authenticated by the native fixture.
        for wrapper in case["outputs"].as_array().unwrap() {
            assert_eq!(
                callbacks.packed_value(selector).unwrap(),
                wrapper["word"].as_u64().unwrap() as u16,
                "case{index} entry{}",
                wrapper["entry"]
            );
        }
    }
}
