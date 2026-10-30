//! Furniture callback 00437880: a destroyed furniture's pickup. The cached
//! H11 entry in EDX picks it by its byte +2 (the low byte of the entry's
//! second WORD); a null entry drops nothing. The pickup is born (0046D870)
//! at the burst's live stack XYZ, and AL is one either way.

use super::FurnitureCallbackContext;
use crate::terrain_runtime::TerrainRuntime;

/// Reached-callback loans; the queue's saved-next traversal keeps its owner.
pub struct FurniturePickupCallbackInputs<'a> {
    pub context: &'a mut FurnitureCallbackContext,
    pub terrain: &'a TerrainRuntime,
    pub rng: &'a mut u32,
}

pub trait FurniturePickupCallbackHost {
    type Error;
    /// 00488AA0's first player and 004752E0 on its grant owner (+9C): whether
    /// it owns `id`, or None without a player.
    fn first_player_owns(&mut self, id: u16) -> Result<Option<bool>, Self::Error>;
    /// 0046D870 with the stack XYZ, which it may move.
    fn birth_pickup(
        &mut self,
        kind: u16,
        value: u16,
        inputs: FurniturePickupCallbackInputs<'_>,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum FurniturePickupError<E> {
    /// 00437A29: any other byte drops a pickup whose kind is the stack XYZ
    /// pointer itself.
    #[error("furniture pickup byte {0:#04x} names the stack XYZ pointer as its kind (00437A29)")]
    PointerKind(u8),
    #[error(transparent)]
    Host(E),
}

/// The pickup 004378A0's table chooses for byte +2 (less 0x20).
pub fn furniture_pickup_choice<E>(
    byte: u8,
    owns: impl FnOnce(u16) -> Result<Option<bool>, E>,
    rng: &mut u32,
) -> Result<(u16, u16), FurniturePickupError<E>> {
    // 004378FE/0043796D: the first player's upgrade replaces the basic kind
    // when it owns it and one draw has bit 8 set.
    let upgraded = |basic: u16, upgrade: u16| -> Result<u16, FurniturePickupError<E>> {
        Ok(match owns(upgrade).map_err(FurniturePickupError::Host)? {
            Some(true) if crate::name_entry::name_entry_rng_next(rng) & 0x100 != 0 => upgrade,
            _ => basic,
        })
    };
    Ok(match byte {
        0x20 => (0x0e, 1),
        0x21 => (0x0f, 1),
        0x22 => (0x10, 1),
        0x23 => (upgraded(0x25, 0xa9)?, 4),
        0x24 => (0xaa, 4),
        0x25 => (upgraded(0xa8, 0xad)?, 4),
        0x26 => (0x1c, 4),
        0x27 => (0x1d, 4),
        0x28 => (0xab, 4),
        0x29 => (4, 1000),
        _ => return Err(FurniturePickupError::PointerKind(byte)),
    })
}

/// Complete 00437880 for the cached entry's byte +2, or nothing for a null
/// entry.
pub fn run_furniture_pickup_callback<H: FurniturePickupCallbackHost + ?Sized>(
    byte: Option<u8>,
    host: &mut H,
    inputs: FurniturePickupCallbackInputs<'_>,
) -> Result<bool, FurniturePickupError<H::Error>> {
    let Some(byte) = byte else {
        return Ok(true);
    };
    let (kind, value) = furniture_pickup_choice(byte, |id| host.first_player_owns(id), inputs.rng)?;
    host.birth_pickup(kind, value, inputs)
        .map_err(FurniturePickupError::Host)?;
    Ok(true)
}

#[cfg(test)]
#[path = "pickup_callback_tests.rs"]
mod tests;
