use super::*;
use serde_json::{json, Value};

fn snapshot(cache: &TerrainProbeCache) -> Value {
    json!({
        "origin": [cache.word_04(), cache.word_06()],
        "dimension": cache.dimension(),
        "stride": cache.stride_words(),
        "words": cache.words(),
    })
}

fn assert_snapshot(cache: &TerrainProbeCache, expected: &Value, label: &str) {
    let mut expected = expected.clone();
    let unknown = expected
        .as_object_mut()
        .unwrap()
        .remove("unknown_header")
        .unwrap();
    // Supplied poison is retained by native code. The port keeps its custody
    // unknown; it must never adopt the poison (or zero) as a header policy.
    assert_eq!(unknown, json!([165, 165]), "native header custody {label}");
    assert_eq!(cache.byte_08(), None, "unknown +08 {label}");
    assert_eq!(cache.byte_0b(), None, "unknown +0B {label}");
    assert_eq!(snapshot(cache), expected, "{label}");
}

fn mutate(cache: &mut TerrainProbeCache, mutation: &Value) {
    if let Some(input) = mutation["replace_input"].as_u64() {
        *cache = TerrainProbeCache::new(input as u8);
    }
    if let Some(origin) = mutation["origin"].as_array() {
        cache.word_04 = origin[0].as_u64().unwrap() as u16;
        cache.word_06 = origin[1].as_u64().unwrap() as u16;
    }
    if let Some(words) = mutation["words"].as_array() {
        assert_eq!(words.len(), cache.words_00.len());
        for (destination, source) in cache.words_00.iter_mut().zip(words) {
            *destination = source.as_u64().unwrap() as u32;
        }
    }
    if mutation["clear"].as_bool().unwrap_or(false) {
        cache.clear_samples();
    }
}

#[test]
fn original_cache_sequences_preserve_masked_shifts_host_order_and_encoded_spill() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/terrain_probe_access_native.json"
    )))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 137);
    let mut receipt_count = 0;
    for case in cases {
        let input = &case["input"];
        let mut cache = TerrainProbeCache::new(input["input"].as_u64().unwrap() as u8);
        let name = input["name"].as_str().unwrap();
        assert_snapshot(&cache, &case["constructor"], name);
        mutate(&mut cache, input);
        for (ordinal, expected) in case["receipts"].as_array().unwrap().iter().enumerate() {
            receipt_count += 1;
            let step = &expected["input"];
            let position_words =
                std::array::from_fn(|axis| step["position"][axis].as_u64().unwrap() as u16);
            let mut events = vec![];
            let result = cache.probe(position_words, |live_cache, actual_words| {
                events.push(json!({"position": actual_words, "before": snapshot(live_cache)}));
                mutate(live_cache, &step["mutation"]);
                if step["fail"].as_bool().unwrap_or(false) {
                    Err("supplied miss failure")
                } else {
                    Ok(step["sample"].as_u64().unwrap_or(0) as u32)
                }
            });
            let (value, error) = match result {
                Ok(value) => (Some(value), None),
                Err(TerrainProbeError::Host(_)) => (None, Some("host")),
                Err(TerrainProbeError::StorageRange { .. }) => (None, Some("storage")),
            };
            assert_eq!(json!(value), expected["result"], "result {name}:{ordinal}");
            assert_eq!(json!(error), expected["error"], "error {name}:{ordinal}");
            let mut native_events = expected["events"].clone();
            for event in native_events.as_array_mut().unwrap() {
                let poison = event["before"]
                    .as_object_mut()
                    .unwrap()
                    .remove("unknown_header")
                    .unwrap();
                assert_eq!(poison, json!([165, 165]));
            }
            assert_eq!(
                json!(events),
                native_events,
                "live miss custody {name}:{ordinal}"
            );
            assert_snapshot(
                &cache,
                &expected["state"],
                &format!("state {name}:{ordinal}"),
            );
        }
    }
    assert_eq!(receipt_count, 350);
}
