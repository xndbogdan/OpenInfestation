//! Live world players and the camera attachment chain.
//!
//! 00469D60 constructs a 0x1A0-byte player and appends its +C8 node to the
//! manager list M+58 (G+60); 00487410 does so during offline preparation, and
//! world cleanup 00478CD0 does not drain that list. This owner retains the
//! bounded attachment/save fields, the embedded +9C grant mask and the three
//! +130 command words. The separate message owner retains its live suppression
//! byte; explicit command bodies do not attach enclosing callers. Spawning
//! (0046A890), input and enabled HUD state remain separate owners.
//!
//! 00488AA0 returns the first list payload. 0046AE60 resolves +D8, falling
//! back to +D4. 00478E00 returns the camera named by M+74, constructing it on
//! demand. 0046AE90 attaches a player to an actor and retargets the camera.

mod commands;
mod constructor;
mod controls;
mod hud;
mod input;
mod keys;
mod message;
mod message_widgets;
mod shown_menu;
mod vehicle_snapshot;

pub use constructor::{
    PlayerConstructionError, PlayerConstructionPhase, PlayerDestructionError,
    PlayerDestructionPhase, WorldPlayerConstruction,
};
pub use input::{
    PlayerInputDescriptor, PlayerInputDescriptorKind, PlayerInputDescriptorRequest,
    PlayerInputDescriptorSource, PlayerInputError, PlayerInputReleaseError,
};

pub use commands::{
    PlayerCommandContext, PlayerCommandError, PlayerCommandOperation, PlayerCommandOutcome,
    PlayerCommandRequest,
};
#[cfg(test)]
pub(crate) use controls::player_analog_fixture;
pub use controls::{
    PlayerControlBlock, PlayerControlsUpdate, PlayerInputConsumers, PlayerNetworkState,
    PlayerUpdateError, PlayerUpdateInputs, PlayerVehicleCall,
};
pub use keys::{
    is_player_key_callback, PlayerKeyError, PlayerKeyEvent, PlayerKeyInputs, PlayerKeyOmission,
};

pub use hud::{
    FreeLookPick, FreeLookSource, HudReticleSettings, MissileViewSource, PlayerHudError,
    PlayerHudUnit,
};
#[cfg(test)]
pub(crate) use message::full_notice_gate;
pub use message::{
    PickupDelivery, PickupDeliveryError, PickupMessageError, PlayerMessage88, PlayerMessageDraw,
    PlayerMessageError, PlayerMessageOperation, PlayerMessageRequest, PlayerMessageState,
};
pub use message_widgets::{
    ComputerCheats, DatabaseEntry, MessageBox, MessageBoxKind, PlayerMessageBoxes,
    PlayerMessageEffect, PlayerMessageEnvironment, PlayerMessageMenus, DATABASE_ENTRIES,
    HINT_COUNT,
};
pub use shown_menu::{ShownMenu, ShownMenuAction, ShownMenuOwner, DATABASE_SELECT};

use crate::object_runtime::FollowTargetRequest;
use inf_formats::{native_basis::NativeBasisMath, GRANT_MASK_WORDS};
use thiserror::Error;

use super::{
    construct_world_object, ActorConstructionRequest, ActorExtension, ActorFactoryInputs,
    ActorHandle, BaseContext, CameraSettings, CreationListenerEffects, CreationListeners,
    ObjectArena, ObjectConstructionError, ObjectConstructionOutcome, ObjectConstructionRequest,
    ObjectError, ObjectId,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::ResidentRuntime;

/// Stable identity of a live player in this arena's M+58 list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerId(u64);

impl PlayerId {
    /// An identity no arena issued, for tests outside the runtime.
    #[cfg(test)]
    pub(crate) const fn unissued(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerActorLookupRequest {
    pub handle: ActorHandle,
}

/// The actual process owner borrowed at the reached lookup. The registry's
/// constructor null handle is independent and never supplies this field.
#[derive(Clone, Copy, Debug)]
pub struct PlayerActorLookupContext<'a> {
    pub process: Option<&'a ParticleFactoryProcessState>,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PlayerActorLookupError {
    #[error("0046B096 requires the actual process NULL handle 0054E894")]
    MissingProcessNullHandle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldPlayer {
    pub id: PlayerId,
    /// Actual message subobject at +88. Only its identity and the proved
    /// constructor/suppression BYTE+1 are retained; +B8 pointer custody is
    /// separate from this owner and is not mirrored as a raw/semantic binding.
    message_88: PlayerMessageState,
    /// Mask at +A0 of the embedded 00475020 grant owner at +9C (vtable
    /// 00527818). 00469E6A/0041314C place it at message+14 = player+9C;
    /// its notification pointer at player+B8 names the message subobject
    /// at player+88, passed by 0041314B, rather than the player base.
    /// Newly granted IDs with record text can reach 004750C0 messages.
    pub grants_9c: [u32; GRANT_MASK_WORDS],
    /// 00469D60 initializes both to the global null handle.
    pub handle_d4: ActorHandle,
    pub handle_d8: ActorHandle,
    /// The docked factory's handle (0046C860): the process NULL from
    /// 00469E8B until a factory docks the player's vehicle.
    pub handle_dc: ActorHandle,
    /// WORD +11C, zeroed by 00469D60 and again by 0046A880.
    pub word_11c: u16,
    /// BYTE +58: constructor 00469D9D stores 1, before the +5C constructor;
    /// the later loader writer 0046A889 also stores 1.
    pub byte_58: Option<u8>,
    /// Six DWORDs at +100 zeroed by 00469D60, tested by 0046BDC0.
    pub bits_100: [u32; 6],
    /// Raw save-copy DWORD +118, zeroed at 00469ECE.
    pub dword_118: u32,
    /// Raw save-copy WORD +11E, zeroed at 00469EDB.
    pub word_11e: u16,
    /// Raw DWORDs +120/+124/+128, zeroed at 00469EE2/00469EE8/00469EEE.
    /// The fourth DWORD in the codec's +120 group is the separate +12C field.
    pub dwords_120_128: [u32; 3],
    /// 00469D60 constants: +12C = 0, +144 = 1, +148 = 0, +14C = 8, +ED/+EE = 0.
    pub dword_12c: u32,
    /// Command DWORDs +130/+134/+138, zeroed at 00469F97/00469F9E/00469FA4.
    /// These are independent of the six-word embedded grant mask.
    pub command_mask_130: [u32; 3],
    /// Eight raw bytes +13C..+143, zeroed by 00469F3C's EAX<8 arm.
    /// Save routines copy two little-endian DWORDs without string processing;
    /// this storage does not widen the three-word command mask's admission.
    /// Damage-reaction consumption of the first six signed bytes is separate.
    pub bytes_13c_143: [u8; 8],
    pub dword_144: u32,
    pub dword_148: u32,
    pub dword_14c: u32,
    /// Nonzero selects the saved spawn at +E0..+E5 and vehicle byte +EC.
    pub byte_ed: u8,
    pub byte_ee: u8,
    /// DWORD +F0, zeroed by 00469D60 and 0046AF70. The checkpoint writer
    /// 0046AF80 later stores the process clock here.
    pub dword_f0: u32,
    /// +E0..+E5 and +EC: the saved spawn's position and vehicle mode, which
    /// only the checkpoint writer 0046AF80 stores.
    pub saved_e0: Option<[u16; 3]>,
    pub byte_ec: Option<u8>,
    /// +E8: the saved spawn's packed angles (00491E80), which the
    /// constructor (00469EA4) zeroes.
    pub dword_e8: u32,
    /// +04..+47, zeroed by 00469F8E; 0046A890 stores +28/+2C/+34 before
    /// the vehicle spawn and 0046A260 refreshes the rest each tick.
    pub controls: PlayerControlBlock,
    /// +48/+4C: the Grenade double tap's clock and state (004694D0); the
    /// constructor stores -1000 and 0 at 00469D84/00469D8B.
    pub dword_48: Option<u32>,
    pub dword_4c: Option<u32>,
    /// The held message keys (0046D2E0), zeroed by the constructor.
    pub dword_50: Option<u32>,
    /// The message keys' repeat timer in milliseconds: 0046D2E0 stores 250
    /// and 0046A260 counts it down; no constructor writes it.
    pub dword_54: Option<i32>,
    /// Fire/Grenade lockout, the +12C charge timer and the paging byte,
    /// zeroed at 00469EBC/00469EC2/00469EC8.
    pub dword_f4: i32,
    pub dword_f8: i32,
    pub byte_fc: u8,
    /// +198, how long the race has seen this player's unit through the
    /// wrong gate (`0046D210`, `0046E880`); `00469D60` clears it.
    pub dword_198: u32,
    /// +19C, the network keys' lockout (`00469D60` clears it): the race's
    /// wrong-way marker sets it for a second (`0046D200`).
    pub dword_19c: i32,
    /// +150..+18F: 00469D60 stores -1 then fifteen zeroes; 0046B110 lowers
    /// them to the 0053A310 limits.
    pub values_150: [i32; 16],
    /// WORDs +190..+194, zeroed by 00469D60: item IDs equipped on respawn.
    pub items_190: [u16; 3],
}

impl WorldPlayer {
    /// The message subobject's BYTE+0 at +88.
    pub fn message_byte_00(&self) -> u8 {
        self.message_88.shown_00()
    }

    #[cfg(test)]
    pub(crate) fn set_message_byte_00(&mut self, value: u8) {
        self.message_88.set_shown_00(value);
    }
}

/// 0046A890's inputs to its mode-1 factory call (resource argument 0,
/// placement -1). The vehicle constructor itself is not ported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerSpawnRequest {
    pub player: PlayerId,
    pub position: [u16; 3],
    /// None when native passes its uninitialized rotation (saved spawn).
    pub rotation: Option<[u16; 3]>,
    /// The spawn marker the network search chose, whose basis the vehicle
    /// takes.
    pub marker: Option<ObjectId>,
}

/// 0046A890's reads in a network game (0055F448 set).
#[derive(Clone, Copy, Debug)]
pub struct NetworkRespawn<'a> {
    /// 0055F2F4, this machine's slot.
    pub local_slot: u8,
    /// 0055F3A0, set on the host.
    pub host: bool,
    /// Settings +00..+4F (00463AB0): the mode +10, the team +14, the
    /// vehicle +18 and the byte +48.
    pub settings: &'a [u32; 20],
    /// 0048F4D0's table, which 00491E80 reads for the camera's heading.
    pub inverse_sine: &'a [i16; 1024],
}

impl NetworkRespawn<'_> {
    /// 0046AC78..0046ACAF and 004698F0's tail: the owner a player's vehicle
    /// takes, its slot plus one in modes 2 and 3, else its team (+14) plus 2.
    fn owner(&self) -> u32 {
        match self.settings[0x10 / 4] {
            2 | 3 => u32::from(self.local_slot) + 1,
            _ => self.settings[0x14 / 4].wrapping_add(2),
        }
    }
}

