//! Metal in Vulkan's modern raster, when no rays are traced
//! ([the plan](../../../docs/rt-plans/modern-raster.md)'s stage 3): a model
//! the tuning makes metal (`materials.names`, or its sprite's own,
//! `materials.sprites`) mirrors the modern sky at its roughness, as the
//! tracer's metal does under the sky. Each of its primitives' commands
//! carries its material and its face as the tracer takes it (each corner
//! where the model drew it, its normal as retail lit it, its colour and its
//! place in its sprite), and the device's fans take those at their own
//! corners, the clipper's among them, wherever they lie on the face
//! ([`MetalFace::at`]); `vk_metal.glsl` shades the pixels they cover.

use std::rc::Rc;

use inf_formats::{ModelInstructionKind, ModelMaterialDomain};

use crate::{
    fast_hash::FastMap,
    model_execution::ModelPrimitiveRequest,
    modern_tuning::{Material, ModernTuning},
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
    },
    scene_capture::{model_name, retail_face, tinted, unit},
};

/// A corner of a metal face as the tracer takes it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MetalCorner {
    /// Where the model drew it, in the view (x right, y up, z ahead).
    pub view: [f32; 3],
    /// Its world normal as retail lit it, unit length; zero where it had
    /// none.
    pub normal: [f32; 3],
    /// Its colour, which its texels are multiplied by, in retail's tones
    /// (0 to 1).
    pub albedo: [f32; 3],
    /// Its V in its sprite (0 at the top), which the material's split reads.
    pub v: f32,
}

/// A metal model's face, which its fans' corners take theirs from.
#[derive(Clone, Debug, PartialEq)]
pub struct MetalFace {
    /// Metalness and roughness in 255ths as the tracer packs them, then the
    /// same of the material above the split.
    pub material: [u8; 4],
    /// The share of its sprite's height, from the top in 65536ths, above
    /// which the second material holds (the truck's hub: the tyre's side
    /// wall over its metal); 0: none.
    pub split: u16,
    /// A sprite's own material: the bare surface, no paint (the tyres'
    /// rubber), which the rain darkens and smooths rather than coats.
    pub bare: bool,
    /// Its corners, once or, for a mirrored primitive, for each of its
    /// `passes`, one pass's after the other's.
    pub corners: tinyvec::TinyVec<[MetalCorner; 8]>,
    pub passes: usize,
}

impl MetalFace {
    /// Each pass's polygon.
    fn polygons(&self) -> std::slice::Chunks<'_, MetalCorner> {
        self.corners
            .chunks((self.corners.len() / self.passes.max(1)).max(1))
    }

    /// What a fan's corner at `view` takes: the corners' normals, colours
    /// and V blended where it lies on the face, as the tracer blends them
    /// across its triangles (each polygon's first corner with each next
    /// two); none where no triangle has an area. A corner where one of the
    /// face's lies (within a thousandth of the way to its next nearest)
    /// takes that one's own: the clipper made none of it.
    pub fn at(&self, view: [f32; 3]) -> Option<MetalCorner> {
        let away = |corner: &MetalCorner| -> f32 {
            (0..3)
                .map(|axis| (corner.view[axis] - view[axis]).powi(2))
                .sum()
        };
        for polygon in self.polygons() {
            let mut near = polygon.iter().map(|corner| (away(corner), corner));
            let Some(mut nearest) = near.next() else {
                continue;
            };
            let mut next = f32::INFINITY;
            for (distance, corner) in near {
                if distance < nearest.0 {
                    next = nearest.0;
                    nearest = (distance, corner);
                } else {
                    next = next.min(distance);
                }
            }
            if nearest.0 <= 1.0e-6 * next {
                return Some(MetalCorner { view, ..*nearest.1 });
            }
        }
        let mut best: Option<(f64, [f32; 3], [&MetalCorner; 3])> = None;
        for polygon in self.polygons() {
            let Some(first) = polygon.first() else {
                continue;
            };
            for pair in polygon[1..].windows(2) {
                let corners = [first, &pair[0], &pair[1]];
                let Some((weights, fit)) = weights(corners.map(|corner| corner.view), view) else {
                    continue;
                };
                if best.as_ref().is_none_or(|(known, ..)| fit > *known) {
                    best = Some((fit, weights, corners));
                }
            }
        }
        let (_, weights, corners) = best?;
        let blend = |pick: fn(&MetalCorner) -> [f32; 3]| -> [f32; 3] {
            std::array::from_fn(|axis| (0..3).map(|k| pick(corners[k])[axis] * weights[k]).sum())
        };
        Some(MetalCorner {
            view,
            normal: blend(|corner| corner.normal),
            albedo: blend(|corner| corner.albedo),
            v: (0..3).map(|k| corners[k].v * weights[k]).sum(),
        })
    }
}

