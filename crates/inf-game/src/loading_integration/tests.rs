//! Real physical OVL reads through the shared callback and Draw policies.
//! The sink records direct packets; it does not claim pixel/SDL acceptance.

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_display::frontend_startup,
    input::InputContextId,
    loading_draw::{draw_loading_tick_40e6b0, LoadingDrawHost, LoadingDrawRequest},
    menu_text::{MenuGlyphQuad, MenuTextHost},
    render_resources::RenderResources,
    scene_sprite::DirectSpriteQuad,
    Error, Installation, ResourceSelection,
};
use std::path::Path;

#[derive(Clone, Copy)]
enum ContextLifetime {
    Current,
    Removed,
}

struct Fixture {
    runtime: ResidentRuntime,
    registry: ScreenRegistry<FrontendRenderContext, InputContextId>,
    owner: ContextOwnerId,
    reads: ResourceReadState,
    process: LoadingProcessState,
    effects: [u8; 16],
    rng: u32,
    mode: Cell<u8>,
    attract: bool,
}

impl Fixture {
    fn new(lifetime: ContextLifetime) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).unwrap();
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime
            .apply(ResidencyRequest::Select {
                selection: ResourceSelection {
                    difficulty: 2,
                    language: 0,
                },
            })
            .unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        let mut registry = ScreenRegistry::new();
        let owner = registry.register_context(FrontendRenderContext {
            projection: frontend_startup(
                &runtime,
                crate::presentation::NativeFrame::original([640, 480]),
            )
            .unwrap()
            .projection,
        });
        if matches!(lifetime, ContextLifetime::Removed) {
            registry.remove_context(owner).unwrap();
        }
        Self {
            runtime,
            registry,
            owner,
            reads: ResourceReadState::native_startup(),
            process: LoadingProcessState::native_startup(assets.executable_bytes()).unwrap(),
            effects: [73; 16],
            rng: 0x1234_5678,
            mode: Cell::new(1),
            attract: false,
        }
    }

    fn world_activation(&mut self, selected: u8, host: &mut Presentation) -> crate::Result<()> {
        let mut reads = WorldLoadReads::Interactive(WorldLoadingInputs {
            reads: &mut self.reads,
            process: &mut self.process,
            effects: &mut self.effects,
            context: LoadingContextSource {
                registry: &self.registry,
                owner: self.owner,
            },
            current_attract: &self.attract,
            current_mode: &self.mode,
            presentation: host,
        });
        activate_world_resources(&mut self.runtime, selected, &mut self.rng, &mut reads)
    }

    fn nonempty_activation(&mut self, level: u8, host: &mut Presentation) {
        self.reads
            .exchange_callback(ResourceReadCallback::LoadingCallback);
        let mut callback = WorldReadCallback {
            process: &mut self.process,
            effects: &mut self.effects,
            context: LoadingContextSource {
                registry: &self.registry,
                owner: self.owner,
            },
            current_attract: &self.attract,
            current_mode: &self.mode,
            presentation: host,
            rng: &mut self.rng,
        };
        self.runtime
            .apply(ResidencyApplyRequest {
                operation: ResidencyRequest::Activate { level },
                reads: ResidencyReads::Loading {
                    state: &mut self.reads,
                    host: &mut callback,
                },
            })
            .unwrap();
    }
}

struct TickRecord {
    committed: usize,
    leaves: Vec<u8>,
    revision: u64,
    process: usize,
    effects: usize,
    rng: usize,
    context: usize,
    delta: i32,
}

struct Presentation {
    now: u64,
    clock_samples: usize,
    available: bool,
    renderer_checks: usize,
    records: Vec<TickRecord>,
    presented: Vec<Vec<u8>>,
    disposed: Vec<String>,
    quads: usize,
    glyphs: usize,
}

impl Presentation {
    fn new(available: bool) -> Self {
        Self {
            now: 0,
            clock_samples: 0,
            available,
            renderer_checks: 0,
            records: Vec::new(),
            presented: Vec::new(),
            disposed: Vec::new(),
            quads: 0,
            glyphs: 0,
        }
    }
}

impl LoadingPresentationHost for Presentation {
    fn sample_clock(&mut self) -> u64 {
        // Advance only for genuine file-read/callback clock requests. No
        // fabricated per-overlay pulse or second scheduler clock is supplied.
        self.clock_samples += 1;
        self.now += 50_001;
        self.now
    }

    fn renderer_available(&mut self) -> Result<bool, String> {
        self.renderer_checks += 1;
        Ok(self.available)
    }

