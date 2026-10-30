//! The cone of the view a frame draws, and the manager's pass drawing the
//! view shown apart from the game's, where runtime allocations refuse
//! writes.

use super::*;
use crate::model_context::ModelProjection;
use crate::object_runtime::ObjectDrawCone;

fn projection(focal: [i32; 2], viewport: [i32; 2], center: [i32; 2]) -> ModelProjection {
    ModelProjection {
        focal,
        viewport,
        center,
        fade: [256, 4096],
    }
}

#[test]
fn the_drawn_cone_reaches_the_corners_of_the_view_over_its_focals() {
    // Retail's tiers draw half their width at a focal of half their width,
    // and 3/4 of it down: retail's view, so nothing beyond the manager's
    // cone, though its corners reach past it.
    for [width, height] in [[640, 480], [800, 600], [1024, 768]] {
        let view = projection([width / 2; 2], [width, height], [width / 2, height / 2]);
        assert_eq!(ObjectDrawCone::for_projection(view), ObjectDrawCone::Retail);
    }
    // A zoomed view (a longer focal) is narrower still.
    let zoomed = projection([600, 600], [1024, 768], [512, 384]);
    assert_eq!(
        ObjectDrawCone::for_projection(zoomed),
        ObjectDrawCone::Retail
    );
    // The Remastered camera's 100 degrees on the 4:3 tier: 512 across and
    // 384 down over a focal of 430, 50 degrees either side.
    let remastered =
        ObjectDrawCone::for_projection(projection([430, 430], [1024, 768], [512, 384]));
    assert_eq!(
        remastered,
        ObjectDrawCone::View {
            half: [512, 384],
            focal: [430, 430]
        }
    );
    // An off-centre view reaches its far side; one only taller than
    // retail's reaches down; a widescreen frame at the tier's focal across.
    assert_eq!(
        ObjectDrawCone::for_projection(projection([430, 430], [1024, 768], [600, 384])),
        ObjectDrawCone::View {
            half: [600, 384],
            focal: [430, 430]
        }
    );
    assert_eq!(
        ObjectDrawCone::for_projection(projection([320, 320], [640, 600], [320, 300])),
        ObjectDrawCone::View {
            half: [320, 300],
            focal: [320, 320]
        }
    );
    let widescreen =
        ObjectDrawCone::for_projection(projection([512, 512], [1365, 768], [682, 384]));
    assert_eq!(
        widescreen,
        ObjectDrawCone::View {
            half: [683, 384],
            focal: [512, 512]
        }
    );
    // Depth 1000: the Remastered corners reach 1 + (512/430)^2 +
    // (384/430)^2 = 3.215 251 times the squared depth; straight to the
    // side, 50 degrees reach 1190 (squared 2 417 790), and retail stops
    // at 1000.
    assert!(remastered.admits(3_215_251, 1000));
    assert!(!remastered.admits(3_215_252, 1000));
    let side = 1000 * 1000 + 1150 * 1150;
    assert!(remastered.admits(side, 1000));
    assert!(!ObjectDrawCone::Retail.admits(side, 1000));
}

fn runtime_allocation() -> crate::object_runtime::RuntimeAllocation {
    let source: std::sync::Arc<[u8]> = vec![0; 16].into();
    crate::object_runtime::RuntimeAllocation::new(
        source,
        inf_formats::ByteSpan { start: 0, end: 16 },
    )
    .unwrap()
}

/// A runtime allocation the shown draw's arena copy shares refuses every
/// write while that draw draws, and only then; one its draws make, which
/// only the copy holds, takes them.
#[test]
fn runtime_allocations_refuse_writes_only_inside_the_shown_draw() {
    use crate::object_runtime::allocation::{refusing_writes, take_refused};
    let allocation = runtime_allocation();
    take_refused();
    assert_eq!(
        refusing_writes(|| allocation.write_word(4, 7)),
        Err(crate::object_runtime::RuntimeAllocationError::ShownDrawWrite { offset: 4 })
    );
    assert!(take_refused() && !take_refused());
    assert_eq!(allocation.word(4), Ok(0));
    refusing_writes(|| {
        let made = runtime_allocation();
        made.write_word(4, 7).unwrap();
        assert_eq!(made.word(4), Ok(7));
    });
    assert!(!take_refused());
    allocation.write_word(4, 7).unwrap();
    assert_eq!(allocation.word(4), Ok(7));
}

/// The manager's pass with two objects ahead, the first one's model
/// writing a runtime allocation: in the view shown, where allocations
/// refuse writes, the refusal omits that object alone, the pass going on
/// with the next and the allocation keeping its bytes; in the game's pass
/// the write lands and nothing is omitted. Either pass lists the visual
/// behind the eye for the tracer and draws nothing of it, nor lists the
/// base object behind, whose near callback draws nothing.
#[test]
fn a_refused_write_omits_one_object_of_the_shown_draw() {
    use crate::object_runtime::allocation::refusing_writes;
    let raw = executable_bytes();
    for shown in [true, false] {
        let mut fixture = Fixture::new(&raw);
        fixture.context.far = 1000;
        let mut arena = arena();
        let first = visual(&mut arena, ObjectClassKind::VisualBase, [0, 0, 500], 24);
        let second = visual(&mut arena, ObjectClassKind::VisualBase, [10, 0, 500], 25);
        let behind = visual(
            &mut arena,
            ObjectClassKind::VisualBase,
            [0, 0, -400_i16 as u16],
            26,
        );
        let base = arena.allocate(ObjectClassKind::Base);
        position(&mut arena, base, [0, 0, -300_i16 as u16], 0);
        let value = terrain();
        let (mut terrain, mut host, trace) = services(&value);
        let allocation = std::sync::Arc::new(runtime_allocation());
        host.write_at = Some((first, std::sync::Arc::clone(&allocation)));
        let mut pass = || {
            arena.run_flag0_object_draw_pass(Flag0ObjectDrawRequest {
                context: &mut fixture.context,
                cone: ObjectDrawCone::Retail,
                process: &mut fixture.process,
                network_flag: &fixture.network_flag,
                terrain: &mut terrain,
                lighting: ObjectDrawLighting {
                    grid: &fixture.grid,
                    tint_word: &mut fixture.tint,
                    direction: [0.25, -0.5, 0.75],
                    spacing: fixture.spacing,
                    executable: fixture.executable,
                },
                host: &mut host,
                failure_policy: Flag0ObjectDrawFailurePolicy::Stop,
            })
        };
        let drawn = if shown {
            refusing_writes(&mut pass)
        } else {
            pass()
        }
        .unwrap();
        assert_eq!(
            (drawn.near_callbacks, drawn.common_models),
            (2, 1 + usize::from(!shown))
        );
        let omitted = shown.then_some(Flag0ObjectDrawOmission {
            object: first,
            reason: ObjectDrawCallbackBoundary::ShownDrawWrite,
        });
        assert_eq!(drawn.omissions, omitted.into_iter().collect::<Vec<_>>());
        let culled: Vec<_> = drawn
            .culled
            .iter()
            .map(|object| (object.id, object.offset))
            .collect();
        assert_eq!(culled, [(behind, [0, 0, -400])]);
        let models: Vec<_> = trace
            .borrow()
            .iter()
            .filter_map(|event| match event {
                Event::Model(packet, ..) => Some(packet.source),
                _ => None,
            })
            .collect();
        assert_eq!(models, [first, second]);
        assert_eq!(allocation.word(0), Ok(if shown { 0 } else { 0x1234 }));
    }
}