/// `point`'s weights on the triangle `corners`, none below zero, and how
/// well it fits it: its least weight before that clamp (inside, at least
/// zero) less how far it lies off the triangle's plane, as a share of its
/// longest edge; none for a triangle without an area.
fn weights(corners: [[f32; 3]; 3], point: [f32; 3]) -> Option<([f32; 3], f64)> {
    let [a, b, c] = corners.map(|corner| corner.map(f64::from));
    let point = point.map(f64::from);
    let sub = |p: [f64; 3], q: [f64; 3]| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
    let dot = |p: [f64; 3], q: [f64; 3]| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
    let (e0, e1, p) = (sub(b, a), sub(c, a), sub(point, a));
    let (d00, d01, d11) = (dot(e0, e0), dot(e0, e1), dot(e1, e1));
    let det = d00 * d11 - d01 * d01;
    if det <= 1.0e-9 * d00 * d11 || det <= 0.0 {
        return None;
    }
    let (d20, d21) = (dot(p, e0), dot(p, e1));
    let v = (d11 * d20 - d01 * d21) / det;
    let w = (d00 * d21 - d01 * d20) / det;
    let u = 1.0 - v - w;
    let off = sub(p, std::array::from_fn(|axis| e0[axis] * v + e1[axis] * w));
    let longest = d00.max(d11).max(dot(sub(c, b), sub(c, b))).sqrt();
    let fit = u.min(v).min(w) - dot(off, off).sqrt() / longest;
    let clamped = [u, v, w].map(|weight| weight.max(0.0));
    let sum: f64 = clamped.iter().sum();
    Some((clamped.map(|weight| (weight / sum) as f32), fit))
}

/// The tracer's packing of a material: metalness and roughness in 255ths.
fn packed(material: Material) -> [u8; 2] {
    [material.metal, material.rough].map(|value| (value * 255.0).round() as u8)
}

/// The materials of the models Vulkan's raster draws, by their names, kept
/// from world to world while the tuning stays the same.
pub struct RasterMetals {
    tuning: Rc<ModernTuning>,
    /// The display's shifts, which a flat face's colour is read with, while
    /// a world whose metal mirrors the sky is drawn.
    shifts: Option<[u32; 3]>,
    /// Each model's material by its global index, with its name.
    models: FastMap<u16, (Box<[u8]>, Material)>,
    /// The model looked up last in this world's draw, whose primitives
    /// come one after another.
    last: Option<(u16, Material)>,
}

impl Default for RasterMetals {
    fn default() -> Self {
        Self {
            tuning: crate::modern_tuning::current(),
            shifts: None,
            models: FastMap::default(),
            last: None,
        }
    }
}

impl RasterMetals {
    /// A world whose metal mirrors the sky begins, drawn under the
    /// display's material `shifts`, with the tuning as it is now.
    pub fn begin(&mut self, shifts: [u32; 3]) {
        let tuning = crate::modern_tuning::current();
        if !Rc::ptr_eq(&tuning, &self.tuning) {
            self.models.clear();
            self.tuning = tuning;
        }
        self.shifts = Some(shifts);
        self.last = None;
    }