/// The loader tail's player branch after 0046A880.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoaderPlayerSetup {
    /// No player is listed; 0048DC30 skips the whole branch.
    NoPlayer,
    /// 0046A890(player, 1) must spawn the player's vehicle. Offline loads
    /// and network sessions with setting +10 outside {0, 4} take this path.
    Respawn(PlayerId),
    /// Network setting +10 of 0 or 4: the player was attached to the camera.
    AttachedToCamera(PlayerId),
}

#[derive(Clone, Debug, Default)]
pub(super) struct WorldPlayers {
    next_id: u64,
    pub(super) list: Vec<WorldPlayer>,
    /// Manager M+74. None is a handle that has never been written.
    pub(super) camera_74: Option<ActorHandle>,
    /// The line and hint boxes each player's message holds at +4/+8.
    pub(super) message_boxes: Vec<(PlayerId, PlayerMessageBoxes)>,
    /// The process values the messages reach; None until the session sets
    /// them, which leaves enabled delivery a boundary.
    pub(super) message_environment: Option<PlayerMessageEnvironment>,
    /// 0054ED14, the clock before which 004693C0 says nothing. A process
    /// global natively; the retained arena keeps it across worlds.
    pub(super) full_notice_gate: u32,
    pub(super) message_effects: Vec<PlayerMessageEffect>,
    /// Each player's shown menu +0C, while its controller lives.
    pub(super) shown_menus: Vec<(PlayerId, ShownMenu)>,
    /// The shown menus' commands (`00413950`'s `004537E0`), in order, for the
    /// owner that holds the menus' services.
    pub(super) menu_commands: Vec<(PlayerId, u32)>,
    /// The menus a destroyed controller took with it, whose destructors run
    /// next (a shop menu undocks its factory).
    pub(super) destroyed_menus: Vec<(PlayerId, ShownMenuOwner)>,
    /// The row actions of shown menus the arena does not own (the phase-5
    /// menu, the computer and their children), for the session, which holds
    /// their services.
    pub(super) menu_actions: Vec<ShownMenuAction>,
    /// The interface cues `004391D0` plays, in order.
    pub(super) ui_cues: Vec<u16>,
    /// The broadcasts from a unit (`0047DD20`) that Tab's computer and
    /// Escape sent from a key callback, for the owner of their receivers'
    /// services to run before the next input event.
    pub(super) unit_broadcasts: Vec<super::UnitBroadcast>,
}

/// H13 factory dependencies for 00478E00's on-demand camera.
pub struct CameraFactoryInputs<'a> {
    /// Current 0054E894, reached only after camera resolution misses.
    pub process: Option<&'a ParticleFactoryProcessState>,
    pub runtime: &'a ResidentRuntime,
    pub math: &'a NativeBasisMath,
    pub allocation: ActorConstructionRequest<'a>,
    pub local_owner: u8,
    pub settings: CameraSettings,
    pub listeners: &'a mut CreationListeners,
    pub listener_effects: &'a mut dyn CreationListenerEffects,
    /// The world RNG 0040FB10's target search draws from when the camera is
    /// built with no first-player actor to follow; None fails that search.
    pub rng: Option<&'a mut u32>,
}

/// What 00478E00's lookup finds before it would build a camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorldCameraLookup {
    Found(ObjectId),
    /// M+74 names an actor whose +60 gives no camera.
    NoCamera,
    /// No live M+74 actor: 00478E00 builds the camera here.
    NeedsCamera,
}

/// What 0040F740's camera lookup found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldCameraRefresh {
    /// The camera took the settings.
    Refreshed(ObjectId),
    /// M+74 names an actor whose +60 gives no camera: nothing to refresh.
    NoCamera,
    /// No live M+74 actor, where 00478E00 builds the camera; one built
    /// later takes the settings then.
    NeedsCamera,
}

#[derive(Debug, Error)]
pub enum PlayerAttachError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Construction(#[from] ObjectConstructionError),
    #[error("unknown player {0:?}")]
    UnknownPlayer(PlayerId),
    #[error("00478E3D requires the current process NULL before the camera factory")]
    MissingCameraFactoryProcessNullHandle,
    #[error("0046AE90 dereferences a null camera from 00478E00")]
    MissingCamera,
    #[error("00478EFB's camera search 0040FB10 needs the world RNG")]
    MissingCameraSearchRng,
    #[error(transparent)]
    CameraSearch(#[from] crate::object_runtime::CameraPoseError),
    #[error(transparent)]
    WorldStart(#[from] crate::world_start::WorldStartError),
    #[error("respawning over a first-player vehicle at lifecycle 3 retires it through 0047CE90, which this path does not own")]
    LiveVehicleBoundary(ObjectId),
    #[error("pickup 0x77's boost (004231C0) reads the vehicle's unwritten position")]
    NetworkPickup77Position,
    #[error("player {0:?} selects a saved spawn whose +E0/+EC inputs are unknown")]
    MissingSavedSpawn(PlayerId),
    #[error(transparent)]
    Derived(#[from] super::DerivedActorError),
    #[error("0046A890's post-spawn path for {0:?} requires the selector-2 vehicle")]
    NotVehicle(ObjectId),
    #[error("0046AF80 dereferences player {0:?}'s missing unit (0046AE60)")]
    MissingSpawnUnit(PlayerId),
    #[error("respawn equipment (player +190 items or world 0x13's item 2) needs vehicle +4C and 00421810")]
    RespawnEquipmentBoundary,
    #[error("respawn equipment: {0}")]
    Equipment(String),
    #[error("0046BDC0 for {player:?} reads upgrade word {word}, past the six +100 DWORDs")]
    UpgradeWordBoundary { player: PlayerId, word: u32 },
    #[error(transparent)]
    Grant(#[from] inf_formats::GrantClosureError),
    #[error("pickup {0:#04x} has a class-specific 0046B2B0 case that is not ported")]
    PickupCaseBoundary(u16),
    #[error("pickup {0:#04x} is outside the 0..=0xAE records 00475320 resolves")]
    PickupOutOfRange(u16),
    #[error("player {player:?}'s token level {offset} lies past its +13C..+143 bytes")]
    TokenLevelBoundary { player: PlayerId, offset: usize },
    #[error("granting {0:#04x} notifies the player with record text through 004750C0")]
    GrantNotificationBoundary(u16),
    #[error("actor {0:?} requires its own virtual +64 vehicle view")]
    VehicleViewBoundary(ObjectId),
    #[error("vehicle {vehicle:?} item array lacks reached storage for index {index}")]
    VehicleItemStorageBoundary { vehicle: ObjectId, index: usize },
    /// A weapon pickup reached a vehicle without a +7C behavior, a missing
    /// row, or a zero primary count (IDIV).
    #[error("vehicle {0:?} weapon behavior lacks what the pickup reads")]
    VehicleWeaponBoundary(ObjectId),
}

/// 0046B2B0's HUD message request for 004135D0. The detail text is shown
/// only when that string is nonempty ("%s %s - %s", else "%s %s").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickupMessage {
    /// Text resource 0x139, or 0x13A for the class cases that select it.
    pub prefix_text: u16,
    /// The ID's record WORD +0.
    pub name_text: u16,
    /// Record text + 1, offered only for an ID not already in +100.
    pub detail_text: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickupOutcome {
    /// 0046B2B0's AL result.
    pub accepted: bool,
    pub message: Option<PickupMessage>,
    /// 004750C0's notice for a newly granted ID, sent before the rest of
    /// 0046B2B0 natively; the caller delivers it ahead of `message`.
    pub notification: Option<GrantNotification>,
    /// The grants a class case makes of other IDs first (0x1C..0x1E and 0x9C
    /// call 0046B2B0 again), each delivered whole after the notice.
    pub nested: Vec<PickupOutcome>,
    /// A full weapon row's 004693C0 on the player's message, after `nested`.
    pub full: bool,
    /// The class case's player command (0046C760), which comes after the
    /// notice and before `message`; it runs online too.
    pub command: Option<PlayerCommandOperation>,
    /// `004231C0`'s sound, played inside the class case: the caller plays
    /// it before the rest (`deliver_pickup_outcome` refuses one left).
    pub sound: Option<PickupSound>,
}

/// `004231C0`'s `00487EC0`: a sound at the vehicle, both its values 0x10000.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickupSound {
    pub sound: u32,
    pub position: [u16; 3],
}

/// `004231C4`: a network game's boost (pickup 0x77), two seconds of full
/// power and steer (+5AC and +224), and its sound.
const BOOST_MICROS: u32 = 2_000_000;
const BOOST_SOUND: u32 = 0x19;

/// 004750C0's notice of a newly granted ID whose record has text: offline,
/// through the player's message at +88.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantNotification {
    pub id: u16,
    /// Record WORD +0, the H0 text.
    pub text: u16,
}

/// The world owners 0046A890's equipment loop lends its services.
pub struct PlayerEquipmentWorld<'s> {
    pub world_start: &'s mut crate::world_start::WorldStartState,
    pub rng: &'s mut u32,
}

/// 0046A890's equipment services on the vehicle view: virtual +4C
/// (0047AAF0) with a NULL position, whose out slot names the item, then
/// 00421810.
pub trait PlayerEquipmentHost {
    fn create_item(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        resource: super::ActorChildResource,
        factory: &mut CameraFactoryInputs<'_>,
        world: &mut PlayerEquipmentWorld<'_>,
    ) -> Result<Option<ObjectId>, String>;

    fn attach_item(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        item: ObjectId,
        rng: &mut u32,
    ) -> Result<bool, String>;
}

/// A host and the world it borrows. Absent, a respawn that reaches the
/// equipment loop stops at `RespawnEquipmentBoundary`.
pub struct PlayerEquipment<'s> {
    pub host: &'s mut dyn PlayerEquipmentHost,
    pub world: PlayerEquipmentWorld<'s>,
}

