use super::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Receipts {
    executable_sha256: String,
    records: Vec<u32>,
    guards: Vec<(u32, u32, String)>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    operations: Vec<(String, u32)>,
    initial_counts: Vec<u32>,
    initial_nesting: u32,
    callback_returns: Vec<(u32, u32)>,
    stop_body: Option<u32>,
    events: Vec<(String, u32)>,
    results: Vec<Option<u32>>,
    counts: Vec<u32>,
    nesting: u32,
}
struct Host<'a> {
    case: &'a Case,
    events: Vec<(String, u32)>,
}
impl SoundLifecycleHost for Host<'_> {
    type Error = u32;

    fn callback(&mut self, address: u32) -> Result<u32, u32> {
        self.events.push(("callback".into(), address));
        if self.case.stop_body == Some(address) {
            Err(address)
        } else {
            Ok(self
                .case
                .callback_returns
                .iter()
                .find(|(key, _)| *key == address)
                .map_or(0, |(_, result)| *result))
        }
    }

    fn report_error(&mut self, error: u32) -> Result<(), u32> {
        self.events.push(("report".into(), error));
        Ok(())
    }
}

#[test]
fn sound_resource_lifecycle_matches_native_acquisition_unwind_and_release() {
    let original = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&original).unwrap();
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../tests/fixtures/sound_lifecycle_native.json"
    ))
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&original)),
        receipts.executable_sha256
    );
    // This original receipt set covers the first five resources. The record
    // pool uses the same kernel and is exercised by production loan tests.
    assert_eq!(receipts.records, RESOURCES[..5]);
    for (start, end, digest) in receipts.guards {
        assert_eq!(
            format!(
                "{:x}",
                // The authenticated BF09 image has .text raw offsets equal
                // to its RVAs. FrontendExecutable::mapped accepts data only.
                Sha256::digest(
                    &original[(start - 0x0040_0000) as usize..(end - 0x0040_0000) as usize]
                )
            ),
            digest,
            "{start:08X}"
        );
    }
    assert_eq!(receipts.cases.len(), 22);
    for case in &receipts.cases {
        let mut process = SoundResourceLifecycle::native_startup(executable).unwrap();
        for (&address, &count) in receipts.records.iter().zip(&case.initial_counts) {
            *process.counts.get_mut(&address).unwrap() = count;
        }
        process.nesting = case.initial_nesting;
        let mut host = Host {
            case,
            events: Vec::new(),
        };
        let mut results = Vec::new();
        for (operation, resource) in &case.operations {
            let result = match operation.as_str() {
                "acquire" => process.acquire(*resource, executable, &mut host),
                "release" => process
                    .release(*resource, executable, &mut host)
                    .map(|()| 0),
                _ => panic!("unknown operation"),
            };
            match result {
                Ok(value) => results.push(Some(value)),
                Err(SoundLifecycleError::Host(address)) => {
                    assert_eq!(Some(address), case.stop_body, "{}", case.name);
                    results.push(None);
                    break;
                }
                Err(error) => panic!("{}: {error:?}", case.name),
            }
        }
        assert_eq!(results, case.results, "{} results", case.name);
        assert_eq!(host.events, case.events, "{} events", case.name);
        assert_eq!(process.nesting(), case.nesting, "{} nesting", case.name);
        assert_eq!(
            receipts
                .records
                .iter()
                .map(|&r| process.count(r).unwrap())
                .collect::<Vec<_>>(),
            case.counts,
            "{} counts",
            case.name
        );
    }
}
