//! Scalar inputs of 004276B0/00433C90/0043D270, 0040D9C0/0048BBF0 and the
//! 0040D800 family's 0040A1D0/0043B7D0. Keeping these separate preserves each
//! wrapper's boundary above the shared actor inputs.

use super::*;

pub(super) fn decode(
    request: &InstanceCommandRequest<'_>,
    allocation: &InstanceAllocation<'_>,
    code: i32,
    offset: u32,
) -> Result<Option<(u32, InstanceCommandPayload)>, InstanceAssetError> {
    use InstanceCommandPayload::*;
    if matches!(request.target, InstanceCommandTarget::Constructor(10 | 21)) && code == 25 {
        return Ok(Some((0x0042_5240, NoPayloadRead)));
    }
    let family4236d0 = matches!(
        request.target,
        InstanceCommandTarget::Family4236d0
            | InstanceCommandTarget::Constructor(8 | 9 | 10 | 21 | 36)
    );
    let family428650 = matches!(
        request.target,
        InstanceCommandTarget::Family428650 | InstanceCommandTarget::Constructor(12 | 13 | 34 | 35)
    ) || family4236d0;
    if family428650 && (19..=22).contains(&code) {
        let value = match code {
            19 => NoPayloadRead,
            20 => FamilyHomeTerrainXZ(words(allocation, offset, "family terrain XZ")?),
            21 => FamilyHomeTerrainXYZ(words(allocation, offset, "family terrain XYZ")?),
            22 => FamilyPlacementReference(allocation.u32(offset, "family placement reference")?),
            _ => unreachable!(),
        };
        return Ok(Some((0x0042_76b0, value)));
    }
    if family4236d0 && matches!(code, 23 | 24) {
        let value = if code == 23 {
            NoPayloadRead
        } else {
            RetainedPayloadPointer {
                allocation_offset: offset,
            }
        };
        return Ok(Some((0x0042_39e0, value)));
    }
    if matches!(request.target, InstanceCommandTarget::Constructor(12 | 13)) && code == 23 {
        return Ok(Some((
            0x0046_72c0,
            RetainedPayloadPointer {
                allocation_offset: offset,
            },
        )));
    }
    if request.target == InstanceCommandTarget::Constructor(34) && code == 23 {
        return Ok(Some((
            0x0043_3c90,
            RetainedPayloadPointer {
                allocation_offset: offset,
            },
        )));
    }
    if request.target == InstanceCommandTarget::Constructor(35) && matches!(code, 23 | 24) {
        let value = if code == 23 {
            RetainedPayloadPointer {
                allocation_offset: offset,
            }
        } else {
            Selector35Packed240(allocation.u32(offset, "selector35 packed240")?)
        };
        return Ok(Some((0x0043_d270, value)));
    }
    let byte = |context| {
        let span = allocation.range(offset, 1, context)?;
        Ok::<_, InstanceAssetError>(InstanceField {
            span,
            value: request.source[span.start],
        })
    };
    // 00454A30 passes codes through 15 to 0047ADA0 and refuses codes above
    // 19 without a read.
    if request.target == InstanceCommandTarget::Constructor(53) && (16..=19).contains(&code) {
        let value = match code {
            16 => Selector53Kind138(byte("selector53 kind138")?),
            17 => Selector53Count139(byte("selector53 count139")?),
            18 => Selector53Seconds13a(byte("selector53 seconds13A")?),
            19 => Selector53Repeat13b(byte("selector53 repeat13B")?),
            _ => unreachable!(),
        };
        return Ok(Some((0x0045_4a30, value)));
    }
    // 00458C40, the Bomb's +04, passes codes through 15 to 0047ADA0 and
    // refuses codes above 16 without a read.
    if request.target == InstanceCommandTarget::Constructor(29) && code == 16 {
        return Ok(Some((
            0x0045_8c40,
            Selector29Fuse(allocation.u32(offset, "selector29 fuse")?),
        )));
    }
    // 0042D6F0, the zone's +04, passes codes through 15 (signed) to 0047ADA0
    // and reads the operand's WORD for every other code before it switches.
    if request.target == InstanceCommandTarget::Constructor(74) && code > 15 {
        let value = allocation.u16(offset, "selector74 extent")?;
        let value = match code {
            16 => Selector74Width(value),
            17 => Selector74Depth(value),
            18 => Selector74Height(value),
            _ => Selector74Refused(value),
        };
        return Ok(Some((0x0042_d6f0, value)));
    }
    // 0043C2A0, the capture node's +04, passes codes through 15 to 0047ADA0
    // and refuses codes above 16 without a read.
    if request.target == InstanceCommandTarget::Constructor(84) && code == 16 {
        return Ok(Some((
            0x0043_c2a0,
            Selector84Link(allocation.u32(offset, "selector84 link")?),
        )));
    }
    // 00408880, the eye's +04, passes codes through 18 to selector 77's
    // 00426030 and refuses codes above 19 without a read.
    if request.target == InstanceCommandTarget::Constructor(79) && code == 19 {
        return Ok(Some((
            0x0040_8880,
            Selector79Link(allocation.u32(offset, "selector79 link")?),
        )));
    }
    // 00454880 passes codes through 15 to 0047ADA0 and refuses codes above
    // 16 without a read.
    if request.target == InstanceCommandTarget::Constructor(52) && code == 16 {
        return Ok(Some((
            0x0045_4880,
            Selector52Stopped(byte("selector52 stopped")?),
        )));
    }
    // 00405D10, the Boulders' +04, passes codes through 15 to 0047ADA0 and
    // refuses codes above 19 without a read.
    if request.target == InstanceCommandTarget::Constructor(59) && (16..=19).contains(&code) {
        let value = match code {
            16 => Selector59Wake(byte("selector59 wake")?),
            17 => Selector59Crushes144(byte("selector59 crushes144")?),
            18 => Selector59CrushesAny145(byte("selector59 crushes any145")?),
            19 => Selector59Smokes146(byte("selector59 smokes146")?),
            _ => unreachable!(),
        };
        return Ok(Some((0x0040_5d10, value)));
    }
    // 00478860, the Force Field's +04, passes codes through 15 to 0047ADA0
    // and refuses codes above 0x14 without a read.
    if request.target == InstanceCommandTarget::Constructor(18) && (0x10..=0x14).contains(&code) {
        let value = match code {
            0x10 => Selector18Radius(allocation.u16(offset, "selector18 radius")?),
            0x11 => Selector18Height(allocation.u16(offset, "selector18 height")?),
            0x12 => Selector18KeyGrant(allocation.u16(offset, "selector18 key grant")?),
            0x13 => Selector18TeamPasses(byte("selector18 team passes")?),
            0x14 => Selector18Hint(allocation.u32(offset, "selector18 hint")?),
            _ => unreachable!(),
        };
        return Ok(Some((0x0047_8860, value)));
    }
    // 0042F100, 0040A1D0 and 0043B7D0 pass codes below 21 to 0040D9C0, like
    // 0048BBF0.
    let family40d800 = matches!(
        request.target,
        InstanceCommandTarget::Constructor(15 | 23 | 30 | 37 | 39 | 47 | 49)
    );
    if family40d800 && (16..=20).contains(&code) {
        let value = match code {
            16 => {
                let raw = byte("0040D9C0 signed maximum BYTE")?;
                Actor40d800Maximum(InstanceField {
                    span: raw.span,
                    value: raw.value as i8,
                })
            }
            17 => Actor40d800Flag14e(byte("0040D9C0 flag14E BYTE")?),
            18 => Actor40d800Refill(byte("0040D9C0 refill BYTE")?),
            19 => Actor40d800Word144(allocation.u16(offset, "0040D9C0 word144")?),
            20 => Actor40d800Words146(words(allocation, offset, "0040D9C0 words146")?),
            _ => unreachable!(),
        };
        return Ok(Some((0x0040_d9c0, value)));
    }
    // 0047FDC0, the Siren's +04: code 21 accepts without a read, 22 copies a
    // BYTE and 23 a signed WORD; higher codes refuse without a read.
    if request.target == InstanceCommandTarget::Constructor(47) && (21..=23).contains(&code) {
        let value = match code {
            21 => NoPayloadRead,
            22 => Selector47Seconds16c(byte("selector47 seconds16C")?),
            23 => {
                let field = allocation.u16(offset, "selector47 radius164")?;
                Selector47Radius164(InstanceField {
                    span: field.span,
                    value: field.value as i16,
                })
            }
            _ => unreachable!(),
        };
        return Ok(Some((0x0047_fdc0, value)));
    }
    if request.target == InstanceCommandTarget::Constructor(15) && (21..=25).contains(&code) {
        let value = match code {
            21 => NoPayloadRead,
            22 => Selector15Dword15c(allocation.u32(offset, "selector15 dword15C")?),
            23..=25 => Selector15GrantWord(allocation.u16(offset, "selector15 grant WORD")?),
            _ => unreachable!(),
        };
        return Ok(Some((0x0042_f100, value)));
    }
    if request.target == InstanceCommandTarget::Constructor(37) && (21..=28).contains(&code) {
        let value = match code {
            21 => Selector37Word158(allocation.u16(offset, "selector37 word158")?),
            22 => Selector37Word15c(allocation.u16(offset, "selector37 word15C")?),
            23 => {
                let raw = allocation.u16(offset, "selector37 signed word160")?;
                Selector37SignedWord160(InstanceField {
                    span: raw.span,
                    value: raw.value as i16,
                })
            }
            24 => Selector37Seconds164(byte("selector37 seconds164 BYTE")?),
            25 => Selector37Flag166(byte("selector37 flag166 BYTE")?),
            26 => Selector37Word17c(allocation.u16(offset, "selector37 word17C")?),
            27 => Selector37Byte165(byte("selector37 byte165")?),
            28 => Selector37Word17e(allocation.u16(offset, "selector37 word17E")?),
            _ => unreachable!(),
        };
        return Ok(Some((0x0040_a1d0, value)));
    }
    // 004763F0, the Seismic Stabiliser's, takes WORDs for 21 and 22 and
    // DWORDs for 23 and 24; its default returns zero.
    if request.target == InstanceCommandTarget::Constructor(49) && (21..=24).contains(&code) {
        let value = match code {
            21 => Selector49Word158(allocation.u16(offset, "selector49 word158")?),
            22 => Selector49Word15a(allocation.u16(offset, "selector49 word15A")?),
            23 => Selector49Seconds15c(allocation.u32(offset, "selector49 seconds15C")?),
            24 => Selector49Reference16c(allocation.u32(offset, "selector49 reference16C")?),
            _ => unreachable!(),
        };
        return Ok(Some((0x0047_63f0, value)));
    }
    // 0043B7D0 has no case 26; its default returns zero.
    if request.target == InstanceCommandTarget::Constructor(39) && matches!(code, 21..=25 | 27 | 28)
    {
        let value = match code {
            21 => RetainedPayloadPointer {
                allocation_offset: offset,
            },
            22 => Selector39Dword15c(allocation.u32(offset, "selector39 dword15C")?),
            24 => Selector39Byte160(byte("selector39 byte160")?),
            23 | 25 | 27 | 28 => Selector39Flag(byte("selector39 flag BYTE")?),
            _ => unreachable!(),
        };
        return Ok(Some((0x0043_b7d0, value)));
    }
    if request.target != InstanceCommandTarget::Constructor(30) || !(21..=32).contains(&code) {
        return Ok(None);
    }
    let value = match code {
        21 => Selector30Placement180(allocation.u16(offset, "selector30 placement180")?),
        22 => Selector30Byte15c(byte("selector30 byte15C")?),
        23 => Selector30Byte160(byte("selector30 byte160")?),
        24 => Selector30Byte164(byte("selector30 byte164")?),
        25 => Selector30Byte18f(byte("selector30 byte18F")?),
        26 => Selector30Byte167(byte("selector30 byte167")?),
        27 => Selector30Flag18e(byte("selector30 flag18E")?),
        28 => Selector30Flag166(byte("selector30 flag166")?),
        29 => Selector30Flag190(byte("selector30 flag190")?),
        30 => Selector30Flag191(byte("selector30 flag191")?),
        31 => Selector30Flag192(byte("selector30 flag192")?),
        32 => Selector30Word18c(allocation.u16(offset, "selector30 word18C")?),
        _ => unreachable!(),
    };
    Ok(Some((0x0048_bbf0, value)))
}

fn words<const N: usize>(
    allocation: &InstanceAllocation<'_>,
    offset: u32,
    context: &'static str,
) -> Result<InstanceField<[u16; N]>, InstanceAssetError> {
    let span = allocation.range(offset, N * 2, context)?;
    let mut value = [0; N];
    for (index, word) in value.iter_mut().enumerate() {
        *word = allocation
            .u16(add(offset, index * 2, context)?, context)?
            .value;
    }
    Ok(InstanceField { span, value })
}

#[cfg(test)]
#[path = "instance_command_views_extended_actors_tests.rs"]
mod tests;
