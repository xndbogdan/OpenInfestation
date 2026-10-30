//! The computer's Cheats Menu (Tab, row 6). `004146B0` shows a plain menu
//! over the row's definition `0052F9D8`, whose eight rows `00413410` opens
//! from the settings' cheat word when Tab builds the computer, and the
//! rows' actions run here with the session's owners. The bytes they keep,
//! `00546A04` and `0052F880`, outlive worlds in [`ComputerCheats`].

use inf_formats::native_basis::NativeBasisMath;
use inf_formats::{
    ByteSpan, FrontendExecutable, FrontendExecutableError, FrontendMenuItem, GrantTable,
    GRANT_TEXT_IDS,
};

use super::{
    particles::damage::{world_retirement_host, BehaviorFactoryLoan, WorldDamageOwners},
    world_actor_factory::{
        RequiredListenerEffects, WorldActorChildHost, WorldActorFactoryRequest,
        WorldFactoryInitialization, WorldFactoryPosition,
    },
    world_actor_update::CarryEffects,
    world_owners::current_grant_text,
    PortPlatform, PortSession,
};
use crate::{
    frontend_settings::CHEATS_MENU_BITS,
    grant_text::PickupTextData,
    menu::{
        MenuActionRequest, MenuBasis, MenuCommand, MenuData, MenuDerivedData, MenuLayouts,
        MenuState,
    },
    object_runtime::{
        ActorChildResource, ActorComponentQueryMode, ActorComponentQueryRequest, ComputerCheats,
        GrantOwner, LiveGrantContext, LiveGrantOperation, LiveGrantRequest, ObjectArena,
        ObjectClass, ObjectList, ObjectNode, PickupDelivery, PickupTables, PlayerCommandContext,
        PlayerId, PlayerMessageOperation, PlayerMessageRequest, ShownMenuOwner,
        VehicleCarryContext,
    },
    process_particles::{
        births::{ParticleBirthInputs, ParticleBirthRequest},
        factories::ParticleFactoryProcessState,
    },
    profile_name_format::ProfileNameClassification,
    session::{SessionPhase, SessionState},
    shown_menu_draw::{TELEPORT_ROW, WORLD_ROW},
    world_completion::{
        complete_world, WorldCompletionInputs, WorldCompletionOutcome, WorldCompletionRequest,
    },
    ResidentRuntime,
};

/// `004146B0`: the Cheats Menu, `00452310` over the row's argument 1 shown
/// through `004137D0(0, menu)`, whatever the code.
pub(super) const CHEATS_MENU: u32 = 0x0041_46b0;
/// The rows of `0052F888` after its title.
pub(super) const SKIP_TO: u32 = 0x0041_48b0;
pub(super) const SHOW_FULL_DATABASE: u32 = 0x0041_4ad0;
pub(super) const FULL_RESEARCH_TREE: u32 = 0x0041_4ae0;
pub(super) const FULL_SHIP_UPGRADES: u32 = 0x0041_4ba0;
pub(super) const INVULNERABILITY: u32 = 0x0041_4b80;
pub(super) const COMPLETE_LEVEL: u32 = 0x0041_4b00;
pub(super) const SCAVANGE_LEVEL: u32 = 0x0041_4b20;
pub(super) const FILL_CRYSTAL_TANKS: u32 = 0x0041_4b40;

/// The port's own rows (2026-10-23: a cheat option in the cheats menu,
/// 'Teleport to world', accessible only if all cheats are enabled in game;
/// Enter on it shows the whole worlds list, including World 1): a ninth row while all eight of
/// retail's are open, and its list of the worlds 0 to 24. Their actions are
/// addresses no retail code has; the rows draw in `shown_menu_draw`.
pub(super) const TELEPORT_TO: u32 = 0xf000_0002;
pub(super) const TELEPORT_WORLD: u32 = 0xf000_0004;
const TELEPORT_WORLDS: u32 = 25;
/// The list shows as many rows at once as the Cheats Menu's row group.
const TELEPORT_VISIBLE_ROWS: u8 = 9;

/// The rows' argument pointers: Skip to's world and the database byte.
pub(super) const SKIP_WORLD_CELL: u32 = 0x0052_f880;
pub(super) const FULL_DATABASE_CELL: u32 = 0x0054_6a04;

/// `00413410`: the cheat-word bit that opens each row of `0052F888`. With
/// no bit outside `0xC0` it leaves the rows and the selection alone, and
/// the computer's Cheats row stays shut.
const ROW_BITS: [(usize, u32); 8] = [
    (1, 1),
    (2, 4),
    (3, 4),
    (4, 4),
    (5, 2),
    (6, 1),
    (7, 1),
    (8, 0x20),
];
const MENU_SHUT_BITS: u32 = 0xc0;

/// The grant IDs `00475320` resolves: 0 to 0xAE.
const GRANT_IDS: u16 = 0xaf;
/// Invulnerability, which Get Full Ship Upgrades skips and its own row
/// grants (`0046B2B0(0x29, 0)`).
const INVULNERABILITY_ID: u16 = 0x29;
/// `0046B2B0`'s amounts: the upgrades' and Skip to's, the sweep's.
const UPGRADE_AMOUNT: i16 = 8;
const SWEEP_AMOUNT: i16 = 4;
/// Fill resource crystal tanks and Skip to: 20 of each crystal counter.
const CRYSTALS: [u16; 3] = [0x0e, 0x0f, 0x10];
const CRYSTAL_AMOUNT: i16 = 0x14;

