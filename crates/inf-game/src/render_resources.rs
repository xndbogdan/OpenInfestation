//! Borrowed render resources from the native resident registration owner.
//!
//! Pool identities follow the original table at 0053AA14. Every lookup uses
//! the registration's local index in its owning overlay; source bytes and
//! provenance cannot outlive a residency mutation. Missing/lazy resources fail
//! explicitly here: decoding bytes is not backend allocation or conversion.

use inf_formats::{Font, MaterialValue, ModelEntry, SpriteEntry, SpriteRenderMode, SpriteVariant};
use thiserror::Error;

use crate::{HandlerResources, ResidentRegistration, ResidentRuntime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderResourceKind {
    String,
    Sprite,
    Font,
    Style,
    Material,
    Model,
}

impl RenderResourceKind {
    pub const fn handler(self) -> u8 {
        match self {
            Self::String => 0,
            Self::Sprite => 3,
            Self::Font => 4,
            Self::Style => 6,
            Self::Material => 7,
            Self::Model => 8,
        }
    }
    pub const fn native_pool(self) -> u32 {
        crate::native_pools::NATIVE_HANDLER_POOLS[self.handler() as usize].pool
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderResourceRequest {
    pub kind: RenderResourceKind,
    /// Native lookups sign-extend the LOW word for the pool index. The full
    /// argument remains distinct because lazy-loader calls receive it intact.
    pub argument: i32,
}

impl RenderResourceRequest {
    pub const fn slot(self) -> i16 {
        self.argument as i16
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RenderSource<'a> {
    pub registration: &'a ResidentRegistration,
    pub bytes: &'a [u8],
}

#[derive(Clone, Copy, Debug)]
pub enum RenderResourceData<'a> {
    String(&'a [u8]),
    Sprite(&'a SpriteEntry),
    Font(&'a Font),
    Style(u32),
    Material(MaterialValue),
    Model(&'a ModelEntry),
}

#[derive(Clone, Copy, Debug)]
pub struct RenderResource<'a> {
    pub request: RenderResourceRequest,
    pub source: RenderSource<'a>,
    pub data: RenderResourceData<'a>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RenderResourceError {
    #[error("render resource {request:?} has no valid signed low-word index")]
    NegativeIndex { request: RenderResourceRequest },
    #[error("render resource {request:?} is not resident; lazy loading requires its owner")]
    NotResident { request: RenderResourceRequest },
    #[error("render resource {request:?} registration does not resolve to its typed local record")]
    Registration { request: RenderResourceRequest },
    #[error("sprite {slot} has no first physical variant for logical-entry dimensions")]
    SpriteDimensions { slot: i16 },
    #[error("sprite {slot} has no physical variant for {mode:?}")]
    SpriteVariant { slot: i16, mode: SpriteRenderMode },
}

/// A Handler-10 registration read outside the typed render pools.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Handler10Error {
    #[error("Handler-10 global slot {0} is not resident")]
    NotResident(u16),
    #[error("Handler-10 global slot {0} does not resolve to its local record")]
    Registration(u16),
    #[error("Handler-10 global slot {0}'s +10 dimensions lie outside its payload")]
    Bounds(u16),
    #[error("Handler-10 global slot {slot}: {error}")]
    Grid {
        slot: u16,
        error: inf_formats::SampleGridError,
    },
}

#[derive(Clone, Copy)]
pub struct RenderResources<'a> {
    runtime: &'a ResidentRuntime,
    /// Port-authored H0 strings for the loaded world (Pelagia), if any. The
    /// session sets this while the fix is on and that world is loaded, so
    /// the briefing, the map and the menus name it; retail's own bytes stay
    /// untouched and every other residency reads retail's.
    world_strings: Option<WorldStrings>,
    /// Second residency consulted for models the primary one does not
    /// register (Pelagia's borrowed Command Centre models). Other handler
    /// kinds never fall back.
    model_fallback: Option<&'a ResidentRuntime>,
}

/// Port-authored H0 strings for one loaded world, in one language, as
/// Pelagia's rework (`rework`) or retail's world 1 has them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldStrings {
    pub world: u32,
    pub language: u8,
    pub rework: bool,
}

impl<'a> RenderResources<'a> {
    pub fn new(runtime: &'a ResidentRuntime) -> Self {
        Self {
            runtime,
            world_strings: None,
            model_fallback: None,
        }
    }

    /// The port-authored strings this draw names its world with, if any:
    /// set only while Pelagia is on and loaded.
    pub fn world_strings(&self) -> Option<WorldStrings> {
        self.world_strings
    }

    /// Draw with the port's H0 strings for [`WorldStrings::world`] where the
    /// code owns them, falling back to retail's everywhere else.
    pub fn with_world_strings(mut self, strings: WorldStrings) -> Self {
        self.world_strings = Some(strings);
        self
    }

    /// Resolve models the primary residency has not loaded from `runtime`
    /// (which must outlive the draw), as the database's second residency
    /// does for its entries. Anything the fallback also lacks still fails
    /// as not resident.
    pub fn with_model_fallback(mut self, runtime: &'a ResidentRuntime) -> Self {
        self.model_fallback = Some(runtime);
        self
    }

    /// Reached non-render pools must use this same registration snapshot.
    pub fn resident_runtime(self) -> &'a ResidentRuntime {
        self.runtime
    }

    /// `00471A80`/`00471AB0` for global `slot`: the width and height DWORDs
    /// at the registration's +10 allocation offset, as the sample-grid
    /// decoder reads them relative to the bank payload.
    pub fn handler10_dimensions(self, slot: u16) -> Result<[u32; 2], Handler10Error> {
        let (source, bank, record) = self.handler10_record(slot)?;
        let start = bank.payload.start + record.relative_offsets[0] as usize;
        if start + 8 > bank.payload.end {
            return Err(Handler10Error::Bounds(slot));
        }
        let bytes = source
            .get(start..start + 8)
            .ok_or(Handler10Error::Bounds(slot))?;
        let dword =
            |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
        Ok([dword(0), dword(4)])
    }

    /// Global `slot`'s grid read as `role`, as `00470170` reads slots 0..2.
    pub fn handler10_sample_grid(
        self,
        slot: u16,
        role: inf_formats::SampleGridRole,
    ) -> Result<inf_formats::SampleGrid, Handler10Error> {
        let (source, bank, record) = self.handler10_record(slot)?;
        inf_formats::decode_sample_grid(source, bank.payload, record, role)
            .map_err(|error| Handler10Error::Grid { slot, error })
    }

    fn handler10_record(
        self,
        slot: u16,
    ) -> Result<
        (
            &'a [u8],
            &'a inf_formats::Handler10Bank,
            &'a inf_formats::Handler10Record,
        ),
        Handler10Error,
    > {
        let resource = self
            .runtime
            .resource(10, slot)
            .ok_or(Handler10Error::NotResident(slot))?;
        let Some(HandlerResources::Records10(bank)) = resource
            .overlay
            .handlers()
            .get(resource.registration.region_index)
        else {
            return Err(Handler10Error::Registration(slot));
        };
        let record = bank
            .records
            .get(usize::from(resource.registration.local_index))
            .ok_or(Handler10Error::Registration(slot))?;
        Ok((resource.overlay.bytes(), bank, record))
    }

    /// PreLoad's allocation count, also stored at the native pool's +08.
    /// Some native lookups accept index==count, an unchecked one-past access;
    /// callers must distinguish that from the proven index>count null path.
    pub fn pool_capacity(self, kind: RenderResourceKind) -> usize {
        self.runtime
            .resource_capacity(kind.handler())
            .expect("typed render handler")
    }

    pub fn resolve(
        self,
        request: RenderResourceRequest,
    ) -> Result<RenderResource<'a>, RenderResourceError> {
        let slot = u16::try_from(request.slot())
            .map_err(|_| RenderResourceError::NegativeIndex { request })?;
        let resource = match self.runtime.resource(request.kind.handler(), slot) {
            Some(resource) => resource,
            None => {
                // Only models borrow a second residency; every other kind
                // keeps retail's missing-resource behavior.
                if request.kind != RenderResourceKind::Model {
                    return Err(RenderResourceError::NotResident { request });
                }
                let fallback = self
                    .model_fallback
                    .ok_or(RenderResourceError::NotResident { request })?;
                fallback
                    .resource(request.kind.handler(), slot)
                    .ok_or(RenderResourceError::NotResident { request })?
            }
        };
        let local = usize::from(resource.registration.local_index);
        let invalid = || RenderResourceError::Registration { request };
        // region_index belongs to this registration. Never search another
        // same-numbered handler or use global slot as a local record index.
        let bank = resource
            .overlay
            .handlers()
            .get(resource.registration.region_index)
            .ok_or_else(invalid)?;
        let source = RenderSource {
            registration: resource.registration,
            bytes: resource.overlay.bytes(),
        };
        let data = match (request.kind, bank) {
            (RenderResourceKind::String, HandlerResources::Strings(bank)) => {
                RenderResourceData::String(
                    bank.entries
                        .get(local)
                        .and_then(|entry| entry.raw(source.bytes))
                        .ok_or_else(invalid)?,
                )
            }
            (RenderResourceKind::Sprite, HandlerResources::Sprites(bank)) => {
                RenderResourceData::Sprite(bank.entries.get(local).ok_or_else(invalid)?)
            }
            (RenderResourceKind::Font, HandlerResources::Fonts(bank)) => {
                RenderResourceData::Font(bank.fonts.get(local).ok_or_else(invalid)?)
            }
            (RenderResourceKind::Style, HandlerResources::Words(bank)) => {
                RenderResourceData::Style(bank.entries.get(local).ok_or_else(invalid)?.raw)
            }
            (RenderResourceKind::Material, HandlerResources::Materials(bank)) => {
                RenderResourceData::Material(*bank.entries.get(local).ok_or_else(invalid)?)
            }
            (RenderResourceKind::Model, HandlerResources::Models(bank)) => {
                RenderResourceData::Model(bank.entries.get(local).ok_or_else(invalid)?)
            }
            _ => return Err(invalid()),
        };
        Ok(RenderResource {
            request,
            source,
            data,
        })
    }
}

impl<'a> RenderResource<'a> {
    /// 004843D0 reads logical entry +14/+16, i.e. the FIRST physical
    /// descriptor's width/height. This does not use the render-mode selector.
    pub fn sprite_dimensions(self) -> Result<[u16; 2], RenderResourceError> {
        let RenderResourceData::Sprite(entry) = self.data else {
            return Err(RenderResourceError::Registration {
                request: self.request,
            });
        };
        let first = entry
            .variants
            .first()
            .ok_or(RenderResourceError::SpriteDimensions {
                slot: self.request.slot(),
            })?;
        Ok([first.width, first.height])
    }

    pub fn select_sprite(
        self,
        mode: SpriteRenderMode,
    ) -> Result<RenderSprite<'a>, RenderResourceError> {
        let RenderResourceData::Sprite(entry) = self.data else {
            return Err(RenderResourceError::Registration {
                request: self.request,
            });
        };
        let variant = entry
            .variant(mode)
            .ok_or(RenderResourceError::SpriteVariant {
                slot: self.request.slot(),
                mode,
            })?;
        Ok(RenderSprite {
            source: self.source,
            entry,
            variant,
        })
    }
}

