use super::*;
use std::collections::VecDeque;
use std::io::ErrorKind;
use std::path::Path;

use crate::frontend_assets::FrontendAssets;
use inf_formats::FrontendExecutable;
use sha2::{Digest, Sha256};

type Snapshot = (ResourceReadCallback, u32, u64, u32);

fn snapshot(state: &ResourceReadState) -> Snapshot {
    (state.callback, state.chunk, state.last_clock, state.pending)
}

#[derive(Default)]
struct Source {
    bytes: Vec<u8>,
    cursor: usize,
    requests: Vec<usize>,
    interrupted: bool,
    max_return: Option<usize>,
}

impl Source {
    fn filled(length: usize) -> Self {
        Self {
            bytes: vec![0x5a; length],
            ..Self::default()
        }
    }
}

impl Read for Source {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        self.requests.push(destination.len());
        if self.interrupted {
            self.interrupted = false;
            return Err(io::Error::from(ErrorKind::Interrupted));
        }
        let length = destination
            .len()
            .min(self.bytes.len() - self.cursor)
            .min(self.max_return.unwrap_or(usize::MAX));
        destination[..length].copy_from_slice(&self.bytes[self.cursor..self.cursor + length]);
        self.cursor += length;
        Ok(length)
    }
}

enum Action {
    Continue,
    Clear,
    Chunk(u32),
    StopAndMutate,
    Return(u32),
}

#[derive(Default)]
struct Host {
    clocks: VecDeque<u64>,
    actions: VecDeque<Action>,
    callbacks: Vec<(u32, Snapshot)>,
}

impl Host {
    fn clocks(values: &[u64]) -> Self {
        Self {
            clocks: values.iter().copied().collect(),
            ..Self::default()
        }
    }
}

impl ResourceReadHost for Host {
    fn sample_clock(&mut self) -> u64 {
        self.clocks
            .pop_front()
            .expect("only reached native clock samples")
    }

    fn invoke_loading_callback(&mut self, state: &mut ResourceReadState, progress: u32) -> u32 {
        self.callbacks.push((progress, snapshot(state)));
        match self.actions.pop_front().unwrap_or(Action::Continue) {
            Action::Continue => 0,
            Action::Clear => {
                state.exchange_callback(ResourceReadCallback::None);
                0
            }
            Action::Chunk(chunk) => {
                state.chunk = chunk;
                0
            }
            Action::StopAndMutate => {
                state.chunk = 1233;
                state.pending = 0xabc;
                state.last_clock = 55_000;
                state.exchange_callback(ResourceReadCallback::None);
                7
            }
            Action::Return(value) => value,
        }
    }
}

fn loading() -> ResourceReadState {
    let mut state = ResourceReadState::native_startup();
    state.exchange_callback(ResourceReadCallback::LoadingCallback);
    state
}

fn read(
    state: &mut ResourceReadState,
    source: &mut Source,
    destination: &mut [u8],
    host: &mut Host,
) -> Result<(), ResourceReadError> {
    read_resource_4f3de0(ResourceReadRequest {
        state,
        source,
        destination,
        host,
    })
}

fn native_span(bytes: &[u8], address: u32, length: usize) -> &[u8] {
    let word = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let half = |offset| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
    let pe = word(0x3c) as usize;
    let rva = (address - word(pe + 24 + 28)) as usize;
    let sections = pe + 24 + usize::from(half(pe + 20));
    for index in 0..usize::from(half(pe + 6)) {
        let section = sections + index * 40;
        let start = word(section + 12) as usize;
        let size = word(section + 16) as usize;
        if rva >= start && rva - start + length <= size {
            let file = word(section + 20) as usize + rva - start;
            return &bytes[file..file + length];
        }
    }
    panic!("required file-backed original span {address:08x}");
}

