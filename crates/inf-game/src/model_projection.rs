//! Concrete vertex projection selected by the native table at 00544740.
//!
//! Near local/world are 004EB5A0/004EB730; far local/world are
//! 004EB8E0/004EBAA0. This owner writes the actual 24-byte slot's projection
//! tail. It does not cull primitives, populate world coordinates, or allocate
//! renderer commands. EE/EF operate on deferred draw groups, not this state.

use std::convert::Infallible;

use crate::{
    frontend_scene::SceneView,
    model_context::{fistp_low32, ModelDispatch, ModelProjection, ModelTransform},
    model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector},
};

/// Published native globals for one model invocation. The enclosing model
/// owner republishes the parent's transform after returning from a child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelProjectionBackend {
    pub transform: ModelTransform,
    pub view: SceneView,
    pub projection: ModelProjection,
    pub dispatch: ModelDispatch,
    /// 0055F250, with the x86 shift count masked to five bits.
    pub coordinate_shift: u8,
    /// Live 0055F244, normally published by 0048FB00 from the fade interval.
    /// Keep the actual value: saved render state also restores this DWORD.
    pub fog_factor: i32,
}

impl ModelVertexProjector for ModelProjectionBackend {
    type Error = Infallible;

    fn project(
        &mut self,
        input: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        project_with_inputs(self, input, output)
    }

    fn view_point(&mut self, input: ModelVertexInput) -> Option<[i32; 3]> {
        Some(self.view_coordinates(input))
    }

    fn exact_view_point(&mut self, input: ModelVertexInput, exact: [f64; 3]) -> Option<[f64; 3]> {
        let rows = vectors(self.view.basis);
        let toward = |world: [f64; 3]| {
            let delta: [f64; 3] = std::array::from_fn(|axis| {
                crate::model_vertices::exact::wrap(
                    world[axis] - f64::from(self.view.eye[axis]),
                    self.coordinate_shift,
                )
            });
            rows.map(|row| row[0] * delta[0] + row[1] * delta[1] + row[2] * delta[2])
        };
        Some(match input {
            // The composed basis the model's own rows publish, from its
            // exact origin's view point rather than the rounded one.
            ModelVertexInput::Local(point) => {
                let origin = toward(exact);
                let words = self.transform.view_basis.words;
                std::array::from_fn(|axis| {
                    (0..3)
                        .map(|source| {
                            f64::from(point[source])
                                * f64::from(f32::from_bits(words[1 + source * 3 + axis]))
                        })
                        .sum::<f64>()
                        + origin[axis]
                })
            }
            ModelVertexInput::World(_) => toward(exact),
        })
    }

    fn exact_screen_point(&mut self, [x, y, depth]: [f64; 3]) -> Option<[f64; 2]> {
        // Retail clips what lies nearer than 64 and halves what lands past
        // its 16-bit screen; those corners keep its whole pixels.
        if depth < 64.0 {
            return None;
        }
        let [focal_x, focal_y] = self.projection.focal.map(f64::from);
        let [centre_x, centre_y] = self.projection.center.map(f64::from);
        let screen = [
            centre_x + focal_x * x / depth,
            centre_y - focal_y * y / depth,
        ];
        screen
            .iter()
            .all(|axis| axis.abs() < 32767.0)
            .then_some(screen)
    }
}

/// A basis's three rows as reals.
fn vectors(basis: crate::menu::MenuBasis) -> [[f64; 3]; 3] {
    std::array::from_fn(|axis| {
        std::array::from_fn(|component| {
            f64::from(f32::from_bits(basis.words[1 + axis * 3 + component]))
        })
    })
}

impl ModelProjectionBackend {
    fn view_coordinates(&self, input: ModelVertexInput) -> [i32; 3] {
        match input {
            ModelVertexInput::Local(point) => {
                local_coordinates(point, self.transform, self.dispatch)
            }
            ModelVertexInput::World(point) => {
                world_coordinates(point, self.view, self.coordinate_shift)
            }
        }
    }

    fn fog(&self, depth: i32) -> u8 {
        if depth < self.projection.fade[0] {
            0
        } else if depth >= self.projection.fade[1] {
            255
        } else {
            (depth
                .wrapping_sub(self.projection.fade[0])
                .wrapping_mul(self.fog_factor)
                >> 16) as u8
        }
    }
}

///One arithmetic owner with explicit lazy source reads. Existing supplied
///ModelProjectionBackend remains its intrinsic, fully-known input domain.
///The process child reads the genuinely selected alias and live publications.
trait ProjectionInputs {
    type Error;
    fn selected_dispatch(&mut self, input: ModelVertexInput) -> Result<ModelDispatch, Self::Error>;
    fn coordinates(
        &mut self,
        input: ModelVertexInput,
        dispatch: ModelDispatch,
    ) -> Result<[i32; 3], Self::Error>;
    fn focal(&mut self) -> Result<[i32; 2], Self::Error>;
    fn center(&mut self) -> Result<[i32; 2], Self::Error>;
    fn viewport(&mut self) -> Result<[i32; 2], Self::Error>;
    fn fog(&mut self, depth: i32) -> Result<u8, Self::Error>;
}
impl ProjectionInputs for ModelProjectionBackend {
    type Error = Infallible;
    fn selected_dispatch(&mut self, _: ModelVertexInput) -> Result<ModelDispatch, Self::Error> {
        Ok(self.dispatch)
    }
    fn coordinates(
        &mut self,
        input: ModelVertexInput,
        _: ModelDispatch,
    ) -> Result<[i32; 3], Self::Error> {
        Ok(self.view_coordinates(input))
    }
    fn focal(&mut self) -> Result<[i32; 2], Self::Error> {
        Ok(self.projection.focal)
    }
    fn center(&mut self) -> Result<[i32; 2], Self::Error> {
        Ok(self.projection.center)
    }
    fn viewport(&mut self) -> Result<[i32; 2], Self::Error> {
        Ok(self.projection.viewport)
    }
    fn fog(&mut self, depth: i32) -> Result<u8, Self::Error> {
        Ok(ModelProjectionBackend::fog(self, depth))
    }
}