impl crate::menu_text::MenuTextResources for RenderResources<'_> {
    type Error = RenderResourceError;

    fn string(&self, argument: i32) -> Result<&[u8], Self::Error> {
        if let Some(strings) = self.world_strings {
            if let Some(text) =
                crate::pelagia::text(strings.world, strings.language, argument, strings.rework)
            {
                return Ok(text);
            }
        }
        let request = RenderResourceRequest {
            kind: RenderResourceKind::String,
            argument,
        };
        match self.resolve(request)?.data {
            RenderResourceData::String(value) => Ok(value),
            _ => Err(RenderResourceError::Registration { request }),
        }
    }

    fn font(&self, argument: i32) -> Result<&Font, Self::Error> {
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Font,
            argument,
        };
        match self.resolve(request)?.data {
            RenderResourceData::Font(value) => Ok(value),
            _ => Err(RenderResourceError::Registration { request }),
        }
    }

    fn style(&self, argument: i32) -> Result<u32, Self::Error> {
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Style,
            argument,
        };
        match self.resolve(request)?.data {
            RenderResourceData::Style(value) => Ok(value),
            _ => Err(RenderResourceError::Registration { request }),
        }
    }

    fn sprite_dimensions(&self, argument: i32) -> Result<[u16; 2], Self::Error> {
        self.resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument,
        })?
        .sprite_dimensions()
    }
}

