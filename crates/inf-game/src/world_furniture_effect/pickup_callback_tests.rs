use super::*;

fn choose(byte: u8, owns: Option<bool>, mut rng: u32) -> (Result<(u16, u16), String>, u32) {
    let result = furniture_pickup_choice::<String>(byte, |_| Ok(owns), &mut rng)
        .map_err(|error| error.to_string());
    (result, rng)
}

/// Every byte the table names, without a player.
#[test]
fn each_furniture_byte_names_its_pickup() {
    let expected = [
        (0x20, (0x0e, 1)),
        (0x21, (0x0f, 1)),
        (0x22, (0x10, 1)),
        (0x23, (0x25, 4)),
        (0x24, (0xaa, 4)),
        (0x25, (0xa8, 4)),
        (0x26, (0x1c, 4)),
        (0x27, (0x1d, 4)),
        (0x28, (0xab, 4)),
        (0x29, (4, 1000)),
    ];
    for (byte, pickup) in expected {
        assert_eq!(choose(byte, None, 7), (Ok(pickup), 7), "byte {byte:#x}");
    }
    assert!(choose(0x2a, None, 7).0.unwrap_err().contains("00437A29"));
    assert!(choose(0x1f, None, 7).0.unwrap_err().contains("00437A29"));
}

/// The three recorded drops: Infest02's two crates (byte 0x23) gave the
/// standard grenade to a player without 0xA9, drawing nothing; Infest03's
/// player owned 0xA9 and the draw from 634D6F0A set bit 8.
#[test]
fn the_grenade_crate_upgrades_on_a_draw_when_the_player_owns_the_power_grenade() {
    assert_eq!(choose(0x23, Some(false), 0x1234), (Ok((0x25, 4)), 0x1234));
    assert_eq!(
        choose(0x23, Some(true), 0x634d_6f0a),
        (Ok((0xa9, 4)), 0xf32d_6eb5)
    );
    // A draw without bit 8 keeps the basic grenade.
    let (result, rng) = choose(0x23, Some(true), 0);
    assert_eq!(result, Ok((0x25, 4)));
    assert_eq!(rng, 0x0013_d573);
    // The 0x25 crate does the same with the chain grenade.
    assert_eq!(
        choose(0x25, Some(true), 0x634d_6f0a),
        (Ok((0xad, 4)), 0xf32d_6eb5)
    );
    assert_eq!(choose(0x25, Some(false), 9), (Ok((0xa8, 4)), 9));
}
