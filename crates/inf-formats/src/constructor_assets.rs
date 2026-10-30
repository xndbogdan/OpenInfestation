//! Consumer-proven views of Handler-13 constructor data.
//!
//! Dispatch is `004603B0`; `00460120` passes header +2 through context +8
//! and payload +4 separately. Model initialization consumes the former, never
//! the first word of the class payload. These are initial asset inputs, not
//! final live-object state or complete class decoders.

use serde::Serialize;
use thiserror::Error;

use crate::{
    parse_constructor_bank, ByteSpan, ConstructorBank, ConstructorResource, HandlerRegion,
    WorldAssetError,
};

#[path = "constructor_actor_derived.rs"]
mod derived;
#[path = "constructor_actor_specialized.rs"]
mod specialized;
pub use specialized::{
    BehaviorResourcePayload, BehaviorResourcePayloadTail, Family40d950Payload, Family4236d0Payload,
    Family425190Payload, Family467070Payload, Selector15ConstructorPayload,
    Selector30ConstructorPayload, Selector34ConstructorPayload, Selector35ConstructorPayload,
    Selector74ConstructorPayload,
};

pub use derived::{ConstructorSoundBinding, DerivedActorConstructorPayload};

/// Native selector range in the supported retail constructor switch.
pub const CONSTRUCTOR_SELECTOR_COUNT: u16 = 94;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ConstructorDispatch {
    pub selector: u16,
    pub constructor: u32,
    pub vtable: u32,
    /// Virtual +08, receiving the shared initialization context.
    pub context_initializer: u32,
    /// Virtual +0C, receiving context and the class payload as separate arguments.
    pub payload_initializer: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConstructorAssetBank {
    pub prefixes: ConstructorBank,
    /// One view per local registration, preserving aliases and native order.
    pub assets: Vec<ConstructorAsset>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConstructorAsset {
    pub local_index: u32,
    /// None means an unsupported selector, not a fabricated default class.
    pub dispatch: Option<ConstructorDispatch>,
    /// Only consumers which reach 0047A050 bind the header argument to H8.
    /// Absence does not classify unrelated class-specific resource fields.
    pub model_binding: Option<ConstructorModelBinding>,
    pub payload: ConstructorPayloadView,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ConstructorModelBinding {
    pub source_span: ByteSpan,
    pub raw_word: u16,
    /// Signed H8 global resource argument; -1 takes the native disabled branch,
    /// other negative values skip lookup. Neither is turned into slot zero.
    pub signed_slot: i16,
    pub consumer: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ConstructorField<T> {
    pub span: ByteSpan,
    pub value: T,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConstructorPayloadView {
    Opaque,
    Flags(ConstructorField<u32>),
    TerrainLight(TerrainLightConstructorPayload),
    Selector86(Selector86ConstructorPayload),
    Selector83(Selector83ConstructorPayload),
    Selector45(Selector45ConstructorPayload),
    Selector46(Selector46ConstructorPayload),
    FlagsWords(FlagsWordsConstructorPayload),
    Goal(GoalConstructorPayload),
    SharedActor(ActorConstructorPayload),
    PointTable16(Box<PointTableConstructorPayload>),
}

/// 0042DEA0 applies flags then 0042DEC0 retains payload +4, reads the
/// signed rate at +6 and writes the computed interval at +10. The view
/// preserves authored bytes; the writable interval belongs to runtime state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Selector86ConstructorPayload {
    pub prefix_span: ByteSpan,
    pub flags: ConstructorField<u32>,
    pub retained_offset: u32,
    pub rate: ConstructorField<i16>,
}

/// Selector 83: 0043C3B0 reads the flags (00479D70) and keeps the payload
/// itself at +78, whose WORDs +4..+E its update reads (0043C3D0, 0043C4C0,
/// 0043C620). Authored inputs only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Selector83ConstructorPayload {
    pub prefix_span: ByteSpan,
    pub flags: ConstructorField<u32>,
    /// +4 and +6: the patch's extent along X and along Z.
    pub extent: [ConstructorField<i16>; 2],
    /// +8: how far below the water plane the patch sinks.
    pub depth: ConstructorField<i16>,
    /// +A: the sinking and the rising each take this many 1,024 µs steps.
    pub travel: ConstructorField<i16>,
    /// +C: the steps it stays risen.
    pub risen: ConstructorField<i16>,
    /// +E: the steps it stays sunk.
    pub sunk: ConstructorField<i16>,
}

/// Selector 45, an arena's spawn marker: 0045BFC0 reads the flags
/// (00479D70), its team (+4: 0 is 1, else one more) and two offsets
/// (+8, +E) it turns by the marker's basis (0047A6C0).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Selector45ConstructorPayload {
    pub prefix_span: ByteSpan,
    pub flags: ConstructorField<u32>,
    pub team: ConstructorField<i32>,
    pub offsets: [ConstructorField<[i16; 3]>; 2],
}

/// Selector 46, an arena's territory: 00475710 reads the flags (00479D70),
/// its score (+4, kept four times over at +6C) and the seconds it takes
/// (+8, kept in microseconds at +70).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Selector46ConstructorPayload {
    pub prefix_span: ByteSpan,
    pub flags: ConstructorField<u32>,
    pub score: ConstructorField<i32>,
    pub seconds: ConstructorField<i32>,
}

/// Selectors 71 and 72, a race's markers: `0046F890` and `0046FB20` read
/// the flags (00479D70) and two signed WORDs, +4 and +6.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FlagsWordsConstructorPayload {
    pub prefix_span: ByteSpan,
    pub flags: ConstructorField<u32>,
    pub words: [ConstructorField<i16>; 2],
}