/// Inputs of 0046A890 beyond the camera factory.
pub struct PlayerRespawnInputs<'a, 'b> {
    pub start: crate::world_start::WorldStartContext<'b>,
    /// World header DWORD +18, 0048ED60's fallback vehicle selection.
    pub world_header_18: u32,
    /// The vehicle allocation's pre-context WORDs and terrain inputs.
    pub vehicle_allocation: ActorConstructionRequest<'a>,
    /// Signed H14 +0E/+0F bytes for 00425E60's RNG draw.
    pub world_mask_limits: [i8; 2],
    pub rng: &'b mut u32,
    pub vehicle_table: &'a [u32; inf_formats::VEHICLE_BEHAVIOR_TABLE_DWORDS],
    /// Mission +A0 (G+250).
    pub current_world_a0: u32,
    /// Settings +84, the cheat word; bit 0x40 adds the Cranial Implanter
    /// (pickup and grant 0x33).
    pub settings_84: u32,
    pub pickups: PickupTables<'a>,
    pub equipment: Option<&'b mut dyn PlayerEquipmentHost>,
    /// Some in a network game.
    pub network: Option<NetworkRespawn<'a>>,
}

/// The completed offline 0046A890.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerRespawn {
    /// None when the mode-1 factory rejected the vehicle; native then skips
    /// the vehicle wiring but still runs the pickups.
    pub vehicle: Option<ObjectId>,
    /// The player's messages from the pickups and grants, in call order.
    pub messages: Vec<RespawnMessage>,
}

/// One of 0046A890's messages to the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RespawnMessage {
    /// 004750C0's notice of a newly granted ID.
    Notice(GrantNotification),
    /// 0046B2B0's 004135D0 request.
    Pickup(PickupMessage),
}

/// Executable tables consumed by 0046B2B0 and its callees.
#[derive(Clone, Copy, Debug)]
pub struct PickupTables<'a> {
    pub grants: &'a inf_formats::GrantTable,
    pub texts: &'a [u16; inf_formats::GRANT_TEXT_IDS],
    pub upgrade_limits: &'a [i32; 16],
    /// 0055F448, which 0046B2B0 and 004750C0 read.
    pub network_active: bool,
}

impl ObjectArena {
    /// Copy this actual player's complete 0046C870 save data into the existing
    /// intrinsic shape. This pure export does not execute the serializer's
    /// buffer/cursor/capacity branches, select the first player or resolve handles.
    pub fn export_player_save_state(
        &self,
        player: PlayerId,
    ) -> Result<inf_formats::PlayerSaveState, PlayerAttachError> {
        let live = self.world_player(player)?;
        Ok(inf_formats::PlayerSaveState {
            owner_100: live.bits_100,
            owner_118: live.dword_118,
            owner_11c: live.word_11c,
            owner_11e: live.word_11e,
            owner_120: [
                live.dwords_120_128[0],
                live.dwords_120_128[1],
                live.dwords_120_128[2],
                live.dword_12c,
            ],
            owner_144: live.dword_144,
            owner_148: live.dword_148,
            owner_14c: live.dword_14c,
            owner_150: live.values_150.map(|word| word as u32),
            // 0046C97A/0046C983 read both complete DWORDs, including bytes
            // after a NUL and high-bit data. No identity/string policy applies.
            owner_13c: [
                u32::from_le_bytes([
                    live.bytes_13c_143[0],
                    live.bytes_13c_143[1],
                    live.bytes_13c_143[2],
                    live.bytes_13c_143[3],
                ]),
                u32::from_le_bytes([
                    live.bytes_13c_143[4],
                    live.bytes_13c_143[5],
                    live.bytes_13c_143[6],
                    live.bytes_13c_143[7],
                ]),
            ],
            owner_190: live.items_190,
            owner_130: live.command_mask_130,
            owner_a0: live.grants_9c,
        })
    }

    /// Restore the complete raw save data in 0046C9F0's destination order.
    /// Complete typed source data does not model native partial buffer faults.
    /// Handles +D4/+D8 and checkpoint +ED/+F0 are outside this prefix.
    pub fn restore_player_save_prefix(
        &mut self,
        player: PlayerId,
        saved: &inf_formats::PlayerSaveState,
    ) -> Result<(), PlayerAttachError> {
        let live = self.world_player_mut(player)?;
        live.bits_100 = saved.owner_100;
        live.dword_118 = saved.owner_118;
        live.word_11c = saved.owner_11c;
        live.word_11e = saved.owner_11e;
        live.dwords_120_128.copy_from_slice(&saved.owner_120[..3]);
        // 0046CA83/0046CA86 restores the fourth +120 group DWORD at +12C.
        live.dword_12c = saved.owner_120[3];
        live.dword_144 = saved.owner_144;
        live.dword_148 = saved.owner_148;
        live.dword_14c = saved.owner_14c;
        live.values_150 = saved.owner_150.map(|word| word as i32);
        // 0046CAE3/0046CAEE store two DWORDs at +13C/+140. Keep all bytes.
        live.bytes_13c_143[..4].copy_from_slice(&saved.owner_13c[0].to_le_bytes());
        live.bytes_13c_143[4..].copy_from_slice(&saved.owner_13c[1].to_le_bytes());
        live.items_190 = saved.owner_190;
        // Actual command words restored at 0046CB23/0046CB28/0046CB2E.
        live.command_mask_130 = saved.owner_130;
        // 00475080's nested grant payload at player +A0 follows the prefix.
        live.grants_9c = saved.owner_a0;
        Ok(())
    }

    /// 00469D60's list custody: append after every existing player.
    pub fn add_world_player(&mut self, grants: [u32; GRANT_MASK_WORDS]) -> PlayerId {
        let null = self
            .actor_handles()
            .map(|handles| handles.null_handle())
            .unwrap_or(ActorHandle::from_raw(0));
        let arena = self.identity;
        let players = &mut self.players;
        let id = PlayerId(players.next_id);
        players.next_id += 1;
        players.list.push(WorldPlayer {
            id,
            message_88: PlayerMessageState::new(arena, id),
            grants_9c: grants,
            handle_d4: null,
            handle_d8: null,
            handle_dc: null,
            word_11c: 0,
            byte_58: Some(1),
            bits_100: [0; 6],
            dword_118: 0,
            word_11e: 0,
            dwords_120_128: [0; 3],
            dword_12c: 0,
            command_mask_130: [0; 3],
            bytes_13c_143: [0; 8],
            dword_144: 1,
            dword_148: 0,
            dword_14c: 8,
            byte_ed: 0,
            byte_ee: 0,
            dword_f0: 0,
            saved_e0: None,
            byte_ec: None,
            dword_e8: 0,
            controls: PlayerControlBlock::default(),
            dword_48: None,
            dword_4c: None,
            dword_50: None,
            dword_54: None,
            dword_f4: 0,
            dword_f8: 0,
            byte_fc: 0,
            dword_198: 0,
            dword_19c: 0,
            values_150: {
                let mut values = [0; 16];
                values[0] = -1;
                values
            },
            items_190: [0; 3],
        });
        id
    }

    pub fn world_players(&self) -> &[WorldPlayer] {
        &self.players.list
    }

    /// Where the vehicles stand (16-bit world words) whose players thrust
    /// them, for the tracer's trails and thruster lights: a boost running,
    /// Thrust held on the hovercraft, the VTOL or the skimmer, or the
    /// skimmer's power ahead from Climb (Up) or the stick, as its drive
    /// takes them (00422574).
    /// It changes nothing, not even the handle lookup's cache.
    pub fn thrusting_vehicles(&self) -> Vec<[u16; 3]> {
        let Some(handles) = self.actor_handles() else {
            return Vec::new();
        };
        self.players
            .list
            .iter()
            .filter_map(|player| {
                let vehicle = handles.peek(player.handle_d4)?;
                let actor = self.selector2_actor(vehicle).ok()?;
                let boosting = actor.dwords_220_224[1] != 0;
                let form = actor.dwords_1e8_1f4[0];
                let controls = &player.controls;
                let thrust = controls.thrust_1c > 0 && (3..=5).contains(&form)
                    || form == 5
                        && controls.thrust_1c == 0
                        && (controls.pitch_18 < 0
                            || controls.pitch_18 == 0 && controls.analog_forward());
                (boosting || thrust)
                    .then(|| self.object(vehicle).ok()?.base.position_words)
                    .flatten()
            })
            .collect()
    }

    /// The first player's vehicle and controls, for the remastered camera.
    /// Like [`Self::thrusting_vehicles`], it changes nothing.
    pub fn first_player_drive(&self) -> Option<(ObjectId, super::PlayerControlBlock)> {
        let player = self.players.list.first()?;
        let vehicle = self.actor_handles()?.peek(player.handle_d4)?;
        Some((vehicle, player.controls))
    }

    /// Whether the first player steers a remote unit (its +D8 actor, the
    /// Cranial Implanter's missile), whose view the camera takes. Like
    /// [`Self::thrusting_vehicles`], it changes nothing.
    pub fn first_player_steers_remote(&self) -> bool {
        match (self.players.list.first(), self.actor_handles()) {
            (Some(player), Some(handles)) => handles.peek(player.handle_d8).is_some(),
            _ => false,
        }
    }

    /// 00488AA0.
    pub fn first_player(&self) -> Option<PlayerId> {
        self.players.list.first().map(|player| player.id)
    }

    /// 0046B060: the first player's +D8 actor, its remotely controlled unit.
    pub fn first_player_remote_actor(&mut self) -> Result<Option<ObjectId>, ObjectError> {
        let Some(player) = self.players.list.first() else {
            return Ok(None);
        };
        self.resolve_handle(player.handle_d8)
    }

    /// 0046B030 resolves only the first player's primary +D4 handle. It never
    /// consults +D8; an empty player list does not require a registry lookup.
    pub fn first_player_primary_actor(&mut self) -> Result<Option<ObjectId>, ObjectError> {
        let Some(player) = self.players.list.first() else {
            return Ok(None);
        };
        self.resolve_handle(player.handle_d4)
    }

    /// 0046B090 captures current 0054E894 before the manager/list read. A raw
    /// +D8 unequal to that null is authoritative; otherwise compare +D4.
    /// This pure body reads no registry, actor lifecycle or position. The
    /// arena supplies the actual manager; raw absent-manager/list aliases and
    /// unmapped backing remain outside this typed owner's admission.
    pub fn find_player_for_actor_handle(
        &self,
        request: PlayerActorLookupRequest,
        context: PlayerActorLookupContext<'_>,
    ) -> Result<Option<PlayerId>, PlayerActorLookupError> {
        let null = context
            .process
            .and_then(|process| process.process_null_handle)
            .ok_or(PlayerActorLookupError::MissingProcessNullHandle)?;
        if request.handle == null {
            return Ok(None);
        }
        Ok(self
            .players
            .list
            .iter()
            .find(|player| {
                if player.handle_d8 != null {
                    player.handle_d8 == request.handle
                } else {
                    player.handle_d4 == request.handle
                }
            })
            .map(|player| player.id))
    }

