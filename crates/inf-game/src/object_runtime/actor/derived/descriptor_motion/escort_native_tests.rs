//! Native 00428990 escort receipts through the actual score wrappers. The
//! receipt's supplied 0047AD90 result maps to a live referent or a handle
//! that this registry cannot resolve; native handle values are not reused.
use super::*;

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|axis| word(&value[axis]) as u16)
}

#[test]
fn escort_source_matches_original_wrappers_and_miss_writes() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/family_escort_source_native.json"
    ))
    .unwrap();
    let null = word(&receipt["process_null"]);
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 30);
    for case in cases {
        let name = format!("{} {}", case["profile"], case["name"]);
        let (profile, kind, offset) = match case["profile"].as_str().unwrap() {
            "selector35" => (
                FamilyBehaviorProfile::Selector35,
                FamilyDescriptorScoreKind::Source428990,
                0x10,
            ),
            "selector34" => (
                FamilyBehaviorProfile::Selector34,
                FamilyDescriptorScoreKind::Source428990Selector34,
                0x0e,
            ),
            other => panic!("unknown escort profile {other}"),
        };
        assert_eq!(kind.address(), word(&case["callback"]), "{name}");
        let (mut arena, actor) = fixture(profile);
        assert_eq!(arena.actor_handles().unwrap().null_handle().raw(), null);
        let referent = arena.allocate_selector35_actor(request(), &mut 0).unwrap();
        arena.actor_mut(referent).unwrap().word_c4 = word(&case["lifecycle"]);
        arena.owned_mut(referent).unwrap().state.base.position_words =
            Some(words(&case["referent_position"]));
        arena.owned_mut(actor).unwrap().state.base.position_words = Some(words(&case["position"]));
        arena.derived_actor_mut(actor).unwrap().word_164 = word(&case["radius"]) as i32;
        let mut bytes = vec![0xa5; 0x20];
        bytes[offset as usize..offset as usize + 2]
            .copy_from_slice(&(word(&case["coefficient"]) as u16).to_le_bytes());
        retain(&mut arena, actor, profile, &allocation(bytes), 0);
        let handle = if word(&case["handle"]) == null {
            null
        } else if case["resolved"] == true {
            arena.actor(referent).unwrap().handle.raw()
        } else {
            let stale = ActorHandle::from_raw(word(&case["handle"]));
            assert_eq!(arena.resolve_actor_handle(stale), Ok(None), "{name}");
            stale.raw()
        };
        arena.family428650_actor_mut(actor).unwrap().word_20c = handle;
        arena.family428650_actor_mut(actor).unwrap().word_210 = 0x5555;
        let result = arena.score_family_descriptor_in_process(
            actor,
            FamilyDescriptorScoreRequest {
                kind,
                current: word(&case["current"]) != 0,
                bias: case["bias"].as_i64().unwrap() as i32,
            },
            FamilyDescriptorScoreRng::RequiredAtReach,
            Some(ActorHandle::from_raw(null)),
            None,
        );
        assert_eq!(
            result,
            Ok(case["result"].as_i64().unwrap() as i32),
            "{name}"
        );
        let family = arena.family428650_actor(actor).unwrap();
        let expected_20c = if word(&case["final_20c"]) == null {
            null
        } else {
            handle
        };
        assert_eq!(
            (family.word_20c, family.word_210),
            (expected_20c, word(&case["final_210"])),
            "{name}"
        );
        // A nonzero coefficient reaches 0047AD90 exactly when +20C is not NULL.
        let requests = case["resolve_requests"].as_array().unwrap().len();
        assert_eq!(
            requests,
            usize::from(word(&case["coefficient"]) != 0 && word(&case["handle"]) != null),
            "{name}"
        );
    }
}

#[test]
fn escort_source_without_process_null_stops_after_the_coefficient_read() {
    let (mut arena, actor) = fixture(FamilyBehaviorProfile::Selector35);
    let bytes = vec![1; 0x20];
    retain(
        &mut arena,
        actor,
        FamilyBehaviorProfile::Selector35,
        &allocation(bytes),
        0,
    );
    let before = format!("{arena:?}");
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            FamilyDescriptorScoreRequest {
                kind: FamilyDescriptorScoreKind::Source428990,
                current: false,
                bias: 0,
            },
            FamilyDescriptorScoreRng::RequiredAtReach,
        ),
        Err(FamilyDescriptorScoreError::MissingProcessNull {
            callback: 0x0043_fce0,
            address: 0x0042_8990,
        })
    );
    assert_eq!(format!("{arena:?}"), before);
}