/// `004148B0`'s worlds: Left and Right step through 2 to 24, wrapping.
const SKIP_FIRST_WORLD: u32 = 2;
const SKIP_LAST_WORLD: u32 = 0x18;
/// Its tables: grant IDs by stage, each stage closed by `0xDC + stage`
/// and the list by a byte from `0xFA`; worlds (below 100), each stage's
/// opened by `100 + stage`, the list ended by a byte from 200; each stage's
/// tokens; and the token levels' offsets from player `+13C`, ended by -1.
pub(super) const SKIP_GRANTS: u32 = 0x0052_fd84;
pub(super) const SKIP_STAGES: u32 = 0x0052_fdcc;
pub(super) const SKIP_TOKENS: u32 = 0x0052_fdf8;
const SKIP_LEVELS: u32 = 0x0052_fe0c;
/// The longest table any of those reads.
const SKIP_TABLE_BYTES: usize = 0x100;
/// The scientists (H13 resource 2) that fill the hold, 0x200 along X and Z
/// from the vehicle.
const SCIENTIST_RESOURCE: u32 = 2;
const SKIP_SPAWN_OFFSET: u16 = 0x200;

/// `00488AC0`'s classes: the pickups (resource 9), the actors whose `+7A`
/// only teaches (resources 0x12 and 0x13), the aliens' side `0x12` it
/// retires, and resource 0xF, which it spares.
const PICKUP_RESOURCE: u16 = 9;
const TEACHING_RESOURCES: [u16; 2] = [0x12, 0x13];
const ALIEN_TEAM: u32 = 0x12;
const SPARED_RESOURCE: u16 = 0x0f;
/// `00489A90`'s selector for a swept pickup's flash.
const PICKUP_FLASH: u32 = 3;

const NO_WORLD: &str = "no world holds the menu";

/// The bytes at process start: `00546A04` is zero-filled data, `0052F880`
/// holds 2.
pub(super) fn computer_cheats_startup(
    executable: FrontendExecutable<'_>,
) -> Result<ComputerCheats, FrontendExecutableError> {
    let world = executable.mapped(SKIP_WORLD_CELL, 4)?.bytes;
    Ok(ComputerCheats {
        full_database: false,
        skip_world: u32::from_le_bytes(world.try_into().expect("four mapped bytes")),
    })
}

/// Whether `address` is one of the Cheats Menu's actions.
pub(super) fn is_cheat_action(address: u32) -> bool {
    matches!(
        address,
        CHEATS_MENU
            | SKIP_TO
            | SHOW_FULL_DATABASE
            | FULL_RESEARCH_TREE
            | FULL_SHIP_UPGRADES
            | INVULNERABILITY
            | COMPLETE_LEVEL
            | SCAVANGE_LEVEL
            | FILL_CRYSTAL_TANKS
            | TELEPORT_TO
            | TELEPORT_WORLD
    )
}

/// A port row: no retail record, so no address or span of its own.
fn port_row(draw: u32, action: u32, arguments: [u32; 2]) -> FrontendMenuItem {
    FrontendMenuItem {
        virtual_address: 0,
        span: ByteSpan::new(0, 0),
        flags: 1,
        draw_callback: draw,
        draw_arguments: arguments,
        action_callback: action,
        action_arguments: arguments,
        row_height: 0,
        age: 0,
    }
}

/// The Cheats Menu `data` with the port's Teleport to world row after
/// retail's eight: retail sized their row group for nine. A copy, so the
/// authored menu stays retail's. Its action takes the definition, whose
/// title and layouts the list reuses.
fn with_teleport_row(data: &MenuData, definition: u32) -> MenuData {
    let records = data.records();
    let mut items = records.items.to_vec();
    items.push(port_row(TELEPORT_ROW, TELEPORT_TO, [0, definition]));
    MenuData::derived(MenuDerivedData {
        constructor: 0,
        vtable: 0,
        items: Some(items),
        layouts: Some(data.layouts()),
        initial_selection: records.initial_selection,
        flags: records.flags,
    })
}

/// The worlds 0 to 24 under the Cheats Menu's own title, scrolling in its
/// row group. A row's second argument is 1 where the port names the world
/// (Pelagia, with `prototype_world`).
fn world_list(data: &MenuData, prototype_world: bool) -> Result<MenuData, String> {
    let records = data.records();
    let source = data
        .layout_source()
        .ok_or("the Cheats Menu has no layouts to list the worlds in")?;
    let mut layouts = records.layouts.to_vec();
    let rows = layouts
        .get_mut(1)
        .ok_or("the Cheats Menu has no row group to list the worlds in")?;
    rows.item_count = TELEPORT_WORLDS as u8;
    rows.visible_rows = TELEPORT_VISIBLE_ROWS;
    let title = *records
        .items
        .first()
        .ok_or("the Cheats Menu has no title")?;
    let mut items = vec![title];
    items.extend((0..TELEPORT_WORLDS).map(|world| {
        let named = crate::pelagia::world_name(world, prototype_world).is_some();
        port_row(WORLD_ROW, TELEPORT_WORLD, [world, u32::from(named)])
    }));
    Ok(MenuData::derived(MenuDerivedData {
        constructor: 0,
        vtable: 0,
        items: Some(items),
        layouts: Some(MenuLayouts::new(source.address, source.terminator, layouts)),
        initial_selection: 1,
        flags: records.flags,
    }))
}

