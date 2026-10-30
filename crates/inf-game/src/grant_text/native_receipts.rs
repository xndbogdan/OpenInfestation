//! Actual fixed formatter parity. The sibling audit owns original-byte/native
//! cadence and masked support state; this module compares complete typed results
//! and supplied current/source custody, with no Rust observer or pointer map.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{
    format_grant_text, native_evidence::load_and_validate_fixture, FormattedGrantText,
    GrantTextData, GrantTextNullBinding, GrantTextPairClassification, GrantTextRequest,
    GrantTextSource,
};

const DIAGNOSTICS: [&str; 3] = [
    "diagnostic-null-substitute-dword-unavailable",
    "diagnostic-pair-ctype-pointer-unavailable",
    "diagnostic-unknown-source-byte-before-nul",
];
const ADMITTED: [&str; 9] = [
    "current-null-substitute-binding-change-between-requests",
    "pair-both-null-separate-current-reads",
    "pair-empty-operands-still-classifies-literals",
    "pair-exact191-including-separators",
    "pair-known-category-and-raw-text",
    "single-empty-bypasses-unavailable-globals",
    "single-exact191-output",
    "single-null-current-substitute",
    "single-raw-percent-and-high-bytes",
];

type MaskedInputs = BTreeMap<String, Vec<Option<u8>>>;

fn array(value: &Value) -> &[Value] {
    value.as_array().expect("fixture array")
}

fn mask(value: &Value) -> Vec<Option<u8>> {
    array(value)
        .iter()
        .map(|byte| {
            if byte.is_null() {
                None
            } else {
                Some(u8::try_from(byte.as_u64().expect("known byte")).unwrap())
            }
        })
        .collect()
}

fn complete_record(bytes: &[Option<u8>]) -> Vec<u8> {
    let mut record = Vec::new();
    for byte in bytes {
        let byte = byte.expect("admitted record is known through first NUL");
        record.push(byte);
        if byte == 0 {
            return record;
        }
    }
    panic!("admitted record has a first NUL")
}

fn optional_word(bytes: &[Option<u8>]) -> Option<u32> {
    assert_eq!(bytes.len(), 4);
    if bytes.iter().all(Option::is_none) {
        return None;
    }
    Some(u32::from_le_bytes(std::array::from_fn(|index| {
        bytes[index].expect("partial pointer diagnostics excluded before typed setup")
    })))
}

/// Named caller-held input allocations. The alternate record is declared setup
/// for the later binding patch, initially unbound; its native unwritten storage
/// is audited separately and has no fabricated portable mask-parity claim.
#[derive(Clone, Debug, PartialEq, Eq)]
struct InputRecords {
    first: Vec<u8>,
    second: Vec<u8>,
    startup_null: Vec<u8>,
    alternate_null: Option<Vec<u8>>,
}

impl InputRecords {
    fn from_case(case: &Value) -> Self {
        let alternate_patches: Vec<_> = array(&case["input"]["steps"])
            .iter()
            .flat_map(|request| array(&request["patches"]))
            .filter(|patch| patch["region"] == "alternate_null")
            .collect();
        assert!(alternate_patches.len() <= 1);
        let alternate_null = alternate_patches.first().map(|patch| {
            assert_eq!(patch["offset"], 0);
            complete_record(&mask(&patch["bytes"]))
        });
        Self {
            first: complete_record(&mask(&case["initial_state"]["first"])),
            second: complete_record(&mask(&case["initial_state"]["second"])),
            startup_null: complete_record(&mask(&case["initial_state"]["null_string"])),
            alternate_null,
        }
    }

    fn source<'a>(&'a self, argument: &Value) -> GrantTextSource<'a> {
        if argument.is_null() {
            return GrantTextSource::Null;
        }
        let record = match argument.as_str().unwrap() {
            "first" => &self.first,
            "second" => &self.second,
            other => panic!("unadmitted source {other}"),
        };
        GrantTextSource::Record(Some(record))
    }

    fn null_binding(&self, pointer: Option<u32>) -> Option<GrantTextNullBinding<'_>> {
        pointer.map(|pointer| {
            // Native values select only declared fixture allocations. They are
            // never converted into a Rust pointer or passed to the runtime API.
            let record = match pointer {
                0x0052_99f4 => &self.startup_null,
                0x0072_1300 => self
                    .alternate_null
                    .as_ref()
                    .expect("declared later backing"),
                other => panic!("unadmitted substitute binding {other:#x}"),
            };
            GrantTextNullBinding {
                record: Some(record),
            }
        })
    }
}

fn inputs(case: &Value) -> MaskedInputs {
    [
        "first",
        "second",
        "null_string",
        "alternate_null",
        "null_pointer",
        "ctype_pointer",
        "colon_high",
        "space_high",
    ]
    .into_iter()
    .map(|name| (name.to_owned(), mask(&case["initial_state"][name])))
    .collect()
}

fn pair_binding(current: &MaskedInputs) -> Option<GrantTextPairClassification> {
    optional_word(&current["ctype_pointer"]).map(|pointer| {
        assert_eq!(pointer, 0x0054_5c7e);
        assert_eq!(current["colon_high"].len(), 1);
        assert_eq!(current["space_high"].len(), 1);
        GrantTextPairClassification {
            colon_high: current["colon_high"][0],
            space_high: current["space_high"][0],
        }
    })
}

fn apply_patch(current: &mut MaskedInputs, patch: &Value) {
    let region = patch["region"].as_str().unwrap();
    assert!(matches!(region, "alternate_null" | "null_pointer"));
    let offset = usize::try_from(patch["offset"].as_u64().unwrap()).unwrap();
    let bytes = mask(&patch["bytes"]);
    current.get_mut(region).unwrap()[offset..offset + bytes.len()].clone_from_slice(&bytes);
}