/// Selector 92, Powerball's goal: `00403F10` reads the flags (00479D70)
/// and its side (+4), kept at +6C only when 1 or 2.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GoalConstructorPayload {
    pub prefix_span: ByteSpan,
    pub flags: ConstructorField<u32>,
    pub side: ConstructorField<i32>,
}

/// Selector 19: 004481C0 copies this ten-byte prefix after shared flags, and
/// selector 54's 004649E0 copies it to +6C..+72 of its rotating visual.
/// The channel consumer 0045B1A0 sign-extends each byte independently; no
/// fourth channel is initialized or consumed. These are authored inputs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TerrainLightConstructorPayload {
    pub prefix_span: ByteSpan,
    pub flags: ConstructorField<u32>,
    pub channels: ConstructorField<[i8; 3]>,
    /// Copied to object +4A (+72); gates its later 00461C30 (00461C60) model
    /// draw.
    pub draw_gate: ConstructorField<u8>,
    /// Zero-extended from object +48 for the later 0045B1A0 terrain update.
    pub radius: ConstructorField<u16>,
}

/// Selector 70: 0046E830 stores payload +4 at object +338 and 0046F300
/// follows the relocated +0C pointer to sixteen coordinate-WORD pairs.
/// Neither this prefix nor the table is a complete class schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PointTableConstructorPayload {
    pub prefix_span: ByteSpan,
    pub word_04: ConstructorField<u32>,
    /// Allocation-relative target before native relocation.
    pub points_pointer: ConstructorField<u32>,
    pub points_span: ByteSpan,
    /// Native multiplies each raw WORD by 220 with WORD wrapping, then rotates
    /// [first,0,second] and adds object position. Keep unscaled authored bits.
    pub points: [ConstructorField<[u16; 2]>; 16],
}

/// Reads proved by 0047AC10, relative to the class payload (H13 entry +4).
/// Unknown bytes, pointed tables and derived-class extensions remain in the
/// owning allocation; prefix_span is a read boundary, not a complete record size.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActorConstructorPayload {
    pub prefix_span: ByteSpan,
    pub flags: ConstructorField<u32>,
    /// Copied to current/max WORDs +A0/+A2; damage consumer 0047CBC0.
    pub initial_health: ConstructorField<u16>,
    /// Copied to object +8C; semantic role remains unnamed.
    pub word_08: ConstructorField<u16>,
    /// Signed input to the switch producing object +B4; not a constructor selector.
    pub selector_0c: ConstructorField<i32>,
    /// Allocation-relative address stored at object +A4, passed to 0042A8F0.
    pub embedded_10_offset: u32,
    /// Ten damage-channel responses at payload +10..+38. The consumer indexes
    /// these by the incoming descriptor's unsigned byte +2, without a bounds check.
    /// Channel names and arbitrary runtime descriptor validity are not inferred.
    pub damage_responses: Box<[ConstructorDamageResponse; 10]>,
    /// Copied to object +7A, then values above 174 are replaced with zero at runtime.
    pub word_38: ConstructorField<u16>,
    /// Nonzero input replaces object +28 with its low WORD; signedness is unproven.
    pub word_3c: ConstructorField<u32>,
    /// Copied to object +76; semantic role remains unnamed.
    pub word_40: ConstructorField<u16>,
    /// Present only for initializer paths proven to reach 00426C10 unchanged.
    pub derived: Option<Box<DerivedActorConstructorPayload>>,
    /// Present only on the proven 00423850 payload-initializer chains.
    pub family4236d0: Option<Box<Family4236d0Payload>>,
    /// The additional 00425290 DWORD on selector10/21's initializer chain.
    pub family425190: Option<Box<Family425190Payload>>,
    /// Retained input on the selector12/13 00468330 initializer chain.
    pub family467070: Option<Box<Family467070Payload>>,
    /// 0040D950's maximum and spawn-offset fields on the 0040D920/0042EDD0
    /// wrappers; selector 30's own wrapper keeps `selector30`.
    pub family40d950: Option<Box<Family40d950Payload>>,
    /// 0042EE00's +50 DWORD on selector 15's 0042EDD0 wrapper.
    pub selector15: Option<Box<Selector15ConstructorPayload>>,
    pub selector30: Option<Box<Selector30ConstructorPayload>>,
    pub selector34: Option<Box<Selector34ConstructorPayload>>,
    pub selector35: Option<Box<Selector35ConstructorPayload>>,
    /// 0042D780's reads on selector 74's 0042D760.
    pub selector74: Option<Box<Selector74ConstructorPayload>>,
    /// The reads past +42 of the behavior registry's resource classes.
    pub behavior_resource: Option<Box<BehaviorResourcePayload>>,
}

