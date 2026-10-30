use super::*;
use crate::process_sound::lifecycle::SOUND_DEVICE_RESOURCE;

#[test]
fn sound_application_record_and_handler_loans_follow_original_table_order() {
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    // Exact native parent edges: application first reaches the world owner,
    // whose table reaches records; its separate voice reference comes later.
    assert_eq!(executable.word(0x0052_2120).unwrap(), 0x0048_8000);
    assert_eq!(executable.word(0x0052_8798).unwrap(), 0x0047_ff40);
    assert_eq!(executable.word(0x0052_2130).unwrap(), 0x004f_4210);
    assert_eq!(executable.word(0x0052_7dc0).unwrap(), 0x004f_4210);
    struct Calls(Vec<u32>);
    impl SoundLifecycleHost for Calls {
        type Error = std::convert::Infallible;
        fn callback(&mut self, address: u32) -> Result<u32, Self::Error> {
            self.0.push(address);
            Ok(0)
        }
        fn report_error(&mut self, _: u32) -> Result<(), Self::Error> {
            unreachable!()
        }
    }
    let mut graph = SoundResourceLifecycle::native_startup(executable).unwrap();
    let mut calls = Calls(Vec::new());
    for resource in [
        SOUND_HANDLER_RESOURCE,
        SOUND_RECORD_RESOURCE,
        SOUND_VOICE_RESOURCE,
    ] {
        assert_eq!(graph.acquire(resource, executable, &mut calls).unwrap(), 0);
    }
    assert_eq!(graph.count(SOUND_VOICE_RESOURCE), Some(2));
    assert_eq!(graph.count(SOUND_DEVICE_RESOURCE), Some(2));
    assert_eq!(graph.count(SOUND_RECORD_RESOURCE), Some(1));
    assert_eq!(
        calls.0,
        [0x0049_4840, 0x0051_0250, 0x004f_40b0, 0x0047_fef0]
    );
    graph
        .release(SOUND_VOICE_RESOURCE, executable, &mut calls)
        .unwrap();
    assert_eq!(calls.0.len(), 4); // first release leaves record-owned voices alive
    graph
        .release(SOUND_RECORD_RESOURCE, executable, &mut calls)
        .unwrap();
    assert_eq!(&calls.0[4..], [0x0047_ff20, 0x004f_40f0]);
    assert_eq!(graph.count(SOUND_DEVICE_RESOURCE), Some(1));
    graph
        .release(SOUND_HANDLER_RESOURCE, executable, &mut calls)
        .unwrap();
    assert_eq!(calls.0.last(), Some(&0x0051_0330));
    for resource in [
        SOUND_HANDLER_RESOURCE,
        SOUND_RECORD_RESOURCE,
        SOUND_VOICE_RESOURCE,
        SOUND_DEVICE_RESOURCE,
    ] {
        assert_eq!(graph.count(resource), Some(0));
    }
    assert_eq!(graph.nesting(), 0);
}
