//! The Vulkan device's own timing, a line when it closes, beside the
//! tracer's (`trace.rs`): per frame (from one swap to the next), how long
//! it took, how much of that the CPU worked and how much it waited for the
//! GPU, the GPU's time over all of the frame's submissions (the trace, the
//! raster's draws, the HUD, the copy to the screen), the swapchain's
//! acquire and present, and the CPU's time recording the traced world.
//! The device does not wait for a frame's submission at its swap: the next
//! frame's CPU work runs while the GPU finishes it, and `gpu wait` is the
//! time the CPU still waited (for the slot it records next, or for the last
//! frame where it rewrites what that frame's GPU work reads).

use std::time::{Duration, Instant};

use ash::vk;

use super::{Core, FRAMES_IN_FLIGHT};

/// The first frames, which load the world and make what it needs, are left
/// out: the report is of play.
pub(super) const WARMUP_FRAMES: u64 = 30;

/// What each frame is measured by, as the report names them.
const MEASURES: [&str; 6] = ["frame", "cpu", "gpu wait", "gpu", "present", "trace record"];

#[derive(Default)]
pub(super) struct FramePerf {
    pool: vk::QueryPool,
    /// Nanoseconds a timestamp tick.
    period: f64,
    /// Each frame slot's recording began with a timestamp (its pair of
    /// queries in `pool`).
    stamped: [bool; FRAMES_IN_FLIGHT],
    /// This frame so far, in milliseconds: waiting, the GPU's time, the
    /// swapchain's and the trace's recording.
    wait: f64,
    gpu: f64,
    present: f64,
    trace_record: f64,
    last_swap: Option<Instant>,
    /// Frames swapped, and the frames measured after the warm-up.
    swaps: u64,
    frames: u64,
    sums: [f64; MEASURES.len()],
    most: [f64; MEASURES.len()],
}

fn milliseconds(span: Duration) -> f64 {
    span.as_secs_f64() * 1000.0
}

impl FramePerf {
    /// The timing with a pool of two timestamps a frame slot where the device stamps its
    /// graphics and compute work; without one, the CPU's side only.
    pub(super) unsafe fn new(core: &Core) -> Self {
        let limits = core
            .instance
            .get_physical_device_properties(core.physical)
            .limits;
        let mut perf = Self::default();
        if limits.timestamp_compute_and_graphics == vk::TRUE {
            if let Ok(pool) = core.device.create_query_pool(
                &vk::QueryPoolCreateInfo::default()
                    .query_type(vk::QueryType::TIMESTAMP)
                    .query_count(2 * FRAMES_IN_FLIGHT as u32),
                None,
            ) {
                perf.pool = pool;
                perf.period = f64::from(limits.timestamp_period);
            }
        }
        perf
    }

    pub(super) unsafe fn destroy(&mut self, core: &Core) {
        if self.pool != vk::QueryPool::null() {
            core.device.destroy_query_pool(self.pool, None);
            self.pool = vk::QueryPool::null();
        }
    }

    /// A recording begins on `command`, in frame slot `slot`: stamp its
    /// start.
    pub(super) unsafe fn opened(&mut self, core: &Core, command: vk::CommandBuffer, slot: usize) {
        if self.pool == vk::QueryPool::null() {
            return;
        }
        let first = 2 * slot as u32;
        core.device
            .cmd_reset_query_pool(command, self.pool, first, 2);
        core.device.cmd_write_timestamp2(
            command,
            vk::PipelineStageFlags2::TOP_OF_PIPE,
            self.pool,
            first,
        );
        self.stamped[slot] = true;
    }

    /// The recording on `command`, in frame slot `slot`, ends: stamp its
    /// end.
    pub(super) unsafe fn closing(&mut self, core: &Core, command: vk::CommandBuffer, slot: usize) {
        if self.stamped[slot] {
            core.device.cmd_write_timestamp2(
                command,
                vk::PipelineStageFlags2::ALL_COMMANDS,
                self.pool,
                2 * slot as u32 + 1,
            );
        }
    }

    /// Frame slot `slot`'s submission was waited for, `waited` long: its
    /// GPU time is known.
    pub(super) unsafe fn waited(&mut self, core: &Core, slot: usize, waited: Duration) {
        self.wait += milliseconds(waited);
        if !std::mem::take(&mut self.stamped[slot]) {
            return;
        }
        let mut ticks = [0_u64; 2];
        if core
            .device
            .get_query_pool_results(
                self.pool,
                2 * slot as u32,
                &mut ticks,
                vk::QueryResultFlags::TYPE_64,
            )
            .is_ok()
        {
            self.gpu += ticks[1].wrapping_sub(ticks[0]) as f64 * self.period / 1.0e6;
        }
    }

    /// The swapchain's acquire and present took `span`.
    pub(super) fn presented(&mut self, span: Duration) {
        self.present += milliseconds(span);
    }

    /// The traced world's recording took the CPU `span`.
    pub(super) fn traced(&mut self, span: Duration) {
        self.trace_record += milliseconds(span);
    }

    /// The frame ends with its swap.
    pub(super) fn swapped(&mut self) {
        let now = Instant::now();
        self.swaps += 1;
        if let Some(last) = self
            .last_swap
            .replace(now)
            .filter(|_| self.swaps > WARMUP_FRAMES)
        {
            let frame = milliseconds(now - last);
            let measures = [
                frame,
                (frame - self.wait - self.present).max(0.0),
                self.wait,
                self.gpu,
                self.present,
                self.trace_record,
            ];
            self.frames += 1;
            for (index, value) in measures.into_iter().enumerate() {
                self.sums[index] += value;
                self.most[index] = self.most[index].max(value);
            }
        }
        self.wait = 0.0;
        self.gpu = 0.0;
        self.present = 0.0;
        self.trace_record = 0.0;
    }

    /// One line on the frames, once there were some.
    pub(super) fn report(&self) {
        if self.frames == 0 {
            return;
        }
        let parts: Vec<String> = MEASURES
            .iter()
            .zip(self.sums.iter().zip(&self.most))
            .map(|(name, (sum, most))| {
                format!("{name} {:.2}/{:.2}", sum / self.frames as f64, most)
            })
            .collect();
        eprintln!(
            "Vulkan frames: {} frames after {WARMUP_FRAMES}, mean/most ms: {}",
            self.frames,
            parts.join(", ")
        );
    }
}