/// One 4-byte response consumed by 0042A8F0 through 0047C6E0/0047C7C0.
/// Both WORDs are zero-extended, independently of the signed incoming damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ConstructorDamageResponse {
    pub channel: u8,
    /// Positive damage must exceed this value before the multiplier is applied.
    pub threshold: ConstructorField<u16>,
    /// Multiplies the excess above threshold, followed by a logical right shift
    /// of eight: 256 is unity. This asset view does not execute damage policy.
    pub multiplier_q8: ConstructorField<u16>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConstructorAssetError {
    #[error(transparent)]
    Prefix(#[from] WorldAssetError),
    #[error("constructor resource {local_index} selector {selector} needs {length} bytes at allocation offset {offset:#x}, allocation has {allocation_bytes}")]
    Truncated {
        local_index: u32,
        selector: u16,
        offset: u32,
        length: usize,
        allocation_bytes: usize,
    },
    #[error("constructor resource {local_index} allocation offset {offset:#x} overflows the native pointer representation")]
    OffsetOverflow { local_index: u32, offset: u32 },
    #[error("constructor resource {local_index} point-table pointer field {field_offset:#x} is absent from the native relocation table")]
    MissingPointTableRelocation { local_index: u32, field_offset: u32 },
}

/// Return the exact dispatch profile, without inferring a class from asset names.
pub fn constructor_dispatch(selector: u16) -> Option<ConstructorDispatch> {
    let index = usize::from(selector.checked_sub(1)?);
    let &(constructor, vtable, context_initializer, payload_initializer) = DISPATCH.get(index)?;
    Some(ConstructorDispatch {
        selector,
        constructor,
        vtable,
        context_initializer,
        payload_initializer,
    })
}

pub fn parse_constructor_asset_bank(
    source: &[u8],
    region: &HandlerRegion,
) -> Result<ConstructorAssetBank, ConstructorAssetError> {
    let prefixes = parse_constructor_bank(source, region)?;
    let main = prefixes
        .main_payload
        .get(source)
        .expect("validated allocation");
    let mut assets = Vec::with_capacity(prefixes.entries.len());
    for entry in &prefixes.entries {
        let dispatch = constructor_dispatch(entry.constructor_selector);
        let model_binding = dispatch
            .filter(|d| {
                matches!(
                    d.context_initializer,
                    0x0047_a050 | 0x0047_a610 | 0x0040_4c30
                )
            })
            .map(|_| ConstructorModelBinding {
                source_span: ByteSpan::new(entry.prefix_span.start + 2, entry.prefix_span.end),
                raw_word: entry.argument_word,
                signed_slot: entry.argument_word as i16,
                consumer: 0x0047_a050,
            });
        let layout = dispatch
            .map(|d| payload_layout(d.payload_initializer))
            .unwrap_or(PayloadLayout::Opaque);
        let required = match layout {
            PayloadLayout::Opaque => 0,
            PayloadLayout::Flags => 4,
            PayloadLayout::TerrainLight => 10,
            PayloadLayout::Selector86 => 12,
            PayloadLayout::Selector83 => 0x10,
            PayloadLayout::Selector45 => 0x14,
            PayloadLayout::Selector46 => 0x0c,
            PayloadLayout::FlagsWords | PayloadLayout::Goal => 8,
            PayloadLayout::SharedActor => 0x42,
            PayloadLayout::PointTable16 => 0x10,
        };
        let offset = entry.payload_offset as usize;
        let bytes = offset
            .checked_add(required)
            .and_then(|end| main.get(offset..end))
            .ok_or(ConstructorAssetError::Truncated {
                local_index: entry.local_index,
                selector: entry.constructor_selector,
                offset: entry.payload_offset,
                length: required,
                allocation_bytes: main.len(),
            })?;
        let start = prefixes.main_payload.start + offset;
        let u16_field = |at| ConstructorField {
            span: ByteSpan::new(start + at, start + at + 2),
            value: u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()),
        };
        let u32_field = |at| ConstructorField {
            span: ByteSpan::new(start + at, start + at + 4),
            value: u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
        };
        let i32_field = |at| {
            let field = u32_field(at);
            ConstructorField {
                span: field.span,
                value: field.value as i32,
            }
        };
        let payload = match layout {
            PayloadLayout::Opaque => ConstructorPayloadView::Opaque,
            PayloadLayout::Flags => ConstructorPayloadView::Flags(u32_field(0)),
            PayloadLayout::Selector86 => {
                ConstructorPayloadView::Selector86(Selector86ConstructorPayload {
                    prefix_span: ByteSpan::new(start, start + required),
                    flags: u32_field(0),
                    retained_offset: entry.payload_offset.checked_add(4).ok_or(
                        ConstructorAssetError::OffsetOverflow {
                            local_index: entry.local_index,
                            offset: entry.payload_offset,
                        },
                    )?,
                    rate: ConstructorField {
                        span: u16_field(6).span,
                        value: u16_field(6).value as i16,
                    },
                })
            }
            PayloadLayout::Selector45 => {
                let offset = |at: usize| ConstructorField {
                    span: ByteSpan::new(start + at, start + at + 6),
                    value: std::array::from_fn(|k| {
                        i16::from_le_bytes([bytes[at + 2 * k], bytes[at + 2 * k + 1]])
                    }),
                };
                ConstructorPayloadView::Selector45(Selector45ConstructorPayload {
                    prefix_span: ByteSpan::new(start, start + required),
                    flags: u32_field(0),
                    team: i32_field(4),
                    offsets: [offset(8), offset(0x0e)],
                })
            }
            PayloadLayout::Goal => ConstructorPayloadView::Goal(GoalConstructorPayload {
                prefix_span: ByteSpan::new(start, start + required),
                flags: u32_field(0),
                side: i32_field(4),
            }),
            PayloadLayout::Selector46 => {
                ConstructorPayloadView::Selector46(Selector46ConstructorPayload {
                    prefix_span: ByteSpan::new(start, start + required),
                    flags: u32_field(0),
                    score: i32_field(4),
                    seconds: i32_field(8),
                })
            }
            PayloadLayout::FlagsWords => {
                let i16_field = |at| {
                    let field = u16_field(at);
                    ConstructorField {
                        span: field.span,
                        value: field.value as i16,
                    }
                };
                ConstructorPayloadView::FlagsWords(FlagsWordsConstructorPayload {
                    prefix_span: ByteSpan::new(start, start + required),
                    flags: u32_field(0),
                    words: [i16_field(4), i16_field(6)],
                })
            }
            PayloadLayout::Selector83 => {
                let i16_field = |at| {
                    let field = u16_field(at);
                    ConstructorField {
                        span: field.span,
                        value: field.value as i16,
                    }
                };
                ConstructorPayloadView::Selector83(Selector83ConstructorPayload {
                    prefix_span: ByteSpan::new(start, start + required),
                    flags: u32_field(0),
                    extent: [i16_field(4), i16_field(6)],
                    depth: i16_field(8),
                    travel: i16_field(0x0a),
                    risen: i16_field(0x0c),
                    sunk: i16_field(0x0e),
                })
            }
            PayloadLayout::TerrainLight => {
                ConstructorPayloadView::TerrainLight(TerrainLightConstructorPayload {
                    prefix_span: ByteSpan::new(start, start + required),
                    flags: u32_field(0),
                    channels: ConstructorField {
                        span: ByteSpan::new(start + 4, start + 7),
                        value: [bytes[4] as i8, bytes[5] as i8, bytes[6] as i8],
                    },
                    draw_gate: ConstructorField {
                        span: ByteSpan::new(start + 7, start + 8),
                        value: bytes[7],
                    },
                    radius: u16_field(8),
                })
            }
            PayloadLayout::PointTable16 => ConstructorPayloadView::PointTable16(Box::new(
                parse_point_table(source, &prefixes, entry, u32_field(4), u32_field(0x0c))?,
            )),
            PayloadLayout::SharedActor => {
                ConstructorPayloadView::SharedActor(ActorConstructorPayload {
                    prefix_span: ByteSpan::new(start, start + required),
                    flags: u32_field(0),
                    initial_health: u16_field(4),
                    word_08: u16_field(8),
                    selector_0c: i32_field(0x0c),
                    embedded_10_offset: entry.payload_offset.checked_add(0x10).ok_or(
                        ConstructorAssetError::OffsetOverflow {
                            local_index: entry.local_index,
                            offset: entry.payload_offset,
                        },
                    )?,
                    damage_responses: Box::new(std::array::from_fn(|channel| {
                        ConstructorDamageResponse {
                            channel: channel as u8,
                            threshold: u16_field(0x10 + channel * 4),
                            multiplier_q8: u16_field(0x12 + channel * 4),
                        }
                    })),
                    word_38: u16_field(0x38),
                    word_3c: u32_field(0x3c),
                    word_40: u16_field(0x40),
                    family4236d0: if dispatch.is_some_and(|d| {
                        matches!(
                            d.payload_initializer,
                            0x0042_39c0 | 0x0042_5270 | 0x0048_2480
                        )
                    }) {
                        Some(Box::new(specialized::family4236d0(
                            main,
                            prefixes.main_payload,
                            entry,
                        )?))
                    } else {
                        None
                    },
                    family425190: if dispatch.is_some_and(|d| d.payload_initializer == 0x0042_5270)
                    {
                        Some(Box::new(specialized::family425190(
                            main,
                            prefixes.main_payload,
                            entry,
                        )?))
                    } else {
                        None
                    },
                    family467070: if dispatch.is_some_and(|d| d.payload_initializer == 0x0046_8310)
                    {
                        Some(Box::new(specialized::family467070(entry)?))
                    } else {
                        None
                    },
                    family40d950: if dispatch
                        .is_some_and(|d| matches!(d.payload_initializer, 0x0040_d920 | 0x0042_edd0))
                    {
                        Some(Box::new(specialized::family40d950(
                            main,
                            prefixes.main_payload,
                            entry,
                        )?))
                    } else {
                        None
                    },
                    selector15: if entry.constructor_selector == 15 {
                        Some(Box::new(specialized::selector15(
                            main,
                            prefixes.main_payload,
                            entry,
                        )?))
                    } else {
                        None
                    },
                    selector30: if entry.constructor_selector == 30 {
                        Some(Box::new(specialized::selector30(
                            main,
                            prefixes.main_payload,
                            entry,
                        )?))
                    } else {
                        None
                    },
                    selector34: if entry.constructor_selector == 34 {
                        Some(Box::new(specialized::selector34(entry)?))
                    } else {
                        None
                    },
                    selector35: if entry.constructor_selector == 35 {
                        Some(Box::new(specialized::selector35(
                            main,
                            prefixes.main_payload,
                            entry,
                        )?))
                    } else {
                        None
                    },
                    selector74: if dispatch.is_some_and(|d| d.payload_initializer == 0x0042_d760) {
                        Some(Box::new(specialized::selector74(
                            main,
                            prefixes.main_payload,
                            entry,
                        )?))
                    } else {
                        None
                    },
                    behavior_resource: match dispatch {
                        Some(d) => specialized::behavior_resource(
                            d.payload_initializer,
                            main,
                            prefixes.main_payload,
                            entry,
                        )?
                        .map(Box::new),
                        None => None,
                    },
                    derived: if dispatch
                        .is_some_and(|d| derived::has_derived_actor(d.payload_initializer))
                    {
                        Some(Box::new(derived::parse_derived_actor(
                            main,
                            prefixes.main_payload,
                            entry,
                        )?))
                    } else {
                        None
                    },
                })
            }
        };
        assets.push(ConstructorAsset {
            local_index: entry.local_index,
            dispatch,
            model_binding,
            payload,
        });
    }
    Ok(ConstructorAssetBank { prefixes, assets })
}