#[test]
fn complete_original_read_exchange_and_once_seed_are_authenticated() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    for (address, length, hash) in [
        (
            0x004f_3de0,
            638,
            "4375084d6c88fb9e078ae11eb5597f82d391983fa18648fce2161c0635d1bcee",
        ),
        (
            0x004f_40a0,
            12,
            "9da62d61f51f350cddce7eeb3e0e93ef5e5fc356e472b8072bbf0ba8b46350ef",
        ),
    ] {
        assert_eq!(
            format!("{:x}", Sha256::digest(native_span(bytes, address, length))),
            hash
        );
    }
    assert_eq!(
        FrontendExecutable::parse(bytes)
            .unwrap()
            .word(0x0054_4cc8)
            .unwrap(),
        8000
    );
    // Actual .data raw/virtual bounds authenticate PE-zero owners. Do not
    // relax FrontendExecutable's file-backed mapping to manufacture bytes.
    let word = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let half = |offset| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
    let pe = word(0x3c) as usize;
    let base = word(pe + 24 + 28);
    let sections = pe + 24 + usize::from(half(pe + 20));
    let data = (0..usize::from(half(pe + 6)))
        .map(|i| sections + i * 40)
        .find(|&at| &bytes[at..at + 8] == b".data\0\0\0")
        .unwrap();
    let raw_end = base + word(data + 12) + word(data + 16);
    let virtual_end = base + word(data + 12) + word(data + 8);
    for (address, length) in [(0x0065_eca0, 4), (0x0065_eca8, 8), (0x0065_ecb0, 4)] {
        assert!(address >= raw_end && address + length <= virtual_end);
    }
    let mut state = ResourceReadState::native_startup();
    assert_eq!(snapshot(&state), (ResourceReadCallback::None, 8000, 0, 0));
    state.chunk = 3210;
    state.last_clock = 0x1234_5678_ffff_ffff;
    state.pending = 0xdead_beef;
    assert_eq!(
        state.exchange_callback(ResourceReadCallback::LoadingCallback),
        ResourceReadCallback::None
    );
    assert_eq!(
        state.exchange_callback(ResourceReadCallback::None),
        ResourceReadCallback::LoadingCallback
    );
    assert_eq!(
        snapshot(&state),
        (
            ResourceReadCallback::None,
            3210,
            0x1234_5678_ffff_ffff,
            0xdead_beef
        )
    );
}

#[test]
fn passive_exact_and_zero_requests_never_sample_or_mutate_read_state() {
    struct ZeroRequest(usize);
    impl Read for ZeroRequest {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("exact request owns this test")
        }
        fn read_exact(&mut self, destination: &mut [u8]) -> io::Result<()> {
            assert!(destination.is_empty());
            self.0 += 1;
            Ok(())
        }
    }
    let mut state = ResourceReadState {
        callback: ResourceReadCallback::None,
        chunk: 0,
        last_clock: 99,
        pending: 77,
    };
    let before = snapshot(&state);
    let mut zero = ZeroRequest(0);
    let mut host = Host::default();
    read_resource_4f3de0(ResourceReadRequest {
        state: &mut state,
        source: &mut zero,
        destination: &mut [],
        host: &mut host,
    })
    .unwrap();
    assert_eq!(zero.0, 1);
    let mut source = Source::filled(13);
    let mut destination = [0; 13];
    read(&mut state, &mut source, &mut destination, &mut host).unwrap();
    assert_eq!(destination, [0x5a; 13]);
    assert_eq!(source.requests, [13]);
    assert_eq!(snapshot(&state), before);
    assert!(host.callbacks.is_empty());
}

