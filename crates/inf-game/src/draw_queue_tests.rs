use super::*;

fn command(depth: i32, value: i32) -> DrawSubmission<i32> {
    DrawSubmission {
        depth: depth as u32,
        payload_blocks: 1,
        receiver_offset: 0,
        command: value,
    }
}

fn prepare<C>(_: &mut DrawQueue<C>) -> Result<(), Infallible> {
    Ok(())
}

fn replay(queue: &mut DrawQueue<i32>) -> Vec<i32> {
    let mut values = Vec::new();
    let count = queue
        .flush(|_, value| {
            values.push(*value);
            Ok::<_, Infallible>(())
        })
        .unwrap();
    assert_eq!(count, values.len());
    values
}

#[test]
fn sorted_root_compares_signed_depth_and_allocated_pointer_for_equal_depths() {
    let mut queue = DrawQueue::new(4096).unwrap();
    for (i, depth) in [0, -1, i32::MAX, i32::MIN, 0, 1, i32::MAX]
        .into_iter()
        .enumerate()
    {
        let mut item = command(depth, i as i32);
        item.payload_blocks = i + 1; // allocation addresses, not vector indices, are ties
        queue.submit(item).unwrap();
    }
    assert_eq!(replay(&mut queue), [2, 6, 5, 0, 4, 1, 3]);
}

#[test]
fn model_prefix_containment_requires_the_actual_running_implicit_root() {
    let mut queue = DrawQueue::new(4096).unwrap();
    assert!(queue.has_only_implicit_root());
    queue.submit(command(10, 1)).unwrap();
    assert!(queue.has_only_implicit_root());
    queue.begin_group(20, false, prepare).unwrap();
    assert!(!queue.has_only_implicit_root());
    queue.submit(command(30, 2)).unwrap();
    queue.end_group().unwrap();
    assert!(queue.has_only_implicit_root());
    assert_eq!(replay(&mut queue), [2, 1]);
    assert!(!queue.has_only_implicit_root());
    queue.reset();
    assert!(queue.has_only_implicit_root());
    assert!(queue.submit(command(0, 3)).is_ok());
    let mut overflow = command(0, 4);
    overflow.payload_blocks = 256;
    assert!(queue.submit(overflow).is_err());
    assert!(!queue.has_only_implicit_root());
}

#[test]
fn groups_are_parent_entries_and_unsorted_children_keep_submission_order() {
    let mut queue = DrawQueue::new(4096).unwrap();
    queue.submit(command(10, 1)).unwrap();
    queue.begin_group(20, false, prepare).unwrap();
    queue.submit(command(-100, 2)).unwrap();
    queue.begin_group(0, true, prepare).unwrap();
    queue.submit(command(5, 3)).unwrap();
    queue.submit(command(8, 4)).unwrap();
    queue.end_group().unwrap();
    queue.submit(command(100, 5)).unwrap();
    queue.end_group().unwrap();
    queue.submit(command(30, 6)).unwrap();
    assert_eq!(replay(&mut queue), [6, 2, 4, 3, 5, 1]);
}

#[test]
fn implicit_close_preserves_equal_depth_group_position() {
    let mut queue = DrawQueue::new(4096).unwrap();
    queue.submit(command(10, 1)).unwrap();
    queue.begin_group(10, false, prepare).unwrap();
    queue.submit(command(100, 2)).unwrap();
    queue.begin_group(0, false, prepare).unwrap();
    queue.submit(command(0, 3)).unwrap();
    assert_eq!(queue.state().open_groups, 3);
    assert_eq!(replay(&mut queue), [1, 2, 3]);
    assert_eq!(queue.state().open_groups, 0);
}

