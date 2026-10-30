//! World flash 00487D70 and the deferred quad suffix004881B8..00488326.
//! The world owns sequence/color; camera acquisition, resources and renderer
//! allocation remain live services. Packet+20 is native allocation carry.

use crate::{
    model_primitives::{
        PrimitiveCallback, PrimitiveCommand, TextureSortError, TextureSortIdentity,
    },
    object_runtime::{native_trunc_low32, ObjectArena, ObjectError, ObjectId},
    render_resources::{RenderResource, RenderResourceData, RenderResourceError},
};
use inf_formats::{native_basis::NativeBasis, FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldFlashPosition {
    Null,
    Object(ObjectId),
    Words([u16; 3]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldFlashRequest {
    pub position: WorldFlashPosition,
    pub radius: u32,
    pub style_argument: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldFlashState {
    ///00487C4C initializes only the sequence, not its retained color.
    pub sequence_5b8: u32,
    pub color_5bc: Option<u32>,
    ///Portable per-feature containment; no failed draw prefix is replayed.
    pub stopped_draw: bool,
}

pub trait WorldFlashHost {
    type Error;
    fn acquire_camera(
        &mut self,
        arena: &mut ObjectArena,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error>;
    fn camera_basis(
        &mut self,
        arena: &mut ObjectArena,
        camera: ObjectId,
    ) -> Result<NativeBasis, Self::Error>;
    ///H6 signed LOWWORD lookup, preserving the full argument for a lazy supplier.
    fn style_word(&mut self, argument: u32) -> Result<u32, Self::Error>;
}

pub trait WorldFlashDrawHost<'r> {
    type Error;
    fn sprite(&mut self, slot: i16) -> Result<Option<RenderResource<'r>>, Self::Error>;
    fn material_sorting(&mut self) -> Result<bool, Self::Error>;
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, Self::Error>;
    fn submit(&mut self, command: PrimitiveCommand<'r>) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum WorldFlashError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Sort(#[from] TextureSortError),
    #[error("world flash at00487DEE requires unwritten native stack XYZ after NULL camera")]
    MissingCameraStackWords,
    #[error("world flash draw requires retained world+5BC")]
    MissingColor,
    #[error("world flash service at {address:08X} failed: {error}")]
    Service { address: u32, error: E },
}

impl WorldFlashState {
    pub fn trigger<H: WorldFlashHost + ?Sized>(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldFlashRequest,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<bool, WorldFlashError<H::Error>> {
        let delta = if matches!(request.position, WorldFlashPosition::Null) {
            [0_i32; 3]
        } else {
            let camera = host
                .acquire_camera(arena, rng)
                .map_err(|error| WorldFlashError::Service {
                    address: 0x487d8a,
                    error,
                })?
                .ok_or(WorldFlashError::MissingCameraStackWords)?;
            let basis =
                host.camera_basis(arena, camera)
                    .map_err(|error| WorldFlashError::Service {
                        address: 0x487d9a,
                        error,
                    })?;
            let radius = f64::from(request.radius as i16);
            let offset = [7, 8, 9].map(|i| {
                native_trunc_low32(radius * f64::from(f32::from_bits(basis.words[i]))) as u16
            });
            let camera_position = arena
                .object(camera)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(camera))?;
            let position = match request.position {
                WorldFlashPosition::Words(words) => words,
                WorldFlashPosition::Object(object) => arena
                    .object(object)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(object))?,
                WorldFlashPosition::Null => unreachable!(),
            };
            std::array::from_fn(|axis| {
                camera_position[axis]
                    .wrapping_add(offset[axis])
                    .wrapping_sub(position[axis]) as i16 as i32
            })
        };
        let square = delta[0]
            .wrapping_mul(delta[0])
            .wrapping_add(delta[1].wrapping_mul(delta[1]))
            .wrapping_add(delta[2].wrapping_mul(delta[2]));
        if square > (request.radius as i32).wrapping_mul(request.radius as i32) {
            return Ok(false);
        }
        self.sequence_5b8 = 1;
        let argument = if request.style_argument == 0 {
            14
        } else {
            self.sequence_5b8 = 4;
            request.style_argument
        };
        let color = host
            .style_word(argument)
            .map_err(|error| WorldFlashError::Service {
                address: 0x487e69,
                error,
            })?;
        self.color_5bc = Some(color);
        Ok(true)
    }

    pub fn draw<'r, H: WorldFlashDrawHost<'r> + ?Sized>(
        &mut self,
        viewport: [u32; 2],
        allocation_mode: Option<u32>,
        executable: FrontendExecutable<'_>,
        host: &mut H,
    ) -> Result<bool, WorldFlashError<H::Error>> {
        if self.sequence_5b8 == 0 {
            return Ok(false);
        }
        let address = 0x53c684_u32.wrapping_add(self.sequence_5b8.wrapping_mul(4));
        let slot = i16::from_le_bytes(executable.mapped(address, 2)?.bytes.try_into().unwrap());
        let texture = host
            .sprite(slot)
            .map_err(|error| WorldFlashError::Service {
                address: 0x4881d0,
                error,
            })?;
        let sorting = host
            .material_sorting()
            .map_err(|error| WorldFlashError::Service {
                address: 0x488211,
                error,
            })?;
        let mut sort = 0;
        if sorting {
            if let Some(texture) = texture {
                let RenderResourceData::Sprite(entry) = texture.data else {
                    return Err(RenderResourceError::Registration {
                        request: texture.request,
                    }
                    .into());
                };
                let selector = entry.selectors.mode0;
                sort = if selector == 255 {
                    0xc000_0000
                } else {
                    let variant = entry.variants.get(usize::from(selector)).ok_or(
                        RenderResourceError::Registration {
                            request: texture.request,
                        },
                    )?;
                    if variant.flags.raw() & 0x39 != 0 {
                        0xc000_0000
                    } else {
                        host.texture_sort_identity(texture)
                            .map_err(|error| WorldFlashError::Service {
                                address: 0x48825d,
                                error,
                            })?
                            .sort_word(selector)?
                    }
                };
            }
        }
        let mut command = PrimitiveCommand::new(PrimitiveCallback::EffectQuadNear, sort, 3);
        command.write_u32(0x28, self.color_5bc.ok_or(WorldFlashError::MissingColor)?);
        let [width, height] = viewport.map(|v| v as u16);
        for (i, [x, y]) in [[0, 0], [width, 0], [width, height], [0, height]]
            .into_iter()
            .enumerate()
        {
            command.write_u32(i * 8, u32::from(x) | (u32::from(y) << 16));
            command.write_u32(i * 8 + 4, 0x40);
        }
        // Native488268..488305 never writes+20. Its allocator carry is an
        // explicit input; None remains unknown for the actual pixel consumer.
        if let Some(mode) = allocation_mode {
            command.write_u32(0x20, mode);
        }
        command.write_texture(0x24, texture);
        host.submit(command)
            .map_err(|error| WorldFlashError::Service {
                address: 0x488288,
                error,
            })?;
        self.sequence_5b8 = self.sequence_5b8.wrapping_add(1);
        let address = 0x53c684_u32.wrapping_add(self.sequence_5b8.wrapping_mul(4));
        if u32::from_le_bytes(executable.mapped(address, 4)?.bytes.try_into().unwrap()) == 0 {
            self.sequence_5b8 = 0;
        }
        Ok(true)
    }
}
