use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Receipts {
    executable_sha256: String,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    operation: String,
    initial: [u32; 3],
    created: Option<u32>,
    hresult: u32,
    error: u32,
    stop: Option<String>,
    events: Vec<(String, u32)>,
    #[serde(rename = "final")]
    final_state: [u32; 3],
    stopped: bool,
}
struct Host<'a> {
    case: &'a Case,
    events: Vec<(String, u32)>,
}
impl Host<'_> {
    fn event(&mut self, name: &str, value: u32) -> Result<(), String> {
        self.events.push((name.into(), value));
        if self.case.stop.as_deref() == Some(name) {
            Err(name.into())
        } else {
            Ok(())
        }
    }
}
fn pointer(raw: u32) -> Option<SoundDeviceIdentity> {
    (raw != 0).then_some(SoundDeviceIdentity(u64::from(raw)))
}
fn raw(pointer: Option<SoundDeviceIdentity>) -> u32 {
    pointer.map_or(0, |p| p.0 as u32)
}
impl SoundDeviceHost for Host<'_> {
    type Error = String;
    fn create(&mut self, output: &mut Option<SoundDeviceIdentity>) -> Result<i32, String> {
        let before = raw(*output);
        if let Some(created) = self.case.created {
            *output = pointer(created);
        }
        self.event("create", before)?;
        Ok(self.case.hresult as i32)
    }
    fn hresult_error(&mut self, result: i32) -> Result<Option<u32>, String> {
        self.event("convert", result as u32)?;
        Ok((self.case.error != 0).then_some(self.case.error))
    }
    fn report_error(&mut self, error: u32) -> Result<u32, String> {
        self.event("report", error)?;
        Ok(error)
    }
    fn dispose_error(&mut self, error: u32) -> Result<(), String> {
        self.event("dispose", error)
    }
    fn release(&mut self, device: SoundDeviceIdentity) -> Result<(), String> {
        self.event("release", device.0 as u32)
    }
}
#[test]
fn sound_device_initialize_release_match_native_field_and_error_order() {
    use sha2::{Digest, Sha256};
    let original = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&original).unwrap();
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../tests/fixtures/sound_device_native.json"
    ))
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&original)),
        receipts.executable_sha256
    );
    assert_eq!(receipts.cases.len(), 18);
    for case in &receipts.cases {
        let mut state = SoundDeviceRuntime::native_startup(executable).unwrap();
        state.enabled = case.initial[0] as u8;
        state.device = pointer(case.initial[1]);
        state.configuration = case.initial[2] as u8;
        let mut host = Host {
            case,
            events: Vec::new(),
        };
        let result = match case.operation.as_str() {
            "initialize" => state.initialize(&mut host),
            "release" => state.release(&mut host),
            _ => panic!("operation"),
        };
        assert_eq!(result.is_err(), case.stopped, "{} result", case.name);
        assert_eq!(host.events, case.events, "{} events", case.name);
        assert_eq!(
            [
                u32::from(state.enabled),
                raw(state.device),
                u32::from(state.configuration)
            ],
            case.final_state,
            "{} state",
            case.name
        );
    }
}
#[test]
fn enabled_device_without_pointer_is_a_required_boundary() {
    let case = Case {
        name: "missing pointer".into(),
        operation: "release".into(),
        initial: [1, 0, 0],
        created: None,
        hresult: 0,
        error: 0,
        stop: None,
        events: Vec::new(),
        final_state: [1, 0, 0],
        stopped: true,
    };
    let mut host = Host {
        case: &case,
        events: Vec::new(),
    };
    let mut state = SoundDeviceRuntime {
        enabled: 1,
        device: None,
        configuration: 19,
    };
    assert_eq!(
        state.release(&mut host),
        Err(SoundDeviceError::MissingDevice)
    );
    assert!(host.events.is_empty());
    assert_eq!(state.configuration, 19);
}