#[derive(Clone, Copy)]
enum PayloadLayout {
    Opaque,
    Flags,
    TerrainLight,
    Selector86,
    Selector83,
    Selector45,
    Selector46,
    FlagsWords,
    Goal,
    SharedActor,
    PointTable16,
}

fn payload_layout(initializer: u32) -> PayloadLayout {
    match initializer {
        // 00448200 -> 004481C0, with the class-data base unchanged; selector
        // 54's 00464A20 -> 004649E0 copies the same ten bytes to +6C.
        0x0044_8200 | 0x0046_4a20 => PayloadLayout::TerrainLight,
        0x0042_de80 => PayloadLayout::Selector86,
        // 0043C3B0 -> 0043C3A0 keeps the payload at +78 after 00479D70.
        0x0043_c3b0 => PayloadLayout::Selector83,
        // 0046E830 -> 0046F300; deliberately not the flags/shared actor layout.
        0x0046_e860 => PayloadLayout::PointTable16,
        // Every listed initializer reaches 0047AC10 with unchanged class-data base.
        0x0040_38e0 | 0x0040_5de0 | 0x0040_d920 | 0x0042_39c0 | 0x0042_5270 | 0x0042_6bf0
        | 0x0042_d760 | 0x0042_edd0 | 0x0043_3c70 | 0x0043_d250 | 0x0045_5720 | 0x0045_57d0
        | 0x0045_6840 | 0x0045_8380 | 0x0045_8ca0 | 0x0045_b9a0 | 0x0045_bee0 | 0x0046_8310
        | 0x0047_ad70 | 0x0048_2480 | 0x0048_5460 | 0x0048_b390 => PayloadLayout::SharedActor,
        // These initializers reach 00479D70 at the same base without the actor layout.
        // 0045BFA0 -> 0045BFC0: the flags, then the team and two offsets.
        0x0045_bfa0 => PayloadLayout::Selector45,
        // 004756F0 -> 00475710: the flags, then the score and the seconds.
        0x0047_56f0 => PayloadLayout::Selector46,
        // 0046F870 -> 0046F890 and 0046FB00 -> 0046FB20: the flags, then
        // two WORDs.
        0x0046_f870 | 0x0046_fb00 => PayloadLayout::FlagsWords,
        // 00403EF0 -> 00403F10: the flags, then the goal's side.
        0x0040_3ef0 => PayloadLayout::Goal,
        0x0047_9db0 => PayloadLayout::Flags,
        _ => PayloadLayout::Opaque,
    }
}

