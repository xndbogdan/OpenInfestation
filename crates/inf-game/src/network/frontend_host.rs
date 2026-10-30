//! The game protocol's host where no world is loaded: the join wait
//! (`0045FF80`) runs `0045C940` and the drain there. The mission's `+12` is
//! 0 in the frontend (its constructor and the leave's `0048E890` clear it),
//! so `0045C3D0` never lets a frame past its header: only the header's
//! states, the host's settings and its world reach the game. The frontend's
//! resident levels hold no terrain, so a changed detail is refused there
//! (`0045C506`), as retail's H12 loader refuses it.

use super::game::GameHeaderHost;
use super::game_tick::{ActorSendTimer, GameFrameHost};
use super::link::NetworkLink;
use super::protocol::{GameProtocol, GameSend, GameWorldHost};
use super::receivers::{GameCall, GameMessageHost};
use crate::frontend_settings::{FrontendSettings, SidecarIdentityStore};
use crate::player_identity::{IdentityRead, PlayerIdentity};

/// What a protocol pass borrows from the platform.
pub struct NetworkPlatform<'a> {
    pub link: &'a mut NetworkLink,
    pub settings: &'a mut FrontendSettings,
    pub identity: &'a mut PlayerIdentity,
}

/// `strncpy` into `N` bytes, then the last byte cleared.
fn text<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut out = [0; N];
    let length = bytes.len().min(N);
    out[..length].copy_from_slice(&bytes[..length]);
    out[N - 1] = 0;
    out
}

impl NetworkPlatform<'_> {
    /// `00463D10` into the 16-byte name and 6-byte initials `0045C940`
    /// passes, for the local slot's `%dup` initials.
    pub fn net_identity(&mut self) -> ([u8; 16], [u8; 6]) {
        let slot = u32::from(self.link.transport.state.local_slot);
        let loaded = self
            .identity
            .read(
                IdentityRead::NetPlayerName,
                &mut SidecarIdentityStore(self.settings),
                slot,
            )
            .unwrap_or_else(|never| match never {});
        (text(&loaded.name), text(&loaded.initials))
    }

    /// `0045FF80`'s receive: with `clear_world` the host's world word
    /// `0054E814` first, then `0045C940` and `00492830`. Returns this
    /// machine's slot and the host's world.
    pub fn wait_receive(&mut self, clear_world: bool) -> Result<(u8, i32), String> {
        let identity = self.net_identity();
        let link = &mut *self.link;
        if clear_world {
            link.game.host_world = 0;
        }
        let mut host = FrontendGameHost {
            settings: self.settings.words_mut(),
            identity,
        };
        let mut protocol = GameProtocol::new(&mut link.game, &mut host);
        protocol
            .receive_slots(&link.transport.state)
            .map_err(|error| error.to_string())?;
        protocol
            .drain(&mut link.transport, &mut link.table)
            .map_err(|error| error.to_string())?;
        Ok((link.transport.state.local_slot, link.game.host_world))
    }

    /// `0045CB70` with the settings' game mode `+10`, and its line groups
    /// from `lines`.
    pub fn reset_game(&mut self, lines: &super::game_tick::LineGroupSource) {
        let mode = self.settings.words()[0x10 / 4];
        let link = &mut *self.link;
        link.game.reset_game(mode, &mut link.frame);
        link.frame.load_line_groups(|kind| lines.load(kind));
    }
}

/// The frontend's world: settings, identity and nothing else.
pub struct FrontendGameHost<'a> {
    pub settings: &'a mut [u32],
    pub identity: ([u8; 16], [u8; 6]),
}

const NO_WORLD: &str = "the frontend has no world for the game protocol";

impl GameMessageHost for FrontendGameHost<'_> {
    type Error = String;
    fn call(&mut self, call: GameCall, _: &mut super::game::GameNetState) -> Result<u32, String> {
        Err(format!("{NO_WORLD}: {call:?}"))
    }
    fn flags(&mut self, _: u32) -> Result<u32, String> {
        Err(NO_WORLD.into())
    }
    fn word_70(&mut self, _: u32) -> Result<u32, String> {
        Err(NO_WORLD.into())
    }
    fn handle_84(&mut self, _: u32) -> Result<u32, String> {
        Err(NO_WORLD.into())
    }
}

impl GameHeaderHost for FrontendGameHost<'_> {
    type Error = String;
    fn settings_word(&mut self, offset: usize) -> u32 {
        self.settings[offset / 4]
    }
    fn set_settings_word(&mut self, offset: usize, value: u32) {
        self.settings[offset / 4] = value;
    }
    /// `+12` is 0 here; the world is never compared.
    fn mission(&self) -> (i32, u8) {
        (0, 0)
    }
    fn rebuild_terrain(&mut self, _: u32) -> Result<bool, String> {
        Ok(false)
    }
}

impl GameWorldHost for FrontendGameHost<'_> {
    fn null_handle(&self) -> u32 {
        0
    }
    fn encode_actor(&mut self, _: u32, _: &mut [u8]) -> Result<i32, String> {
        Err(NO_WORLD.into())
    }
    fn receive_actor(&mut self, _: &[u8], _: &mut Vec<(u32, GameSend)>) -> Result<i32, String> {
        Err(NO_WORLD.into())
    }
    fn world_values(&mut self) -> Result<[u32; 4], String> {
        Err(NO_WORLD.into())
    }
    /// `0055EF10` is empty without a world.
    fn territory(&mut self) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
    /// The frontend has no race (`0054ED70` is null): 0046F410 writes
    /// nothing, and 0046F5E0 would write through the null.
    fn encode_race(&mut self, _: &mut [u8]) -> Result<i32, String> {
        Ok(0)
    }
    fn receive_race(&mut self, _: &[u8]) -> Result<i32, String> {
        Err(NO_WORLD.into())
    }
    fn receive_ball(&mut self, _: u32, _: &[u8]) -> Result<(), String> {
        Err(NO_WORLD.into())
    }
    /// Over an empty list they change nothing; 41's line has no first
    /// player.
    fn receive_territory(
        &mut self,
        _: u32,
        _: &[u8],
        _: &mut crate::network::game::GameNetState,
        _: &mut Vec<(u32, GameSend)>,
    ) -> Result<(), String> {
        Ok(())
    }
}

impl GameFrameHost for FrontendGameHost<'_> {
    /// The tables exist and hold nothing without a world.
    fn slot_handles(&mut self, slot: u8) -> Result<Option<Vec<u32>>, String> {
        Ok((usize::from(slot) < super::game::SLOTS).then(Vec::new))
    }
    fn send_timer(&mut self, _: u32) -> Result<ActorSendTimer, String> {
        Err(NO_WORLD.into())
    }
    fn set_send_timer(&mut self, _: u32, _: u32, _: u32) -> Result<(), String> {
        Err(NO_WORLD.into())
    }
    fn deadline(&mut self, _: u32) -> Result<i32, String> {
        Err(NO_WORLD.into())
    }
    fn expire(&mut self, _: u32) -> Result<bool, String> {
        Err(NO_WORLD.into())
    }
    fn race_actor(&mut self) -> bool {
        false
    }
    fn random(&mut self) -> Result<u32, String> {
        Err(NO_WORLD.into())
    }
    fn clock(&self) -> u32 {
        0
    }
    fn identity(&mut self) -> Result<([u8; 16], [u8; 6]), String> {
        Ok(self.identity)
    }
}
