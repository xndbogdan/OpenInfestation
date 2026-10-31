//! Deferred command ownership: allocator 004A42B0, reset 004A4400,
//! group 004A4740, close/sort 004A44A0, and replay 004A4430/004A4460.
//!
//! The native arena grows commands forward and pending entries backward in
//! 16-byte units. Payloads remain typed here; offsets and capacity consumption
//! preserve the original allocation and pointer tie order without raw pointers.

use std::convert::Infallible;

use thiserror::Error;

mod memory;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawSubmission<C> {
    /// Original sort word, compared as signed i32. Its producer may encode
    /// something other than geometric depth; the queue does not reinterpret it.
    pub depth: u32,
    /// Native allocator's payload count, excluding its one-block header.
    pub payload_blocks: usize,
    /// Header+04: added to the renderer receiver by the replay callback.
    /// This is an opaque DWORD, not a checked Rust object offset.
    pub receiver_offset: u32,
    pub command: C,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawInvocation {
    pub command_offset: u32,
    pub receiver_offset: u32,
}

impl DrawInvocation {
    /// 004A444C/004A448D perform a wrapping 32-bit receiver addition.
    pub fn native_receiver(self, renderer_address: u32) -> u32 {
        renderer_address.wrapping_add(self.receiver_offset)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum DrawQueueFault {
    #[error("draw arena size or payload block count exceeds supported 32-bit allocation")]
    AllocationSize,
    #[error("draw arena exhausted: requested {requested} bytes with {available} remaining")]
    ArenaFull { requested: u32, available: u32 },
    #[error("draw group close has no open group")]
    GroupUnderflow,
    #[error("new draw commands require an open group; reset the completed batch first")]
    NoOpenGroup,
    #[error("draw queue stopped after an earlier failure; reset it before reuse")]
    Stopped,
}

impl DrawQueueFault {
    pub fn native_error_code(self) -> Option<u32> {
        match self {
            // 00528F68/70 -> native4949A0 exception records.
            Self::ArenaFull { .. } => Some(0x301),
            Self::GroupUnderflow => Some(0x302),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum DrawQueueError<E = Infallible> {
    #[error(transparent)]
    Fault(#[from] DrawQueueFault),
    #[error("required draw queue backend operation failed")]
    Host(E),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawQueueState {
    pub capacity: u32,
    pub command_cursor: u32,
    pub pending_cursor: u32,
    /// Includes the implicit sorted root until that root is closed.
    pub open_groups: usize,
    pub command_count: usize,
    pub last_flush_completed: usize,
    pub stopped: bool,
}

#[derive(Debug)]
struct Entry {
    depth: u32,
    node: usize,
}

#[derive(Debug)]
struct Group {
    sorting: bool,
    /// Native group+08 before closing, restored to arena+20 on close.
    pending_start: u32,
    /// After close these are in replay order, the reverse of the native
    /// forward list. Unsorted groups replay insertion order.
    entries: Vec<Entry>,
}

#[derive(Debug)]
enum Command<C> {
    Payload(C),
    Group(usize),
}

#[derive(Debug)]
struct Node<C> {
    invocation: DrawInvocation,
    command: Command<C>,
    allocation_bytes: u32,
}

/// Concrete deferred lists and arena accounting; actual command callbacks are
/// required at flush. Successful flush does not reset, consume, or clear data.
/// Host/validation failures stop further operations until explicit reset, a
/// Rust containment rule rather than native exception recovery.
#[derive(Debug)]
pub struct DrawQueue<C> {
    capacity: u32,
    front: u32,
    back: u32,
    nodes: Vec<Node<C>>,
    groups: Vec<Group>,
    stack: Vec<usize>,
    stopped: bool,
    /// The fault that stopped the queue, when one did.
    stop_fault: Option<DrawQueueFault>,
    last_flush_completed: usize,
    replay_completed: bool,
    memory: Option<memory::NativeDrawMemory<C>>,
    /// Emptied group lists kept from the last reset, so a frame's groups
    /// reuse their capacity.
    spare_entries: Vec<Vec<Entry>>,
}

impl<C> DrawQueue<C> {
    /// The reset-owned implicit root is still the sole open group. This is a
    /// custody check, not a count comparison between unrelated nested groups.
    pub fn has_only_implicit_root(&self) -> bool {
        !self.stopped && self.stack.as_slice() == [0]
    }

    pub fn new(capacity: u32) -> Result<Self, DrawQueueFault> {
        // 004A4320/004A43A0 align to16. Reject the native overflow instead of
        // manufacturing an undersized arena from an unrepresentable request.
        let capacity = capacity
            .checked_add(15)
            .ok_or(DrawQueueFault::AllocationSize)?
            & !15;
        let mut queue = Self {
            capacity,
            front: 0,
            back: capacity,
            nodes: Vec::new(),
            groups: Vec::new(),
            stack: Vec::new(),
            stopped: false,
            stop_fault: None,
            last_flush_completed: 0,
            replay_completed: false,
            memory: None,
            spare_entries: Vec::new(),
        };
        queue.reset();
        Ok(queue)
    }

    /// 004A4400 creates the root descriptor outside the command arena, with
    /// sorting=true. Reset also explicitly releases a stopped Rust batch.
    pub fn reset(&mut self) {
        self.front = 0;
        self.back = self.capacity;
        self.nodes.clear();
        for mut group in self.groups.drain(..) {
            group.entries.clear();
            self.spare_entries.push(group.entries);
        }
        let entries = self.spare_entries.pop().unwrap_or_default();
        self.groups.push(Group {
            sorting: true,
            pending_start: self.capacity,
            entries,
        });
        self.stack.clear();
        self.stack.push(0);
        self.stopped = false;
        self.stop_fault = None;
        self.last_flush_completed = 0;
        self.replay_completed = false;
    }

    pub fn state(&self) -> DrawQueueState {
        DrawQueueState {
            capacity: self.capacity,
            command_cursor: self.front,
            pending_cursor: self.back,
            open_groups: self.stack.len(),
            command_count: self.nodes.len(),
            last_flush_completed: self.last_flush_completed,
            stopped: self.stopped,
        }
    }

    /// True only after every payload was replayed successfully and all groups
    /// closed. Group headers consume arena nodes but are not pixel callbacks.
    /// This does not consume or release any native-style retained payload.
    pub fn is_fully_replayed(&self) -> bool {
        self.replay_completed
            && !self.stopped
            && self.stack.is_empty()
            && self.last_flush_completed
                == self
                    .nodes
                    .iter()
                    .filter(|node| matches!(node.command, Command::Payload(_)))
                    .count()
    }

    pub fn payloads(&self) -> impl Iterator<Item = &C> {
        self.nodes.iter().filter_map(|node| match &node.command {
            Command::Payload(payload) => Some(payload),
            Command::Group(_) => None,
        })
    }

    pub fn payloads_mut(&mut self) -> impl Iterator<Item = &mut C> {
        self.nodes
            .iter_mut()
            .filter_map(|node| match &mut node.command {
                Command::Payload(payload) => Some(payload),
                Command::Group(_) => None,
            })
    }

    pub fn submit(
        &mut self,
        submission: DrawSubmission<C>,
    ) -> Result<DrawInvocation, DrawQueueFault> {
        self.ensure_running()?;
        let result = self.allocate(
            submission.depth,
            submission.payload_blocks,
            submission.receiver_offset,
            Command::Payload(submission.command),
        );
        self.finish_fault(result)
    }

    /// Native renderer virtual+04 runs before any group entry/header allocation;
    /// its return value is ignored. Required preparation may submit pending
    /// commands or change groups. Allocation uses the resulting live state.
    pub fn begin_group<E>(
        &mut self,
        depth: u32,
        sorting: bool,
        prepare: impl FnOnce(&mut Self) -> Result<(), E>,
    ) -> Result<(), DrawQueueError<E>> {
        self.ensure_running()?;
        if let Err(error) = prepare(self) {
            self.stopped = true;
            return Err(DrawQueueError::Host(error));
        }
        // Catch an inner failure swallowed by the preparation host.
        self.ensure_running()?;
        let group = self.groups.len();
        let result = self.allocate(depth, 1, 0, Command::Group(group));
        let invocation = self.finish_fault(result)?;
        if let Some(memory) = &mut self.memory {
            memory.group_sorting(invocation.command_offset, sorting);
        }
        let entries = self.spare_entries.pop().unwrap_or_default();
        self.groups.push(Group {
            sorting,
            pending_start: self.back,
            entries,
        });
        self.stack.push(group);
        Ok(())
    }

    /// 004A44A0 can close the implicit root as well as explicit child groups.
    /// Closing copies16 bytes per entry to the forward arena and restores the
    /// group's saved backward cursor. No callbacks execute here.
    pub fn end_group(&mut self) -> Result<(), DrawQueueFault> {
        self.ensure_running()?;
        let result = self.close_inner();
        self.finish_fault(result)
    }

    /// 004A4460 closes every open group, then replays the root. Group commands
    /// recurse at their position in the parent list (004A4430); their depth
    /// does not bias children's independent sort words. Mutable callback data
    /// and completed side effects survive a later error or a successful replay.
    pub fn flush<E>(
        &mut self,
        mut execute: impl FnMut(DrawInvocation, &mut C) -> Result<(), E>,
    ) -> Result<usize, DrawQueueError<E>> {
        self.ensure_running()?;
        self.last_flush_completed = 0;
        self.replay_completed = false;
        while !self.stack.is_empty() {
            self.end_group()?;
        }
        // Use an explicit traversal stack; native group callbacks recurse.
        // There is no public queue mutation during an individual command.
        let mut traversal = vec![(0usize, 0usize)];
        while let Some((group, next)) = traversal.last_mut() {
            let Some(entry) = self.groups[*group].entries.get(*next) else {
                traversal.pop();
                continue;
            };
            *next += 1;
            let node = &mut self.nodes[entry.node];
            match &mut node.command {
                Command::Group(group) => traversal.push((*group, 0)),
                Command::Payload(command) => {
                    let result = execute(node.invocation, command);
                    if let Some(memory) = &mut self.memory {
                        memory.record(
                            node.invocation.command_offset,
                            node.allocation_bytes,
                            node.invocation.receiver_offset,
                            command,
                        );
                    }
                    if let Err(error) = result {
                        self.stopped = true;
                        return Err(DrawQueueError::Host(error));
                    }
                    self.last_flush_completed += 1;
                }
            }
        }
        self.replay_completed = true;
        Ok(self.last_flush_completed)
    }

    fn allocate(
        &mut self,
        depth: u32,
        blocks: usize,
        receiver_offset: u32,
        command: Command<C>,
    ) -> Result<DrawInvocation, DrawQueueFault> {
        let group = *self.stack.last().ok_or(DrawQueueFault::NoOpenGroup)?;
        let payload_bytes = u32::try_from(blocks)
            .ok()
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_mul(16))
            .ok_or(DrawQueueFault::AllocationSize)?;
        let entry_bytes = if self.groups[group].sorting { 32 } else { 16 };
        let requested = payload_bytes
            .checked_add(entry_bytes)
            .ok_or(DrawQueueFault::AllocationSize)?;
        let available = self.back - self.front;
        //004A42B9/CF and004A4754/64 write pending scratch BEFORE the
        //capacity check. Portable native pointers stay explicitly unknown.
        if let Some(memory) = &mut self.memory {
            memory.pending(self.back, self.groups[group].sorting, depth);
        }
        if requested > available {
            // Native may write scratch backward entries before raising0301,
            // but commits neither cursor nor group ownership on that path.
            return Err(DrawQueueFault::ArenaFull {
                requested,
                available,
            });
        }
        let invocation = DrawInvocation {
            command_offset: self.front,
            receiver_offset,
        };
        self.front += payload_bytes;
        self.back -= entry_bytes;
        let node = self.nodes.len();
        self.nodes.push(Node {
            invocation,
            command,
            allocation_bytes: payload_bytes,
        });
        if let Some(memory) = &mut self.memory {
            match &self.nodes[node].command {
                Command::Payload(command) => memory.record(
                    invocation.command_offset,
                    payload_bytes,
                    receiver_offset,
                    command,
                ),
                Command::Group(_) => memory.group(invocation.command_offset),
            }
        }
        self.groups[group].entries.push(Entry { depth, node });
        Ok(invocation)
    }

    fn close_inner(&mut self) -> Result<(), DrawQueueFault> {
        let group_index = *self.stack.last().ok_or(DrawQueueFault::GroupUnderflow)?;
        let group = &mut self.groups[group_index];
        if group.sorting {
            // Native min-heap extracts ascending signed depth, descending
            // unsigned COMMAND POINTER; replay reverses its output list.
            // Forward arena addresses are unique and strictly increasing, so
            // offset ordering reproduces every equal-depth tie (not material
            // address ordering and not an unspecified stable-sort assumption).
            group.entries.sort_unstable_by(|a, b| {
                (b.depth as i32).cmp(&(a.depth as i32)).then_with(|| {
                    self.nodes[a.node]
                        .invocation
                        .command_offset
                        .cmp(&self.nodes[b.node].invocation.command_offset)
                })
            });
        }
        // These bytes are covered by the group's pending entries, already
        // reserved at allocation; closing needs no additional capacity test.
        if let Some(memory) = &mut self.memory {
            if let Some(node) = self
                .nodes
                .iter()
                .find(|node| matches!(node.command, Command::Group(index) if index == group_index))
            {
                memory.group_closed(node.invocation.command_offset);
            }
            memory.close(
                self.front,
                self.back,
                group.pending_start,
                group.entries.len(),
                group.sorting,
            );
        }
        self.front += (group.entries.len() as u32) * 16;
        self.back = group.pending_start;
        self.stack.pop();
        Ok(())
    }

    fn ensure_running(&self) -> Result<(), DrawQueueFault> {
        if self.stopped {
            Err(DrawQueueFault::Stopped)
        } else {
            Ok(())
        }
    }

    fn finish_fault<T>(&mut self, result: Result<T, DrawQueueFault>) -> Result<T, DrawQueueFault> {
        if let Err(fault) = &result {
            self.stopped = true;
            self.stop_fault = Some(*fault);
        }
        result
    }

    /// The queue stopped at an arena overflow (native 0x301).
    pub fn overflowed(&self) -> bool {
        self.stopped && matches!(self.stop_fault, Some(DrawQueueFault::ArenaFull { .. }))
    }

    /// A native 0x301 catch disposing its error (004923C0): the failed
    /// allocation committed nothing, so the arena carries on as it was.
    pub fn recover_overflow(&mut self) -> bool {
        let overflowed = self.overflowed();
        if overflowed {
            self.stopped = false;
            self.stop_fault = None;
        }
        overflowed
    }
}

#[cfg(test)]
#[path = "draw_queue_tests.rs"]
mod tests;