fn parse_point_table(
    source: &[u8],
    bank: &ConstructorBank,
    entry: &ConstructorResource,
    word_04: ConstructorField<u32>,
    points_pointer: ConstructorField<u32>,
) -> Result<PointTableConstructorPayload, ConstructorAssetError> {
    let field_offset =
        entry
            .payload_offset
            .checked_add(0x0c)
            .ok_or(ConstructorAssetError::OffsetOverflow {
                local_index: entry.local_index,
                offset: entry.payload_offset,
            })?;
    if !bank
        .relocations
        .iter()
        .any(|r| r.field_offset == field_offset)
    {
        return Err(ConstructorAssetError::MissingPointTableRelocation {
            local_index: entry.local_index,
            field_offset,
        });
    }
    let main = bank.main_payload.get(source).expect("validated allocation");
    let target = points_pointer.value as usize;
    let points = target
        .checked_add(64)
        .and_then(|end| main.get(target..end))
        .ok_or(ConstructorAssetError::Truncated {
            local_index: entry.local_index,
            selector: entry.constructor_selector,
            offset: points_pointer.value,
            length: 64,
            allocation_bytes: main.len(),
        })?;
    let start = bank.main_payload.start + target;
    Ok(PointTableConstructorPayload {
        prefix_span: ByteSpan::new(
            bank.main_payload.start + entry.payload_offset as usize,
            points_pointer.span.end,
        ),
        word_04,
        points_pointer,
        points_span: ByteSpan::new(start, start + 64),
        points: std::array::from_fn(|index| ConstructorField {
            span: ByteSpan::new(start + index * 4, start + index * 4 + 4),
            value: std::array::from_fn(|component| {
                let at = index * 4 + component * 2;
                u16::from_le_bytes([points[at], points[at + 1]])
            }),
        }),
    })
}