/// `004148B0`'s stage for `world`: the stage whose list names it, else the
/// last stage opened.
pub(super) fn skip_stage(world: u32, stages: &[u8]) -> u32 {
    let mut stage = 1;
    for &byte in stages {
        match byte {
            0..=99 if u32::from(byte) == world => break,
            0..=99 => {}
            100..=199 => stage = u32::from(byte) - 100,
            _ => break,
        }
    }
    stage
}

/// `004148B0`'s grants for `stage`: every ID of the stages up to it.
pub(super) fn skip_grants(stage: u32, grants: &[u8]) -> Vec<u16> {
    let mut ids = Vec::new();
    for &byte in grants {
        match byte {
            0..=0xdb => ids.push(u16::from(byte)),
            0xdc..=0xf9 if u32::from(byte - 0xdc) == stage => break,
            0xdc..=0xf9 => {}
            _ => break,
        }
    }
    ids
}

/// `004148B0`'s token level: the first `stage` stages' tokens shared over
/// the seven levels, at most 100.
pub(super) fn skip_token_level(stage: u32, tokens: &[u8]) -> i32 {
    let sum: i32 = tokens
        .iter()
        .take(stage as usize)
        .map(|&token| i32::from(token))
        .sum();
    (sum / 7).min(100)
}

/// The executable's grant tables, which `0046B2B0` and its delivery read.
pub(super) struct GrantTables {
    grants: GrantTable,
    texts: [u16; GRANT_TEXT_IDS],
    limits: [i32; 16],
    flags: [u32; GRANT_TEXT_IDS],
}

impl GrantTables {
    pub(super) fn parse(executable: FrontendExecutable<'_>) -> Result<Self, String> {
        Ok(Self {
            grants: inf_formats::parse_grant_table(executable).map_err(|e| e.to_string())?,
            texts: inf_formats::parse_grant_texts(executable).map_err(|e| e.to_string())?,
            limits: inf_formats::parse_vehicle_upgrade_limits(executable)
                .map_err(|e| e.to_string())?,
            flags: inf_formats::parse_grant_flags(executable).map_err(|e| e.to_string())?,
        })
    }
}

/// The owners a grant borrows for the length of one call.
#[derive(Clone, Copy)]
pub(super) struct GrantLoan<'a> {
    pub executable: FrontendExecutable<'a>,
    pub residency: &'a ResidentRuntime,
    pub process: &'a ParticleFactoryProcessState,
    pub text: &'a PickupTextData<'a>,
    /// The animation clock `005471C0`, for a full row's notice.
    pub clock: u32,
}

/// `004750C0` on the player's grants `+9C`, with its notices.
fn know(
    arena: &mut ObjectArena,
    player: PlayerId,
    id: u16,
    loan: GrantLoan<'_>,
) -> Result<(), String> {
    arena
        .mutate_live_grants(
            LiveGrantRequest {
                target: GrantOwner::Player9c(player),
                operation: LiveGrantOperation::SetRaw { id: u32::from(id) },
            },
            &LiveGrantContext {
                process: Some(loan.process),
                executable: Some(loan.executable),
                resident: Some(loan.residency),
                text_data: Some(&loan.text.common),
            },
        )
        .map_err(|error| format!("004750C0({id:#x}): {error}"))
}

/// `0046B2B0(id, amount)` on the player, then its notice, commands and
/// message. Returns its answer: whether the player took it.
pub(super) fn pickup(
    arena: &mut ObjectArena,
    player: PlayerId,
    id: u16,
    amount: i16,
    tables: &GrantTables,
    loan: GrantLoan<'_>,
) -> Result<bool, String> {
    let outcome = arena
        .grant_player_pickup(
            player,
            id,
            amount,
            PickupTables {
                grants: &tables.grants,
                texts: &tables.texts,
                upgrade_limits: &tables.limits,
                network_active: loan.process.network_flag != 0,
            },
        )
        .map_err(|error| format!("0046B2B0({id:#x}, {amount}): {error}"))?;
    let accepted = outcome.accepted;
    let commands = PlayerCommandContext {
        process: Some(loan.process),
        executable: Some(loan.executable),
        resident: Some(loan.residency),
    };
    arena
        .deliver_pickup_outcome(
            player,
            outcome,
            &PickupDelivery {
                online: loan.process.network_flag != 0,
                flags: &tables.flags,
                residency: loan.residency,
                text: Some(loan.text),
                commands: &commands,
                clock: loan.clock,
            },
        )
        .map_err(|error| format!("0046B2B0({id:#x}) delivery: {error}"))?;
    Ok(accepted)
}

/// The pickup text the live profile-name classification gives.
pub(super) fn pickup_text(classification: &ProfileNameClassification) -> PickupTextData<'_> {
    PickupTextData {
        common: current_grant_text(classification),
        hyphen_high: classification
            .high_bytes
            .as_ref()
            .and_then(|bytes| bytes[b'-' as usize]),
    }
}

/// The node after `node` in the list that holds it now, as a native walk
/// reads the node's next link; None after a list's last node.
fn node_successor(arena: &ObjectArena, node: ObjectNode) -> Result<Option<ObjectNode>, String> {
    for role in [
        ObjectList::Flag0Primary,
        ObjectList::Flag0Secondary,
        ObjectList::Parking,
        ObjectList::Flag1Primary,
        ObjectList::Flag1Secondary,
        ObjectList::Disposal,
    ] {
        let list = arena.list(role);
        if let Some(at) = list.iter().position(|&entry| entry == node) {
            return Ok(list.get(at + 1).copied());
        }
    }
    Err(format!("the sweep's node {node:?} is in no list"))
}