#[test]
fn exact_read_retries_interrupted_and_retains_actual_short_read_bytes_without_publication() {
    let mut state = loading();
    let before = snapshot(&state);
    let mut source = Source::filled(6);
    source.interrupted = true;
    source.max_return = Some(2);
    let mut destination = [0; 6];
    read(
        &mut state,
        &mut source,
        &mut destination,
        &mut Host::default(),
    )
    .unwrap();
    assert_eq!(source.requests, [6, 6, 4, 2]);
    assert_eq!(destination, [0x5a; 6]);
    assert_eq!(state.pending, 6); // final <= quarter, no clock
    state.pending = 0;
    let mut source = Source::filled(3500);
    let mut destination = vec![0x81; 9000];
    assert!(
        matches!(read(&mut state, &mut source, &mut destination, &mut Host::default()),
        Err(ResourceReadError::Io { offset: 0, length: 8000, source }) if source.kind() == ErrorKind::UnexpectedEof)
    );
    assert_eq!(source.cursor, 3500);
    assert!(destination[..3500].iter().all(|&byte| byte == 0x5a));
    assert!(destination[3500..].iter().all(|&byte| byte == 0x81));
    assert_eq!(snapshot(&state), before);
}

#[test]
fn real_chunk_reads_preserve_main_progress_and_final_requested_total_doubling() {
    let mut state = loading();
    let mut source = Source::filled(30000);
    let mut destination = vec![0; 30000];
    let mut host = Host::clocks(&[10000, 20000, 50000, 100000]);
    read(&mut state, &mut source, &mut destination, &mut host).unwrap();
    assert_eq!(source.requests, [8000, 9000, 10000, 3000]);
    assert_eq!(
        host.callbacks
            .iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        [27000, 60000]
    );
    assert_eq!(
        host.callbacks[0].1,
        (ResourceReadCallback::LoadingCallback, 10000, 0, 27000)
    );
    assert_eq!(
        host.callbacks[1].1,
        (ResourceReadCallback::LoadingCallback, 9000, 50000, 60000)
    );
    assert_eq!(
        snapshot(&state),
        (ResourceReadCallback::LoadingCallback, 8000, 100000, 0)
    );
    assert!(destination.iter().all(|&byte| byte == 0x5a));
    assert!(host.clocks.is_empty());
}

#[test]
fn strict_signed_adaptation_and_unsigned_shrink_floor_preserve_boundary_cases() {
    for (sample, final_sample, chunk, callback) in [
        (35999, 40000, 5000, false),
        (36000, 40000, 4000, false),
        (40000, 40000, 4000, false),
        (40001, 80001, 4000, true),
        (44000, 84000, 4000, true),
        (44001, 84001, 3000, true),
        (0xffff_ffff, 40000, 5000, false),
        (0x8000_0000, 40000, 3000, false),
    ] {
        let mut state = loading();
        state.chunk = 4000;
        let mut source = Source::filled(5000);
        let mut destination = vec![0; 5000];
        let mut host = Host::clocks(&[sample, final_sample]);
        read(&mut state, &mut source, &mut destination, &mut host).unwrap();
        assert_eq!(source.requests, [4000, 1000]);
        assert_eq!(state.chunk, chunk, "clock {sample:x}");
        assert_eq!(host.callbacks.len(), usize::from(callback));
        if callback {
            assert_eq!(host.callbacks[0].0, 4000);
        }
        assert_eq!(state.last_clock, if callback { sample } else { 0 });
        assert_eq!(state.pending, 0);
    }
    for (chunk, total, final_chunk) in [(2000, 2500, 2000), (2001, 2500, 1001)] {
        let mut state = loading();
        state.chunk = chunk;
        let mut source = Source::filled(total);
        let mut destination = vec![0; total];
        let mut host = Host::clocks(&[44001, 84001]);
        read(&mut state, &mut source, &mut destination, &mut host).unwrap();
        assert_eq!(state.chunk, final_chunk);
        assert_eq!(host.callbacks[0].0, chunk);
    }
}