    fn tick_loading(&mut self, request: LoadingDrawInputs<'_>) -> Result<(), String> {
        self.records.push(TickRecord {
            committed: request.residency as *const ResidentRuntime as usize,
            leaves: request.residency.summary().active_leaves,
            revision: request.residency.revision(),
            process: &*request.tick.state as *const LoadingProcessState as usize,
            effects: &*request.tick.effect_bytes as *const [u8; 16] as usize,
            rng: &*request.tick.rng as *const u32 as usize,
            context: request.context as *const FrontendRenderContext as usize,
            delta: request.tick.delta_micros,
        });
        let mut sink = DirectPackets {
            attract: request.current_attract,
            mode: request.current_mode,
            quads: &mut self.quads,
            glyphs: &mut self.glyphs,
        };
        // Use actual current banks and the complete shared Draw owner, not a
        // hand-written effect/RNG approximation in this integration harness.
        draw_loading_tick_40e6b0(
            LoadingDrawRequest {
                tick: request.tick,
                context_origin_pixels: [320, 240],
            },
            &RenderResources::new(request.residency),
            &mut sink,
        )
        .map_err(|error| error.to_string())
    }

    fn present_loading(&mut self, residency: &ResidentRuntime) -> Result<(), String> {
        self.presented.push(residency.summary().active_leaves);
        Ok(())
    }

    fn dispose_loading_error(&mut self, error: String) {
        self.disposed.push(error);
    }
}

/// Required recording sink for the genuine direct producer. Its dimensions
/// are explicitly supplied classic framebuffer values, not retail evidence.
struct DirectPackets<'a> {
    attract: &'a bool,
    mode: &'a Cell<u8>,
    quads: &'a mut usize,
    glyphs: &'a mut usize,
}

impl MenuTextHost for DirectPackets<'_> {
    type Error = String;
    fn submit_glyph(&mut self, _: MenuGlyphQuad) -> Result<(), String> {
        *self.glyphs += 1;
        Ok(())
    }
}

