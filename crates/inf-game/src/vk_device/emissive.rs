//! The traced scene's emissive light list ([the full path tracing
//! plan](../../../../docs/rt-plans/full-path-tracing.md), stage 5): every
//! face retail drew unlit, gathered each frame from the feed into groups by
//! the object that drew it and where it lies, each face with its power (its
//! area times its light). The path pass samples them as lights (next-event
//! estimation, `pt_trace.comp`'s `emissive_sample`), weighed against the
//! bounce ray that meets the same face, so a small lamp or sign lights what
//! stands by it without waiting for a bounce ray to find it.

use std::collections::HashMap;

use crate::scene_feed::{SceneSurface, SceneTriangle, SceneVertex};

/// The most groups the path pass weighs at each pixel (its pick loops over
/// them all), the likeliest from the eye first; faces of the groups past
/// them light only through the rays that meet them, as before.
pub(super) const MAX_GROUPS: usize = 64;
/// A group's faces lie within one cell this wide (world units, about a
/// building's side), so its bounding sphere weighs it well up close.
const CELL: f32 = 256.0;
/// No face in the list (`pt_trace.comp`'s `NO_LIGHT`).
pub(super) const NO_FACE: u32 = u32::MAX;

/// A face of the list, as `pt_trace.comp`'s `EmissiveFace`: its triangle
/// (in the trace's triangle order), its group, its area, its power (area
/// times its light's luminance, the pick's weight within its group), how
/// many times brighter than retail it glows (`materials.emitters`) and its
/// group's power up to and with it, which the pick binary-searches.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct EmissiveFace {
    pub triangle: u32,
    pub group: u32,
    pub area: f32,
    pub power: f32,
    pub gain: f32,
    pub running: f32,
}

/// A group of faces, as `pt_trace.comp`'s `EmissiveGroup`: the sphere round
/// them (eye-relative), their power, and where its faces lie in the list.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct EmissiveGroup {
    pub centre: [f32; 3],
    pub radius: f32,
    pub power: f32,
    pub first: u32,
    pub count: u32,
    pub pad: u32,
}

/// The frame's list: the faces by group, the groups the likeliest from the
/// eye first, how many of them the path pass samples, and each triangle's
/// face (`NO_FACE` for none).
#[derive(Debug, Default)]
pub(super) struct EmissiveList {
    pub faces: Vec<EmissiveFace>,
    pub groups: Vec<EmissiveGroup>,
    pub sampled: usize,
    pub of_triangle: Vec<u32>,
}