fn assert_current_custody(
    data: &GrantTextData<'_>,
    records: &InputRecords,
    current: &MaskedInputs,
    receipt: &Value,
) {
    assert_eq!(
        data.null_substitute,
        records.null_binding(optional_word(&current["null_pointer"]))
    );
    assert_eq!(data.pair_classification, pair_binding(current));
    assert_eq!(
        complete_record(&current["first"]),
        records.first,
        "real input record remains unchanged"
    );
    assert_eq!(complete_record(&current["second"]), records.second);
    assert_eq!(
        complete_record(&current["null_string"]),
        records.startup_null
    );
    for (name, bytes) in current {
        // Only supplied input regions are compared here, never native output
        // or stack snapshots applied to portable state.
        assert_eq!(*bytes, mask(&receipt["state"][name]), "{name} custody");
    }
}

#[test]
fn fixed_grant_text_matches_original_completed_histories() {
    // This all12/13 audit runs before any typed source/data construction, and
    // owns raw diagnostics/support evidence independently of these actual calls.
    let Some(fixture) = load_and_validate_fixture() else {
        return;
    };
    let cases = array(&fixture["cases"]);
    assert_eq!(cases.len(), 12);
    let mut skipped = BTreeSet::new();
    let mut admitted = BTreeSet::new();
    let mut compared_calls = 0;
    let mut changed_calls = 0;

    for case in cases {
        let name = case["input"]["name"].as_str().unwrap();
        if DIAGNOSTICS.contains(&name) {
            assert_eq!(case["evidence"], "missing_data_diagnostic");
            assert!(skipped.insert(name));
            continue;
        }
        assert!(
            ADMITTED.contains(&name),
            "unexpected typed admission {name}"
        );
        assert!(admitted.insert(name));
        let changed = name == "current-null-substitute-binding-change-between-requests";
        assert_eq!(
            case["evidence"],
            if changed {
                "bounded_current_binding_change"
            } else {
                "bounded_original_body"
            }
        );

        let records = InputRecords::from_case(case);
        let mut current = inputs(case);
        let mut data = GrantTextData {
            null_substitute: records.null_binding(optional_word(&current["null_pointer"])),
            pair_classification: pair_binding(&current),
        };
        let requests = array(&case["input"]["steps"]);
        let receipts = array(&case["steps"]);
        assert_eq!(requests.len(), receipts.len());
        assert_eq!(requests.len(), if changed { 2 } else { 1 });
        let mut completed: Vec<FormattedGrantText> = Vec::new();

        for (step, (request, receipt)) in requests.iter().zip(receipts).enumerate() {
            assert_eq!(receipt["outcome"], "original_body_complete");
            let patches = array(&request["patches"]);
            assert_eq!(patches.len(), if changed && step == 1 { 2 } else { 0 });
            for patch in patches {
                apply_patch(&mut current, patch);
                if patch["region"] == "null_pointer" {
                    // The same carrier changes only at the declared BETWEEN-
                    // request binding store. No synchronous callback is invented.
                    data.null_substitute =
                        records.null_binding(optional_word(&current["null_pointer"]));
                }
            }

            let arguments = array(&request["arguments"]);
            let actual_request = match request["mode"].as_str().unwrap() {
                "single" => {
                    assert_eq!(arguments.len(), 1);
                    GrantTextRequest::Single {
                        record: records.source(&arguments[0]),
                    }
                }
                "pair" => {
                    assert_eq!(arguments.len(), 2);
                    GrantTextRequest::Pair {
                        category: records.source(&arguments[0]),
                        record: records.source(&arguments[1]),
                    }
                }
                other => panic!("unadmitted fixed mode {other}"),
            };
            let before_records = records.clone();
            let before_data = data;
            let before_results: Vec<_> = completed
                .iter()
                .map(|result| result.bytes_with_nul().to_vec())
                .collect();
            // Preserve genuinely unavailable current data. A known nonNULL
            // Single can run without any CRT owner, as its ordinary test proves.
            let data_borrow =
                if data.null_substitute.is_none() && data.pair_classification.is_none() {
                    None
                } else {
                    Some(&data)
                };
            let actual = format_grant_text(actual_request, data_borrow)
                .unwrap_or_else(|error| panic!("{name} step{step}: {error}"));
            let native_length = u32::try_from(receipt["result"].as_u64().unwrap()).unwrap();
            assert!(native_length <= 191);
            let output = mask(&receipt["state"]["output"]);
            let native_complete: Vec<_> = output[..native_length as usize + 1]
                .iter()
                .map(|byte| byte.expect("native initialized complete prefix"))
                .collect();
            assert_eq!(actual.length(), native_length, "{name} step{step}");
            assert_eq!(
                actual.bytes_with_nul(),
                native_complete,
                "{name} step{step}"
            );
            assert_eq!(actual.bytes_with_nul().last(), Some(&0));
            assert_eq!(records, before_records);
            assert_eq!(data, before_data);
            assert_current_custody(&data, &records, &current, receipt);
            assert_eq!(
                completed
                    .iter()
                    .map(|result| result.bytes_with_nul().to_vec())
                    .collect::<Vec<_>>(),
                before_results,
                "earlier owned results survive current-binding replacement"
            );
            completed.push(actual);
            compared_calls += 1;
            changed_calls += usize::from(changed);
        }
        if changed {
            assert_eq!(completed[0].bytes_with_nul(), b"(null)\0");
            assert_eq!(completed[1].bytes_with_nul(), b"<none>\0");
        }
    }
    assert_eq!(skipped, DIAGNOSTICS.into_iter().collect());
    assert_eq!(admitted, ADMITTED.into_iter().collect());
    assert_eq!(compared_calls, 10);
    assert_eq!(changed_calls, 2);
}