// Selector-1 indexes the native 004603B0 cases 1..94. Columns: constructor
// called by the case, final vtable (including inline derived overrides), +08, +0C.
// Profiles are checked against every original executable vtable in corpus tests.
const DISPATCH: [(u32, u32, u32, u32); CONSTRUCTOR_SELECTOR_COUNT as usize] = [
    (0x00479c60, 0x00527b24, 0x00479d40, 0x00479db0), // 1
    (0x004156e0, 0x00520860, 0x0047a610, 0x00426bf0), // 2
    (0x0047a7a0, 0x00527c00, 0x0047a610, 0x0047ad70), // 3
    (0x0047a7a0, 0x00527c00, 0x0047a610, 0x0047ad70), // 4
    (0x0047a7a0, 0x00527c00, 0x0047a610, 0x0047ad70), // 5
    (0x0047a7a0, 0x00527c00, 0x0047a610, 0x0047ad70), // 6
    (0x00425e60, 0x00520e58, 0x0047a610, 0x00426bf0), // 7
    (0x004236d0, 0x00520b84, 0x0047a610, 0x004239c0), // 8
    (0x004236d0, 0x00520b84, 0x0047a610, 0x004239c0), // 9
    (0x00425190, 0x00520cc8, 0x0047a610, 0x00425270), // 10
    (0x004014f0, 0x0051f080, 0x0047a610, 0x00426bf0), // 11
    (0x00467070, 0x00527058, 0x0047a610, 0x00468310), // 12
    (0x00467070, 0x005269c4, 0x0047a610, 0x00468310), // 13
    (0x0046d840, 0x00527208, 0x0047a050, 0x00479db0), // 14
    (0x0042e250, 0x00521940, 0x0047a610, 0x0042edd0), // 15
    (0x0040f590, 0x005204d0, 0x0047a610, 0x00479db0), // 16
    (0x0046e120, 0x005272a0, 0x0047a610, 0x0047ad70), // 17
    (0x00476750, 0x005279e0, 0x0047a610, 0x0047ad70), // 18
    (0x0047a030, 0x00526978, 0x0047a050, 0x00448200), // 19
    (0x00450400, 0x00525030, 0x0047a610, 0x0047ad70), // 20
    (0x00405590, 0x0051f7a8, 0x0047a610, 0x00425270), // 21
    (0x0042dbe0, 0x00521830, 0x0047a050, 0x00479db0), // 22
    (0x0040d800, 0x005203c0, 0x0047a610, 0x0040d920), // 23
    (0x00455560, 0x0052577c, 0x0047a610, 0x00455720), // 24
    (0x00458250, 0x00525b88, 0x0047a610, 0x00458380), // 25
    (0x00455560, 0x005268ac, 0x0047a610, 0x004557d0), // 26
    (0x00455560, 0x00525848, 0x0047a610, 0x004557d0), // 27
    (0x00455560, 0x005259e8, 0x0047a610, 0x00455720), // 28
    (0x00457fe0, 0x00525ab4, 0x0047a610, 0x00458ca0), // 29
    (0x0048b170, 0x005289c0, 0x0047a610, 0x0048b390), // 30
    (0x00485480, 0x00528524, 0x0047a610, 0x00485460), // 31
    (0x00485f50, 0x005285f0, 0x0047a610, 0x00485460), // 32
    (0x00425e60, 0x00520e58, 0x0047a610, 0x00426bf0), // 33
    (0x00433b20, 0x00521eac, 0x0047a610, 0x00433c70), // 34
    (0x0043d070, 0x0052273c, 0x0047a610, 0x0043d250), // 35
    (0x00481110, 0x00527ee0, 0x0047a610, 0x00482480), // 36
    (0x004099a0, 0x0051ff54, 0x0047a610, 0x0040d920), // 37
    (0x00455560, 0x00525914, 0x0047a610, 0x00456840), // 38
    (0x0043b6d0, 0x00522398, 0x0047a610, 0x0040d920), // 39
    (0x0045b4a0, 0x005260a0, 0x0047a610, 0x0045b9a0), // 40
    (0x0047a7a0, 0x005267e0, 0x0047a610, 0x0045bee0), // 41
    (0x00401020, 0x0051f018, 0x0047a050, 0x00479db0), // 42
    (0x004541c0, 0x00525730, 0x0047a050, 0x00479db0), // 43
    (0x00453e70, 0x00525648, 0x0047a610, 0x0047ad70), // 44
    (0x0045bf00, 0x0052616c, 0x0047a610, 0x0045bfa0), // 45
    (0x00475610, 0x005278a0, 0x0047a610, 0x004756f0), // 46
    (0x0047fa40, 0x00527cd4, 0x0047a610, 0x0040d920), // 47
    (0x0043c7f0, 0x005225d0, 0x0047a050, 0x00479db0), // 48
    (0x00476380, 0x00527908, 0x0047a610, 0x0040d920), // 49
    (0x0043fe60, 0x00522888, 0x0047a050, 0x00479db0), // 50
    (0x00404b20, 0x0051f658, 0x00404c30, 0x00479db0), // 51
    (0x0047a7a0, 0x00526714, 0x0047a610, 0x0047ad70), // 52
    (0x0047a7a0, 0x00526648, 0x0047a610, 0x0047ad70), // 53
    (0x0047a490, 0x005265fc, 0x0047a610, 0x00464a20), // 54
    (0x00473d10, 0x005274c4, 0x0047a610, 0x0047ad70), // 55
    (0x0047a7a0, 0x00526530, 0x0047a610, 0x0047ad70), // 56
    (0x0047a7a0, 0x00526464, 0x0047a610, 0x0047ad70), // 57
    (0x004753e0, 0x00527838, 0x0047a610, 0x00479db0), // 58
    (0x00405780, 0x0051f900, 0x0047a610, 0x00405de0), // 59
    (0x00480800, 0x00527de8, 0x0047a610, 0x00426bf0), // 60
    (0x004128c0, 0x00520700, 0x0047a610, 0x0047ad70), // 61
    (0x00463f60, 0x00526dc8, 0x0047a610, 0x0047ad70), // 62
    (0x004404f0, 0x005229d8, 0x0047a610, 0x00479db0), // 63
    (0x004407c0, 0x00522a28, 0x0047a610, 0x00479db0), // 64
    (0x0042dc60, 0x0052187c, 0x0047a050, 0x00479db0), // 65
    (0x00405170, 0x0051f6c0, 0x0047a610, 0x0047ad70), // 66
    (0x00486530, 0x005286d8, 0x00479d40, 0x00479db0), // 67
    (0x00455560, 0x00526398, 0x0047a610, 0x00455720), // 68
    (0x00411090, 0x005205d8, 0x0047a610, 0x0047ad70), // 69
    (0x0046e770, 0x00527390, 0x0047a610, 0x0046e860), // 70
    (0x0046f7f0, 0x005273dc, 0x0047a610, 0x0046f870), // 71
    (0x0046f9e0, 0x00527428, 0x0047a610, 0x0046fb00), // 72
    (0x00450400, 0x005262cc, 0x0047a610, 0x0047ad70), // 73
    (0x0042d550, 0x00521764, 0x0047a610, 0x0042d760), // 74
    (0x004352f0, 0x00522008, 0x00479d40, 0x00479db0), // 75
    (0x00431ed0, 0x00521a40, 0x0047a610, 0x00479db0), // 76
    (0x00407410, 0x0051fbb0, 0x0047a610, 0x00426bf0), // 77
    (0x00406640, 0x0051fa9c, 0x0047a610, 0x0047ad70), // 78
    (0x00408740, 0x0051fc8c, 0x0047a610, 0x00426bf0), // 79
    (0x00408fc0, 0x0051fd68, 0x0047a610, 0x00426bf0), // 80
    (0x00409250, 0x0051fe44, 0x00479d40, 0x00479db0), // 81
    (0x00409890, 0x0051fe88, 0x0047a610, 0x00405de0), // 82
    (0x0043c300, 0x00522570, 0x0047a610, 0x0043c3b0), // 83
    (0x0043c070, 0x00522488, 0x0047a610, 0x0047ad70), // 84
    (0x00405f10, 0x0051f9cc, 0x0047a610, 0x0047ad70), // 85
    (0x0042de50, 0x005218cc, 0x00479d40, 0x0042de80), // 86
    (0x0048c710, 0x00528ab0, 0x00479d40, 0x00479db0), // 87
    (0x00443890, 0x00524ac0, 0x0047a610, 0x0047ad70), // 88
    (0x0046e060, 0x00527254, 0x0047a050, 0x00479db0), // 89
    (0x00462660, 0x00526b50, 0x0047a610, 0x0047ad70), // 90
    (0x004027e0, 0x0051f1e0, 0x0047a610, 0x004038e0), // 91
    (0x00403ec0, 0x0051f2b0, 0x0047a610, 0x00403ef0), // 92
    (0x00440010, 0x005228f0, 0x0047a610, 0x0047ad70), // 93
    (0x004458e0, 0x00524e20, 0x00479d40, 0x00479db0), // 94
];

#[cfg(test)]
#[path = "constructor_assets_tests.rs"]
mod tests;
