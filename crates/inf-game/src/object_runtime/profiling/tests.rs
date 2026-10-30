use super::*;
use sha2::{Digest, Sha256};
use std::path::Path;

fn executable_bytes() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .expect("required original retail executable")
}
fn state(raw: &[u8]) -> ProcessProfiling {
    ProcessProfiling::native_startup(FrontendExecutable::parse(raw).unwrap()).unwrap()
}
fn request(phase: ProfilingPhase) -> ProfilingRequest {
    ProfilingRequest {
        index: 13,
        bank: 1,
        phase,
    }
}

#[test]
fn complete_native_marker_clock_helpers_and_startup_ranges_remain_authenticated() {
    let raw = executable_bytes();
    for (offset, length, expected) in [
        (
            0x88020,
            855,
            "10f08731b6644636d23f70b4a742cbf56eac602a6d5e1820f3fbd50042106e5f",
        ), //complete488020 caller
        (
            673936,
            33,
            "32c7e05beb7f49821cd001b3660c1811979f1a0246a6fa52383641dfbab8178b",
        ), //clock_sample_4A4890
        (
            598976,
            9,
            "5db923804e3a8e57324affd9e175d7250cc1c715ff9a54c4f5b80de364aa5d9a",
        ), //nullable_completion_dispatch_4923C0
        (
            1125680,
            52,
            "3d0898d998acc8af3ea17032ff56fd1b963c685b072f669c52d4b8267f9d0a78",
        ), //pure_64bit_multiply_512D30
        (
            234912,
            27,
            "314f255fabf4c9f673612845a649585d7fef575d68f0f442a91a1e04b2a688e8",
        ), //complete_profile_begin
        (
            234944,
            78,
            "c101fa816df865817b3c83adaf06915fef74a8931d165cc56d75cd547dbe5939",
        ), //complete_profile_end
    ] {
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw[offset..offset + length])),
            expected
        );
    }
    assert_eq!(&raw[0x88160..0x88165], &[0xb9, 0x13, 0, 0, 0]);
    assert_eq!(&raw[0x209e2..0x209e7], &[0xb9, 0x0d, 0, 0, 0]);
    assert_eq!(slot(19, 1).unwrap(), 46); //World outer and vehicle indices differ.
    let exe = FrontendExecutable::parse(&raw).unwrap();
    assert_eq!(exe.mapped(ELAPSED_BASE, 200).unwrap().bytes, &[0; 200]);
    assert!(exe.mapped(ELAPSED_BASE, SLOTS * 8).is_err());
    assert_eq!(
        exe.loader_zero_data(0x0054_7000, 232).unwrap().end(),
        0x0054_70e8
    );
    assert_eq!(
        exe.loader_zero_data(START_BASE, SLOTS * 8).unwrap().end(),
        0x0054_7378
    );
    let state = state(&raw);
    assert_eq!(state.starts, [0; SLOTS]);
    assert_eq!(state.elapsed, [0; SLOTS]);
    assert_eq!(slot(13, 1).unwrap(), 40);
    assert_eq!(START_BASE + 40 * 8, 0x0054_7308);
    assert_eq!(ELAPSED_BASE + 40 * 8, 0x0054_7078);
}

#[test]
fn nested_same_slot_overwrites_start_and_end_keeps_the_reached_start() {
    let raw = executable_bytes();
    let mut state = state(&raw);
    state
        .record_milliseconds(request(ProfilingPhase::Begin), 1000)
        .unwrap();
    state
        .record_milliseconds(request(ProfilingPhase::Begin), 1500)
        .unwrap();
    state
        .record_milliseconds(request(ProfilingPhase::End), 1600)
        .unwrap();
    assert_eq!(state.counters(13, 1).unwrap(), (1_500_000, 100_000));
    //Outer end uses the inner start, not a restored stopwatch stack.
    state
        .record_milliseconds(request(ProfilingPhase::End), 2000)
        .unwrap();
    assert_eq!(state.counters(13, 1).unwrap(), (1_500_000, 500_000));
    assert_eq!(state.counters(13, 0).unwrap(), (0, 0));
    let before = state.clone();
    assert!(state
        .record_milliseconds(
            ProfilingRequest {
                index: 13,
                bank: 2,
                phase: ProfilingPhase::Begin,
            },
            3000
        )
        .is_err());
    assert_eq!(state, before); //Unsupported portable domain cannot index other globals.
}

#[test]
fn native_signed_dword_clock_and_sub_sbb_preserve_wrap_and_unclosed_begin() {
    let raw = executable_bytes();
    let mut state = state(&raw);
    state
        .record_milliseconds(request(ProfilingPhase::Begin), 0xffff_fff0)
        .unwrap();
    state
        .record_milliseconds(request(ProfilingPhase::End), 0x10)
        .unwrap();
    assert_eq!(
        state.counters(13, 1).unwrap(),
        (0xffff_ffff_ffff_c180, 32_000)
    );
    state
        .record_milliseconds(request(ProfilingPhase::Begin), 0x7fff_ffff)
        .unwrap();
    state
        .record_milliseconds(request(ProfilingPhase::End), 0x8000_0000)
        .unwrap();
    assert_eq!(
        state.counters(13, 1).unwrap(),
        (2_147_483_647_000, 0xffff_fc18_0000_03e8)
    );
    state
        .record_milliseconds(request(ProfilingPhase::Begin), 0xffff_ffff)
        .unwrap();
    //No implicit end/clear on a stopped callback: elapsed remains the prior value.
    assert_eq!(
        state.counters(13, 1).unwrap(),
        (0xffff_ffff_ffff_fc18, 0xffff_fc18_0000_03e8)
    );
}