#[test]
fn final_quarter_gate_pending_carry_and_wrapping_doubling_are_distinct() {
    let mut state = loading();
    let mut host = Host::clocks(&[50000]);
    let mut source = Source::filled(2000);
    read(&mut state, &mut source, &mut vec![0; 2000], &mut host).unwrap();
    assert_eq!(state.pending, 2000); // equality to chunk/4 retains
    let mut source = Source::filled(1000);
    read(&mut state, &mut source, &mut vec![0; 1000], &mut host).unwrap();
    assert_eq!(host.callbacks[0].0, 6000);
    assert_eq!(
        snapshot(&state),
        (ResourceReadCallback::LoadingCallback, 7000, 50000, 0)
    );

    let mut state = loading();
    state.pending = u32::MAX - 1000;
    read(
        &mut state,
        &mut Source::filled(3000),
        &mut vec![0; 3000],
        &mut Host::default(),
    )
    .unwrap();
    assert_eq!(state.pending, 1999); // wrapping total addition reaches small gate
    let mut state = loading();
    state.pending = 0x8000_0000;
    let mut host = Host::clocks(&[50000]);
    read(
        &mut state,
        &mut Source::filled(3000),
        &mut vec![0; 3000],
        &mut host,
    )
    .unwrap();
    assert_eq!(host.callbacks[0].0, 6000); // doubled raw accumulator wraps

    let mut state = loading();
    state.pending = 3000;
    let mut host = Host::clocks(&[50000]);
    read(&mut state, &mut Source::filled(0), &mut [], &mut host).unwrap();
    assert_eq!(host.callbacks[0].0, 6000); // zero total still reaches final phase
}

#[test]
fn final_only_timing_uses_same_strict_band_and_keeps_clock_when_callback_unreached() {
    for (clock, chunk, callbacks) in [
        (35999, 9000, 0),
        (36000, 8000, 0),
        (40000, 8000, 0),
        (40001, 8000, 1),
        (44000, 8000, 1),
        (44001, 7000, 1),
    ] {
        let mut state = loading();
        let mut host = Host::clocks(&[clock]);
        read(
            &mut state,
            &mut Source::filled(2001),
            &mut vec![0; 2001],
            &mut host,
        )
        .unwrap();
        assert_eq!(host.callbacks.len(), callbacks);
        if callbacks != 0 {
            assert_eq!(host.callbacks[0].0, 4002);
        }
        assert_eq!(
            snapshot(&state),
            (
                ResourceReadCallback::LoadingCallback,
                chunk,
                if callbacks == 0 { 0 } else { clock },
                0
            )
        );
    }
}

#[test]
fn callback_exchange_and_current_mutation_preserve_later_null_bypasses() {
    let mut state = loading();
    state.pending = 73;
    state.chunk = 4123;
    state.last_clock = 91;
    state.exchange_callback(ResourceReadCallback::None);
    let before = snapshot(&state);
    read(
        &mut state,
        &mut Source::filled(5),
        &mut [0; 5],
        &mut Host::default(),
    )
    .unwrap();
    assert_eq!(snapshot(&state), before);

    let mut state = loading();
    let mut source = Source::filled(20000);
    let mut destination = vec![0; 20000];
    let mut host = Host::clocks(&[50000]);
    host.actions.push_back(Action::Clear);
    read(&mut state, &mut source, &mut destination, &mut host).unwrap();
    assert_eq!(source.requests, [8000, 7000, 5000]);
    assert_eq!(host.callbacks.len(), 1);
    assert_eq!(host.callbacks[0].0, 8000);
    assert_eq!(
        snapshot(&state),
        (ResourceReadCallback::None, 7000, 50000, 0)
    );
}

#[test]
fn nonzero_callback_retains_actual_mutations_and_main_or_final_read_prefix() {
    let mut state = loading();
    state.chunk = 4000;
    let mut source = Source::filled(5000);
    let mut destination = vec![0x81; 5000];
    let mut host = Host::clocks(&[50000]);
    host.actions.push_back(Action::StopAndMutate);
    assert!(matches!(
        read(&mut state, &mut source, &mut destination, &mut host),
        Err(ResourceReadError::CallbackReturn { value: 7 })
    ));
    assert_eq!(source.requests, [4000]);
    assert!(destination[..4000].iter().all(|&b| b == 0x5a));
    assert!(destination[4000..].iter().all(|&b| b == 0x81));
    assert_eq!(
        snapshot(&state),
        (ResourceReadCallback::None, 1233, 55000, 0xabc)
    );

    let mut state = loading();
    state.pending = 11;
    let mut host = Host::clocks(&[50000]);
    host.actions.push_back(Action::Return(9));
    let mut destination = vec![0; 3000];
    assert!(matches!(
        read(
            &mut state,
            &mut Source::filled(3000),
            &mut destination,
            &mut host
        ),
        Err(ResourceReadError::CallbackReturn { value: 9 })
    ));
    assert!(destination.iter().all(|&b| b == 0x5a));
    assert_eq!(
        snapshot(&state),
        (ResourceReadCallback::LoadingCallback, 8000, 0, 6022)
    );
}