/// A byte table of the executable, up to its terminator.
pub(super) fn table_bytes(
    executable: FrontendExecutable<'_>,
    address: u32,
) -> Result<Vec<u8>, String> {
    executable
        .mapped(address, SKIP_TABLE_BYTES)
        .map(|span| span.bytes.to_vec())
        .map_err(|error| format!("table {address:#010x}: {error}"))
}

/// The token levels' offsets, DWORDs ended by -1.
fn level_offsets(executable: FrontendExecutable<'_>) -> Result<Vec<usize>, String> {
    let bytes = table_bytes(executable, SKIP_LEVELS)?;
    let mut offsets = Vec::new();
    for word in bytes.chunks_exact(4) {
        let value = i32::from_le_bytes(word.try_into().expect("four bytes"));
        if value == -1 {
            return Ok(offsets);
        }
        offsets.push(value as usize);
    }
    Err(format!(
        "{SKIP_LEVELS:#010x} has no -1 within {SKIP_TABLE_BYTES} bytes"
    ))
}

impl<P: PortPlatform> PortSession<P> {
    /// One of the Cheats Menu's actions ([`is_cheat_action`]).
    pub(super) fn run_cheat_action(
        &mut self,
        player: PlayerId,
        request: MenuActionRequest,
        state: &mut SessionState,
    ) -> Result<(), String> {
        let select = request.command == MenuCommand::Activate;
        match request.callback_address {
            CHEATS_MENU => self.show_cheats_menu(player, request.arguments[1]),
            SKIP_TO => {
                if request.arguments[1] != SKIP_WORLD_CELL {
                    return Err(format!(
                        "Skip to's world is {:#010x}, not 0052F880",
                        request.arguments[1]
                    ));
                }
                self.skip_to(player, request.command, state)
            }
            // 00414AD0 flips the byte whatever the code.
            SHOW_FULL_DATABASE => {
                if request.arguments[1] != FULL_DATABASE_CELL {
                    return Err(format!(
                        "Show Full Database's byte is {:#010x}, not 00546A04",
                        request.arguments[1]
                    ));
                }
                self.computer_cheats.full_database = !self.computer_cheats.full_database;
                self.lend_computer_cheats();
                Ok(())
            }
            // 00414AE0 checks no code either.
            FULL_RESEARCH_TREE => self.full_research_tree(player),
            FULL_SHIP_UPGRADES if select => self.full_ship_upgrades(player),
            INVULNERABILITY if select => self.cheat_pickups(player, &[(INVULNERABILITY_ID, 0)]),
            COMPLETE_LEVEL if select => self.sweep_level(false, state),
            SCAVANGE_LEVEL if select => self.sweep_level(true, state),
            TELEPORT_TO if select => {
                let data = self
                    .catalog
                    .menu_data(request.arguments[1])
                    .map_err(|error| error.to_string())?;
                let list = world_list(&data, self.platform.prototype_world())?;
                let menu = MenuState::new(list, MenuBasis::INITIAL);
                self.show_computer_menu(player, menu, ShownMenuOwner::Plain)
            }
            TELEPORT_WORLD if select => self.teleport(player, request.arguments[0], state),
            FILL_CRYSTAL_TANKS if select => {
                // 00414B48: 00488AA0's first player, read through unchecked.
                let first = self
                    .world
                    .as_ref()
                    .ok_or(NO_WORLD)?
                    .arena
                    .first_player()
                    .ok_or("Fill resource crystal tanks finds no first player (00488AA0)")?;
                self.cheat_pickups(first, &CRYSTALS.map(|id| (id, CRYSTAL_AMOUNT)))
            }
            _ => Ok(()),
        }
    }

    /// The world's message environment takes the session's cheat bytes.
    fn lend_computer_cheats(&mut self) {
        let cheats = self.computer_cheats;
        if let Some(environment) = self
            .world
            .as_mut()
            .and_then(|world| world.arena.player_message_environment_mut())
        {
            environment.cheats = cheats;
        }
    }

    /// `004137D0(0, menu)` from the computer's rows, its unit broadcast
    /// included, which runs at once as it does inside the native call.
    pub(super) fn show_computer_menu(
        &mut self,
        player: PlayerId,
        menu: MenuState,
        owner: ShownMenuOwner,
    ) -> Result<(), String> {
        let executable = FrontendExecutable::parse(self.catalog.assets().executable_bytes())
            .map_err(|error| error.to_string())?;
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        world
            .arena
            .show_player_menu_from_unit(player, 0, menu, owner)
            .map_err(|error| error.to_string())?;
        super::world_actor_update::run_key_broadcasts(
            &mut world.arena,
            &mut world.actor_updates.shop_stop,
            &mut world.draw_omissions_logged,
            &self.factory_process,
            executable,
        );
        Ok(())
    }

