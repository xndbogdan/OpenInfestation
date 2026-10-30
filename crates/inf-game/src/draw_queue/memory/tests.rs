use super::*;

#[derive(Debug)]
struct Writes(Vec<(usize, u8)>);

fn record(command: &Writes, bytes: &mut [Option<u8>]) {
    for &(offset, byte) in &command.0 {
        bytes[offset] = Some(byte);
    }
}

fn submit(queue: &mut DrawQueue<Writes>, blocks: usize, writes: &[(usize, u8)]) {
    queue
        .submit(DrawSubmission {
            depth: 0x12345678,
            payload_blocks: blocks,
            receiver_offset: 0xaabbccdd,
            command: Writes(writes.to_vec()),
        })
        .unwrap();
}

fn word(memory: &NativeDrawMemory<Writes>, offset: usize) -> Option<u32> {
    let bytes = &memory.bytes[offset..offset + 4];
    Some(u32::from_le_bytes([
        bytes[0]?, bytes[1]?, bytes[2]?, bytes[3]?,
    ]))
}

#[test]
fn native_draw_memory_reset_retains_only_prior_stores_and_replacement_is_unknown() {
    let mut queue = DrawQueue::with_native_memory(256, record).unwrap();
    assert_eq!(queue.next_payload_word(0x20), None);
    submit(&mut queue, 3, &[(48, 1), (49, 2), (50, 3), (51, 4)]);
    assert_eq!(word(queue.memory.as_ref().unwrap(), 4), Some(0xaabbccdd));
    queue.reset();
    assert_eq!(queue.next_payload_word(0x20), Some(0x04030201));
    // A new producer writes three color bytes; native high-byte carry survives.
    submit(&mut queue, 3, &[(48, 5), (49, 6), (50, 7)]);
    queue.reset();
    assert_eq!(queue.next_payload_word(0x20), Some(0x04070605));
    let fresh = DrawQueue::with_native_memory(256, record).unwrap();
    assert_eq!(fresh.next_payload_word(0x20), None);
    let generic: DrawQueue<Writes> = DrawQueue::new(256).unwrap();
    assert_eq!(generic.next_payload_word(0x20), None);
}

/// 004A43A0's 0xC8000-byte arena lies past the heap's virtual-memory
/// threshold: its fresh pages are zero, and only stores replace them.
#[test]
fn a_large_arena_starts_zero() {
    let mut queue = DrawQueue::with_native_memory(0xc8000, record).unwrap();
    assert_eq!(queue.next_payload_word(0x20), Some(0));
    submit(&mut queue, 3, &[(48, 1), (49, 2)]);
    queue.reset();
    assert_eq!(queue.next_payload_word(0x20), Some(0x0201));
    let small = DrawQueue::with_native_memory(0x7f000, record).unwrap();
    assert_eq!(small.next_payload_word(0x20), None);
}

/// A renewed arena holds what a new one would: its stores are gone, its
/// fresh pages zero (or unknown below the threshold), its lists empty.
#[test]
fn a_renewed_arena_is_a_new_one() {
    for capacity in [0xc8000, 256] {
        let mut queue = DrawQueue::with_native_memory(capacity, record).unwrap();
        submit(&mut queue, 3, &[(48, 1), (49, 2)]);
        queue.renew_with_native_memory(capacity, record).unwrap();
        let fresh = DrawQueue::with_native_memory(capacity, record).unwrap();
        assert_eq!(queue.state(), fresh.state());
        assert_eq!(queue.next_payload_word(0x20), fresh.next_payload_word(0x20));
        assert_eq!(
            queue.memory.as_ref().unwrap().bytes,
            fresh.memory.as_ref().unwrap().bytes
        );
    }
    // Another size is made anew.
    let mut queue = DrawQueue::with_native_memory(256, record).unwrap();
    queue.renew_with_native_memory(0xc8000, record).unwrap();
    assert_eq!(
        queue.state(),
        DrawQueue::<Writes>::with_native_memory(0xc8000, record)
            .unwrap()
            .state()
    );
    assert_eq!(queue.next_payload_word(0x20), Some(0));
}

#[test]
fn native_draw_memory_failed_callback_keeps_its_stores_and_failed_allocation_keeps_scratch() {
    let mut queue = DrawQueue::with_native_memory(128, record).unwrap();
    submit(&mut queue, 3, &[(48, 1), (49, 2), (50, 3), (51, 4)]);
    assert!(queue
        .flush(|_, command| {
            command.0.push((48, 9));
            Err::<(), _>("late pixel failure")
        })
        .is_err());
    queue.reset();
    assert_eq!(queue.next_payload_word(0x20), Some(0x04030209));
    let before = queue.state();
    let error = queue.submit(DrawSubmission {
        depth: 0x87654321,
        payload_blocks: 100,
        receiver_offset: 0,
        command: Writes(Vec::new()),
    });
    assert!(matches!(error, Err(DrawQueueFault::ArenaFull { .. })));
    assert_eq!(queue.state().command_cursor, before.command_cursor);
    assert_eq!(queue.state().pending_cursor, before.pending_cursor);
    assert_eq!(queue.state().command_count, 0);
    let memory = queue.memory.as_ref().unwrap();
    assert_eq!(word(memory, 96), Some(0x87654321));
    assert_eq!(word(memory, 112), None);
}

#[test]
fn native_draw_memory_unsorted_close_copies_padding_sorted_close_preserves_destination_padding() {
    for sorting in [false, true] {
        let mut queue = DrawQueue::with_native_memory(256, record).unwrap();
        // An earlier real allocation spans these future scratch locations.
        let writes: Vec<_> = (0..224).map(|offset| (offset, offset as u8)).collect();
        submit(&mut queue, 13, &writes);
        queue.reset();
        queue
            .begin_group(0, sorting, |_| Ok::<_, Infallible>(()))
            .unwrap();
        submit(&mut queue, 1, &[]);
        let memory = queue.memory.as_ref().unwrap();
        let old_destination_padding = memory.bytes[68..80].to_vec();
        let pending_padding =
            memory.bytes[queue.back as usize + 4..queue.back as usize + 16].to_vec();
        queue.end_group().unwrap();
        let memory = queue.memory.as_ref().unwrap();
        assert_eq!(word(memory, 64), None);
        assert_eq!(
            memory.bytes[68..80],
            if sorting {
                old_destination_padding
            } else {
                pending_padding
            }
        );
        assert_eq!(word(memory, 0), Some(0x4a4430));
        assert_eq!(word(memory, 4), Some(0));
        assert_eq!(memory.bytes[16], Some(u8::from(sorting)));
        assert_eq!(&memory.bytes[17..20], &[Some(17), Some(18), Some(19)]);
        assert_eq!(word(memory, 20), None);
        assert_eq!(word(memory, 24), None);
        assert_eq!(word(memory, 28), None);
    }
}