/// sRGB's transfer from a byte to linear light.
fn linear(byte: u8) -> f32 {
    let c = f32::from(byte) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// How bright a texture's RGBA texels are on average, in linear light
/// (their luminance; keyed texels, which rays pass, none): a face's power
/// counts its texture by it.
pub(super) fn texture_brightness(rgba: &[u8]) -> f32 {
    let count = rgba.len() / 4;
    if count == 0 {
        return 1.0;
    }
    let sum: f64 = rgba
        .chunks_exact(4)
        .filter(|texel| texel[3] >= 128)
        .map(|texel| {
            f64::from(
                0.2126 * linear(texel[0]) + 0.7152 * linear(texel[1]) + 0.0722 * linear(texel[2]),
            )
        })
        .sum();
    (sum / count as f64) as f32
}

/// The list over `triangles` (the trace's order) and their `vertices`:
/// each `Emissive` triangle with light, `gain` its brightness over
/// retail's and `brightness` its texture's (texture_brightness). The
/// `Shown` faces (the land band, the lamps' lenses) stay out: they are
/// seen, not lights.
pub(super) fn emissive_list<'a>(
    triangles: impl IntoIterator<Item = &'a SceneTriangle>,
    vertices: &[SceneVertex],
    gain: impl Fn(&SceneTriangle) -> f32,
    brightness: impl Fn(&SceneTriangle) -> f32,
) -> EmissiveList {
    struct Gathered {
        faces: Vec<EmissiveFace>,
        low: [f32; 3],
        high: [f32; 3],
    }
    let mut of_triangle = Vec::new();
    let mut gathered: Vec<Gathered> = Vec::new();
    let mut by_key: HashMap<(Option<u32>, Option<u16>, [i32; 3]), usize> = HashMap::new();
    for (index, triangle) in triangles.into_iter().enumerate() {
        of_triangle.push(NO_FACE);
        if triangle.surface != SceneSurface::Emissive {
            continue;
        }
        let [a, b, c] = triangle.vertices.map(|vertex| &vertices[vertex as usize]);
        let edge =
            |to: &SceneVertex| std::array::from_fn::<f32, 3, _>(|k| to.position[k] - a.position[k]);
        let (e0, e1) = (edge(b), edge(c));
        let cross = [
            e0[1] * e1[2] - e0[2] * e1[1],
            e0[2] * e1[0] - e0[0] * e1[2],
            e0[0] * e1[1] - e0[1] * e1[0],
        ];
        let area = 0.5 * cross.iter().map(|v| v * v).sum::<f32>().sqrt();
        let colour: [f32; 3] = std::array::from_fn(|k| {
            (linear(a.color[k]) + linear(b.color[k]) + linear(c.color[k])) / 3.0
        });
        let luminance = 0.2126 * colour[0] + 0.7152 * colour[1] + 0.0722 * colour[2];
        let gain = gain(triangle).max(0.0);
        let power = area * luminance * gain * brightness(triangle);
        if !(power > 0.0) {
            continue;
        }
        let middle: [f32; 3] =
            std::array::from_fn(|k| (a.position[k] + b.position[k] + c.position[k]) / 3.0);
        let key = (
            triangle.object,
            triangle.model,
            middle.map(|v| (v / CELL).floor() as i32),
        );
        let at = *by_key.entry(key).or_insert_with(|| {
            gathered.push(Gathered {
                faces: Vec::new(),
                low: [f32::MAX; 3],
                high: [f32::MIN; 3],
            });
            gathered.len() - 1
        });
        let group = &mut gathered[at];
        for corner in [a, b, c] {
            for k in 0..3 {
                group.low[k] = group.low[k].min(corner.position[k]);
                group.high[k] = group.high[k].max(corner.position[k]);
            }
        }
        group.faces.push(EmissiveFace {
            triangle: index as u32,
            group: 0,
            area,
            power,
            gain,
            running: 0.0,
        });
    }
    // Each group's sphere, and how likely it lights what the eye sees: its
    // power over its distance squared, its own size at the least.
    let mut spheres: Vec<(f32, EmissiveGroup, Vec<EmissiveFace>)> = gathered
        .into_iter()
        .map(|group| {
            let centre: [f32; 3] = std::array::from_fn(|k| 0.5 * (group.low[k] + group.high[k]));
            let radius = (0..3)
                .map(|k| 0.5 * (group.high[k] - group.low[k]))
                .map(|half| half * half)
                .sum::<f32>()
                .sqrt();
            let power = group.faces.iter().map(|face| face.power).sum::<f32>();
            let d2 = centre.iter().map(|v| v * v).sum::<f32>();
            let likely = power / d2.max(radius * radius).max(1.0);
            let sphere = EmissiveGroup {
                centre,
                radius,
                power,
                ..Default::default()
            };
            (likely, sphere, group.faces)
        })
        .collect();
    spheres.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut list = EmissiveList {
        sampled: spheres.len().min(MAX_GROUPS),
        of_triangle,
        ..Default::default()
    };
    for (number, (_, mut group, faces)) in spheres.into_iter().enumerate() {
        group.first = list.faces.len() as u32;
        group.count = faces.len() as u32;
        let mut running = 0.0;
        for mut face in faces {
            face.group = number as u32;
            running += face.power;
            face.running = running;
            list.of_triangle[face.triangle as usize] = list.faces.len() as u32;
            list.faces.push(face);
        }
        list.groups.push(group);
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex(position: [f32; 3], grey: u8) -> SceneVertex {
        SceneVertex {
            position,
            color: [grey, grey, grey, 255],
            ..Default::default()
        }
    }

    fn triangle(first: u32, surface: SceneSurface, object: Option<u32>) -> SceneTriangle {
        SceneTriangle {
            vertices: [first, first + 1, first + 2],
            texture: None,
            surface,
            model: Some(7),
            light: None,
            cutout: false,
            own_fog: false,
            object,
            culled: false,
        }
    }

    #[test]
    fn the_list_groups_unlit_faces_by_object_and_place_and_weighs_them_by_power() {
        let vertices = vec![
            // A white face of area 50, 100 units ahead.
            vertex([0.0, 0.0, 100.0], 255),
            vertex([10.0, 0.0, 100.0], 255),
            vertex([0.0, 10.0, 100.0], 255),
            // The same object's grey face beside it: the same group.
            vertex([20.0, 0.0, 100.0], 128),
            vertex([30.0, 0.0, 100.0], 128),
            vertex([20.0, 10.0, 100.0], 128),
            // Another object's, far off: a group of its own.
            vertex([0.0, 0.0, 3000.0], 255),
            vertex([10.0, 0.0, 3000.0], 255),
            vertex([0.0, 10.0, 3000.0], 255),
            // A lit face and a lens, which stay out; a face with no area,
            // no light.
            vertex([0.0, 0.0, 50.0], 255),
            vertex([10.0, 0.0, 50.0], 255),
            vertex([0.0, 10.0, 50.0], 0),
        ];
        let triangles = [
            triangle(6, SceneSurface::Emissive, Some(2)),
            triangle(0, SceneSurface::Emissive, Some(1)),
            triangle(9, SceneSurface::Opaque, Some(1)),
            triangle(3, SceneSurface::Emissive, Some(1)),
            triangle(9, SceneSurface::Shown, None),
            SceneTriangle {
                vertices: [11, 11, 11],
                ..triangle(9, SceneSurface::Emissive, Some(1))
            },
        ];
        let list = emissive_list(&triangles, &vertices, |_| 1.0, |_| 1.0);
        assert_eq!(list.groups.len(), 2);
        assert_eq!(list.sampled, 2);
        // The near group first: the eye likely sees what it lights.
        let near = list.groups[0];
        assert_eq!((near.first, near.count), (0, 2));
        assert!((near.centre[0] - 15.0).abs() < 1.0e-4 && (near.centre[2] - 100.0).abs() < 1.0e-4);
        assert!((near.radius - (15.0_f32 * 15.0 + 5.0 * 5.0).sqrt()).abs() < 1.0e-3);
        let [white, grey] = [list.faces[0], list.faces[1]];
        assert_eq!((white.triangle, grey.triangle), (1, 3));
        assert!((white.area - 50.0).abs() < 1.0e-4);
        assert!((white.power - 50.0).abs() < 1.0e-3, "{}", white.power);
        // 128 decodes to 0.216 of full light.
        assert!((grey.power / white.power - 0.2158).abs() < 1.0e-3);
        assert!((near.power - white.power - grey.power).abs() < 1.0e-3);
        // The running powers end at the group's, as the pick's search needs.
        assert_eq!((white.running, grey.running), (white.power, near.power));
        assert_eq!(list.faces[2].triangle, 0);
        assert_eq!(list.faces[2].group, 1);
        assert_eq!(list.of_triangle, [2, 0, NO_FACE, 1, NO_FACE, NO_FACE]);
    }

    #[test]
    fn a_tuned_gain_brightens_a_face_and_the_cap_leaves_the_rest_unsampled() {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for k in 0..MAX_GROUPS + 3 {
            let z = 1000.0 + 300.0 * k as f32;
            let first = vertices.len() as u32;
            vertices.extend([
                vertex([0.0, 0.0, z], 255),
                vertex([10.0, 0.0, z], 255),
                vertex([0.0, 10.0, z], 255),
            ]);
            triangles.push(triangle(first, SceneSurface::Emissive, Some(k as u32)));
        }
        let list = emissive_list(
            &triangles,
            &vertices,
            |triangle| if triangle.object == Some(3) { 4.0 } else { 1.0 },
            |_| 0.5,
        );
        assert_eq!(list.groups.len(), MAX_GROUPS + 3);
        assert_eq!(list.sampled, MAX_GROUPS);
        let tuned = list.faces[list.of_triangle[3] as usize];
        assert_eq!(tuned.gain, 4.0);
        assert!((tuned.power - 50.0 * 4.0 * 0.5).abs() < 1.0e-3);
        // The farthest go past the cap.
        assert!(list.of_triangle[MAX_GROUPS + 2] as usize >= MAX_GROUPS);
        assert!(list.faces[list.of_triangle[MAX_GROUPS + 2] as usize].group as usize >= MAX_GROUPS);
    }

    #[test]
    fn a_texture_counts_by_its_drawn_texels_in_linear_light() {
        assert_eq!(texture_brightness(&[]), 1.0);
        let half = texture_brightness(&[255, 255, 255, 255, 255, 255, 255, 0]);
        assert!((half - 0.5).abs() < 1.0e-4);
        let grey = texture_brightness(&[188, 188, 188, 255]);
        assert!((grey - 0.5).abs() < 0.01, "{grey}");
    }
}