#[test]
fn short_later_chunks_stop_before_unreached_progress_and_clock_phases() {
    for (available, total, chunk, offset, length, pending) in [
        (6000, 10000, 4000, 4000, 4000, 4000),
        (8500, 9000, 8000, 8000, 1000, 8000),
    ] {
        let mut state = loading();
        state.chunk = chunk;
        let mut source = Source::filled(available);
        let mut destination = vec![0x81; total];
        let mut host = Host::clocks(&[40000]);
        assert!(
            matches!(read(&mut state, &mut source, &mut destination, &mut host),
            Err(ResourceReadError::Io { offset: at, length: count, .. }) if at == offset && count == length)
        );
        assert_eq!(
            snapshot(&state),
            (ResourceReadCallback::LoadingCallback, chunk, 0, pending)
        );
        assert!(destination[..available].iter().all(|&b| b == 0x5a));
        assert!(destination[available..].iter().all(|&b| b == 0x81));
        assert!(host.callbacks.is_empty());
        assert!(host.clocks.is_empty());
    }
}

#[test]
fn low_clock_wrap_and_actual_high_publication_use_retained_read_sample() {
    let mut state = loading();
    state.chunk = 4000;
    state.last_clock = 0x1234_5678_ffff_0000;
    let first = 0x9876_5432_0000_2710;
    let mut host = Host::clocks(&[first, first + 40000]);
    read(
        &mut state,
        &mut Source::filled(5000),
        &mut vec![0; 5000],
        &mut host,
    )
    .unwrap();
    assert_eq!(host.callbacks.len(), 1);
    assert_eq!(host.callbacks[0].0, 4000);
    assert_eq!(
        snapshot(&state),
        (ResourceReadCallback::LoadingCallback, 3000, first, 0)
    );
}

#[test]
fn reached_zero_and_wrapped_physical_range_are_containment_without_erasing_prior_prefix() {
    for chunk in [0, u32::MAX] {
        let mut state = loading();
        state.chunk = 4000;
        let mut source = Source::filled(5000);
        let mut destination = vec![0x81; 5000];
        let mut host = Host::clocks(&[50000]);
        host.actions.push_back(Action::Chunk(chunk));
        let error = read(&mut state, &mut source, &mut destination, &mut host).unwrap_err();
        if chunk == 0 {
            assert!(matches!(
                error,
                ResourceReadError::ZeroChunk {
                    offset: 4000,
                    total: 5000
                }
            ));
            assert_eq!(state.chunk, 0);
        } else {
            assert!(matches!(
                error,
                ResourceReadError::BackingRange {
                    offset: 4000,
                    length: 4_294_966_295,
                    total: 5000
                }
            ));
            assert_eq!(state.chunk, 4_294_966_295);
        }
        assert_eq!(source.requests, [4000]);
        assert_eq!(state.last_clock, 50000);
        assert_eq!(state.pending, 0);
        assert!(destination[..4000].iter().all(|&b| b == 0x5a));
        assert!(destination[4000..].iter().all(|&b| b == 0x81));
    }
    let mut state = loading();
    state.chunk = 0;
    let mut source = Source::filled(1);
    assert!(matches!(
        read(&mut state, &mut source, &mut [0], &mut Host::default()),
        Err(ResourceReadError::ZeroChunk {
            offset: 0,
            total: 1
        })
    ));
    assert!(source.requests.is_empty());
}
