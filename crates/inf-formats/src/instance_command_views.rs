//! Bounded virtual-+4 command inputs. Select either an actual constructor resolved
//! in the caller's scene or the exact shared actor class. An H13 global slot is
//! not a constructor selector.

use serde::Serialize;

use crate::instance_assets::{add, InstanceAllocation};
use crate::{AssetRelocation, ByteSpan, InstanceAssetError, InstanceField};

#[path = "instance_command_views_extended_actors.rs"]
mod extended_actors;

/// The concrete native command dispatch being decoded. Shared actor construction
/// is an intermediate class with virtual method 0047ADA0, not an H13 selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "selector", rename_all = "snake_case")]
pub enum InstanceCommandTarget {
    Constructor(u16),
    SharedActor,
    /// Intermediate 00425E60 actor, whose virtual command handler is 00426030.
    DerivedActor,
    /// Intermediate 00428650 actor, whose virtual command handler is 004276B0.
    Family428650,
    /// Intermediate 004236D0 actor, whose virtual command handler is 004239E0.
    Family4236d0,
}

pub struct InstanceCommandRequest<'a> {
    pub source: &'a [u8],
    pub main_payload: ByteSpan,
    /// Canonical H14 relocation records; nested command pointers must be present.
    pub relocations: &'a [AssetRelocation],
    pub command_offset: u32,
    pub target: InstanceCommandTarget,
    pub read_scope: InstanceCommandReadScope,
}