impl crate::settings_row::SettingsRowResources for RenderResources<'_> {
    fn string_count(&self) -> Result<usize, Self::Error> {
        Ok(self.pool_capacity(RenderResourceKind::String))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RenderSprite<'a> {
    pub source: RenderSource<'a>,
    pub entry: &'a SpriteEntry,
    pub variant: &'a SpriteVariant,
}

/// Display-owned RGB bit shifts at 0065F118/1C/20. These are 5-bit component
/// destinations, not component masks and not an assumed RGBA/SDL pixel format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeColorLayout {
    pub red_shift: u32,
    pub green_shift: u32,
    pub blue_shift: u32,
}

impl NativeColorLayout {
    /// 0050F540 doubles the H7 entry count before 0050E690: BOTH RGB555
    /// halfwords of each raw DWORD are converted. Preserve x86 shift masking,
    /// wrapping ADD (not OR), and the low-word store. Original bytes stay fixed.
    pub fn convert_material(self, value: MaterialValue) -> u32 {
        let convert = |word: u16| {
            let red = u32::from((word >> 10) & 31).wrapping_shl(self.red_shift);
            let green = u32::from((word >> 5) & 31).wrapping_shl(self.green_shift);
            let blue = u32::from(word & 31).wrapping_shl(self.blue_shift);
            red.wrapping_add(green).wrapping_add(blue) as u16
        };
        u32::from(convert(value.raw() as u16))
            | (u32::from(convert((value.raw() >> 16) as u16)) << 16)
    }
}