fn project_with_inputs<P: ProjectionInputs>(
    inputs: &mut P,
    input: ModelVertexInput,
    output: &mut ModelVertexProjection,
) -> Result<u32, P::Error> {
    let dispatch = inputs.selected_dispatch(input)?;
    let [x, y, depth] = inputs.coordinates(input, dispatch)?;
    if dispatch == ModelDispatch::Near {
        output.write_u32(4, depth as u32);
    }
    if depth < 64 {
        write_xy(output, x, y);
        output.write_u32(4, depth as u32);
        output.bytes[8] = Some(0xc0);
        return Ok(depth as u32);
    }
    let focal = inputs.focal()?;
    let center = inputs.center()?;
    let projected_x = fistp_low32((f64::from(focal[0]) * f64::from(x)) / f64::from(depth));
    let projected_y = fistp_low32((f64::from(focal[1]) * f64::from(y)) / f64::from(depth));
    let mut screen_x = projected_x.wrapping_add(center[0]);
    let mut screen_y = center[1].wrapping_sub(projected_y);
    let mut magnitude = screen_x.wrapping_abs() | screen_y.wrapping_abs();
    while magnitude >= 32767 {
        magnitude >>= 1;
        screen_x >>= 1;
        screen_y >>= 1;
    }
    if dispatch == ModelDispatch::Far {
        output.bytes[9] = Some(inputs.fog(depth)?);
    }
    write_xy(output, screen_x, screen_y);
    output.write_u32(4, depth as u32);
    //Native viewport reads follow XY/depth stores, so a required backing error
    //keeps those reached bytes; ModelVertices already commits projection Err.
    let viewport = inputs.viewport()?;
    output.bytes[8] = Some(
        axis_clip(screen_x, viewport[0], [1, 2, 4]) | axis_clip(screen_y, viewport[1], [8, 16, 32]),
    );
    Ok(depth as u32)
}

//48FB40 composed vectors are columns for local input. Near calculates Z then
//X/Y, Far calculates X/Y/Z; the reused dot retains Z+Y then X x87 ordering.
fn local_coordinates(
    point: [i32; 3],
    transform: ModelTransform,
    dispatch: ModelDispatch,
) -> [i32; 3] {
    let component = |axis: usize| {
        let vector = std::array::from_fn(|source_axis| {
            transform.view_basis.words[1 + source_axis * 3 + axis]
        });
        dot(point, vector).wrapping_add(transform.view_position[axis])
    };
    match dispatch {
        ModelDispatch::Near => {
            let depth = component(2);
            let x = component(0);
            let y = component(1);
            [x, y, depth]
        }
        ModelDispatch::Far => [component(0), component(1), component(2)],
    }
}
//World input has already passed the model transform. Use current camera rows
//and the actual x86-masked coordinate shift; never apply the model a second time.
fn world_coordinates(point: [i32; 3], view: SceneView, coordinate_shift: u8) -> [i32; 3] {
    let shift = u32::from(coordinate_shift & 31);
    let delta = std::array::from_fn(|axis| {
        point[axis].wrapping_sub(view.eye[axis]).wrapping_shl(shift) >> shift
    });
    let component = |axis: usize| {
        dot(
            delta,
            std::array::from_fn(|source_axis| view.basis.words[1 + axis * 3 + source_axis]),
        )
    };
    let depth = component(2);
    let x = component(0);
    let y = component(1);
    [x, y, depth]
}

mod process;
pub(crate) use process::ProcessModelProjector;

fn dot(point: [i32; 3], basis_words: [u32; 3]) -> i32 {
    let [x, y, z] = point.map(f64::from);
    let [bx, by, bz] = basis_words.map(|word| f64::from(f32::from_bits(word)));
    // Retained x87 arithmetic has the supported CRT's 53-bit precision. Both
    // the Z+Y then X addition order and lack of an f32 store are deliberate.
    fistp_low32((z * bz + y * by) + x * bx)
}

fn write_xy(output: &mut ModelVertexProjection, x: i32, y: i32) {
    output.write_u32(0, u32::from(x as u16) | (u32::from(y as u16) << 16));
}

fn axis_clip(value: i32, limit: i32, codes: [u8; 3]) -> u8 {
    if (value as u32) < (limit as u32) {
        codes[1]
    } else if value < 0 {
        codes[0]
    } else {
        codes[2]
    }
}

#[cfg(test)]
#[path = "model_projection_tests.rs"]
mod tests;
