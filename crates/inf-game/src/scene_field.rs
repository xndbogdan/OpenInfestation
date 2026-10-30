//! The field across an active warpgate (modern mode, 2026-10-22: a
//! better, more modern effect): a shimmering sheet of energy
//! filling the gate's opening while its lightning plays, which the
//! composite lays over the world, under retail's arcs
//! ([the plan](../../../docs/INFESTATION-RT-IMPLEMENTATION.md#light-sources)).
//!
//! Its shape comes from the arcs themselves, the blended edges of the
//! models the tuning file's `materials.fields` names (`gatebod`'s lightning,
//! sprites 595 to 597, drawn only while the gate is on): most of them meet
//! at one point, and their far ends trace the opening's frame. The field is
//! the ellipse inscribed in those ends' extent, in their plane.

use crate::scene_feed::SceneArc;

/// Arcs this close to another (world units) belong to the same gate.
const GATE_REACH: f32 = 1500.0;
/// Ends this close are one point.
const SAME_POINT: f32 = 4.0;
/// The fewest arcs, and the fewest meeting at one point, that make a field.
const FEWEST_ARCS: usize = 4;
const FEWEST_MEETING: usize = 3;

/// A field: its centre (eye-relative), its half-width and half-height as
/// vectors in its plane, how strongly it shows and its colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Field {
    pub centre: [f32; 3],
    pub across: [f32; 3],
    pub up: [f32; 3],
    pub strength: f32,
    pub color: [f32; 3],
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

fn scaled(a: [f32; 3], by: f32) -> [f32; 3] {
    a.map(|v| v * by)
}

/// The fields `arcs` span, the nearest first.
pub fn fields(arcs: &[SceneArc]) -> Vec<Field> {
    // Each gate's arcs together.
    let mut gates: Vec<Vec<&SceneArc>> = Vec::new();
    for arc in arcs {
        let near = |other: &&SceneArc| {
            other.model == arc.model
                && other
                    .ends
                    .iter()
                    .any(|a| arc.ends.iter().any(|b| length(sub(*a, *b)) < GATE_REACH))
        };
        match gates.iter_mut().find(|gate| gate.iter().any(near)) {
            Some(gate) => gate.push(arc),
            None => gates.push(vec![arc]),
        }
    }
    let mut fields: Vec<Field> = gates.iter().filter_map(|gate| field(gate)).collect();
    fields.sort_by(|a, b| length(a.centre).total_cmp(&length(b.centre)));
    fields
}