impl LoadingDrawHost for DirectPackets<'_> {
    fn acquire_renderer(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn surface_width(&mut self) -> Result<i32, String> {
        Ok(640)
    }
    fn surface_height(&mut self) -> Result<i32, String> {
        Ok(480)
    }
    fn acquire_surface(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn reset_batch_40e6fd(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn reset_surface_40e70a(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn current_attract_mode(&mut self) -> u8 {
        u8::from(*self.attract)
    }
    fn current_loading_mode(&mut self) -> u8 {
        self.mode.get()
    }
    fn draw_model_background(
        &mut self,
        _: &LoadingProcessState,
        _: &mut u32,
    ) -> Result<(), String> {
        panic!("direct packet integration fixture admits nonzero loading mode only")
    }
    fn finish_background_495770(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn flush_batch_4a4460(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn submit_loading_quad(&mut self, _: DirectSpriteQuad) -> Result<(), String> {
        *self.quads += 1;
        Ok(())
    }
}

#[test]
fn ordinary_activations_use_committed_48_and_same_retained_sources_across_repeated_loads() {
    let mut f = Fixture::new(ContextLifetime::Current);
    f.process.counter_546918 = 31;
    f.process.counter_54691c = 47;
    f.process.age_micros = 91;
    f.process.pulse = 0xbeef_0000;
    let pointers = (
        &f.runtime as *const ResidentRuntime as usize,
        &f.process as *const LoadingProcessState as usize,
        &f.effects as *const [u8; 16] as usize,
        &f.rng as *const u32 as usize,
        f.registry.backend(f.owner).unwrap() as *const FrontendRenderContext as usize,
    );
    let mut host = Presentation::new(true);
    let mut age = 91u32;
    for selected in [32, 28] {
        let first = host.records.len();
        let old_leaves = f.runtime.summary().active_leaves;
        let old_revision = f.runtime.revision();
        let old_clock = f.process.previous_clock;
        f.world_activation(selected, &mut host).unwrap();
        assert!(host.records.len() > first);
        for record in &host.records[first..] {
            // Empty48 can require its nonempty ancestor1. Those reads see
            // the previous committed table; selected-world reads see48.
            if record.revision == old_revision {
                assert_eq!(record.leaves, old_leaves);
            } else {
                assert_eq!(record.revision, old_revision + 1);
                assert!(record.leaves.contains(&48));
                assert!(!record.leaves.contains(&selected));
            }
            assert_eq!(
                (
                    record.committed,
                    record.process,
                    record.effects,
                    record.rng,
                    record.context
                ),
                pointers
            );
            age = age.wrapping_add(record.delta as u32);
        }
        assert!(host.records[first..].iter().any(|r| r.leaves.contains(&48)));
        if selected == 32 {
            assert!(host.records[first..]
                .iter()
                .any(|r| r.revision == old_revision));
        }
        assert_eq!(f.process.age_micros, age);
        assert!(f.process.previous_clock > old_clock);
        assert_eq!(f.process.pulse >> 16, 0xbeef);
        assert_eq!(
            [
                f.process.counter_546918,
                f.process.counter_54691c,
                f.process.counter_546920
            ],
            [31, 47, 1]
        );
        assert_eq!(f.reads.callback, ResourceReadCallback::None);
        assert!(f.reads.last_clock > 0);
        assert!(f.runtime.summary().active_leaves.contains(&selected));
    }
    assert!(host.disposed.is_empty());
    assert_eq!(host.presented.len(), host.records.len());
    assert_eq!(host.quads, host.records.len() * 16);
    assert!(host.glyphs > 0);
    assert_ne!(f.effects, [73; 16]);
    assert_ne!(f.rng, 0x1234_5678);
}

#[test]
fn real_activation_views_follow_commits_and_empty_48_still_reads_its_required_ancestor() {
    let mut f = Fixture::new(ContextLifetime::Current);
    let before = f.runtime.summary().active_leaves;
    let revision = f.runtime.revision();
    let mut host = Presentation::new(true);
    f.nonempty_activation(47, &mut host);
    assert!(!host.records.is_empty());
    assert!(host
        .records
        .iter()
        .all(|r| r.leaves == before && r.revision == revision));
    let old_records = host.records.len();
    f.world_activation(32, &mut host).unwrap();
    assert!(host.records.len() > old_records);
    let records = &host.records[old_records..];
    assert!(records.iter().any(|r| !r.leaves.contains(&48)));
    assert!(records.iter().any(|r| r.leaves.contains(&48)));
    assert!(records
        .iter()
        .all(|r| !r.leaves.contains(&32) && r.revision > revision));
    assert!(host.disposed.is_empty());
    assert_eq!(f.reads.callback, ResourceReadCallback::None);
}

#[test]
fn unknown_selected_leaf_keeps_48_ancestor_read_prefix_and_installed_callback() {
    let mut f = Fixture::new(ContextLifetime::Current);
    f.reads.chunk = 3100;
    f.reads.last_clock = 0x1234_5678_9abc_def0;
    f.reads.pending = 93;
    f.process.counter_546918 = 17;
    f.process.raw_progress = 0x8765_4321;
    let old_revision = f.runtime.revision();
    let mut expected = Fixture::new(ContextLifetime::Current);
    expected.reads.chunk = f.reads.chunk;
    expected.reads.last_clock = f.reads.last_clock;
    expected.reads.pending = f.reads.pending;
    expected.process = f.process.clone();
    let mut expected_host = Presentation::new(true);
    // This actual same48 activation performs the same ancestor reads; its
    // second48 request is empty. It reaches normal NULL removal instead of
    // the unknown-leaf error. Nothing fabricates the expected read prefix.
    expected.world_activation(48, &mut expected_host).unwrap();
    let mut host = Presentation::new(true);
    assert!(matches!(
        f.world_activation(255, &mut host),
        Err(Error::UnknownScene { level: 255 })
    ));
    assert_eq!(f.runtime.summary(), expected.runtime.summary());
    assert_eq!(f.runtime.revision(), old_revision + 1);
    assert_eq!(f.reads.callback, ResourceReadCallback::LoadingCallback);
    assert_eq!(expected.reads.callback, ResourceReadCallback::None);
    assert_eq!(
        (f.reads.chunk, f.reads.last_clock, f.reads.pending),
        (
            expected.reads.chunk,
            expected.reads.last_clock,
            expected.reads.pending
        )
    );
    assert_eq!(f.process, expected.process);
    assert_eq!(f.rng, expected.rng);
    assert_eq!(f.effects, expected.effects);
    assert!(host.clock_samples > 0);
    assert_eq!(host.clock_samples, expected_host.clock_samples);
    assert_eq!(host.renderer_checks, expected_host.renderer_checks);
    assert_eq!(host.quads, expected_host.quads);
    assert!(host.disposed.is_empty() && expected_host.disposed.is_empty());
}

#[test]
fn renderer_null_bypasses_removed_context_but_present_renderer_contains_its_reached_error() {
    for available in [false, true] {
        let mut f = Fixture::new(ContextLifetime::Removed);
        let old_effects = f.effects;
        let old_rng = f.rng;
        let mut host = Presentation::new(available);
        f.world_activation(32, &mut host).unwrap();
        assert!(host.renderer_checks > 0);
        assert!(f.process.previous_clock > 0 && f.process.age_micros > 0);
        assert!(f.process.raw_progress > 0);
        assert_eq!(f.process.counter_546920, 1);
        assert_ne!(f.rng, old_rng); // pulse RNG precedes the renderer/context guard
        assert_eq!(f.effects, old_effects);
        assert!(host.records.is_empty() && host.presented.is_empty());
        assert_eq!(host.quads, 0);
        assert_eq!(f.reads.callback, ResourceReadCallback::None);
        assert!(f.runtime.summary().active_leaves.contains(&32));
        if available {
            assert_eq!(host.disposed.len(), host.renderer_checks);
            assert!(host
                .disposed
                .iter()
                .all(|error| error.starts_with("loading session context:")));
        } else {
            assert!(host.disposed.is_empty());
        }
    }
}