    /// The world is drawn.
    pub fn end(&mut self) {
        self.shifts = None;
    }

    /// These materials while a world whose metal mirrors the sky is drawn.
    pub fn drawing(&mut self) -> Option<&mut Self> {
        self.shifts.is_some().then_some(self)
    }

    /// The face `request` draws, where its material is metal or its
    /// sprite's own bare surface; none for any other, and for a face drawn
    /// unlit, which the tracer shades as a light.
    pub fn face(
        &mut self,
        request: &ModelPrimitiveRequest<'_>,
        resources: &RenderResources<'_>,
    ) -> Option<Rc<MetalFace>> {
        let shifts = self.shifts?;
        // The material first, which most faces end at (matte).
        let material = match &request.instruction.kind {
            ModelInstructionKind::Triangle(triangle) => triangle.material,
            ModelInstructionKind::Quad(quad) => quad.material,
            ModelInstructionKind::TriangleFan(fan) => fan.material,
            _ => return None,
        };
        let textured = matches!(material.domain, ModelMaterialDomain::Handler3Texture);
        let own = textured
            .then(|| self.tuning.materials().of_sprite(material.global_id))
            .flatten();
        let (below, above, bare) = match own {
            Some((own, above)) => (own, above, true),
            None => (self.model(request), None, false),
        };
        if below.metal <= 0.0 && !bare && above.is_none_or(|(_, above)| above.metal <= 0.0) {
            return None;
        }
        let face = retail_face(request)?;
        if face.unlit || face.slots.len() < 3 {
            return None;
        }
        let colour = if textured {
            [255; 3]
        } else {
            let resource = resources
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Material,
                    argument: i32::from(material.global_id),
                })
                .ok()?;
            let RenderResourceData::Material(value) = resource.data else {
                return None;
            };
            crate::scene_feed::material_rgb(value.raw(), shifts)
        };
        let [metal, rough] = packed(below);
        let [above_metal, above_rough] = above.map_or([0; 2], |(_, above)| packed(above));
        let mut metal_face = MetalFace {
            material: [metal, rough, above_metal, above_rough],
            split: above.map_or(0, |(v, _)| ((v * 65536.0).round() as u32).min(65535) as u16),
            bare,
            corners: tinyvec::TinyVec::new(),
            passes: 0,
        };
        let passes: &[i16] = if face.mirror { &[1, 0] } else { &[0] };
        for &mirror in passes {
            let first = metal_face.corners.len();
            for (((slot, light), uv), &factors) in face
                .slots
                .iter()
                .zip(&face.lighting)
                .zip(&face.uvs)
                .zip(&face.factors)
            {
                let index = (slot.raw() as i16) ^ mirror;
                // Opcode 2C's ground shadows, which the tracer leaves out.
                if request.vertices.depth_biased(index) {
                    metal_face.corners.truncate(first);
                    break;
                }
                let view = request.vertices.drawn_view(index)?;
                let shade_slot = i32::from((light.raw() as i16) ^ mirror);
                metal_face.corners.push(MetalCorner {
                    view: view.map(|axis| axis as f32),
                    normal: request.shades.normal(shade_slot).map_or([0.0; 3], unit),
                    albedo: tinted(colour, factors).map(|channel| f32::from(channel) / 255.0),
                    v: uv[1],
                });
            }
            if metal_face.corners.len() > first {
                metal_face.passes += 1;
            }
        }
        (metal_face.passes > 0).then(|| Rc::new(metal_face))
    }

    /// The material of `request`'s model by its name, as the tracer takes
    /// it (none named: matte).
    fn model(&mut self, request: &ModelPrimitiveRequest<'_>) -> Material {
        let index = request.model.identity.global_index;
        if let Some((last, material)) = self.last {
            if last == index {
                return material;
            }
        }
        let material = match model_name(request) {
            None => Material {
                metal: 0.0,
                rough: 1.0,
            },
            Some(name) => match self.models.get(&index) {
                Some((known, material)) if **known == *name => *material,
                _ => {
                    let material = self.tuning.materials().of(&String::from_utf8_lossy(name));
                    self.models.insert(index, (name.into(), material));
                    material
                }
            },
        };
        self.last = Some((index, material));
        material
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corner(view: [f32; 3], normal: [f32; 3], v: f32) -> MetalCorner {
        MetalCorner {
            view,
            normal,
            albedo: [v; 3],
            v,
        }
    }

    fn face(polygons: &[&[MetalCorner]]) -> MetalFace {
        MetalFace {
            material: [153, 89, 0, 0],
            split: 0,
            bare: false,
            corners: polygons
                .iter()
                .flat_map(|corners| corners.iter().copied())
                .collect(),
            passes: polygons.len(),
        }
    }

    /// A fan's corner where a face's corner lies takes that corner's own;
    /// one the clipper made on an edge, the edge's ends' blended along it;
    /// one inside, its triangle's (the polygon's first corner with each
    /// next two), as the tracer blends them.
    #[test]
    fn a_fans_corners_take_the_faces_where_they_lie() {
        let quad = [
            corner([0.0, 0.0, 100.0], [1.0, 0.0, 0.0], 0.0),
            corner([10.0, 0.0, 100.0], [0.0, 1.0, 0.0], 0.25),
            corner([10.0, 10.0, 100.0], [0.0, 0.0, 1.0], 0.5),
            corner([0.0, 10.0, 100.0], [0.0, 1.0, 0.0], 1.0),
        ];
        let face = face(&[&quad]);
        let own = face.at(quad[2].view).unwrap();
        assert!((own.v - quad[2].v).abs() < 1.0e-6 && (own.normal[2] - 1.0).abs() < 1.0e-6);
        let edge = face.at([10.0, 4.0, 100.0]).unwrap();
        assert!((edge.v - (0.25 + 0.4 * 0.25)).abs() < 1.0e-5, "{edge:?}");
        assert!((edge.normal[1] - 0.6).abs() < 1.0e-5 && (edge.normal[2] - 0.4).abs() < 1.0e-5);
        // Inside the second triangle (corners 0, 2 and 3).
        let inside = face.at([2.0, 6.0, 100.0]).unwrap();
        let expected = 0.4 * 0.0 + 0.2 * 0.5 + 0.4 * 1.0;
        assert!((inside.v - expected).abs() < 1.0e-5, "{inside:?}");
        // A point drawn a hair off the face still takes it.
        assert!(face.at([5.0, 5.0, 100.01]).is_some());
    }

    /// A mirrored primitive's two passes lie apart: each corner takes the
    /// pass it lies on.
    #[test]
    fn a_mirrored_faces_corners_take_their_own_pass() {
        let left = [
            corner([-20.0, 0.0, 100.0], [-1.0, 0.0, 0.0], 0.0),
            corner([-10.0, 0.0, 100.0], [-1.0, 0.0, 0.0], 0.0),
            corner([-10.0, 10.0, 100.0], [-1.0, 0.0, 0.0], 0.0),
        ];
        let right = left.map(|corner| MetalCorner {
            view: [-corner.view[0], corner.view[1], corner.view[2]],
            normal: [1.0, 0.0, 0.0],
            ..corner
        });
        let face = face(&[&left, &right]);
        let normal_x = |x: f32| face.at([x, 1.0, 100.0]).unwrap().normal[0];
        assert!((normal_x(12.0) - 1.0).abs() < 1.0e-5);
        assert!((normal_x(-12.0) + 1.0).abs() < 1.0e-5);
        // A face without an area gives nothing.
        let flat = [left[0], left[1], left[1]];
        assert_eq!(face_of(&flat).at([-15.0, 0.0, 100.0]), None);
    }

    fn face_of(corners: &[MetalCorner]) -> MetalFace {
        face(&[corners])
    }
}