fn field(arcs: &[&SceneArc]) -> Option<Field> {
    if arcs.len() < FEWEST_ARCS {
        return None;
    }
    let ends: Vec<[f32; 3]> = arcs.iter().flat_map(|arc| arc.ends).collect();
    // Where most of them meet.
    let meeting = |point: [f32; 3]| {
        ends.iter()
            .filter(|end| length(sub(**end, point)) < SAME_POINT)
            .count()
    };
    let (hub, count) = ends
        .iter()
        .map(|end| (*end, meeting(*end)))
        .max_by_key(|(_, count)| *count)?;
    if count < FEWEST_MEETING {
        return None;
    }
    // The far ends, each once.
    let mut frame: Vec<[f32; 3]> = Vec::new();
    for end in &ends {
        if length(sub(*end, hub)) >= SAME_POINT
            && frame
                .iter()
                .all(|other| length(sub(*other, *end)) >= SAME_POINT)
        {
            frame.push(*end);
        }
    }
    if frame.len() < 3 {
        return None;
    }
    // Their plane, round the hub: the largest turn between two of them
    // sets the side, and every pair's adds to it.
    let spokes: Vec<[f32; 3]> = frame.iter().map(|end| sub(*end, hub)).collect();
    let mut turns = Vec::new();
    for (i, a) in spokes.iter().enumerate() {
        for b in &spokes[i + 1..] {
            turns.push(cross(*a, *b));
        }
    }
    let side = *turns
        .iter()
        .max_by(|a, b| length(**a).total_cmp(&length(**b)))?;
    let mut normal = [0.0; 3];
    for turn in &turns {
        let sign = if dot(*turn, side) < 0.0 { -1.0 } else { 1.0 };
        for axis in 0..3 {
            normal[axis] += turn[axis] * sign;
        }
    }
    let size = length(normal);
    if size <= f32::EPSILON {
        return None;
    }
    let normal = scaled(normal, 1.0 / size);
    // Across it level, and up it as near the world's up as it goes.
    let level = cross([0.0, 1.0, 0.0], normal);
    let across = if length(level) > 1.0e-3 {
        scaled(level, 1.0 / length(level))
    } else {
        [1.0, 0.0, 0.0]
    };
    let up = cross(normal, across);
    // The ellipse inscribed in the far ends' extent.
    let along = |axis: [f32; 3]| {
        let values: Vec<f32> = spokes.iter().map(|spoke| dot(*spoke, axis)).collect();
        let low = values.iter().copied().fold(f32::MAX, f32::min).min(0.0);
        let high = values.iter().copied().fold(f32::MIN, f32::max).max(0.0);
        (0.5 * (low + high), 0.5 * (high - low))
    };
    let (mid_across, half_across) = along(across);
    let (mid_up, half_up) = along(up);
    if half_across < 20.0 || half_up < 20.0 {
        return None;
    }
    let centre: [f32; 3] =
        std::array::from_fn(|axis| hub[axis] + across[axis] * mid_across + up[axis] * mid_up);
    Some(Field {
        centre,
        across: scaled(across, half_across),
        up: scaled(up, half_up),
        strength: arcs[0].strength,
        color: arcs[0].color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gate standing upright at `at`, facing along Z: arcs from eight
    /// points of a 300-unit ring to its middle, and two along its frame.
    fn gate(at: [f32; 3]) -> Vec<SceneArc> {
        let ring: Vec<[f32; 3]> = (0..8)
            .map(|index| {
                let angle = std::f32::consts::TAU * index as f32 / 8.0;
                [
                    at[0] + 300.0 * angle.cos(),
                    at[1] + 300.0 * angle.sin(),
                    at[2],
                ]
            })
            .collect();
        let arc = |a, b| SceneArc {
            model: 63,
            ends: [a, b],
            strength: 0.5,
            color: [0.5, 0.7, 1.0],
        };
        let mut arcs: Vec<SceneArc> = ring.iter().map(|end| arc(*end, at)).collect();
        arcs.push(arc(ring[0], ring[1]));
        arcs.push(arc(ring[4], ring[5]));
        arcs
    }

    /// The field fills the ring, in its plane, centred on it; each gate
    /// spans its own, the nearer first.
    #[test]
    fn a_gate_spans_a_field_across_its_ring() {
        let mut arcs = gate([5000.0, 400.0, 2000.0]);
        arcs.extend(gate([1000.0, 400.0, 2000.0]));
        let fields = fields(&arcs);
        assert_eq!(fields.len(), 2);
        let field = fields[0];
        for (axis, value) in [1000.0, 400.0, 2000.0].into_iter().enumerate() {
            assert!((field.centre[axis] - value).abs() < 1.0, "{field:?}");
        }
        assert!((length(field.across) - 300.0).abs() < 1.0, "{field:?}");
        assert!((length(field.up) - 300.0).abs() < 1.0, "{field:?}");
        // Upright: across it level, up it the world's up.
        assert!(
            field.across[1].abs() < 1.0e-3 && field.up[1] > 299.0,
            "{field:?}"
        );
        assert_eq!(field.strength, 0.5);
    }

    /// A few arcs, or arcs that meet nowhere, span no field.
    #[test]
    fn scattered_arcs_span_none() {
        let arcs = gate([0.0, 0.0, 0.0]);
        assert!(fields(&arcs[..3]).is_empty());
        let scattered: Vec<SceneArc> = (0..6)
            .map(|index| SceneArc {
                model: 63,
                ends: [
                    [index as f32 * 100.0, 0.0, 0.0],
                    [index as f32 * 100.0 + 50.0, 80.0, 0.0],
                ],
                strength: 0.5,
                color: [1.0; 3],
            })
            .collect();
        assert!(fields(&scattered).is_empty());
    }
}