    /// 0046AF80, the save pad's checkpoint. At most once per 50 clock ticks
    /// (the signed difference from +F0), the player's unit (0046AE60) gives
    /// the saved spawn: +ED = 1, its position in +E0..+E5, its vehicle mode
    /// +1E8 through its +64 view in +EC (0 without a view), the clock in +F0
    /// and its basis angles (+48, then 00491E80) in +E8.
    pub fn save_player_spawn(
        &mut self,
        player: PlayerId,
        clock: u32,
        math: &NativeBasisMath,
        inverse_sine: &[i16; 1024],
    ) -> Result<(), PlayerAttachError> {
        let since = (clock as i32).wrapping_sub(self.world_player(player)?.dword_f0 as i32);
        if since < 50 {
            return Ok(());
        }
        self.world_player_mut(player)?.byte_ed = 1;
        let unit = self
            .player_actor(player)?
            .ok_or(PlayerAttachError::MissingSpawnUnit(player))?;
        let position = self
            .object(unit)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(unit))?;
        self.world_player_mut(player)?.saved_e0 = Some(position);
        let mode = match &self.actor(unit)?.extension {
            // The vehicle's +64 (00404610) is itself.
            ActorExtension::Derived(derived) => match &derived.class {
                crate::object_runtime::DerivedActorClass::Selector2(vehicle) => {
                    vehicle.dwords_1e8_1f4[0] as u8
                }
                _ => return Err(PlayerAttachError::VehicleViewBoundary(unit)),
            },
            // The other admitted classes' +64 is the null 00401300.
            _ => 0,
        };
        let state = self.world_player_mut(player)?;
        state.byte_ec = Some(mode);
        state.dword_f0 = clock;
        let basis = self.actor(unit)?.rotating.basis;
        self.world_player_mut(player)?.dword_e8 = math.packed_angles(&basis, inverse_sine);
        Ok(())
    }

    /// 0046AF70, after 0048E820 publishes the next mission/start pair.
    pub fn clear_player_completion_checkpoint(
        &mut self,
        player: PlayerId,
    ) -> Result<(), PlayerAttachError> {
        let player = self.world_player_mut(player)?;
        player.byte_ed = 0;
        player.dword_f0 = 0;
        Ok(())
    }

    pub fn world_player(&self, id: PlayerId) -> Result<&WorldPlayer, PlayerAttachError> {
        self.players
            .list
            .iter()
            .find(|player| player.id == id)
            .ok_or(PlayerAttachError::UnknownPlayer(id))
    }

    /// The results screen's medal (0042BF9F): `add word [player+11E], medal`.
    pub fn add_player_word_11e(
        &mut self,
        player: PlayerId,
        amount: u16,
    ) -> Result<(), PlayerAttachError> {
        let live = self.world_player_mut(player)?;
        live.word_11e = live.word_11e.wrapping_add(amount);
        Ok(())
    }

    pub(super) fn world_player_mut(
        &mut self,
        id: PlayerId,
    ) -> Result<&mut WorldPlayer, PlayerAttachError> {
        self.players
            .list
            .iter_mut()
            .find(|player| player.id == id)
            .ok_or(PlayerAttachError::UnknownPlayer(id))
    }

    /// The Cheats Menu's Skip to (004149A9): `mov byte [player + 13C +
    /// offset], level`, a token level of the factory shop's Energy
    /// Distribution.
    pub fn set_player_token_level(
        &mut self,
        player: PlayerId,
        offset: usize,
        level: u8,
    ) -> Result<(), PlayerAttachError> {
        let levels = &mut self.world_player_mut(player)?.bytes_13c_143;
        *levels
            .get_mut(offset)
            .ok_or(PlayerAttachError::TokenLevelBoundary { player, offset })? = level;
        Ok(())
    }

    /// 004383B0 stores 500 ms at the first player's +F4 whenever gameplay
    /// (phase 4) is entered.
    pub fn set_player_lockout_f4(
        &mut self,
        player: PlayerId,
        microseconds: i32,
    ) -> Result<(), PlayerAttachError> {
        self.world_player_mut(player)?.dword_f4 = microseconds;
        Ok(())
    }

    /// 0046BDC0: the low WORD of `id` names a bit of the six +100 DWORDs;
    /// ID zero is always owned.
    pub fn player_upgrade_owned(
        &self,
        player: PlayerId,
        id: u32,
    ) -> Result<bool, PlayerAttachError> {
        let id = id & 0xffff;
        if id == 0 {
            return Ok(true);
        }
        let word = id >> 5;
        let bits = self
            .world_player(player)?
            .bits_100
            .get(word as usize)
            .copied()
            .ok_or(PlayerAttachError::UpgradeWordBoundary { player, word })?;
        Ok(bits & 1 << (id & 31) != 0)
    }

    /// The unit broadcasts the key callbacks sent since the last call, in
    /// order.
    pub fn take_unit_broadcasts(&mut self) -> Vec<super::UnitBroadcast> {
        std::mem::take(&mut self.players.unit_broadcasts)
    }

    /// `004137D0`'s `+34` store after its event-6 broadcast answered.
    pub(crate) fn set_player_message_byte_34(
        &mut self,
        player: PlayerId,
    ) -> Result<(), PlayerAttachError> {
        self.world_player_mut(player)?.message_88.set_byte_34(1);
        Ok(())
    }

    /// Manager M+74, the camera handle last published by 00478E00.
    pub fn camera_handle(&self) -> Option<ActorHandle> {
        self.players.camera_74
    }

    fn resolve_handle(&mut self, handle: ActorHandle) -> Result<Option<ObjectId>, ObjectError> {
        self.resolve_actor_handle(handle)
            .map_err(|_| ObjectError::MissingActorRegistry)
    }

    /// 0046AE60: +D8's actor, otherwise +D4's.
    pub fn player_actor(
        &mut self,
        player: PlayerId,
    ) -> Result<Option<ObjectId>, PlayerAttachError> {
        let state = self.world_player(player)?;
        let (d8, d4) = (state.handle_d8, state.handle_d4);
        match self.resolve_handle(d8)? {
            Some(actor) => Ok(Some(actor)),
            None => Ok(self.resolve_handle(d4)?),
        }
    }

    /// Virtual +60 on a resolved actor: the camera's 00404610 returns itself;
    /// the other admitted classes inherit 00401300 and return null.
    fn camera_view(&self, object: ObjectId) -> Result<Option<ObjectId>, ObjectError> {
        Ok(
            matches!(self.actor(object)?.extension, ActorExtension::Selector16(_))
                .then_some(object),
        )
    }

    /// 00478E00 while M+74 resolves: that actor's +60 view. None where it
    /// would construct a camera instead.
    pub fn existing_world_camera(&mut self) -> Result<Option<Option<ObjectId>>, ObjectError> {
        let Some(handle) = self.players.camera_74 else {
            return Ok(None);
        };
        match self.resolve_handle(handle)? {
            Some(object) => Ok(Some(self.camera_view(object)?)),
            None => Ok(None),
        }
    }

    /// 00478E00. A factory rejection returns null, as native does; port-side
    /// construction errors propagate.
    pub fn world_camera(
        &mut self,
        inputs: &mut CameraFactoryInputs<'_>,
    ) -> Result<Option<ObjectId>, PlayerAttachError> {
        self.world_camera_with_rng(inputs, None)
    }

    /// 00478E00 with the world RNG lent for its 0040FB10 search, in place of
    /// the factory's own.
    pub fn world_camera_with_rng(
        &mut self,
        inputs: &mut CameraFactoryInputs<'_>,
        rng: Option<&mut u32>,
    ) -> Result<Option<ObjectId>, PlayerAttachError> {
        if let Some(handle) = self.players.camera_74 {
            if let Some(object) = self.resolve_handle(handle)? {
                return Ok(self.camera_view(object)?);
            }
        }
        // 00478E3D reads current NULL before 00460310, even when H13
        // later rejects the camera. Its mode-zero constructor does not consume
        // the captured argument; retain the required read without a sidecar.
        inputs
            .process
            .and_then(|process| process.process_null_handle)
            .ok_or(PlayerAttachError::MissingCameraFactoryProcessNullHandle)?;
        let outcome = construct_world_object(
            self,
            inputs.runtime,
            ObjectConstructionRequest {
                resource_argument: 1,
                context: BaseContext {
                    position_words: None,
                    placement_index: u32::MAX,
                },
                suffix: None,
                command_allocation: None,
                network_active: false,
                actor: Some(ActorFactoryInputs {
                    allocation: inputs.allocation,
                    rotation_words: None,
                    local_owner: inputs.local_owner,
                    first_player_grants: None,
                    grant_table: None,
                    math: inputs.math,
                    derived: None,
                    listeners: &mut *inputs.listeners,
                    listener_effects: &mut *inputs.listener_effects,
                    network_world_active: false,
                    terrain: None,
                    world_start: None,
                    camera_settings: Some(inputs.settings),
                    spawn_mode_1: false,
                    network_handle: None,
                    vehicle_table: None,
                }),
            },
        )?;
        let ObjectConstructionOutcome::Constructed(object) = outcome else {
            return Ok(None);
        };
        let camera = self
            .camera_view(object)?
            .ok_or(PlayerAttachError::MissingCamera)?;
        self.players.camera_74 = Some(self.actor(camera)?.handle);
        self.set_camera_byte_187(camera)?;
        // Retail 00478E00 returns the camera here without requiring an
        // attached player actor: the mission-18 load probe shows the
        // player branch calling it right after detaching (handle_d8
        // null), and the spawn tail wires the follow target from the
        // vehicle instead. With no first-player actor 00478EFB starts the
        // camera's own search, 0040FB10, which draws from the world RNG.
        let attached = match self.first_player() {
            Some(player) => self.player_actor(player)?,
            None => None,
        };
        if attached.is_none() {
            let rng = rng
                .or(inputs.rng.as_deref_mut())
                .ok_or(PlayerAttachError::MissingCameraSearchRng)?;
            self.start_camera_search(camera, rng)?;
        }
        if let Some(attached) = attached {
            let target = self
                .allocate_actor_service(crate::object_runtime::ActorServiceAllocationRequest {
                    kind: crate::object_runtime::ActorServiceResourceKind::Target(
                        crate::object_runtime::NativeTargetClass::ActorFollow,
                    ),
                })
                .map_err(ObjectError::from)?;
            if let Some(reference) = target {
                //00478EC3: attached+84 is read only after the20-byte allocation succeeds.
                let handle = self.actor(attached)?.handle;
                let data = self.follow_target(FollowTargetRequest::resolve_actor(handle))?;
                self.construct_native_target(
                    reference,
                    crate::object_runtime::NativeTarget::ActorFollow(data),
                )?;
            }
            self.replace_camera_target(camera, target, inputs.settings)?;
        }
        Ok(Some(camera))
    }

    /// 00478E00's lookup without its construction: the actor M+74 names,
    /// through its +60.
    pub(crate) fn world_camera_lookup(&mut self) -> Result<WorldCameraLookup, ObjectError> {
        let object = match self.players.camera_74 {
            Some(handle) => self.resolve_handle(handle)?,
            None => None,
        };
        let Some(object) = object else {
            return Ok(WorldCameraLookup::NeedsCamera);
        };
        Ok(match self.camera_view(object)? {
            Some(camera) => WorldCameraLookup::Found(camera),
            None => WorldCameraLookup::NoCamera,
        })
    }

    /// 0040F740 with a world: the camera 00478E00 finds (M+74 through its
    /// +60) takes the First Person and Camera Style settings. A handle that
    /// resolves to an actor with no camera leaves it there, as natively.
    pub fn refresh_world_camera(
        &mut self,
        settings: CameraSettings,
    ) -> Result<WorldCameraRefresh, ObjectError> {
        Ok(match self.world_camera_lookup()? {
            WorldCameraLookup::Found(camera) => {
                self.apply_camera_settings(camera, settings)?;
                WorldCameraRefresh::Refreshed(camera)
            }
            WorldCameraLookup::NoCamera => WorldCameraRefresh::NoCamera,
            WorldCameraLookup::NeedsCamera => WorldCameraRefresh::NeedsCamera,
        })
    }

    /// 0046AE90. Returns the native success byte. The unchanged handle and
    /// the global null handle are stored without an actor query or camera.
    pub fn attach_player(
        &mut self,
        player: PlayerId,
        handle: ActorHandle,
        factory: &mut CameraFactoryInputs<'_>,
    ) -> Result<bool, PlayerAttachError> {
        let null = self
            .actor_handles()
            .ok_or(ObjectError::MissingActorRegistry)?
            .null_handle();
        if handle == self.world_player(player)?.handle_d8 || handle == null {
            self.world_player_mut(player)?.handle_d8 = handle;
            return Ok(true);
        }
        let Some(actor) = self.resolve_handle(handle)? else {
            return Ok(false);
        };
        // Virtual +88 receives player +4. Selector 88's and 78's 00444AB0
        // returns true; the camera's 0040F880 clears its +180 and returns true; the
        // missiles' 00457750 (25, 28, 68) holds the missile; 00405750
        // accepts for the family 004236D0's classes (9, 10, 21, 36),
        // selector 12 and selector 13 (005269C4, which the port does not
        // construct). Every other actor table binds 00402720 or the visual
        // base's 00405140, which refuse.
        match self.actor(actor)?.extension {
            ActorExtension::Selector88(_) | ActorExtension::Selector78(_) => {}
            ActorExtension::Selector16(_) => self.attach_camera_actor(actor)?,
            ActorExtension::BehaviorResource(_) if self.is_missile(actor) => {
                let camera = self.world_camera(factory)?;
                self.hold_missile(actor, camera, factory.settings)?;
            }
            ActorExtension::Derived(_)
                if self.family4236d0_actor(actor).is_ok()
                    || self.selector12_actor(actor).is_ok() => {}
            _ => return Ok(false),
        }
        self.world_player_mut(player)?.handle_d8 = handle;
        let camera = self
            .world_camera(factory)?
            .ok_or(PlayerAttachError::MissingCamera)?;
        if self.actor(camera)?.handle == handle {
            return Ok(true);
        }
        let target = self.follow_target_resource(FollowTargetRequest::resolve_actor(handle))?;
        self.replace_camera_target(camera, target, factory.settings)?;
        Ok(true)
    }

    /// 0048DC30's player branch after the after-load passes: 0046A880
    /// (+11C = 0, +58 = 1), then either the respawn request or, for a network
    /// session whose Settings +10 is 0 or 4, 0046AE90 with the camera's handle.
    pub fn run_loader_player_setup(
        &mut self,
        network_active: bool,
        settings_10: u32,
        factory: &mut CameraFactoryInputs<'_>,
    ) -> Result<LoaderPlayerSetup, PlayerAttachError> {
        let Some(player) = self.first_player() else {
            return Ok(LoaderPlayerSetup::NoPlayer);
        };
        let state = self.world_player_mut(player)?;
        state.word_11c = 0;
        state.byte_58 = Some(1);
        if !network_active || (settings_10 != 0 && settings_10 != 4) {
            return Ok(LoaderPlayerSetup::Respawn(player));
        }
        let camera = self
            .world_camera(factory)?
            .ok_or(PlayerAttachError::MissingCamera)?;
        let handle = self.actor(camera)?.handle;
        self.attach_player(player, handle, factory)?;
        Ok(LoaderPlayerSetup::AttachedToCamera(player))
    }

    /// 0046A890 up to its factory call. Detach through 0046AE90(null); a
    /// first-player +D4 vehicle at lifecycle 3 needs 0047CE90 and is a
    /// boundary, while a dying one (the wreck) is left as it is. Then +50 = 0, +12C = 3 when bit 3 of +100 is set, +34 =
    /// 0048ED60's vehicle selection (world +14 as a signed byte, else world
    /// header DWORD +18) or +144 when that is zero, +28 = +148 + 2 and
    /// +2C = +14C + 2. The spawn is the saved +E0/+EC pair when +ED is set,
    /// otherwise 0048DAF0/0048DBC0; a network game then searches the spawn
    /// markers ([`Self::network_spawn`]). Finally +EE = 1.
    pub fn prepare_player_spawn(
        &mut self,
        player: PlayerId,
        start: &crate::world_start::WorldStartContext<'_>,
        world_header_18: u32,
        network: Option<(NetworkRespawn<'_>, &mut u32)>,
        factory: &mut CameraFactoryInputs<'_>,
    ) -> Result<PlayerSpawnRequest, PlayerAttachError> {
        let null = self
            .actor_handles()
            .ok_or(ObjectError::MissingActorRegistry)?
            .null_handle();
        self.attach_player(player, null, factory)?;
        // 0046A91A..0046A930: a dying vehicle (the wreck) stays as it is.
        if let Some(vehicle) = self.first_player_primary_actor()? {
            if self.actor(vehicle)?.word_c4 == 3 {
                return Err(PlayerAttachError::LiveVehicleBoundary(vehicle));
            }
        }
        let selection = match start.state.vehicle_selection as i8 {
            0 => world_header_18,
            byte => i32::from(byte) as u32,
        };
        let state = self.world_player_mut(player)?;
        state.dword_50 = Some(0);
        // 0046BDC0(3): bit 3 of the first +100 DWORD.
        if state.bits_100[0] & 1 << 3 != 0 {
            state.dword_12c = 3;
        }
        state.controls.dword_34 = if selection != 0 {
            selection
        } else {
            state.dword_144
        };
        state.controls.dword_28 = state.dword_148.wrapping_add(2);
        state.controls.dword_2c = state.dword_14c.wrapping_add(2);
        let (mut position, mut rotation) = if state.byte_ed != 0 {
            let (Some(position), Some(vehicle)) = (state.saved_e0, state.byte_ec) else {
                return Err(PlayerAttachError::MissingSavedSpawn(player));
            };
            state.controls.dword_34 = i32::from(vehicle as i8) as u32;
            // SavedSpawnRotationApproximation: 0046A9A2 leaves the factory's
            // rotation words unwritten stack. Their three rotations only
            // advance the vehicle's basis counter here, because 004911F0
            // then replaces every row with the saved angles.
            (position, Some([0; 3]))
        } else {
            (start.resolve_position()?, Some(start.resolve_heading()?))
        };
        let mut marker = None;
        if let Some((network, rng)) = network {
            let team = network.settings[0x14 / 4].wrapping_add(2);
            if let Some((chosen, at)) = self.network_spawn(team, rng)? {
                marker = Some(chosen);
                position = at;
            } else if self.world_player(player)?.byte_ee != 0 {
                // 0046AB47: no marker; a respawn takes the camera's place
                // and heading (00491E80's yaw field, as an angle).
                if let Some(camera) = self.world_camera(factory)? {
                    let basis = self.actor(camera)?.rotating.basis;
                    let packed = factory.math.packed_angles(&basis, network.inverse_sine);
                    rotation = Some([(packed << 6) as u16, 0, 0]);
                    position = self
                        .object(camera)?
                        .base
                        .position_words
                        .ok_or(ObjectError::ActorPositionUnknown(camera))?;
                }
            }
        }
        self.world_player_mut(player)?.byte_ee = 1;
        Ok(PlayerSpawnRequest {
            player,
            position,
            rotation,
            marker,
        })
    }

    /// 0046A9F3..0046AB45: the spawn markers (0054E2F8) of `team` or of
    /// anyone (1), each scored by a draw (0048F060) and, while that beats
    /// the best, by 0x100000 less the squared distance of every player's
    /// vehicle (flag 0x10 on G+48) nearer than 1024. The lowest score wins;
    /// the spawn is its position plus its offset +78.
    fn network_spawn(
        &self,
        team: u32,
        rng: &mut u32,
    ) -> Result<Option<(ObjectId, [u16; 3])>, PlayerAttachError> {
        let mut best = i32::MAX;
        let mut chosen = None;
        for &marker in &self.spawn_markers {
            let state = self.selector45(marker)?;
            if state.team_84 != team && state.team_84 != 1 {
                continue;
            }
            let mut score = crate::name_entry::name_entry_rng_next(rng) as i32;
            if score >= best {
                continue;
            }
            let at = self
                .object(marker)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(marker))?;
            for &actor in self.actor_list() {
                let base = &self.object(actor)?.base;
                if base.flags & 0x10 == 0 {
                    continue;
                }
                let vehicle = base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))?;
                let delta =
                    |axis: usize| i32::from((at[axis] as i16).wrapping_sub(vehicle[axis] as i16));
                let distance = (0..3).fold(0_i32, |sum, axis| {
                    sum.wrapping_add(delta(axis).wrapping_mul(delta(axis)))
                });
                if distance < 0x100000 {
                    score = score.wrapping_add(0x100000 - distance);
                }
            }
            if score < best {
                best = score;
                let offset = state.offset_78;
                chosen = Some((
                    marker,
                    [0, 1, 2].map(|axis| at[axis].wrapping_add(offset[axis])),
                ));
            }
        }
        Ok(chosen)
    }

    /// 0046CB50: clear flag 0x10 on the actor named by +D4, store the new
    /// handle, then set 0x10 on its actor. Each flag change that alters the
    /// low nine bits marks dirty byte bit 1.
    pub fn bind_player_vehicle(
        &mut self,
        player: PlayerId,
        handle: ActorHandle,
    ) -> Result<(), PlayerAttachError> {
        let old = self.world_player(player)?.handle_d4;
        if let Some(actor) = self.resolve_handle(old)? {
            let base = &mut self.owned_mut(actor)?.state.base;
            let flags = base.flags & !0x10;
            super::mark_flag_change(base, flags);
        }
        self.world_player_mut(player)?.handle_d4 = handle;
        if let Some(actor) = self.resolve_handle(handle)? {
            let base = &mut self.owned_mut(actor)?.state.base;
            let flags = base.flags | 0x10;
            super::mark_flag_change(base, flags);
        }
        Ok(())
    }

    /// 0046A890 from its factory return to the camera retarget: camera
    /// 00478E00, 0046CB50, the chosen spawn marker's basis turned a quarter,
    /// vehicle dirty |= 2 and its owner +B4 (2 offline; in a network game
    /// [`NetworkRespawn::owner`], and Powerball launches it), 0041A600
    /// (0046B110 on the first player), the +190 equipment loop and the
    /// world-0x13 item check, then a follow target on the vehicle through
    /// 00410E70 and camera +187 = 1. The 0046B2B0 grants that follow are a
    /// separate owner. `current_world_a0` is mission +A0 (G+250).
    #[allow(clippy::too_many_arguments)]
    pub fn finish_player_spawn(
        &mut self,
        player: PlayerId,
        vehicle: ObjectId,
        current_world_a0: u32,
        upgrade_limits: &[i32; 16],
        factory: &mut CameraFactoryInputs<'_>,
        rng: &mut u32,
        equipment: Option<(
            &mut dyn PlayerEquipmentHost,
            &mut crate::world_start::WorldStartState,
        )>,
        marker: Option<ObjectId>,
        network: Option<NetworkRespawn<'_>>,
    ) -> Result<ObjectId, PlayerAttachError> {
        // 0046ABD1: a saved spawn turns the vehicle's basis (+48, 004026B0's
        // +44) to the saved angles through 004911F0.
        if self.world_player(player)?.byte_ed != 0 {
            let packed = self.world_player(player)?.dword_e8;
            let mut basis = self.actor(vehicle)?.rotating.basis;
            factory.math.orient_from_packed(&mut basis, packed);
            self.actor_mut(vehicle)?.rotating.basis = basis;
        }
        // With the player detached, a camera built here searches (0040FB10).
        let camera = self
            .world_camera_with_rng(factory, Some(&mut *rng))?
            .ok_or(PlayerAttachError::MissingCamera)?;
        let mut equipment = equipment.map(|(host, world_start)| PlayerEquipment {
            host,
            world: PlayerEquipmentWorld { world_start, rng },
        });
        // Vehicle +64 is 00404610 (itself) for selector 2.
        self.selector2_actor(vehicle)
            .map_err(|_| PlayerAttachError::NotVehicle(vehicle))?;
        let handle = self.actor(vehicle)?.handle;
        self.bind_player_vehicle(player, handle)?;
        // 0046AC18..0046AC6A: the marker's whole basis (+48), then 00491530
        // turns the vehicle's a quarter.
        if let Some(marker) = marker {
            let mut basis = self.selector45(marker)?.rotating.basis;
            factory.math.rotate_in_plane(
                &mut basis,
                0x4000,
                inf_formats::native_basis::RotationPlane::Xz,
            );
            self.actor_mut(vehicle)?.rotating.basis = basis;
        }
        self.owned_mut(vehicle)?.state.base.dirty_word |= 2;
        self.actor_mut(vehicle)?.word_b4 = network.map_or(2, |network| network.owner());
        // 0046ACB5..0046ACDC: Powerball (mode 4) pushes the vehicle up and
        // forward (0047B540).
        if network.is_some_and(|network| network.settings[0x10 / 4] == 4) {
            self.add_actor_local_linear_velocity(vehicle, [0, 200, 500])?;
        }
        // 0041A600 -> 0046B110 on the first player, whose +D4 names the
        // behavior owner through its +7C.
        if let Some(first) = self.first_player() {
            let owner = self.resolve_handle(self.world_player(first)?.handle_d4)?;
            let state = self.world_player(first)?;
            let bits = state.bits_100;
            let mut values = state.values_150;
            self.apply_vehicle_upgrades(
                vehicle,
                owner,
                &bits,
                &mut values,
                upgrade_limits,
                network.is_some(),
            )?;
            self.world_player_mut(first)?.values_150 = values;
        }
        // 0046AD06..0046AD7D. Each item WORD is reread, and the item-2 test
        // follows the factory call.
        let mut carried_two = false;
        for index in 0..3 {
            let item = self.world_player(player)?.items_190[index];
            if item == 0 {
                continue;
            }
            let resource = super::ActorChildResource::RegisterLowWord(item);
            if let Some(actor) =
                self.create_equipment_item(&mut equipment, vehicle, resource, factory)?
            {
                if self.world_player(player)?.items_190[index] == 2 {
                    carried_two = true;
                }
                self.attach_equipment_item(&mut equipment, vehicle, actor)?;
            }
        }
        if !carried_two && current_world_a0 == 0x13 {
            let resource = super::ActorChildResource::Full(2);
            if let Some(actor) =
                self.create_equipment_item(&mut equipment, vehicle, resource, factory)?
            {
                self.attach_equipment_item(&mut equipment, vehicle, actor)?;
            }
        }
        let target = self
            .allocate_actor_service(crate::object_runtime::ActorServiceAllocationRequest {
                kind: crate::object_runtime::ActorServiceResourceKind::Target(
                    crate::object_runtime::NativeTargetClass::ActorFollow,
                ),
            })
            .map_err(ObjectError::from)?;
        if let Some(reference) = target {
            //0046ADA0 rereads vehicle+84 after malloc despite the earlier bind read.
            let handle = self.actor(vehicle)?.handle;
            let data = self.follow_target(FollowTargetRequest::resolve_actor(handle))?;
            self.construct_native_target(
                reference,
                crate::object_runtime::NativeTarget::ActorFollow(data),
            )?;
        }
        self.replace_camera_target(camera, target, factory.settings)?;
        self.set_camera_byte_187(camera)?;
        Ok(camera)
    }

    fn create_equipment_item(
        &mut self,
        equipment: &mut Option<PlayerEquipment<'_>>,
        vehicle: ObjectId,
        resource: super::ActorChildResource,
        factory: &mut CameraFactoryInputs<'_>,
    ) -> Result<Option<ObjectId>, PlayerAttachError> {
        let equipment = equipment
            .as_mut()
            .ok_or(PlayerAttachError::RespawnEquipmentBoundary)?;
        equipment
            .host
            .create_item(self, vehicle, resource, factory, &mut equipment.world)
            .map_err(PlayerAttachError::Equipment)
    }

    /// 00421810. Native ignores its result here.
    fn attach_equipment_item(
        &mut self,
        equipment: &mut Option<PlayerEquipment<'_>>,
        vehicle: ObjectId,
        item: ObjectId,
    ) -> Result<(), PlayerAttachError> {
        let equipment = equipment
            .as_mut()
            .ok_or(PlayerAttachError::RespawnEquipmentBoundary)?;
        equipment
            .host
            .attach_item(self, vehicle, item, equipment.world.rng)
            .map(|_| ())
            .map_err(PlayerAttachError::Equipment)
    }

    /// 0046B2B0(id, amount). In a network game (`tables.network_active`) no
    /// first pickup selects its weapon and no message follows. IDs without a class case, the healing
    /// IDs 4/0x83, Turbo charges 3, crystals 7, the counters 0x0E..0x10, the
    /// truck 2 and the weapon IDs (WEAPON_PICKUPS.md). The player's actor
    /// (0046AE60) and its +64 vehicle view must exist, else the result is
    /// false with no effect. 004750C0 grants the ID into +A0 (a dependent
    /// record's notice is a boundary), 0046CBB0 refreshes the player from the
    /// vehicle, the case runs, the ID's bit is set in +100, 0041A600
    /// reapplies upgrades, and the message request follows the record text.
    pub fn grant_player_pickup(
        &mut self,
        player: PlayerId,
        id: u16,
        amount: i16,
        tables: PickupTables<'_>,
    ) -> Result<PickupOutcome, PlayerAttachError> {
        const REJECTED: PickupOutcome = PickupOutcome {
            accepted: false,
            message: None,
            notification: None,
            nested: Vec::new(),
            full: false,
            command: None,
            sound: None,
        };
        if usize::from(id) >= inf_formats::GRANT_TEXT_IDS {
            return Err(PlayerAttachError::PickupOutOfRange(id));
        }
        let Some(actor) = self.player_actor(player)? else {
            return Ok(REJECTED);
        };
        let vehicle = match &self.actor(actor)?.extension {
            ActorExtension::Derived(derived)
                if matches!(derived.class, super::DerivedActorClass::Selector2(_)) =>
            {
                actor
            }
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector30(_)
            | ActorExtension::Selector15(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector39(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector66(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector52(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector93(_)
            | ActorExtension::BehaviorResource(_) => return Ok(REJECTED),
            ActorExtension::Derived(_) => {
                return Err(PlayerAttachError::VehicleViewBoundary(actor))
            }
        };
        let notification = self.grant_player_known(player, id, tables)?;
        self.save_vehicle_into_player(player, vehicle)?;
        // 0046B315..0046B34E: offline, a first pickup of these IDs also
        // selects its weapon below; 0046BDC0 reads +100 before the tail sets
        // the ID's bit.
        let first = !tables.network_active
            && matches!(id, 0x25 | 0x98 | 0x9b | 0xa0 | 0xa1 | 0xa3 | 0xa8..=0xad)
            && !self.player_owns_pickup(player, id)?;
        let mut nothing_needed = false;
        let mut full = false;
        // 0046B2BE: [esp+12h], a repeat pickup's silence; the weapon cases
        // that clear it announce every pickup.
        let mut quiet_repeat = true;
        let mut nested = Vec::new();
        let mut command = None;
        let mut sound = None;
        // 0046B2C3: the message prefix; the class cases below select 13Ah.
        let mut prefix_text = 0x139;
        // 0046D310 against 0053A310: a row whose count is at its limit takes
        // nothing and says so (004693C0); otherwise the player's +150 row
        // gains the signed amount.
        let stock = |arena: &mut Self,
                     row: usize,
                     nothing_needed: &mut bool,
                     full: &mut bool|
         -> Result<(), PlayerAttachError> {
            if arena.vehicle_weapon_count(vehicle, row)? == tables.upgrade_limits[row] {
                *nothing_needed = true;
                *full = true;
            } else {
                let value = &mut arena.world_player_mut(player)?.values_150[row];
                *value = value.wrapping_add(i32::from(amount));
            }
            Ok(())
        };
        match id {
            // 0046B38D: player command 0x38, silent.
            0x11 => {
                command = Some(PlayerCommandOperation::Set {
                    raw_id: 0x38,
                    silent: 1,
                })
            }
            // 0046B50C/0046B56A/0046B53B/0046B9BC: an unlimited weapon row; a
            // first pickup selects primaries 1..3.
            0x9b | 0x98 | 0xa1 | 0x26 => {
                let row = match id {
                    0x9b => 1,
                    0x98 => 2,
                    0xa1 => 3,
                    _ => 6,
                };
                self.world_player_mut(player)?.values_150[row] = -1;
                if first {
                    self.select_vehicle_primary(vehicle, row as u8)?;
                }
            }
            // 0046B599: 0x1E first grants 0xA0, then stocks row 4 as 0xA0
            // does, its own first-pickup flag clear.
            0x1e | 0xa0 => {
                if id == 0x1e {
                    nested.push(self.grant_player_pickup(player, 0xa0, 0, tables)?);
                }
                stock(self, 4, &mut nothing_needed, &mut full)?;
                if first {
                    self.select_vehicle_primary(vehicle, 4)?;
                }
            }
            // 0046B5FD: row 5, primary 5.
            0xa3 => {
                stock(self, 5, &mut nothing_needed, &mut full)?;
                if first {
                    self.select_vehicle_primary(vehicle, 5)?;
                }
            }
            // 0046B653..0046B86B: the grenade and mine rows; a first pickup
            // selects the secondary. 0xA8 keeps its repeat silence.
            0x25 | 0xa8..=0xad => {
                let row = match id {
                    0x25 => 8,
                    0xa8 => 10,
                    0xa9 => 9,
                    other => usize::from(other - 0x9f),
                };
                quiet_repeat = id == 0xa8;
                stock(self, row, &mut nothing_needed, &mut full)?;
                if first {
                    self.vehicle_weapon_behavior_mut(vehicle)?.byte_11 = row as u8;
                }
            }
            // 0046B870: row 15, which nothing selects.
            0xae => stock(self, 15, &mut nothing_needed, &mut full)?,
            // 0046B898: the 0x9B grant.
            0x9c => nested.push(self.grant_player_pickup(player, 0x9b, 0, tables)?),
            // 0046B8AB/0046B8E7: grant 0xA0 or 0xA3, then stock its row, with
            // prefix 13Ah and every pickup announced.
            0x1c | 0x1d => {
                quiet_repeat = false;
                let (granted, row) = if id == 0x1c { (0xa0, 4) } else { (0xa3, 5) };
                nested.push(self.grant_player_pickup(player, granted, 0, tables)?);
                prefix_text = 0x13a;
                stock(self, row, &mut nothing_needed, &mut full)?;
            }
            // 0046B938/0046B988: grenade packs, every listed row gaining the
            // amount with no limit test.
            0x1a | 0x1b => {
                let rows: &[usize] = if id == 0x1a {
                    &[8, 9, 14, 10, 15]
                } else {
                    &[11, 13, 12]
                };
                let values = &mut self.world_player_mut(player)?.values_150;
                for &row in rows {
                    values[row] = values[row].wrapping_add(i32::from(amount));
                }
            }
            // 0046B9C8: a network game's vehicle with a clear +5AC takes the
            // boost (004231C0); otherwise, as offline, nothing is needed.
            0x77 => {
                if tables.network_active && self.selector2_actor(vehicle)?.dwords_5ac_5b0[0] == 0 {
                    let state = self.selector2_actor_mut(vehicle)?;
                    state.dwords_5ac_5b0[0] = BOOST_MICROS;
                    state.dwords_220_224[1] = BOOST_MICROS;
                    let position = self
                        .object(vehicle)?
                        .base
                        .position_words
                        .ok_or(PlayerAttachError::NetworkPickup77Position)?;
                    sound = Some(PickupSound {
                        sound: BOOST_SOUND,
                        position,
                    });
                } else {
                    nothing_needed = true
                }
            }
            // 0046B396: the truck sets player command 0x44, announced.
            2 => {
                command = Some(PlayerCommandOperation::Set {
                    raw_id: 0x44,
                    silent: 0,
                })
            }
            // 0046B46A: Turbo charges.
            3 => {
                let state = self.world_player_mut(player)?;
                state.dword_12c = state.dword_12c.wrapping_add(i32::from(amount) as u32);
                prefix_text = 0x13a;
            }
            // 0046B4BF: a crystal. The count +118 rises and a damaged hull
            // heals by 100, up to its maximum.
            7 => {
                let state = self.world_player_mut(player)?;
                state.dword_118 = state.dword_118.wrapping_add(1);
                let actor_state = self.actor(vehicle)?;
                let maximum = actor_state
                    .maximum_health
                    .ok_or(super::DerivedActorError::MissingSelector2Health(vehicle))?;
                if actor_state.health < maximum {
                    let healed = actor_state.health.wrapping_add(100);
                    self.actor_mut(vehicle)?.health = healed.min(maximum);
                }
                prefix_text = 0x13a;
            }
            // 0046B400/0046B435/0046B48A: the counters +120/+124/+128 rise by
            // two per unit, signed, up to 30; a full counter takes nothing.
            0x0e..=0x10 => {
                let state = self.world_player_mut(player)?;
                let counter = &mut state.dwords_120_128[usize::from(id - 0x0e)];
                nothing_needed = *counter == 30;
                let raised = (*counter as i32).wrapping_add(i32::from(amount).wrapping_mul(2));
                *counter = raised.min(30) as u32;
                prefix_text = 0x13a;
            }
            _ => {}
        }
        if matches!(id, 4 | 0x83) {
            let actor_state = self.actor(vehicle)?;
            let maximum = actor_state
                .maximum_health
                .ok_or(super::DerivedActorError::MissingSelector2Health(vehicle))?;
            let health = actor_state.health;
            let deficit = u32::from(maximum).wrapping_sub(u32::from(health));
            if deficit == 0 {
                nothing_needed = true;
            } else {
                let mut healed = health.wrapping_add(amount as u16);
                if maximum < healed {
                    healed = maximum;
                }
                self.actor_mut(vehicle)?.health = healed;
                let excess = amount.wrapping_sub(deficit as i16);
                if excess > 0 {
                    self.reduce_channel_two_status(vehicle, excess as u32)?;
                }
            }
        }
        let bit = 1_u32 << (id & 31);
        let word = usize::from(id >> 5);
        let previously = self.player_owns_pickup(player, id)?;
        self.world_player_mut(player)?.bits_100[word] |= bit;
        // 0041A600 on the vehicle: 0046B110 through the first player.
        if let Some(first) = self.first_player() {
            let owner = self.resolve_handle(self.world_player(first)?.handle_d4)?;
            let state = self.world_player(first)?;
            let bits = state.bits_100;
            let mut values = state.values_150;
            self.apply_vehicle_upgrades(
                vehicle,
                owner,
                &bits,
                &mut values,
                tables.upgrade_limits,
                tables.network_active,
            )?;
            self.world_player_mut(first)?.values_150 = values;
        }
        if nothing_needed {
            return Ok(PickupOutcome {
                notification,
                nested,
                full,
                command,
                sound,
                ..REJECTED
            });
        }
        // 0046B9F5..0046BA11: a repeat pickup says "%s %s" with prefix 13Ah,
        // and only where its case cleared the silence.
        let text = tables.texts[usize::from(id)];
        let shown = text != 0 && !(previously && quiet_repeat) && !tables.network_active;
        let message = shown.then_some(PickupMessage {
            prefix_text: if previously { 0x13a } else { prefix_text },
            name_text: text,
            detail_text: (!previously).then_some(text.wrapping_add(1)),
        });
        Ok(PickupOutcome {
            accepted: true,
            message,
            notification,
            nested,
            full,
            command,
            sound,
        })
    }

    /// 004750C0 on the player's grant owner +9C. Offline, a newly granted ID
    /// whose record has text is announced; a dependent record's
    /// announcement, from the 0x95..0xAE rescan, is not ported. A network
    /// game announces nothing.
    fn grant_player_known(
        &mut self,
        player: PlayerId,
        id: u16,
        tables: PickupTables<'_>,
    ) -> Result<Option<GrantNotification>, PlayerAttachError> {
        // grant_into returns before any change for an owned ID.
        let mut mask = self.world_player(player)?.grants_9c;
        let before = mask;
        inf_formats::grant_into(&mut mask, tables.grants, u32::from(id))?;
        let mut notification = None;
        let announced = if tables.network_active {
            &[][..]
        } else {
            &mask[..]
        };
        for (word, (&after, &old)) in announced.iter().zip(&before).enumerate() {
            let mut added = after & !old;
            while added != 0 {
                let granted = word as u32 * 32 + added.trailing_zeros();
                added &= added - 1;
                let text = tables.texts.get(granted as usize).copied().unwrap_or(0);
                if text == 0 {
                    continue;
                }
                if granted != u32::from(id) {
                    return Err(PlayerAttachError::GrantNotificationBoundary(granted as u16));
                }
                notification = Some(GrantNotification { id, text });
            }
        }
        self.world_player_mut(player)?.grants_9c = mask;
        Ok(notification)
    }

    /// 0046BDC0: whether the player's +100 holds `id`; ID 0 always does.
    fn player_owns_pickup(&self, player: PlayerId, id: u16) -> Result<bool, PlayerAttachError> {
        let bits = self.world_player(player)?.bits_100;
        Ok(id == 0 || bits[usize::from(id >> 5)] & 1 << (id & 31) != 0)
    }

    /// The vehicle's +7C behavior, whose +18 entries hold the weapon rows.
    fn vehicle_weapon_behavior_mut(
        &mut self,
        vehicle: ObjectId,
    ) -> Result<&mut super::ActorBehaviorState, PlayerAttachError> {
        self.derived_actor_mut(vehicle)?
            .behavior_140
            .as_mut()
            .ok_or(PlayerAttachError::VehicleWeaponBoundary(vehicle))
    }

    /// 0046D310: the behavior's +18 entry `row`, its +00 count.
    fn vehicle_weapon_count(
        &mut self,
        vehicle: ObjectId,
        row: usize,
    ) -> Result<i32, PlayerAttachError> {
        self.vehicle_weapon_behavior_mut(vehicle)?
            .entries_18
            .get(row)
            .map(|entry| entry[0] as i32)
            .ok_or(PlayerAttachError::VehicleWeaponBoundary(vehicle))
    }

    /// 0046B527..0046B533: the primary +10 becomes `row` modulo the signed
    /// primary count +0E (IDIV).
    fn select_vehicle_primary(
        &mut self,
        vehicle: ObjectId,
        row: u8,
    ) -> Result<(), PlayerAttachError> {
        let behavior = self.vehicle_weapon_behavior_mut(vehicle)?;
        let count = i32::from(behavior.byte_0e as i8);
        if count == 0 {
            return Err(PlayerAttachError::VehicleWeaponBoundary(vehicle));
        }
        behavior.byte_10 = (i32::from(row) % count) as u8;
        Ok(())
    }

    /// `0046A8B7..0046A8D7`, 0046A890's first test when its byte argument is
    /// clear (the held camera passes the network flag; a world load passes
    /// 1 and skips it): a nonzero world limit (H14 header +24) no greater
    /// than the signed lost vehicles +11C refuses the respawn. Native then
    /// starts the mission's doom countdown (G+1D4 = 1, G+1C7 = 1) and returns
    /// 0 before touching the player; the caller owns those mission writes.
    pub fn respawn_loss_limit_reached(
        &self,
        player: PlayerId,
        limit: u32,
    ) -> Result<bool, PlayerAttachError> {
        let losses = i32::from(self.world_player(player)?.word_11c as i16);
        Ok(limit != 0 && limit as i32 <= losses)
    }

    /// 0046A890 past its loss-limit test
    /// ([`Self::respawn_loss_limit_reached`]): spawn setup, the mode-1
    /// factory call for H13 argument 0 (placement -1, no suffix), the
    /// vehicle wiring when the factory succeeds, then pickups (1, 0) and
    /// (4, 5000), the Cranial Implanter (0x33) when settings +84 has bit
    /// 0x40, and (0x32, 0) offline or the network loadout 004698F0.
    pub fn respawn_player(
        &mut self,
        player: PlayerId,
        inputs: PlayerRespawnInputs<'_, '_>,
        factory: &mut CameraFactoryInputs<'_>,
    ) -> Result<PlayerRespawn, PlayerAttachError> {
        let PlayerRespawnInputs {
            start,
            world_header_18,
            vehicle_allocation,
            world_mask_limits,
            rng,
            vehicle_table,
            current_world_a0,
            settings_84,
            pickups,
            equipment,
            network,
        } = inputs;
        let request = self.prepare_player_spawn(
            player,
            &start,
            world_header_18,
            network.map(|network| (network, &mut *rng)),
            factory,
        )?;
        let outcome = construct_world_object(
            self,
            factory.runtime,
            ObjectConstructionRequest {
                resource_argument: 0,
                context: BaseContext {
                    position_words: Some(request.position),
                    placement_index: u32::MAX,
                },
                suffix: None,
                command_allocation: None,
                network_active: network.is_some(),
                actor: Some(ActorFactoryInputs {
                    allocation: vehicle_allocation,
                    rotation_words: request.rotation,
                    local_owner: factory.local_owner,
                    first_player_grants: None,
                    grant_table: None,
                    math: factory.math,
                    derived: Some(super::DerivedActorFactoryInputs {
                        world_mask_limits,
                        rng: &mut *rng,
                    }),
                    listeners: &mut *factory.listeners,
                    listener_effects: &mut *factory.listener_effects,
                    network_world_active: network.is_some_and(|network| network.host),
                    terrain: None,
                    world_start: None,
                    camera_settings: None,
                    spawn_mode_1: true,
                    network_handle: None,
                    vehicle_table: Some(super::VehicleFactoryInputs {
                        table: vehicle_table,
                        network_settings: network.map(|network| network.settings),
                    }),
                }),
            },
        )?;
        let vehicle = match outcome {
            ObjectConstructionOutcome::Constructed(vehicle) => {
                self.finish_player_spawn(
                    player,
                    vehicle,
                    current_world_a0,
                    pickups.upgrade_limits,
                    factory,
                    &mut *rng,
                    equipment.map(|host| (host, &mut *start.state)),
                    request.marker,
                    network,
                )?;
                Some(vehicle)
            }
            ObjectConstructionOutcome::Rejected { .. } => None,
        };
        // The respawn returns its messages for the caller to deliver: a
        // grant's notice, then the pickup's own message.
        let mut messages = Vec::new();
        let grant = |arena: &mut Self, messages: &mut Vec<RespawnMessage>, id, amount| {
            let outcome = arena.grant_player_pickup(player, id, amount, pickups)?;
            if outcome.command.is_some() || outcome.full || !outcome.nested.is_empty() {
                return Err(PlayerAttachError::PickupCaseBoundary(id));
            }
            messages.extend(outcome.notification.map(RespawnMessage::Notice));
            messages.extend(outcome.message.map(RespawnMessage::Pickup));
            Ok(())
        };
        grant(self, &mut messages, 1, 0)?;
        grant(self, &mut messages, 4, 5000)?;
        // 0046ADE8..0046AE0E: settings +84 bit 0x40, which finishing the
        // campaign or the name DR FRANKENSTEIN sets, gives the Cranial
        // Implanter. 004750C0(0x33) on +9C follows; the pickup's own grant
        // has made it a no-op unless the pickup was refused.
        if settings_84 & 0x40 != 0 {
            grant(self, &mut messages, 0x33, 0)?;
            messages.extend(
                self.grant_player_known(player, 0x33, pickups)?
                    .map(RespawnMessage::Notice),
            );
        }
        match network {
            None => grant(self, &mut messages, 0x32, 0)?,
            Some(network) => self.network_loadout(player, network, pickups)?,
        }
        Ok(PlayerRespawn { vehicle, messages })
    }

    /// 004698F0, the network game's loadout: the player's +150 rows and
    /// +13C bytes clear (rows 0 and 6 unlimited), +100 and the counters
    /// +120..+12C too; each mode then grants its kit (mode 0 by the vehicle
    /// +18, mode 3 its grenades only with settings +28, and +28 = 6); then
    /// +142 takes settings +48 and the player's unit its owner, marked
    /// dirty. Its grants say nothing in a network game; a command they
    /// raise is not ported.
    fn network_loadout(
        &mut self,
        player: PlayerId,
        network: NetworkRespawn<'_>,
        pickups: PickupTables<'_>,
    ) -> Result<(), PlayerAttachError> {
        let state = self.world_player_mut(player)?;
        state.values_150 = [0; 16];
        state.values_150[0] = -1;
        state.values_150[6] = -1;
        state.bytes_13c_143 = [0; 8];
        state.bits_100 = [0; 6];
        state.dword_12c = 0;
        state.dwords_120_128 = [0; 3];
        let settings = network.settings;
        let mode = settings[0x10 / 4];
        #[rustfmt::skip]
        let kit: &[(u16, i16)] = match (mode, settings[0x18 / 4]) {
            (0, 0) => &[(0x32, 0), (0x25, 8), (3, 5), (0xa3, 8), (0xa0, 0), (0xa4, 8), (0x9b, 0), (0x9d, 0), (0x9c, 0), (0xad, 8), (0xab, 8)],
            (0, 1) => &[(0x32, 0), (0x25, 8), (3, 5), (0x98, 0), (0xa3, 8), (0xa1, 0), (0x1e, 8), (0xa9, 8), (0xae, 8)],
            (0, 2) => &[(0x32, 0), (0x25, 8), (0x26, 0), (0xa3, 8), (0x9b, 0), (0x9d, 0), (0x9c, 0), (0x98, 0), (0x27, 8), (0xaa, 8), (0xad, 8)],
            (0, 3) => &[(0x32, 0), (0x25, 8), (0x9b, 0), (0x9c, 0), (0x9d, 0), (0x9f, 0), (0xa0, 8), (0xad, 8), (0xab, 8), (0xac, 8), (0xa8, 8)],
            (0, 4) => &[(0x32, 0), (0x25, 8), (0x97, 8), (0xa3, 8), (0x9c, 0), (0x9f, 0), (0x9d, 0), (0x9e, 0), (0xa9, 8), (0xad, 8)],
            (0, _) => &[(0x32, 0), (0x25, 8)],
            (1, _) => &[(2, 0), (0xa7, 0), (0xa5, 0), (0xa6, 0), (0x9b, 0), (0x98, 0), (0xa0, 10), (0xa3, 10), (0x25, 10), (0xa1, 0)],
            (2, _) => &[(0xa0, 8), (0x25, 8), (0xab, 8), (0xa9, 8), (0xac, 8), (3, 5)],
            (3, _) if settings[0x28 / 4] != 0 => &[(0xa0, 1), (0xa9, 1), (3, 5)],
            (3, _) => &[(3, 5)],
            (4, _) => &[(3, 5), (0x13, 5)],
            _ => &[],
        };
        for &(id, amount) in kit {
            let outcome = self.grant_player_pickup(player, id, amount, pickups)?;
            if outcome.command.is_some() {
                return Err(PlayerAttachError::PickupCaseBoundary(id));
            }
        }
        if mode == 3 {
            self.world_player_mut(player)?.controls.dword_28 = 6;
        }
        if let Some(unit) = self.player_actor(player)? {
            self.world_player_mut(player)?.bytes_13c_143[6] = settings[0x48 / 4] as u8;
            self.actor_mut(unit)?.word_b4 = network.owner();
            self.owned_mut(unit)?.state.base.dirty_word |= 2;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "player_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "player_actor_lookup/native_receipts.rs"]
mod native_lookup_tests;