#[test]
fn arena_tracks_headers_pending_entries_and_final_lists() {
    let mut queue = DrawQueue::new(255).unwrap();
    assert_eq!(queue.state().capacity, 256);
    let first = queue.submit(command(0, 1)).unwrap();
    assert_eq!(first.command_offset, 0);
    assert_eq!(
        (queue.state().command_cursor, queue.state().pending_cursor),
        (32, 224)
    );
    queue.begin_group(0, false, prepare).unwrap();
    assert_eq!(
        (queue.state().command_cursor, queue.state().pending_cursor),
        (64, 192)
    );
    let second = queue.submit(command(0, 2)).unwrap();
    assert_eq!(second.command_offset, 64);
    assert_eq!(
        (queue.state().command_cursor, queue.state().pending_cursor),
        (96, 176)
    );
    queue.end_group().unwrap(); // one16-byte closed child list, restore192
    assert_eq!(
        (queue.state().command_cursor, queue.state().pending_cursor),
        (112, 192)
    );
    let third = queue.submit(command(0, 3)).unwrap();
    assert_eq!(third.command_offset, 112);
    assert_eq!(replay(&mut queue), [1, 2, 3]);
    assert_eq!(
        (queue.state().command_cursor, queue.state().pending_cursor),
        (192, 256)
    );
}

#[test]
fn exact_capacity_is_accepted_and_failure_does_not_commit_cursors() {
    let mut queue = DrawQueue::new(64).unwrap();
    queue.submit(command(1, 11)).unwrap();
    let before = queue.state();
    assert_eq!((before.command_cursor, before.pending_cursor), (32, 32));
    let error = queue.submit(command(2, 22)).unwrap_err();
    assert_eq!(
        error,
        DrawQueueFault::ArenaFull {
            requested: 64,
            available: 0
        }
    );
    assert_eq!(error.native_error_code(), Some(0x301));
    let after = queue.state();
    assert_eq!(
        (
            after.command_cursor,
            after.pending_cursor,
            after.command_count
        ),
        (32, 32, 1)
    );
    assert!(after.stopped);
    assert_eq!(queue.end_group(), Err(DrawQueueFault::Stopped));
    queue.reset();
    queue.submit(command(1, 33)).unwrap();
    assert_eq!(replay(&mut queue), [33]);
    assert_eq!(
        (queue.state().command_cursor, queue.state().pending_cursor),
        (48, 64)
    );
}

/// A native 0x301 catch resumes the queue: the overflowing request committed
/// nothing, a smaller one still fits, and the flush replays both. Another
/// fault's stop is not an overflow and stays.
#[test]
fn a_caught_overflow_resumes_the_queue() {
    let mut queue = DrawQueue::new(112).unwrap();
    queue.submit(command(0, 11)).unwrap();
    let big = DrawSubmission {
        payload_blocks: 2,
        ..command(0, 22)
    };
    assert!(matches!(
        queue.submit(big),
        Err(DrawQueueFault::ArenaFull { .. })
    ));
    assert!(queue.overflowed());
    assert!(queue.recover_overflow());
    assert!(!queue.overflowed() && !queue.state().stopped);
    let small = DrawSubmission {
        payload_blocks: 0,
        ..command(0, 33)
    };
    queue.submit(small).unwrap();
    assert_eq!(replay(&mut queue), [11, 33]);

    queue.reset();
    queue.end_group().unwrap();
    assert_eq!(queue.end_group(), Err(DrawQueueFault::GroupUnderflow));
    assert!(!queue.recover_overflow());
    assert!(queue.state().stopped);
}

#[test]
fn repeated_flush_replays_payload_mutations_without_reclaiming_arena() {
    let mut queue = DrawQueue::new(128).unwrap();
    let mut submission = command(0, 7);
    submission.receiver_offset = 0xffff_fff0;
    let invocation = queue.submit(submission).unwrap();
    assert_eq!(invocation.native_receiver(0x20), 0x10);
    queue
        .flush(|seen, value| {
            assert_eq!(seen, invocation);
            *value += 1;
            Ok::<_, Infallible>(())
        })
        .unwrap();
    let state = queue.state();
    assert_eq!(replay(&mut queue), [8]);
    assert_eq!(queue.state(), state);
}