    /// `004146B0`, with the rows `00413410` set at Tab: each open row's
    /// flags keep bit 0, a shut row's are 0, and the selection becomes the
    /// first open row (flags exactly 1).
    fn show_cheats_menu(&mut self, player: PlayerId, definition: u32) -> Result<(), String> {
        let data = self
            .catalog
            .menu_data(definition)
            .map_err(|error| error.to_string())?;
        let cheats = self.platform.cheats_menu_word();
        if cheats & !MENU_SHUT_BITS != 0 {
            let mut records = data.records_mut();
            for (row, bit) in ROW_BITS {
                let item = records
                    .items
                    .get_mut(row)
                    .ok_or_else(|| format!("{definition:#010x} has no row {row}"))?;
                item.flags = if cheats & bit != 0 { item.flags | 1 } else { 0 };
            }
            if let Some((row, _)) = ROW_BITS
                .iter()
                .find(|&&(row, _)| records.items[row].flags == 1)
            {
                records.initial_selection = *row as i32;
            }
        }
        let data = if cheats & CHEATS_MENU_BITS == CHEATS_MENU_BITS {
            with_teleport_row(&data, definition)
        } else {
            data
        };
        let menu = MenuState::new(data, MenuBasis::INITIAL);
        self.show_computer_menu(player, menu, ShownMenuOwner::Plain)
    }

    /// The port's Teleport to world: `world_number` loads at its first
    /// start, as the frontend loads a world (phase 3). It is no completion:
    /// the world left is not marked done and no results page shows (world
    /// 1's selection is the campaign's end, its credits), and none of Skip
    /// to's grants are given; the vehicle keeps what it has. It lands at
    /// the world's own start: the save pad's checkpoint (`+ED`) is cleared
    /// as a world's completion clears it (`0046AF70`), which otherwise put
    /// the vehicle at the world left's save pad's position (it retained the position held in the
    /// current world, which is dangerous; 2026-10-26).
    fn teleport(
        &mut self,
        player: PlayerId,
        world_number: u32,
        state: &mut SessionState,
    ) -> Result<(), String> {
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        world.mission.save.mission_selection = world_number;
        world.mission.save.owner_a4 = 0;
        world
            .arena
            .clear_player_completion_checkpoint(player)
            .map_err(|error| error.to_string())?;
        eprintln!("Cheats Menu: Teleport to world {world_number}");
        let _ = state.request_phase(SessionPhase::Phase3);
        world
            .arena
            .hide_player_message(player)
            .map_err(|error| error.to_string())
    }

    /// `00414AE0`: every grant ID into the player's `+9C`, as the
    /// message's `+14` holds it.
    fn full_research_tree(&mut self, player: PlayerId) -> Result<(), String> {
        let executable = FrontendExecutable::parse(self.catalog.assets().executable_bytes())
            .map_err(|error| error.to_string())?;
        let text = pickup_text(&self.profile_name_classification);
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let loan = GrantLoan {
            executable,
            residency: &self.residency,
            process: &self.factory_process,
            text: &text,
            clock: self.process.animation().value(),
        };
        for id in 0..GRANT_IDS {
            know(&mut world.arena, player, id, loan)?;
        }
        Ok(())
    }

    /// `00414BA0`: the research tree, then every ID but Invulnerability
    /// through `0046B2B0(id, 8)` on the message's player.
    fn full_ship_upgrades(&mut self, player: PlayerId) -> Result<(), String> {
        self.full_research_tree(player)?;
        let grants: Vec<_> = (0..GRANT_IDS)
            .filter(|&id| id != INVULNERABILITY_ID)
            .map(|id| (id, UPGRADE_AMOUNT))
            .collect();
        self.cheat_pickups(player, &grants)
    }

    /// `0046B2B0` on `player` for each pair in order.
    fn cheat_pickups(&mut self, player: PlayerId, grants: &[(u16, i16)]) -> Result<(), String> {
        let executable = FrontendExecutable::parse(self.catalog.assets().executable_bytes())
            .map_err(|error| error.to_string())?;
        let tables = GrantTables::parse(executable)?;
        let text = pickup_text(&self.profile_name_classification);
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let loan = GrantLoan {
            executable,
            residency: &self.residency,
            process: &self.factory_process,
            text: &text,
            clock: self.process.animation().value(),
        };
        for &(id, amount) in grants {
            pickup(&mut world.arena, player, id, amount, &tables, loan)?;
        }
        Ok(())
    }

    /// The database's Select under the full database cheat (`00414549`):
    /// `004750C0(entry)` on the message's player's `+9C`, then
    /// `0046B2B0(entry, 8)`.
    pub(super) fn grant_database_entry(
        &mut self,
        player: PlayerId,
        entry: u16,
    ) -> Result<(), String> {
        let executable = FrontendExecutable::parse(self.catalog.assets().executable_bytes())
            .map_err(|error| error.to_string())?;
        let tables = GrantTables::parse(executable)?;
        let text = pickup_text(&self.profile_name_classification);
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let loan = GrantLoan {
            executable,
            residency: &self.residency,
            process: &self.factory_process,
            text: &text,
            clock: self.process.animation().value(),
        };
        know(&mut world.arena, player, entry, loan)?;
        pickup(
            &mut world.arena,
            player,
            entry,
            UPGRADE_AMOUNT,
            &tables,
            loan,
        )
        .map(drop)
    }