/// Asset inspection decodes an authored table in full. Runtime dispatch only
/// decodes immediate operands: retained mutable tables belong to their live
/// consumer, whose earlier writes can change later reads in the same command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstanceCommandReadScope {
    Inspection,
    Dispatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InstanceCommandView {
    pub target: InstanceCommandTarget,
    pub command_offset: u32,
    pub code: InstanceField<i32>,
    /// None means this target's virtual-+4 path has no audited profile here.
    pub proven_method: Option<u32>,
    /// Exact shared consumer of this bounded view, not a claim of execution.
    pub payload_consumer: Option<u32>,
    pub payload: InstanceCommandPayload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct InstanceModelSlot {
    pub raw: InstanceField<u16>,
    /// 0047A110 consumes object +42 as a signed H8 resource argument.
    pub signed_slot: i16,
}

/// A conditional native read whose runtime guard is not evaluated by asset tools.
/// Available means two authored bytes exist, not that the native read executed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum ConditionalInstanceWord {
    Available(InstanceField<u16>),
    /// The guard may skip this read; do not consume relocation-tail bytes or
    /// silently turn the missing operand into zero.
    OutsideAllocation {
        allocation_offset: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum ConditionalInstanceDword {
    Available(InstanceField<u32>),
    OutsideAllocation { allocation_offset: u32 },
}

/// Selector 60 command 19, retained by object +190 and consumed by 00480B30,
/// 00480460, 00480510 and 00480610. Runtime normalization is not applied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlacementSpawnList {
    pub header_span: ByteSpan,
    /// Signed switch input in 00480C80. Other header WORD +2 remains unnamed.
    pub mode_word: InstanceField<i16>,
    pub count: InstanceField<i32>,
    /// Native +8 pointer is dereferenced only for a positive signed count.
    pub rows_pointer: Option<InstanceField<u32>>,
    pub rows_span: Option<ByteSpan>,
    pub rows: Vec<PlacementSpawnRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlacementSpawnRow {
    pub index: u32,
    pub span: ByteSpan,
    /// Full one-based H14 placement argument, not H13/H8 or a campaign index.
    /// 0047F7B0 compares reference-1 to the signed placement index at object +2A.
    pub placement_reference: InstanceField<i32>,
    /// Exact signed WORD inputs at +4,+6,+8,+A,+C,+E. See INSTANCE_ASSETS.md.
    pub signed_words_04_to_0e: [InstanceField<i16>; 6],
    pub flags: InstanceField<u8>,
    /// Original +14 DWORD: zeroed by 00480B30, then replaced with a host pointer.
    pub runtime_pointer_placeholder: InstanceField<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum InstanceCommandPayload {
    /// Unclassified here, including specialized and native rejection paths.
    Opaque,
    /// This particular consumer accepts the code without reading payload bytes.
    NoPayloadRead,
    /// Selectors 78 and 88 command 16 retain without a read; selector 60 command 19
    /// in Dispatch scope defers its reads to the mutable runtime owner.
    /// The address itself may be exactly one past the allocation's last byte.
    RetainedPayloadPointer {
        allocation_offset: u32,
    },
    /// Selector 86 code 2 retains eight bytes and rewrites the final WORD.
    /// This view reads the signed rate but never applies the runtime write.
    Selector86Data {
        allocation_offset: u32,
        rate: InstanceField<i16>,
    },
    ModelSlot(InstanceModelSlot),
    /// 0046D780 code3 delegates to46D500; code4 copies WORD+46.
    PickupKind(InstanceField<u16>),
    PickupValue(InstanceField<u16>),
    /// Selector 19 codes 3..5 copy one channel byte; 0045B1A0 reads it signed.
    /// Selector 54's codes write the same fields at +6C..+72.
    TerrainLightChannel {
        channel: u8,
        value: InstanceField<i8>,
    },
    /// Selector 19 code 6 writes the later model-draw gate at object +4A.
    TerrainLightDrawGate(InstanceField<u8>),
    /// Selector 19 code 7 copies the unsigned terrain-light radius to +48.
    TerrainLightRadius(InstanceField<u16>),
    /// Selector 87 code 2 zero-extends this numeric WORD to object +4C;
    /// it is not an H0 string reference. The runtime also moves +40 to tail.
    MapAnnotationLabel(InstanceField<u16>),
    /// Selector 87 codes 3..5 copy unsigned raw RGB bytes to +52..+54.
    MapAnnotationColor {
        channel: u8,
        value: InstanceField<u8>,
    },
    /// Selector 87 code 6 copies the unsigned radius WORD to +50.
    MapAnnotationRadius(InstanceField<u16>),
    /// Selector 51 code 3: authored one-based placement WORD, decremented
    /// with WORD wrapping into +5E by 00404CC0. Keep the original bits.
    RouteNodePlacementReference(InstanceField<u16>),
    /// Selector 51 code 4: raw DWORD equality switch; only 0 and 1 accept.
    RouteNodeMode(InstanceField<u32>),
    /// Selector 51 codes 5/8 have no operand read. The mask is a recovered
    /// instruction immediate ORed into +48, not an authored operand field.
    RouteNodeFlagOr {
        mask: u8,
    },
    ActorWords8e(InstanceField<[u16; 3]>),
    ActorWord8c(InstanceField<u16>),
    ActorSelectorB4(InstanceField<i32>),
    ActorMaximumHealth(InstanceField<u16>),
    ActorCurrentHealth(InstanceField<u16>),
    ActorWord7a(InstanceField<u16>),
    /// SAR 8 then low-byte store at object +7C; raw signed input is retained.
    ActorShiftedByte7c(InstanceField<i32>),
    /// SAR 8 then low-byte store at object +7D; raw signed input is retained.
    ActorShiftedByte7d(InstanceField<i32>),
    /// Full signed DWORD stored at +C4 and compared with 3.
    ActorStateC4(InstanceField<i32>),
    /// Code 13: when 00488AA0 supplies the first world object, query its +9C
    /// bitset with 004752E0 and call 00479FA0 on false. Index zero tests true.
    ActorDisableUnlessBit(ConditionalInstanceWord),
    /// Code 14: same guarded unsigned WORD query, calling 00479FA0 on true.
    ActorDisableIfBit(ConditionalInstanceWord),
    ActorWord76(InstanceField<u16>),
    /// 00426030 code 17 zero-extends this WORD into object +164.
    ActorWord164(InstanceField<u16>),
    /// 00426030 code 18 negates the raw DWORD into object +144.
    ActorNegatedDword144(InstanceField<u32>),
    Selector11Flag02(InstanceField<u32>),
    Selector11Block1a0(InstanceField<[u8; 10]>),
    Selector11BehaviorWord04(ConditionalInstanceDword),
    Selector11Flag04(InstanceField<u16>),
    Selector11Flag10(InstanceField<u16>),
    /// 004276B0 codes20/21 set home position using the terrain sampler.
    FamilyHomeTerrainXZ(InstanceField<[u16; 2]>),
    FamilyHomeTerrainXYZ(InstanceField<[u16; 3]>),
    /// 004276B0 code22 retains the full DWORD at object+20C.
    FamilyPlacementReference(InstanceField<u32>),
    /// 0043D270 code24 copies the DWORD at+240, then separately clears+243.
    Selector35Packed240(InstanceField<u32>),
    /// 0040D9C0 code16 sign-extends one BYTE into object+13C.
    Actor40d800Maximum(InstanceField<i8>),
    Actor40d800Flag14e(InstanceField<u8>),
    Actor40d800Refill(InstanceField<u8>),
    Actor40d800Word144(InstanceField<u16>),
    Actor40d800Words146(InstanceField<[u16; 3]>),
    Selector30Placement180(InstanceField<u16>),
    Selector30Byte15c(InstanceField<u8>),
    Selector30Byte160(InstanceField<u8>),
    Selector30Byte164(InstanceField<u8>),
    Selector30Byte18f(InstanceField<u8>),
    Selector30Byte167(InstanceField<u8>),
    Selector30Flag18e(InstanceField<u8>),
    Selector30Flag166(InstanceField<u8>),
    Selector30Flag190(InstanceField<u8>),
    Selector30Flag191(InstanceField<u8>),
    Selector30Flag192(InstanceField<u8>),
    Selector30Word18c(InstanceField<u16>),
    /// 0040A1D0 codes 21/22/26/28: zero-extended WORDs into +158/+15C, and
    /// WORD stores into +17C/+17E.
    Selector37Word158(InstanceField<u16>),
    /// 004763F0 code 21: the raised square's half-width +158.
    Selector49Word158(InstanceField<u16>),
    /// 004763F0 code 22: the sink depth +15A.
    Selector49Word15a(InstanceField<u16>),
    /// 004763F0 code 23: the countdown seconds +15C.
    Selector49Seconds15c(InstanceField<u32>),
    /// 004763F0 code 24: a one-based placement reference +16C, which the
    /// postinitialization resolves to that actor's handle.
    Selector49Reference16c(InstanceField<u32>),
    Selector37Word15c(InstanceField<u16>),
    Selector37Word17c(InstanceField<u16>),
    Selector37Word17e(InstanceField<u16>),
    /// 0040A1D0 code 23 sign-extends one WORD into +160.
    Selector37SignedWord160(InstanceField<i16>),
    /// 0040A1D0 code 24 writes the BYTE to +164 and BYTE * 1,000,000 to +170.
    Selector37Seconds164(InstanceField<u8>),
    /// 0040A1D0 code 25 stores BYTE != 0 at +166; code 27 copies a BYTE to +165.
    Selector37Flag166(InstanceField<u8>),
    Selector37Byte165(InstanceField<u8>),
    /// 0043B7D0 code 22 copies a DWORD to +15C.
    Selector39Dword15c(InstanceField<u32>),
    /// 0043B7D0 code 24 copies a BYTE to +160.
    Selector39Byte160(InstanceField<u8>),
    /// 0043B7D0 codes 23/25/27/28 store BYTE != 0 at +162/+163/+161/+165.
    Selector39Flag(InstanceField<u8>),
    /// 00454A30 codes 16..19 each copy one BYTE: the pickup spawner's kind
    /// (+138), count (+139), seconds between pickups (+13A) and repeat flag
    /// (+13B).
    /// 00458C40 code 16 stores this DWORD at +14C and its low WORD at +13A,
    /// the Bomb's fuse.
    Selector29Fuse(InstanceField<u32>),
    Selector53Kind138(InstanceField<u8>),
    Selector53Count139(InstanceField<u8>),
    Selector53Seconds13a(InstanceField<u8>),
    Selector53Repeat13b(InstanceField<u8>),
    /// 0047FDC0 code 22 copies the Siren's alarm seconds BYTE to +16C.
    Selector47Seconds16c(InstanceField<u8>),
    /// 0047FDC0 code 23 sign-extends a WORD into the Siren's radius +164.
    Selector47Radius164(InstanceField<i16>),
    /// 00454880 code 16 stores BYTE == 0 at the mining rig's +138 (nonzero
    /// runs it) and clears object flag 0x40.
    Selector52Stopped(InstanceField<u8>),
    /// 00405D10 code 16: a nonzero BYTE wakes the boulder (+140 = 0, flags
    /// 0x04200000 cleared).
    Selector59Wake(InstanceField<u8>),
    /// 00405D10 code 17 stores BYTE == 0 at +144 (it crushes).
    Selector59Crushes144(InstanceField<u8>),
    /// 00405D10 codes 18/19 store BYTE != 0 at +145 (it crushes at any
    /// speed) and +146 (it smokes).
    Selector59CrushesAny145(InstanceField<u8>),
    Selector59Smokes146(InstanceField<u8>),
    /// 00478860, the Force Field's +04: code 0x10 copies the radius WORD to
    /// +154 and +28, 0x11 the height WORD to +158 and 0x12 the key grant
    /// WORD to +17A; 0x13 stores BYTE != 0 at +178 (its own team passes);
    /// 0x14 copies the hint DWORD to +180.
    Selector18Radius(InstanceField<u16>),
    Selector18Height(InstanceField<u16>),
    Selector18KeyGrant(InstanceField<u16>),
    Selector18TeamPasses(InstanceField<u8>),
    Selector18Hint(InstanceField<u32>),
    /// 0042F100 code 22 copies a DWORD to +15C.
    Selector15Dword15c(InstanceField<u32>),
    /// 0042F100 codes 23..25 grant this WORD ID into the +198 owner (004750C0).
    Selector15GrantWord(InstanceField<u16>),
    /// 00401080 codes 3/4 square these zero-extended WORD radii into
    /// +44 (the trigger) and +48 (the wake radius).
    Selector42Radius(InstanceField<u16>),
    Selector42WakeRadius(InstanceField<u16>),
    /// 00401080 codes 5/6 store BYTE != 0 at +4C (fire on draw) and +4D
    /// (update disabled).
    Selector42OnDraw(InstanceField<u8>),
    Selector42Disabled(InstanceField<u8>),
    /// 00409320 codes 2/3 store these DWORDs at +44 (the members the
    /// spawner keeps) and +48 (the distance a moving player covers).
    Selector81Count44(InstanceField<u32>),
    Selector81Distance48(InstanceField<u32>),
    /// 00454220 codes 3/4 store these WORD seconds times 1,000,000 at +44
    /// (the on phase) and +48 (the off phase).
    Selector43OnSeconds(InstanceField<u16>),
    Selector43OffSeconds(InstanceField<u16>),
    /// 00454220 code 5 stores this WORD at +4C, the spread.
    Selector43Spread(InstanceField<u16>),
    /// 00454220 code 6 stores this BYTE at +50, the ring mode.
    Selector43Ring(InstanceField<u8>),
    /// 00454220 code 7 stores DWORD != 0 at +51.
    Selector43Byte51(InstanceField<u32>),
    /// 0042D6F0, the zone's +04, reads one WORD for every code above 15:
    /// 16 halves it (signed, toward zero) into the half width +152, 17 into
    /// the half depth +154, and 18 copies it to the half height +150; every
    /// other code refuses after the read.
    /// 0043C2A0, the capture node's +04: code 16's DWORD, a one-based
    /// placement reference the after-load resolves (0047F7B0).
    Selector84Link(InstanceField<u32>),
    /// 00408880, the eye's +04: codes through 18 are selector 77's
    /// (00426030), 19's DWORD is a one-based placement reference the
    /// after-load resolves (0047F7B0), and any other refuses unread.
    Selector79Link(InstanceField<u32>),
    Selector74Width(InstanceField<u16>),
    Selector74Depth(InstanceField<u16>),
    Selector74Height(InstanceField<u16>),
    Selector74Refused(InstanceField<u16>),
    /// 0045C010 code 3: the spawn marker's team +84, the DWORD plus one
    /// (1 for 0).
    Selector45Team(InstanceField<u32>),
    /// 0045C010 codes 4 and 5: the offsets +78 and +7E as they stand.
    Selector45Offset {
        code: i32,
        words: InstanceField<[u16; 3]>,
    },
    /// 00475CB0 code 3: the territory's score, kept four times over at +6C.
    Selector46Score(InstanceField<u32>),
    /// 00475CB0 code 4: the seconds it takes, kept in microseconds at +70.
    Selector46Seconds(InstanceField<u32>),
    /// A race marker's DWORD: 0046F810 codes 3 and 4 (selector 71's half
    /// extents +70 and +6C), 0046FA70 codes 3 to 6 (selector 72's number
    /// +6C, the next +70, the half width +74 and the depth +78), and
    /// 00404420 code 3 (Powerball's goal's points +70).
    RaceDword(InstanceField<u32>),
    /// 0043C840 code 3 stores this WORD at +44, the ceiling (signed).
    Selector48Ceiling(InstanceField<u16>),
    /// 0043C840 code 4 stores this WORD at +46, the radius (0: none).
    Selector48Radius(InstanceField<u16>),
    /// 0043C840 code 5 stores this BYTE at +48: the height is relative.
    Selector48Relative(InstanceField<u8>),
    /// 0043C840 code 6 disposes the ceiling when the first player owns
    /// this WORD grant (004752E0).
    Selector48Grant(InstanceField<u16>),
    /// 0043FF40 code 3 squares this zero-extended WORD radius into +44.
    Selector50Radius(InstanceField<u16>),
    /// 0043FF40 code 4 multiplies these WORD seconds by 1,000,000 into +48.
    Selector50Seconds(InstanceField<u16>),
    /// 0043FF40 code 5 copies this player-command ID to +4C, then disposes
    /// the object when the first player already owns the command.
    Selector50Command(InstanceField<u32>),
    /// 0043FF40 code 6 stores DWORD != 0 at +50, the command's silent byte.
    Selector50Silent(InstanceField<u32>),
    /// 00411100 code 16 doubles this input into +14C; later 00411150 uses it signed.
    Selector69DoubledDword14c(InstanceField<i32>),
    /// 00411100 code 17 writes +144, consumed with signed tests by 00411150/00411570.
    Selector69Dword144(InstanceField<i32>),
    PlacementSpawnList(Box<PlacementSpawnList>),
}

pub fn decode_instance_command(
    request: InstanceCommandRequest<'_>,
) -> Result<InstanceCommandView, InstanceAssetError> {
    let allocation = InstanceAllocation::new(request.source, request.main_payload)?;
    let code = allocation.i32(request.command_offset, "instance command code")?;
    let payload_offset = add(request.command_offset, 4, "instance command payload")?;
    let profile = command_profile(request.target);
    let mut view = InstanceCommandView {
        target: request.target,
        command_offset: request.command_offset,
        code,
        proven_method: profile.map(|p| p.0),
        payload_consumer: None,
        payload: InstanceCommandPayload::Opaque,
    };
    let Some((_, family)) = profile else {
        return Ok(view);
    };
    if let Some((consumer, payload)) =
        extended_actors::decode(&request, &allocation, code.value, payload_offset)?
    {
        view.payload_consumer = Some(consumer);
        view.payload = payload;
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(86) && code.value == 2 {
        allocation.range(payload_offset, 8, "selector 86 retained writable data")?;
        let rate_offset = add(payload_offset, 2, "selector 86 rate")?;
        let field = allocation.u16(rate_offset, "selector 86 rate")?;
        view.payload_consumer = Some(0x0042_dec0);
        view.payload = InstanceCommandPayload::Selector86Data {
            allocation_offset: payload_offset,
            rate: InstanceField {
                span: field.span,
                value: field.value as i16,
            },
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(87) && matches!(code.value, 2..=6) {
        view.payload_consumer = Some(0x0048_c7a0);
        view.payload = match code.value {
            2 => InstanceCommandPayload::MapAnnotationLabel(
                allocation.u16(payload_offset, "map annotation numeric label")?,
            ),
            6 => InstanceCommandPayload::MapAnnotationRadius(
                allocation.u16(payload_offset, "map annotation radius")?,
            ),
            channel => {
                let span = allocation.range(payload_offset, 1, "map annotation RGB byte")?;
                InstanceCommandPayload::MapAnnotationColor {
                    channel: (channel - 3) as u8,
                    value: InstanceField {
                        span,
                        value: request.source[span.start],
                    },
                }
            }
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(88) && code.value == 16 {
        view.payload_consumer = Some(0x0044_3900);
        view.payload = InstanceCommandPayload::RetainedPayloadPointer {
            allocation_offset: payload_offset,
        };
        return Ok(view);
    }
    // 00445900, the missile launcher's +04: code 2 keeps the operand's
    // address at +40, its zones.
    if request.target == InstanceCommandTarget::Constructor(94) && code.value == 2 {
        view.payload_consumer = Some(0x0044_5900);
        view.payload = InstanceCommandPayload::RetainedPayloadPointer {
            allocation_offset: payload_offset,
        };
        return Ok(view);
    }
    // 00406800, the ending director's +04: code 16 keeps the operand's
    // address at +154, three one-based route-node numbers.
    if request.target == InstanceCommandTarget::Constructor(78) && code.value == 16 {
        view.payload_consumer = Some(0x0040_6800);
        view.payload = InstanceCommandPayload::RetainedPayloadPointer {
            allocation_offset: payload_offset,
        };
        return Ok(view);
    }
    if matches!(request.target, InstanceCommandTarget::Constructor(14 | 89))
        && matches!(code.value, 3 | 4)
    {
        view.payload_consumer = Some(0x0046_d780);
        let value = allocation.u16(payload_offset, "pickup WORD operand")?;
        view.payload = if code.value == 3 {
            InstanceCommandPayload::PickupKind(value)
        } else {
            InstanceCommandPayload::PickupValue(value)
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(51) && matches!(code.value, 3..=8) {
        view.payload_consumer = Some(0x0040_4cc0);
        view.payload = match code.value {
            3 => InstanceCommandPayload::RouteNodePlacementReference(
                allocation.u16(payload_offset, "route-node placement reference")?,
            ),
            4 => InstanceCommandPayload::RouteNodeMode(
                allocation.u32(payload_offset, "route-node mode")?,
            ),
            5 => InstanceCommandPayload::RouteNodeFlagOr { mask: 2 },
            6 | 7 => InstanceCommandPayload::NoPayloadRead,
            8 => InstanceCommandPayload::RouteNodeFlagOr { mask: 0x10 },
            _ => unreachable!("bounded route-node command"),
        };
        return Ok(view);
    }
    // Selector 54's 00464940 is 00448120 with the fields at +6C.
    if let (InstanceCommandTarget::Constructor(selector @ (19 | 54)), 3..=7) =
        (request.target, code.value)
    {
        view.payload_consumer = Some(if selector == 19 {
            0x0044_8120
        } else {
            0x0046_4940
        });
        view.payload = if code.value == 7 {
            InstanceCommandPayload::TerrainLightRadius(
                allocation.u16(payload_offset, "terrain-light radius")?,
            )
        } else {
            let span = allocation.range(payload_offset, 1, "terrain-light byte")?;
            let value = request.source[span.start];
            if code.value == 6 {
                InstanceCommandPayload::TerrainLightDrawGate(InstanceField { span, value })
            } else {
                InstanceCommandPayload::TerrainLightChannel {
                    channel: (code.value - 3) as u8,
                    value: InstanceField {
                        span,
                        value: value as i8,
                    },
                }
            }
        };
        return Ok(view);
    }
    // 004407E0 code 3 retains the operand address at +74; the after-load
    // pass and the gate's +4C read it later.
    if request.target == InstanceCommandTarget::Constructor(64) && code.value == 3 {
        view.payload_consumer = Some(0x0044_07e0);
        view.payload = InstanceCommandPayload::RetainedPayloadPointer {
            allocation_offset: payload_offset,
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(42) && matches!(code.value, 3..=6) {
        view.payload_consumer = Some(0x0040_1080);
        view.payload = match code.value {
            3 => InstanceCommandPayload::Selector42Radius(
                allocation.u16(payload_offset, "selector 42 radius")?,
            ),
            4 => InstanceCommandPayload::Selector42WakeRadius(
                allocation.u16(payload_offset, "selector 42 wake radius")?,
            ),
            code => {
                let span = allocation.range(payload_offset, 1, "selector 42 flag byte")?;
                let field = InstanceField {
                    span,
                    value: request.source[span.start],
                };
                if code == 5 {
                    InstanceCommandPayload::Selector42OnDraw(field)
                } else {
                    InstanceCommandPayload::Selector42Disabled(field)
                }
            }
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(81) && matches!(code.value, 2 | 3) {
        view.payload_consumer = Some(0x0040_9320);
        let value = allocation.u32(payload_offset, "selector 81 DWORD")?;
        view.payload = if code.value == 2 {
            InstanceCommandPayload::Selector81Count44(value)
        } else {
            InstanceCommandPayload::Selector81Distance48(value)
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(43) && matches!(code.value, 3..=7) {
        view.payload_consumer = Some(0x0045_4220);
        view.payload = match code.value {
            3 => InstanceCommandPayload::Selector43OnSeconds(
                allocation.u16(payload_offset, "selector 43 on seconds")?,
            ),
            4 => InstanceCommandPayload::Selector43OffSeconds(
                allocation.u16(payload_offset, "selector 43 off seconds")?,
            ),
            5 => InstanceCommandPayload::Selector43Spread(
                allocation.u16(payload_offset, "selector 43 spread")?,
            ),
            6 => {
                let span = allocation.range(payload_offset, 1, "selector 43 ring byte")?;
                InstanceCommandPayload::Selector43Ring(InstanceField {
                    span,
                    value: request.source[span.start],
                })
            }
            _ => InstanceCommandPayload::Selector43Byte51(
                allocation.u32(payload_offset, "selector 43 flag")?,
            ),
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(45) && matches!(code.value, 3..=5) {
        view.payload_consumer = Some(0x0045_c010);
        view.payload = if code.value == 3 {
            InstanceCommandPayload::Selector45Team(
                allocation.u32(payload_offset, "selector 45 team")?,
            )
        } else {
            let span = allocation.range(payload_offset, 6, "selector 45 offset")?;
            let mut words = [0; 3];
            for (index, word) in words.iter_mut().enumerate() {
                *word = allocation
                    .u16(
                        add(payload_offset, index * 2, "selector 45 offset")?,
                        "selector 45 offset",
                    )?
                    .value;
            }
            InstanceCommandPayload::Selector45Offset {
                code: code.value,
                words: InstanceField { span, value: words },
            }
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(46) && matches!(code.value, 3 | 4) {
        view.payload_consumer = Some(0x0047_5cb0);
        let value = allocation.u32(payload_offset, "selector 46 value")?;
        view.payload = if code.value == 3 {
            InstanceCommandPayload::Selector46Score(value)
        } else {
            InstanceCommandPayload::Selector46Seconds(value)
        };
        return Ok(view);
    }
    let race = match (request.target, code.value) {
        (InstanceCommandTarget::Constructor(71), 3 | 4) => Some(0x0046_f810),
        (InstanceCommandTarget::Constructor(72), 3..=6) => Some(0x0046_fa70),
        (InstanceCommandTarget::Constructor(92), 3) => Some(0x0040_4420),
        _ => None,
    };
    if let Some(consumer) = race {
        view.payload_consumer = Some(consumer);
        view.payload =
            InstanceCommandPayload::RaceDword(allocation.u32(payload_offset, "race marker value")?);
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(48) && matches!(code.value, 3..=6) {
        view.payload_consumer = Some(0x0043_c840);
        view.payload = match code.value {
            3 => InstanceCommandPayload::Selector48Ceiling(
                allocation.u16(payload_offset, "selector 48 ceiling")?,
            ),
            4 => InstanceCommandPayload::Selector48Radius(
                allocation.u16(payload_offset, "selector 48 radius")?,
            ),
            5 => {
                let span = allocation.range(payload_offset, 1, "selector 48 relative byte")?;
                InstanceCommandPayload::Selector48Relative(InstanceField {
                    span,
                    value: request.source[span.start],
                })
            }
            _ => InstanceCommandPayload::Selector48Grant(
                allocation.u16(payload_offset, "selector 48 grant")?,
            ),
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(50) && matches!(code.value, 3..=6) {
        view.payload_consumer = Some(0x0043_ff40);
        view.payload = match code.value {
            3 => InstanceCommandPayload::Selector50Radius(
                allocation.u16(payload_offset, "selector 50 radius")?,
            ),
            4 => InstanceCommandPayload::Selector50Seconds(
                allocation.u16(payload_offset, "selector 50 seconds")?,
            ),
            5 => InstanceCommandPayload::Selector50Command(
                allocation.u32(payload_offset, "selector 50 command")?,
            ),
            _ => InstanceCommandPayload::Selector50Silent(
                allocation.u32(payload_offset, "selector 50 silent flag")?,
            ),
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(69) && matches!(code.value, 16 | 17) {
        let field = allocation.i32(payload_offset, "selector 69 DWORD input")?;
        view.payload_consumer = Some(0x0041_1100);
        view.payload = if code.value == 16 {
            InstanceCommandPayload::Selector69DoubledDword14c(field)
        } else {
            InstanceCommandPayload::Selector69Dword144(field)
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(60) && code.value == 19 {
        view.payload_consumer = Some(0x0048_0b30);
        view.payload = match request.read_scope {
            InstanceCommandReadScope::Inspection => InstanceCommandPayload::PlacementSpawnList(
                Box::new(placement_spawn_list(&request, &allocation, payload_offset)?),
            ),
            InstanceCommandReadScope::Dispatch => InstanceCommandPayload::RetainedPayloadPointer {
                allocation_offset: payload_offset,
            },
        };
        return Ok(view);
    }
    if request.target == InstanceCommandTarget::Constructor(11) && matches!(code.value, 19..=23) {
        view.payload_consumer = Some(0x0040_2330);
        view.payload = match code.value {
            19 => InstanceCommandPayload::Selector11Flag02(
                allocation.u32(payload_offset, "selector 11 flag 02")?,
            ),
            20 => {
                let span = allocation.range(payload_offset, 10, "selector 11 parameter block")?;
                InstanceCommandPayload::Selector11Block1a0(InstanceField {
                    span,
                    value: request.source[span.start..span.end].try_into().unwrap(),
                })
            }
            21 => InstanceCommandPayload::Selector11BehaviorWord04(
                match allocation.u32(payload_offset, "conditional selector 11 behavior word") {
                    Ok(field) => ConditionalInstanceDword::Available(field),
                    Err(InstanceAssetError::OutOfBounds { .. }) => {
                        ConditionalInstanceDword::OutsideAllocation {
                            allocation_offset: payload_offset,
                        }
                    }
                    Err(error) => return Err(error),
                },
            ),
            22 => InstanceCommandPayload::Selector11Flag04(
                allocation.u16(payload_offset, "selector 11 flag 04")?,
            ),
            23 => InstanceCommandPayload::Selector11Flag10(
                allocation.u16(payload_offset, "selector 11 flag 10")?,
            ),
            _ => unreachable!("bounded selector 11 command"),
        };
        return Ok(view);
    }
    if has_426030_path(request.target) && matches!(code.value, 16..=18) {
        view.payload_consumer = Some(0x0042_6030);
        view.payload = match code.value {
            16 => InstanceCommandPayload::NoPayloadRead,
            17 => InstanceCommandPayload::ActorWord164(
                allocation.u16(payload_offset, "instance actor word +164")?,
            ),
            _ => InstanceCommandPayload::ActorNegatedDword144(
                allocation.u32(payload_offset, "instance actor negated DWORD +144")?,
            ),
        };
        return Ok(view);
    }
    if matches!(code.value, 0 | 1) {
        view.payload_consumer = Some(0x0047_9e70);
        view.payload = InstanceCommandPayload::NoPayloadRead;
        return Ok(view);
    }
    if matches!(family, Family::Base) {
        return Ok(view);
    }
    if code.value == 2 {
        let raw = allocation.u16(payload_offset, "instance model slot")?;
        view.payload_consumer = Some(0x0047_a1a0);
        view.payload = InstanceCommandPayload::ModelSlot(InstanceModelSlot {
            raw,
            signed_slot: raw.value as i16,
        });
        return Ok(view);
    }
    if matches!(family, Family::Visual) {
        return Ok(view);
    }
    // Every audited actor wrapper forwards codes <= 15 unchanged to 0047ADA0.
    // Codes 13/14 retain conditional operand availability without executing
    // the world-object guard, bitset query or object-disable policy.
    view.payload = match code.value {
        3 => InstanceCommandPayload::NoPayloadRead,
        4 => {
            let span = allocation.range(payload_offset, 6, "instance actor words +8E")?;
            let mut words = [0; 3];
            for (index, word) in words.iter_mut().enumerate() {
                *word = allocation
                    .u16(
                        add(payload_offset, index * 2, "instance actor words +8E")?,
                        "instance actor words +8E",
                    )?
                    .value;
            }
            InstanceCommandPayload::ActorWords8e(InstanceField { span, value: words })
        }
        5 => InstanceCommandPayload::ActorWord8c(
            allocation.u16(payload_offset, "instance actor word +8C")?,
        ),
        6 => InstanceCommandPayload::ActorSelectorB4(
            allocation.i32(payload_offset, "instance actor selector +B4")?,
        ),
        7 => InstanceCommandPayload::ActorMaximumHealth(
            allocation.u16(payload_offset, "instance actor maximum health")?,
        ),
        8 => InstanceCommandPayload::ActorCurrentHealth(
            allocation.u16(payload_offset, "instance actor current health")?,
        ),
        9 => InstanceCommandPayload::ActorWord7a(
            allocation.u16(payload_offset, "instance actor word +7A")?,
        ),
        10 => InstanceCommandPayload::ActorShiftedByte7c(
            allocation.i32(payload_offset, "instance actor shifted byte +7C")?,
        ),
        11 => InstanceCommandPayload::ActorShiftedByte7d(
            allocation.i32(payload_offset, "instance actor shifted byte +7D")?,
        ),
        12 => InstanceCommandPayload::ActorStateC4(
            allocation.i32(payload_offset, "instance actor state +C4")?,
        ),
        13 | 14 => {
            let word = match allocation.u16(payload_offset, "conditional actor bit index") {
                Ok(field) => ConditionalInstanceWord::Available(field),
                Err(InstanceAssetError::OutOfBounds { .. }) => {
                    ConditionalInstanceWord::OutsideAllocation {
                        allocation_offset: payload_offset,
                    }
                }
                Err(error) => return Err(error),
            };
            if code.value == 13 {
                InstanceCommandPayload::ActorDisableUnlessBit(word)
            } else {
                InstanceCommandPayload::ActorDisableIfBit(word)
            }
        }
        15 => InstanceCommandPayload::ActorWord76(
            allocation.u16(payload_offset, "instance actor word +76")?,
        ),
        _ => return Ok(view),
    };
    view.payload_consumer = Some(0x0047_ada0);
    Ok(view)
}

#[derive(Clone, Copy)]
enum Family {
    Base,
    Visual,
    Actor,
}

// Bounded set of original vtable +4 profiles, checked against the retail image.
// This is intentionally separate from +08/+0C constructor initialization.
fn command_profile(target: InstanceCommandTarget) -> Option<(u32, Family)> {
    let selector = match target {
        InstanceCommandTarget::Constructor(selector) => selector,
        InstanceCommandTarget::SharedActor => return Some((0x0047_ada0, Family::Actor)),
        InstanceCommandTarget::DerivedActor => return Some((0x0042_6030, Family::Actor)),
        InstanceCommandTarget::Family428650 => return Some((0x0042_76b0, Family::Actor)),
        InstanceCommandTarget::Family4236d0 => return Some((0x0042_39e0, Family::Actor)),
    };
    let result = match selector {
        1 | 67 | 75 => (0x0047_9e70, Family::Base),
        22 | 58 | 63 | 65 | 70 | 76 | 83 => (0x0047_a1a0, Family::Visual),
        3 | 4 | 5 | 6 | 20 | 24 | 25 | 26 | 27 | 28 | 31 | 32 | 38 | 40 | 41 | 44 | 56 | 57
        | 61 | 62 | 66 | 68 | 73 | 85 | 90 | 93 => (0x0047_ada0, Family::Actor),
        2 | 7 | 33 | 77 | 80 => (0x0042_6030, Family::Actor),
        // 004239E0/004672C0 -> 004276B0 -> 00426030 -> 0047ADA0 for codes <=15.
        8 | 9 | 36 => (0x0042_39e0, Family::Actor),
        12 => (0x0046_72c0, Family::Actor),
        23 => (0x0040_d9c0, Family::Actor),
        10 | 21 => (0x0042_5240, Family::Actor),
        11 => (0x0040_2330, Family::Actor),
        13 => (0x0046_89d0, Family::Actor),
        14 | 89 => (0x0046_d780, Family::Visual),
        15 => (0x0042_f100, Family::Actor),
        16 => (0x0040_f7d0, Family::Visual),
        17 => (0x0046_e4f0, Family::Actor),
        18 => (0x0047_8860, Family::Actor),
        19 => (0x0044_8120, Family::Visual),
        29 => (0x0045_8c40, Family::Actor),
        30 => (0x0048_bbf0, Family::Actor),
        34 => (0x0043_3c90, Family::Actor),
        35 => (0x0043_d270, Family::Actor),
        37 => (0x0040_a1d0, Family::Actor),
        39 => (0x0043_b7d0, Family::Actor),
        42 => (0x0040_1080, Family::Visual),
        43 => (0x0045_4220, Family::Visual),
        45 => (0x0045_c010, Family::Visual),
        46 => (0x0047_5cb0, Family::Visual),
        47 => (0x0047_fdc0, Family::Actor),
        48 => (0x0043_c840, Family::Visual),
        49 => (0x0047_63f0, Family::Actor),
        50 => (0x0043_ff40, Family::Visual),
        51 => (0x0040_4cc0, Family::Visual),
        52 => (0x0045_4880, Family::Actor),
        53 => (0x0045_4a30, Family::Actor),
        54 => (0x0046_4940, Family::Visual),
        55 => (0x0047_3f70, Family::Actor),
        59 | 82 | 91 => (0x0040_5d10, Family::Actor),
        60 => (0x0048_0b30, Family::Actor),
        64 => (0x0044_07e0, Family::Visual),
        69 => (0x0041_1100, Family::Actor),
        71 => (0x0046_f810, Family::Visual),
        72 => (0x0046_fa70, Family::Visual),
        74 => (0x0042_d6f0, Family::Actor),
        78 => (0x0040_6800, Family::Actor),
        79 => (0x0040_8880, Family::Actor),
        81 => (0x0040_9320, Family::Base),
        84 => (0x0043_c2a0, Family::Actor),
        86 => (0x0042_def0, Family::Base),
        87 => (0x0048_c7a0, Family::Base),
        88 => (0x0044_3900, Family::Actor),
        92 => (0x0040_4420, Family::Visual),
        94 => (0x0044_5900, Family::Base),
        _ => return None,
    };
    Some(result)
}

fn has_426030_path(target: InstanceCommandTarget) -> bool {
    matches!(
        target,
        InstanceCommandTarget::DerivedActor
            | InstanceCommandTarget::Family428650
            | InstanceCommandTarget::Family4236d0
            | InstanceCommandTarget::Constructor(
                2 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 21 | 33 | 34 | 35 | 36 | 60 | 77 | 79 | 80
            )
    )
}

fn signed_word(
    allocation: &InstanceAllocation<'_>,
    offset: u32,
    context: &'static str,
) -> Result<InstanceField<i16>, InstanceAssetError> {
    let field = allocation.u16(offset, context)?;
    Ok(InstanceField {
        span: field.span,
        value: field.value as i16,
    })
}

fn placement_spawn_list(
    request: &InstanceCommandRequest<'_>,
    allocation: &InstanceAllocation<'_>,
    offset: u32,
) -> Result<PlacementSpawnList, InstanceAssetError> {
    let count = allocation.i32(add(offset, 4, "spawn list count")?, "spawn list count")?;
    let mut list = PlacementSpawnList {
        header_span: allocation.range(offset, 8, "spawn list header")?,
        mode_word: signed_word(allocation, offset, "spawn list mode")?,
        count,
        rows_pointer: None,
        rows_span: None,
        rows: Vec::new(),
    };
    if count.value <= 0 {
        return Ok(list);
    }
    let pointer_offset = add(offset, 8, "spawn list pointer")?;
    let pointer = allocation.u32(pointer_offset, "spawn list pointer")?;
    if !request.relocations.iter().any(|r| {
        r.field_offset == pointer_offset
            && r.field_span == pointer.span
            && r.target_offset == pointer.value
    }) {
        return Err(InstanceAssetError::MissingCommandRelocation(pointer_offset));
    }
    let length = (count.value as usize)
        .checked_mul(0x18)
        .ok_or(InstanceAssetError::IntegerOverflow("spawn list rows"))?;
    let span = allocation.range(pointer.value, length, "spawn list rows")?;
    list.header_span = allocation.range(offset, 12, "spawn list header")?;
    list.rows_pointer = Some(pointer);
    list.rows_span = Some(span);
    list.rows = Vec::with_capacity(count.value as usize);
    for index in 0..count.value as usize {
        let row = add(pointer.value, index * 0x18, "spawn list row")?;
        let word = |relative| {
            signed_word(
                allocation,
                add(row, relative, "spawn row signed WORD")?,
                "spawn row signed WORD",
            )
        };
        let flag_span =
            allocation.range(add(row, 0x10, "spawn row flags")?, 1, "spawn row flags")?;
        list.rows.push(PlacementSpawnRow {
            index: index as u32,
            span: allocation.range(row, 0x18, "spawn list row")?,
            placement_reference: allocation.i32(row, "spawn row placement reference")?,
            signed_words_04_to_0e: [
                word(4)?,
                word(6)?,
                word(8)?,
                word(10)?,
                word(12)?,
                word(14)?,
            ],
            flags: InstanceField {
                span: flag_span,
                value: request.source[flag_span.start],
            },
            runtime_pointer_placeholder: allocation.u32(
                add(row, 0x14, "spawn row runtime placeholder")?,
                "spawn row runtime placeholder",
            )?,
        });
    }
    Ok(list)
}

#[cfg(test)]
#[path = "instance_command_views_tests.rs"]
mod tests;