#[test]
fn callback_failure_retains_completed_effects_and_reset_is_explicit() {
    let mut queue = DrawQueue::new(512).unwrap();
    for value in 1..=3 {
        queue.submit(command(0, value)).unwrap();
    }
    let mut seen = Vec::new();
    let result = queue.flush(|_, value| {
        seen.push(*value);
        *value += 10;
        if seen.len() == 2 {
            Err("backend")
        } else {
            Ok(())
        }
    });
    assert_eq!(result, Err(DrawQueueError::Host("backend")));
    assert_eq!(seen, [1, 2]);
    assert_eq!(queue.state().last_flush_completed, 1);
    assert_eq!(
        queue.flush(|_, _| Ok::<_, &str>(())),
        Err(DrawQueueError::Fault(DrawQueueFault::Stopped))
    );
    let Command::Payload(value) = &queue.nodes[1].command else {
        panic!()
    };
    assert_eq!(*value, 12); // failing callback's writes also survive
    queue.reset();
    assert!(replay(&mut queue).is_empty());
}

#[test]
fn prepare_runs_before_group_allocation_and_uses_its_live_group() {
    let mut queue = DrawQueue::new(512).unwrap();
    queue
        .begin_group(10, false, |queue| {
            assert_eq!(queue.state().command_cursor, 0);
            queue.submit(command(10, 1))?;
            queue.begin_group(10, true, prepare).unwrap();
            Ok::<_, DrawQueueFault>(())
        })
        .unwrap();
    // Outer request is nested under the group created by its preparation.
    assert_eq!(queue.state().open_groups, 3);
    queue.submit(command(0, 2)).unwrap();
    assert_eq!(replay(&mut queue), [1, 2]);
}

#[test]
fn prepare_failure_preserves_prior_effects_and_catches_swallowed_inner_error() {
    let mut queue = DrawQueue::new(64).unwrap();
    let result = queue.begin_group(0, false, |queue| {
        queue.submit(command(0, 1)).unwrap();
        let _ = queue.submit(command(0, 2));
        Ok::<_, &str>(())
    });
    assert_eq!(result, Err(DrawQueueError::Fault(DrawQueueFault::Stopped)));
    assert_eq!(queue.state().command_count, 1);
    queue.reset();
    let result = queue.begin_group(0, false, |queue| {
        queue.submit(command(0, 1)).unwrap();
        let _ = queue.submit(command(0, 2));
        Err("immediate host error")
    });
    assert_eq!(result, Err(DrawQueueError::Host("immediate host error")));
    assert_eq!(queue.state().command_count, 1);
}

#[test]
fn group_capacity_failure_happens_after_successful_preparation() {
    let mut queue = DrawQueue::new(64).unwrap();
    let result = queue.begin_group(0, false, |queue| {
        queue.submit(command(0, 1)).unwrap();
        Ok::<_, Infallible>(())
    });
    assert_eq!(
        result,
        Err(DrawQueueError::Fault(DrawQueueFault::ArenaFull {
            requested: 64,
            available: 0,
        }))
    );
    assert_eq!(queue.state().command_count, 1);
    assert_eq!(queue.state().open_groups, 1);
    assert_eq!(
        (queue.state().command_cursor, queue.state().pending_cursor),
        (32, 32)
    );
}

#[test]
fn empty_groups_close_and_replay_but_underflow_is_native0302() {
    let mut queue = DrawQueue::<i32>::new(128).unwrap();
    queue.begin_group(0, true, prepare).unwrap();
    queue.end_group().unwrap();
    assert!(replay(&mut queue).is_empty());
    assert!(replay(&mut queue).is_empty());
    let error = queue.end_group().unwrap_err();
    assert_eq!(error, DrawQueueFault::GroupUnderflow);
    assert_eq!(error.native_error_code(), Some(0x302));
}

#[test]
fn malformed_sizes_and_submission_after_close_fail_explicitly() {
    assert_eq!(
        DrawQueue::<i32>::new(u32::MAX).unwrap_err(),
        DrawQueueFault::AllocationSize
    );
    let mut queue = DrawQueue::new(64).unwrap();
    let mut item = command(0, 1);
    item.payload_blocks = usize::MAX;
    assert_eq!(queue.submit(item), Err(DrawQueueFault::AllocationSize));
    queue.reset();
    queue.end_group().unwrap();
    assert_eq!(
        queue.submit(command(0, 1)),
        Err(DrawQueueFault::NoOpenGroup)
    );
    assert_eq!(DrawQueueFault::NoOpenGroup.native_error_code(), None);
}