    /// `004148B0`, Skip to. Left and Right step its world; Select gives the
    /// first player what the campaign would have by that world, with the
    /// message silent meanwhile, and ends this world into it.
    fn skip_to(
        &mut self,
        player: PlayerId,
        command: MenuCommand,
        state: &mut SessionState,
    ) -> Result<(), String> {
        let cheats = &mut self.computer_cheats;
        match command {
            MenuCommand::AdjustPrevious => {
                cheats.skip_world = cheats.skip_world.wrapping_sub(1);
                if (cheats.skip_world as i32) < SKIP_FIRST_WORLD as i32 {
                    cheats.skip_world = SKIP_LAST_WORLD;
                }
            }
            MenuCommand::AdjustNext => {
                cheats.skip_world = cheats.skip_world.wrapping_add(1);
                if (cheats.skip_world as i32) > SKIP_LAST_WORLD as i32 {
                    cheats.skip_world = SKIP_FIRST_WORLD;
                }
            }
            MenuCommand::Activate => return self.skip_to_world(player, state),
            _ => return Ok(()),
        }
        self.lend_computer_cheats();
        Ok(())
    }

    fn skip_to_world(&mut self, player: PlayerId, state: &mut SessionState) -> Result<(), String> {
        let world_number = self.computer_cheats.skip_world;
        let executable = FrontendExecutable::parse(self.catalog.assets().executable_bytes())
            .map_err(|error| error.to_string())?;
        let tables = GrantTables::parse(executable)?;
        let stage = skip_stage(world_number, &table_bytes(executable, SKIP_STAGES)?);
        let grants = skip_grants(stage, &table_bytes(executable, SKIP_GRANTS)?);
        let level = skip_token_level(stage, &table_bytes(executable, SKIP_TOKENS)?);
        let offsets = level_offsets(executable)?;
        let text = pickup_text(&self.profile_name_classification);
        let clock = self.process.animation().value();
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        // 004148E1: 00488AA0's first player, used without a NULL test.
        let first = world
            .arena
            .first_player()
            .ok_or("Skip to finds no first player (00488AA0)")?;
        let recipient = world
            .arena
            .player_message(player)
            .map_err(|error| error.to_string())?
            .identity();
        // 004148E8: the message's +1, which silences it.
        world
            .arena
            .apply_player_message(PlayerMessageRequest {
                recipient,
                operation: PlayerMessageOperation::Suppress,
            })
            .map_err(|error| error.to_string())?;
        {
            let loan = GrantLoan {
                executable,
                residency: &self.residency,
                process: &self.factory_process,
                text: &text,
                clock,
            };
            for &id in &grants {
                know(&mut world.arena, first, id, loan)?;
                pickup(&mut world.arena, first, id, UPGRADE_AMOUNT, &tables, loan)?;
            }
        }
        for &offset in &offsets {
            world
                .arena
                .set_player_token_level(first, offset, level as u8)
                .map_err(|error| error.to_string())?;
        }
        // 004149B7: 0046B030, the unit of the player's +D4, read through
        // unchecked; its +64 view gives the hold.
        let handle = world
            .arena
            .world_player(first)
            .map_err(|error| error.to_string())?
            .handle_d4;
        let unit = world
            .arena
            .resolve_actor_handle(handle)
            .map_err(|error| error.to_string())?
            .ok_or("Skip to finds no unit at the player's +D4 (0046B030)")?;
        if world
            .arena
            .actor_is_vehicle(unit)
            .map_err(|error| error.to_string())?
        {
            let null = self
                .factory_process
                .process_null_handle
                .ok_or("Skip to's hold needs the process NULL")?;
            let free = |arena: &ObjectArena| -> Result<usize, String> {
                Ok(arena
                    .selector2_actor(unit)
                    .map_err(|error| error.to_string())?
                    .handles_1a4
                    .handles
                    .iter()
                    .filter(|&&handle| handle == null)
                    .count())
            };
            let sine = &self.catalog.assets().sine_table().values;
            let math = NativeBasisMath::new(*sine);
            let camera_settings = self.platform.world_tick_inputs().camera_settings;
            let grant_text = current_grant_text(&self.profile_name_classification);
            let phase = u32::from(state.current_phase().id());
            let mut left = free(&world.arena)?;
            while left != 0 {
                let mut position = world
                    .arena
                    .object(unit)
                    .map_err(|error| error.to_string())?
                    .base
                    .position_words
                    .ok_or("Skip to's vehicle has no position")?;
                position[0] = position[0].wrapping_add(SKIP_SPAWN_OFFSET);
                position[2] = position[2].wrapping_add(SKIP_SPAWN_OFFSET);
                // 00414A12: 00460310(&position, NULL, -1, 0, NULL), ECX 2.
                let created = WorldActorChildHost {
                    residency: &self.residency,
                    executable,
                    process: &self.factory_process,
                    terrain: &world.terrain,
                    world_flags: world.world_flags,
                    clock,
                    sine,
                    math: &math,
                    camera_settings: Some(camera_settings),
                    world_start: Some(&mut world.world_start),
                    listeners: &mut world.listeners,
                    listener_effects: &mut RequiredListenerEffects,
                }
                .create_object(
                    &mut world.arena,
                    WorldActorFactoryRequest {
                        resource: ActorChildResource::Full(SCIENTIST_RESOURCE),
                        position: WorldFactoryPosition::Local(position),
                        spawn_mode_1: false,
                        network_handle: None,
                        initialization: WorldFactoryInitialization::Bare,
                    },
                    &mut world.rng,
                )
                .map_err(|error| format!("00460310 scientist: {error:?}"))?;
                if let Some(scientist) = created.filter(|&object| world.arena.actor(object).is_ok())
                {
                    world
                        .arena
                        .attach_vehicle_item(
                            unit,
                            scientist,
                            VehicleCarryContext {
                                process: &self.factory_process,
                                executable,
                                resident: &self.residency,
                                text_data: Some(&grant_text),
                            },
                            &mut CarryEffects {
                                particles: &mut self.particles,
                                sounds: &mut self.sounds,
                                residency: &self.residency,
                                executable,
                                process: &self.factory_process,
                                phase,
                            },
                            &mut world.rng,
                        )
                        .map_err(|error| format!("00421810 scientist: {error}"))?;
                }
                let now = free(&world.arena)?;
                if now >= left {
                    // Retail keeps building scientists while a slot is free.
                    return Err(format!(
                        "Skip to's scientist took no slot ({now} free); retail loops forever"
                    ));
                }
                left = now;
            }
        }
        {
            let loan = GrantLoan {
                executable,
                residency: &self.residency,
                process: &self.factory_process,
                text: &text,
                clock,
            };
            for id in CRYSTALS {
                pickup(&mut world.arena, first, id, CRYSTAL_AMOUNT, &tables, loan)?;
            }
        }
        world
            .arena
            .apply_player_message(PlayerMessageRequest {
                recipient,
                operation: PlayerMessageOperation::Enable,
            })
            .map_err(|error| error.to_string())?;
        // 00414A71: 00487560(world, 1, 0), then 004133C0 on the message.
        let outcome = complete_world(
            &mut world.mission,
            &mut world.arena,
            WorldCompletionRequest {
                next_selection: world_number,
                start_ordinal: 1,
                actor: None,
            },
            WorldCompletionInputs {
                network_active: self.factory_process.network_flag != 0,
                process: Some(&self.factory_process),
            },
            &mut self.sounds,
        )
        .map_err(|error| format!("00487560({world_number}, 1, 0): {error:?}"))?;
        eprintln!("Cheats Menu: Skip to world {world_number} (stage {stage}): {outcome:?}");
        if let WorldCompletionOutcome::Accepted { request_phase } = outcome {
            let _ = state.request_phase(request_phase);
        }
        world
            .arena
            .hide_player_message(player)
            .map_err(|error| error.to_string())
    }

