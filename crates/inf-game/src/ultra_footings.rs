//! Ultra ground detail's footings (step 5,
//! [the plan](../../../docs/rt-plans/ultra-ground.md)): the buildings a world
//! stands on its ground, so that Ultra's ground rises under them
//! ([`crate::terrain_runtime::ultra::Footing`]). Retail stands each at the
//! height of one point (furniture at its cell's, a placed model at its
//! origin's), whatever the ground does across it, so on a slope its base
//! floats on one side.

use crate::object_runtime::{ObjectArena, ObjectDrawProcessState, ObjectList};
use crate::residency::ResidentRuntime;
use crate::terrain_furniture::draw::FurnitureDrawState;
use crate::terrain_runtime::{ultra::Footing, TerrainRuntime};

/// Furniture this wide or wider stands on a footing: a building, not a
/// plant or a stone.
const WIDE: f32 = 96.0;
/// A model whose lowest corner stands higher above its origin is held above
/// the ground (a sign, a hovering pad).
const GROUNDED: i16 = 4;
/// A model's lowest corners: within this of its lowest.
const LOWEST: i16 = 8;
/// A placed model stands on the ground when its origin lies within this of
/// it, not flying over it.
const ON_GROUND: f32 = 16.0;

/// The footings of the world's buildings: the terrain's furniture that
/// stands on its base and spans a building's width, and every placed model
/// the tuning names that stands on the ground where it is.
pub(crate) fn world_footings(
    residency: &ResidentRuntime,
    arena: &ObjectArena,
    terrain: &TerrainRuntime,
    furniture: &FurnitureDrawState,
    process: &ObjectDrawProcessState,
    tuning: &crate::modern_tuning::UltraTuning,
) -> Vec<Footing> {
    let mut footings = Vec::new();
    if let Ok(table) = crate::world_furniture_effect::resident::resident_placement_table(residency)
    {
        for bucket in terrain.buckets() {
            for record in &bucket.records {
                let lookup = usize::from(record.resource_lookup.raw());
                let Some(slot) = table.slots.get(lookup).filter(|_| lookup != 0) else {
                    continue;
                };
                let Ok(Some(binding)) =
                    crate::object_runtime::resident_model_resource(residency, slot.resource_word)
                else {
                    continue;
                };
                // 0044CEF0's place: the record's cell, at its height.
                let [x, z] = [record.x << 7, bucket.y << 7];
                let origin = [
                    f32::from(x),
                    f32::from(terrain.raw_height_at([x, z])),
                    f32::from(z),
                ];
                let basis = furniture.basis(record.state);
                footings.extend(footing(binding.model, &basis.words, origin, WIDE));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    for list in [
        ObjectList::Flag0Primary,
        ObjectList::Flag0Secondary,
        ObjectList::Flag1Primary,
        ObjectList::Flag1Secondary,
        ObjectList::Parking,
    ] {
        for node in arena.list(list) {
            if !seen.insert(node.object) {
                continue;
            }
            let Some(packet) = arena.drawn_model(node.object, process) else {
                continue;
            };
            let Ok(Some(binding)) =
                crate::object_runtime::resident_model_resource(residency, packet.model_slot as u16)
            else {
                continue;
            };
            let named = binding
                .model
                .name
                .as_ref()
                .and_then(|name| name.ascii(binding.allocation.bytes()))
                .is_some_and(|name| tuning.footing(name));
            let origin = packet.position.map(|word| word as f32);
            let ground = terrain.height_at([packet.position[0] as u16, packet.position[2] as u16]);
            if named && (origin[1] - f32::from(ground)).abs() <= ON_GROUND {
                footings.extend(footing(binding.model, &packet.basis.words, origin, 0.0));
            }
        }
    }
    footings
}

/// The footing of `model` at `origin` (world X, Y, Z), turned by `basis`
/// (its axes as rows, from word 1, as its draw takes them): the hull of its
/// lowest corners, at its origin's height, or its lowest corner's where
/// that stands above it. None for a model held above its origin, or one
/// narrower than `least` across.
fn footing(
    model: &inf_formats::ModelEntry,
    basis: &[u32; 10],
    origin: [f32; 3],
    least: f32,
) -> Option<Footing> {
    let lowest = model.vertices.iter().map(|vertex| vertex.y).min()?;
    if lowest > GROUNDED {
        return None;
    }
    let rows: [[f32; 3]; 3] =
        [1, 4, 7].map(|first| std::array::from_fn(|k| f32::from_bits(basis[first + k])));
    let corners: Vec<[f32; 2]> = model
        .vertices
        .iter()
        .filter(|vertex| vertex.y <= lowest + LOWEST)
        .map(|vertex| {
            let local = [vertex.x, vertex.y, vertex.z].map(f32::from);
            let along = |axis: usize| (0..3).map(|row| local[row] * rows[row][axis]).sum::<f32>();
            [origin[0] + along(0), origin[2] + along(2)]
        })
        .collect();
    let outline = hull(corners);
    let across = |axis: usize| {
        let values = outline.iter().map(|point| point[axis]);
        values.clone().fold(f32::MIN, f32::max) - values.fold(f32::MAX, f32::min)
    };
    (outline.len() >= 3 && across(0).max(across(1)) >= least).then(|| Footing {
        outline,
        base: origin[1] + f32::from(lowest.max(0)),
    })
}

/// The convex hull of `points`, anticlockwise (Andrew's monotone chain).
fn hull(mut points: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let turn = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower: Vec<[f32; 2]> = Vec::new();
    for &point in &points {
        while lower.len() >= 2 && turn(lower[lower.len() - 2], lower[lower.len() - 1], point) <= 0.0
        {
            lower.pop();
        }
        lower.push(point);
    }
    let mut upper: Vec<[f32; 2]> = Vec::new();
    for &point in points.iter().rev() {
        while upper.len() >= 2 && turn(upper[upper.len() - 2], upper[upper.len() - 1], point) <= 0.0
        {
            upper.pop();
        }
        upper.push(point);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

#[cfg(test)]
#[path = "ultra_footings_tests.rs"]
mod tests;