#[test]
fn replay_completion_requires_the_actual_flush_and_closed_groups() {
    let mut queue = DrawQueue::new(512).unwrap();
    assert!(!queue.is_fully_replayed());
    queue.submit(command(0, 7)).unwrap();
    assert!(!queue.is_fully_replayed());
    queue.end_group().unwrap();
    assert_eq!(queue.state().open_groups, 0);
    assert!(!queue.is_fully_replayed());
    assert_eq!(replay(&mut queue), [7]);
    assert!(queue.is_fully_replayed());
    queue.reset();
    assert!(!queue.is_fully_replayed());
}

#[test]
fn completed_nested_replay_counts_payloads_without_retiring_group_headers() {
    let mut queue = DrawQueue::new(4096).unwrap();
    queue.submit(command(10, 1)).unwrap();
    queue.begin_group(20, false, prepare).unwrap();
    queue.submit(command(-100, 2)).unwrap();
    queue.begin_group(0, true, prepare).unwrap();
    queue.submit(command(5, 3)).unwrap();
    assert_eq!(queue.state().command_count, 5);
    assert_eq!(queue.state().open_groups, 3);
    assert!(!queue.is_fully_replayed());

    assert_eq!(replay(&mut queue), [2, 3, 1]);
    let completed = queue.state();
    assert_eq!(completed.command_count, 5);
    assert_eq!(completed.last_flush_completed, 3);
    assert_eq!(completed.open_groups, 0);
    assert!(queue.is_fully_replayed());
    assert_eq!(queue.state(), completed);
    // Querying completion neither consumes the retained payloads nor disables
    // the original explicit replay policy.
    assert_eq!(replay(&mut queue), [2, 3, 1]);
    assert_eq!(queue.state(), completed);
}

#[test]
fn empty_batch_completion_distinguishes_manual_close_from_genuine_flush() {
    let mut queue = DrawQueue::<i32>::new(512).unwrap();
    queue.end_group().unwrap();
    assert_eq!(queue.state().command_count, 0);
    assert_eq!(queue.state().last_flush_completed, 0);
    assert!(!queue.is_fully_replayed());
    assert_eq!(queue.flush(|_, _| Ok::<_, Infallible>(())).unwrap(), 0);
    assert!(queue.is_fully_replayed());

    queue.reset();
    queue.begin_group(0, true, prepare).unwrap();
    queue.end_group().unwrap();
    queue.end_group().unwrap();
    assert_eq!(queue.state().command_count, 1); // actual group header
    assert_eq!(queue.state().open_groups, 0);
    assert!(!queue.is_fully_replayed());
    assert_eq!(queue.flush(|_, _| Ok::<_, Infallible>(())).unwrap(), 0);
    assert!(queue.is_fully_replayed());
    assert_eq!(queue.state().command_count, 1);
}

#[test]
fn stopped_partial_replay_retains_effects_without_complete_batch_admission() {
    let mut queue = DrawQueue::new(512).unwrap();
    queue.begin_group(0, false, prepare).unwrap();
    for value in 1..=3 {
        queue.submit(command(0, value)).unwrap();
    }
    let mut seen = Vec::new();
    assert_eq!(
        queue.flush(|_, value| {
            seen.push(*value);
            *value += 10;
            if seen.len() == 2 {
                Err("reached renderer failure")
            } else {
                Ok(())
            }
        }),
        Err(DrawQueueError::Host("reached renderer failure"))
    );
    assert_eq!(seen, [1, 2]);
    assert_eq!(queue.state().command_count, 4);
    assert_eq!(queue.state().open_groups, 0);
    assert_eq!(queue.state().last_flush_completed, 1);
    assert!(queue.state().stopped);
    assert!(!queue.is_fully_replayed());
    let retained: Vec<_> = queue
        .nodes
        .iter()
        .filter_map(|node| match &node.command {
            Command::Payload(value) => Some(*value),
            Command::Group(_) => None,
        })
        .collect();
    assert_eq!(retained, [11, 12, 3]);
    let stopped = queue.state();
    assert_eq!(
        queue.flush(|_, _| Ok::<_, &str>(())),
        Err(DrawQueueError::Fault(DrawQueueFault::Stopped))
    );
    assert_eq!(queue.state(), stopped);
    assert!(!queue.is_fully_replayed());
}