    /// `00488AC0`, Complete Level (CL 0) and Scavange Level (CL 1), for the
    /// first player. It walks the world's flag-0 list (`[G+10]`) in order,
    /// reading each node's next before its object:
    ///
    /// - an actor: under Scavange Level its `+7A` is known and picked up
    ///   (`0046B2B0(+7A, 4)`); resources 0x12 and 0x13 teach theirs; a
    ///   factory (`+50`) gives its grants (`00475350`) and each ID it holds
    ///   (`0046B2B0(id, 4)`); one of the aliens' side (`+B4` 0x12) that is
    ///   no factory nor resource 0xF is retired (`0047CE90`); otherwise a
    ///   scientist (`+5C`) teaches and gives its `+7A` and boards the
    ///   player's vehicle (`00421810`);
    /// - a pickup (resource 9) is taken (`0046B0F0`) and, once taken,
    ///   marked collected (`0048E980`), flashed (`00489A90(3)`) and
    ///   disposed.
    fn sweep_level(&mut self, scavenge: bool, state: &SessionState) -> Result<(), String> {
        let executable = FrontendExecutable::parse(self.catalog.assets().executable_bytes())
            .map_err(|error| error.to_string())?;
        let tables = GrantTables::parse(executable)?;
        let text = pickup_text(&self.profile_name_classification);
        let grant_text = current_grant_text(&self.profile_name_classification);
        let clock = self.process.animation().value();
        let phase = u32::from(state.current_phase().id());
        let world = self.world.as_mut().ok_or(NO_WORLD)?;
        let Some(first) = world.arena.first_player() else {
            return Ok(());
        };
        let mut node = world.arena.list(ObjectList::Flag0Primary).first().copied();
        while let Some(current) = node {
            // 00488AF8..00488B07: the next node is read before the object,
            // and the walk goes on from it in whatever list the object's
            // handling has moved it to.
            node = node_successor(&world.arena, current)?;
            let object = current.object;
            let loan = GrantLoan {
                executable,
                residency: &self.residency,
                process: &self.factory_process,
                text: &text,
                clock,
            };
            let resource = world
                .arena
                .object(object)
                .map_err(|error| error.to_string())?
                .base
                .resource_word;
            if !matches!(
                world.arena.object(object).map_err(|e| e.to_string())?.class,
                ObjectClass::Actor(_)
            ) {
                if resource != PICKUP_RESOURCE {
                    continue;
                }
                let pickup_state = world
                    .arena
                    .timed_pickup(object)
                    .map_err(|error| error.to_string())?;
                let kind = pickup_state
                    .kind_44
                    .ok_or("a swept pickup's kind +44 is unwritten")?;
                let (amount, index) = (pickup_state.value_46 as i16, pickup_state.save_index_48);
                if !pickup(&mut world.arena, first, kind, amount, &tables, loan)? {
                    continue;
                }
                // 0048E980 on G+1B0: -1 marks nothing.
                if index != u32::MAX {
                    let word = (world.mission.current_world_a0 as i32)
                        .wrapping_mul(4)
                        .wrapping_add(index as i32 / 32);
                    *usize::try_from(word)
                        .ok()
                        .and_then(|word| world.mission.save.owner_b0.get_mut(word))
                        .ok_or("a swept pickup's mission bit lies outside +B0")? |=
                        1 << (index & 31);
                }
                let position = world
                    .arena
                    .object(object)
                    .map_err(|error| error.to_string())?
                    .base
                    .position_words
                    .ok_or("a swept pickup has no position")?;
                self.particles
                    .birth_pickup_flash(
                        ParticleBirthRequest {
                            particle_type: PICKUP_FLASH,
                            position,
                            velocity: None,
                            flags: 0,
                            inputs: ParticleBirthInputs {
                                network_active: self.factory_process.network_flag != 0,
                                phase: Some(phase),
                                process: &self.factory_process,
                            },
                        },
                        executable,
                        &mut world.rng,
                    )
                    .map_err(|error| format!("00489A90(3): {error:?}"))?;
                world
                    .arena
                    .dispose(object)
                    .map_err(|error| error.to_string())?;
                continue;
            }
            let grant = |arena: &ObjectArena| -> Result<u16, String> {
                Ok(arena.actor(object).map_err(|e| e.to_string())?.word_7a)
            };
            if scavenge {
                let id = grant(&world.arena)?;
                if id != 0 {
                    know(&mut world.arena, first, id, loan)?;
                    pickup(&mut world.arena, first, id, SWEEP_AMOUNT, &tables, loan)?;
                }
            }
            if TEACHING_RESOURCES.contains(&resource) {
                let id = grant(&world.arena)?;
                if id != 0 {
                    know(&mut world.arena, first, id, loan)?;
                }
            }
            let factory = world
                .arena
                .query_actor_component(ActorComponentQueryRequest {
                    actor: object,
                    mode: ActorComponentQueryMode::Self50,
                })
                .map_err(|error| error.to_string())?;
            if let Some(factory) = factory {
                world
                    .arena
                    .mutate_live_grants(
                        LiveGrantRequest {
                            target: GrantOwner::Player9c(first),
                            operation: LiveGrantOperation::Merge {
                                source: GrantOwner::Selector15Factory198(factory),
                            },
                        },
                        &LiveGrantContext {
                            process: Some(&self.factory_process),
                            executable: Some(executable),
                            resident: Some(&self.residency),
                            text_data: Some(&grant_text),
                        },
                    )
                    .map_err(|error| format!("00475350: {error}"))?;
                for id in 0..GRANT_IDS {
                    let mask = world
                        .arena
                        .selector15(factory)
                        .map_err(|error| error.to_string())?
                        .grants_198
                        .mask_04;
                    if inf_formats::grant_is_owned(&mask, u32::from(id))
                        .map_err(|error| error.to_string())?
                    {
                        pickup(&mut world.arena, first, id, SWEEP_AMOUNT, &tables, loan)?;
                    }
                }
            }
            let team = world
                .arena
                .actor(object)
                .map_err(|error| error.to_string())?
                .word_b4;
            if team == ALIEN_TEAM && factory.is_none() && resource != SPARED_RESOURCE {
                let mut owners = WorldDamageOwners {
                    grant_text: Some(&grant_text),
                    camera_settings: Some(self.platform.world_tick_inputs().camera_settings),
                    flash: Some(&mut world.flash),
                    mission: &mut world.mission,
                    tasks: &mut self.tasks,
                    terrain: &mut world.terrain,
                    particles: &mut self.particles,
                    sounds: &mut self.sounds,
                    furniture: &mut self.furniture,
                    residency: &self.residency,
                    executable,
                    factory_process: &mut self.factory_process,
                    behavior_tables: &mut self.actor_behavior_tables,
                    animation_clock: &self.process,
                    session: state,
                    particle_load_factor: world.fade.performance.particle_load_factor,
                    behavior_factory: Some(BehaviorFactoryLoan {
                        listeners: &mut world.listeners,
                        sine: &self.catalog.assets().sine_table().values,
                        world_flags: world.world_flags,
                    }),
                    map: None,
                };
                let mut host = world_retirement_host(&mut owners, &mut world.rng);
                let null = host.owners.factory_process.process_null_handle;
                world
                    .arena
                    .retire_actor(object, null, &mut host)
                    .map_err(|error| format!("0047CE90: {error:?}"))?;
                continue;
            }
            if world.arena.selector12_actor(object).is_err() {
                continue;
            }
            let id = grant(&world.arena)?;
            if id != 0 {
                know(&mut world.arena, first, id, loan)?;
                pickup(&mut world.arena, first, id, SWEEP_AMOUNT, &tables, loan)?;
            }
            // 00488C10: 0046AE60, read through unchecked.
            let unit = world
                .arena
                .player_actor(first)
                .map_err(|error| error.to_string())?
                .ok_or("the sweep finds no unit for the first player (0046AE60)")?;
            if !world
                .arena
                .actor_is_vehicle(unit)
                .map_err(|error| error.to_string())?
            {
                continue;
            }
            world
                .arena
                .attach_vehicle_item(
                    unit,
                    object,
                    VehicleCarryContext {
                        process: &self.factory_process,
                        executable,
                        resident: &self.residency,
                        text_data: Some(&grant_text),
                    },
                    &mut CarryEffects {
                        particles: &mut self.particles,
                        sounds: &mut self.sounds,
                        residency: &self.residency,
                        executable,
                        process: &self.factory_process,
                        phase,
                    },
                    &mut world.rng,
                )
                .map_err(|error| format!("00421810: {error}"))?;
        }
        Ok(())
    }
}
