//! The shown shop menu's commands. `00413950` reaches the controller's
//! current menu through `004537E0`; the shop menu's virtual +4 `00431660`
//! sounds the navigation at its factory, then `00452E20` moves the
//! selection or ends with the selected row's action:
//!
//! - `0042F620` (Back, Disconnect): `00431710` returns Facilities and Energy
//!   Distribution to the Options, otherwise fades the controller out.
//! - `0042F630` (Facilities, Energy Distribution): the state given by the
//!   row; Energy Distribution also sets the first player's command `0x27`.
//! - `0042F7E0` (a product): the purchase `00431740`.
//! - `0042F800` (a token level): AdjustPrevious sets command `0x28`;
//!   AdjustNext raises the level below 100 for one crystal.
//! - `0042F780` (Evacuate factory): the docked player's unit, through its
//!   `+64` view, takes the staff (`0040DCB0`), then the shop backs out.
//! - `0042F670` (Beam down scientist): the docked vehicle's carried
//!   scientists join the factory through its `+D0` while it has room.

use super::factory::{FactoryError, FactoryHost};
use crate::factory_shop::{product_is_valid, ShopPlayer};
use crate::menu::MenuVisibility;
use crate::menu::{MenuActionRequest, MenuBasis, MenuCommand, MenuHost, MenuInput, MenuStack};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId, PlayerId, ShownMenuOwner};

/// The shop rows' actions.
pub const SHOP_BACK_ACTION: u32 = 0x0042_f620;
pub const SHOP_SWITCH_ACTION: u32 = 0x0042_f630;
pub const SHOP_BEAM_DOWN_ACTION: u32 = 0x0042_f670;
pub const SHOP_EVACUATE_ACTION: u32 = 0x0042_f780;
pub const SHOP_BUY_ACTION: u32 = 0x0042_f7e0;
pub const SHOP_LEVEL_ACTION: u32 = 0x0042_f800;

const ERROR_SOUND: u32 = 4;
const CONFIRM_SOUND: u32 = 5;
const BUILD_SOUND: u32 = 0x17;
const BUILD_SOUND_PITCH: u32 = 0x4000;
const FULL: u32 = 0x10000;
/// "Insufficient Staff", "You do not have enough resources to make this
/// item" and "That weapon is already fully stocked".
const INSUFFICIENT_STAFF_TEXT: u16 = 0x300;
const NO_RESOURCES_TEXT: u16 = 0x303;
const FULLY_STOCKED_TEXT: u16 = 0x304;
const NO_RESOURCES_COMMAND: u32 = 7;
/// "No scientist to beam in".
const NO_SCIENTIST_TEXT: u16 = 0x311;
const DISTRIBUTION_COMMAND: u32 = 0x27;
const LEVEL_DOWN_COMMAND: u32 = 0x28;
const LEVEL_LIMIT: i8 = 100;

/// The positional sound `00431660` plays at the factory: 2 for the
/// selection, 3 for AdjustPrevious and 5 for AdjustNext.
fn navigation_sound(command: MenuCommand) -> Option<u32> {
    match command {
        MenuCommand::Previous | MenuCommand::Next => Some(2),
        MenuCommand::AdjustPrevious => Some(3),
        MenuCommand::AdjustNext => Some(5),
        _ => None,
    }
}

fn menu_command(code: u32) -> Option<MenuCommand> {
    Some(match code {
        0 => MenuCommand::Previous,
        1 => MenuCommand::Next,
        2 => MenuCommand::AdjustPrevious,
        3 => MenuCommand::AdjustNext,
        4 => MenuCommand::Activate,
        5 => MenuCommand::SelectLast,
        6 => MenuCommand::Auxiliary6,
        7 => MenuCommand::Auxiliary7,
        _ => return None,
    })
}

/// `00452E20` ends with the row action; the command records it so the
/// action runs with the arena once the menu's borrow ends.
#[derive(Default)]
struct ActionRecorder {
    request: Option<MenuActionRequest>,
}

impl MenuHost for ActionRecorder {
    type Error = String;
    fn action(&mut self, request: MenuActionRequest, _: &mut MenuStack) -> Result<(), String> {
        self.request = Some(request);
        Ok(())
    }
    fn rotate_menu_basis(&mut self, _: &mut MenuBasis, _: i32) -> Result<(), String> {
        Err("a menu command does not rotate the basis".into())
    }
}

impl ObjectArena {
    /// `004537E0(code)` on the player's controller, for its current menu.
    pub fn run_shown_menu_command<H: FactoryHost>(
        &mut self,
        player: PlayerId,
        code: u32,
        visibility: MenuVisibility,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let menu = self
            .player_shown_menu(player)
            .ok_or(FactoryError::MissingShownMenu(player))?;
        let Some(owner) = menu.current_owner() else {
            // 004050E0 found no current menu.
            return Ok(());
        };
        let command = menu_command(code).ok_or(FactoryError::MenuCode(code))?;
        let ShownMenuOwner::Shop { factory } = owner else {
            // A plain menu's +4 is the base 00452E20; the database and the
            // hint list step their entry first. A row action goes to the
            // session with the owner.
            if !self
                .step_shown_list(player, code)
                .map_err(|error| FactoryError::Menu(error.to_string()))?
            {
                return Ok(());
            }
            let mut recorder = ActionRecorder::default();
            self.player_shown_menu_mut(player)
                .ok_or(FactoryError::MissingShownMenu(player))?
                .stack
                .command(
                    MenuInput {
                        command,
                        visibility,
                        context: 0,
                    },
                    &mut recorder,
                )
                .map_err(|error| FactoryError::Menu(format!("{error:?}")))?;
            if let Some(request) = recorder.request {
                self.players
                    .menu_actions
                    .push(crate::object_runtime::ShownMenuAction {
                        player,
                        owner,
                        request,
                    });
            }
            return Ok(());
        };
        if let Some(sound) = navigation_sound(command) {
            let position = self
                .object(factory)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(factory))?;
            host.sound(self, sound, position, FULL, FULL)
                .map_err(|error| FactoryError::Host {
                    address: 0x0043_16b5,
                    error,
                })?;
        }
        let mut recorder = ActionRecorder::default();
        self.player_shown_menu_mut(player)
            .ok_or(FactoryError::MissingShownMenu(player))?
            .stack
            .command(
                MenuInput {
                    command,
                    visibility,
                    context: 0,
                },
                &mut recorder,
            )
            .map_err(|error| FactoryError::Menu(format!("{error:?}")))?;
        match recorder.request {
            Some(request) => self.run_shop_action(player, factory, request, host),
            None => Ok(()),
        }
    }

    fn run_shop_action<H: FactoryHost>(
        &mut self,
        player: PlayerId,
        factory: ObjectId,
        request: MenuActionRequest,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        use MenuCommand::{Activate, AdjustNext, AdjustPrevious};
        let argument = request.arguments[0];
        match (request.callback_address, request.command) {
            (SHOP_BACK_ACTION, Activate) => self.shop_back(player, factory, host),
            (SHOP_SWITCH_ACTION, Activate) => {
                if argument == 2 {
                    // 0042F655: 00488AA0's first player.
                    let first = self
                        .first_player()
                        .ok_or(FactoryError::MissingFirstPlayer)?;
                    host.command(self, first, DISTRIBUTION_COMMAND, 0)
                        .map_err(|error| FactoryError::Host {
                            address: 0x0042_f655,
                            error,
                        })?;
                }
                self.set_factory_state(factory, argument, false, host)
            }
            (SHOP_BUY_ACTION, Activate) => self.purchase_product(factory, argument, host),
            (SHOP_LEVEL_ACTION, AdjustPrevious | AdjustNext) => {
                self.adjust_token_level(factory, request.command, argument, host)
            }
            (SHOP_EVACUATE_ACTION, Activate) => self.evacuate_shop(player, factory, host),
            (SHOP_BEAM_DOWN_ACTION, Activate) => self.beam_down_shop(factory, host),
            (
                SHOP_BACK_ACTION
                | SHOP_SWITCH_ACTION
                | SHOP_BUY_ACTION
                | SHOP_LEVEL_ACTION
                | SHOP_BEAM_DOWN_ACTION
                | SHOP_EVACUATE_ACTION,
                _,
            ) => Ok(()),
            (address, _) => Err(FactoryError::ShopAction(address)),
        }
    }

    /// `0042F780`, Evacuate factory: the docked player's unit (`0046AE60`)
    /// takes the whole staff `+140` through its `+64` view, which only the
    /// vehicle answers; then the shop backs out (`00431710`).
    fn evacuate_shop<H: FactoryHost>(
        &mut self,
        player: PlayerId,
        factory: ObjectId,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let vehicle = match self.factory_player(factory, host.process())? {
            Some(docked) => self
                .player_actor(docked)?
                .filter(|&unit| self.selector2_actor(unit).is_ok()),
            None => None,
        };
        let count = self.selector15(factory)?.intermediate.word_140;
        self.evacuate_factory(factory, count, vehicle, host)?;
        self.shop_back(player, factory, host)
    }

    /// `0042F670`, Beam down scientist: the docked actor (`+188`), through
    /// its `+64` view (the vehicle answers itself), offers its carried
    /// actors to the factory's `+D0` (`0042F170`) from its last occupied
    /// slot back, while the factory has room. Each carried actor goes
    /// through its `+5C` view, which only selector 12 answers. A refusal
    /// tells the docked player "No scientist to beam in" (`0x311`) and ends
    /// it; only a remote factory or scientist refuses.
    fn beam_down_shop<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let handle = self.selector15(factory)?.handle_188;
        let Some(docked) = self.resolve_actor_handle(handle)? else {
            return Ok(());
        };
        if self.selector2_actor(docked).is_err() {
            return Ok(());
        }
        let null = host
            .process()
            .process_null_handle
            .ok_or(FactoryError::MissingProcessNull(factory))?;
        let mut index = {
            let handles = &self.selector2_actor(docked)?.handles_1a4.handles;
            let free = handles.iter().filter(|&&handle| handle == null).count();
            handles.len() as i32 - free as i32 - 1
        };
        loop {
            let staff = &self.selector15(factory)?.intermediate;
            if staff.word_140 >= staff.word_13c || index < 0 {
                return Ok(());
            }
            let handle = self.selector2_actor(docked)?.handles_1a4.handles[index as usize];
            if let Some(carried) = self.resolve_actor_handle(handle)? {
                if self.selector12_actor(carried).is_ok()
                    && !self.factory_take_scientist(factory, carried, host)?
                {
                    if let Some(player) = self.factory_player(factory, host.process())? {
                        host.text(self, player, NO_SCIENTIST_TEXT)
                            .map_err(|error| FactoryError::Host {
                                address: 0x0042_f76c,
                                error,
                            })?;
                        return Ok(());
                    }
                }
            }
            index -= 1;
        }
    }

    /// `00431710`.
    fn shop_back<H: FactoryHost>(
        &mut self,
        player: PlayerId,
        factory: ObjectId,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        if matches!(self.selector15(factory)?.word_160, 1 | 2) {
            return self.set_factory_state(factory, 0, false, host);
        }
        // The menu's controller +54, virtual +4 (004151C0).
        self.player_shown_menu_mut(player)
            .ok_or(FactoryError::MissingShownMenu(player))?
            .stack
            .set_active(false);
        Ok(())
    }

    /// `0046C6D0`: every count must cover its cost (signed); then all four
    /// are paid.
    fn pay_player<E>(
        &mut self,
        player: PlayerId,
        costs: [i32; 4],
    ) -> Result<bool, FactoryError<E>> {
        let state = self.world_player_mut(player)?;
        let have = [
            state.dword_118 as i32,
            state.dwords_120_128[0] as i32,
            state.dwords_120_128[1] as i32,
            state.dwords_120_128[2] as i32,
        ];
        if have.iter().zip(costs).any(|(&count, cost)| count < cost) {
            return Ok(false);
        }
        state.dwords_120_128[2] = have[3].wrapping_sub(costs[3]) as u32;
        state.dwords_120_128[0] = have[1].wrapping_sub(costs[1]) as u32;
        state.dword_118 = have[0].wrapping_sub(costs[0]) as u32;
        state.dwords_120_128[1] = have[2].wrapping_sub(costs[2]) as u32;
        Ok(true)
    }

    /// `00431740(index)`.
    fn purchase_product<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        index: u32,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let kind = self.selector15(factory)?.word_15c;
        let product = *host
            .shop_tables()
            .map_err(|error| FactoryError::Host {
                address: 0x0043_1740,
                error,
            })?
            .product(kind, index)?;
        let player = self.factory_player(factory, host.process())?;
        // 0043176F..00431783: the docked vehicle's +64 view, whose result
        // is unused; a vehicle that no longer resolves would fault.
        let handle = self.selector15(factory)?.handle_188;
        self.resolve_actor_handle(handle)
            .map_err(|_| ObjectError::MissingActorRegistry)?
            .ok_or(FactoryError::Unwritten {
                factory,
                field: "a docked vehicle for the purchase",
            })?;
        let Some(player) = player else {
            return Ok(());
        };
        let position = self
            .object(factory)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(factory))?;
        let sound = |arena: &ObjectArena, host: &mut H, sound, pitch, address| {
            host.sound(arena, sound, position, FULL, pitch)
                .map_err(|error| FactoryError::Host { address, error })
        };
        let text = |arena: &mut ObjectArena, host: &mut H, text, address| {
            host.text(arena, player, text)
                .map_err(|error| FactoryError::Host { address, error })
        };
        // +CC(0), 0040E090: the scientists the factory lacks.
        let staff = &self.selector15(factory)?.intermediate;
        if staff.word_13c.wrapping_sub(staff.word_140) != 0 {
            sound(self, host, ERROR_SOUND, FULL, 0x0043_1919)?;
            return text(self, host, INSUFFICIENT_STAFF_TEXT, 0x0043_1802);
        }
        let entries = self.first_unit_weapon_entries()?;
        let owner = self.world_player(player)?;
        let valid = product_is_valid(
            &product,
            &ShopPlayer {
                crystals_118: owner.dword_118 as i32,
                tokens_120: owner.dwords_120_128.map(|count| count as i32),
                upgrades_100: &owner.bits_100,
                weapon_entries: entries.as_deref(),
            },
        )?;
        if !valid {
            return text(self, host, FULLY_STOCKED_TEXT, 0x0043_1802);
        }
        if self.pay_player(player, product.costs.map(i32::from))? {
            sound(self, host, CONFIRM_SOUND, FULL, 0x0043_18fb)?;
            self.start_factory_production(factory, &product, false)?;
            self.set_factory_state(factory, 3, false, host)?;
            return sound(self, host, BUILD_SOUND, BUILD_SOUND_PITCH, 0x0043_192f);
        }
        sound(self, host, ERROR_SOUND, FULL, 0x0043_1889)?;
        text(self, host, NO_RESOURCES_TEXT, 0x0043_18d6)?;
        host.command(self, player, NO_RESOURCES_COMMAND, 0)
            .map_err(|error| FactoryError::Host {
                address: 0x0043_18e0,
                error,
            })
    }

    /// `0042F800`: the docked player's token level `+13C + index`.
    fn adjust_token_level<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        command: MenuCommand,
        index: u32,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let Some(player) = self.factory_player(factory, host.process())? else {
            return Ok(());
        };
        let slot = usize::try_from(index)
            .ok()
            .filter(|&slot| slot < 8)
            .ok_or(FactoryError::TokenLevel(index))?;
        let level = self.world_player(player)?.bytes_13c_143[slot] as i8;
        match command {
            MenuCommand::AdjustPrevious => host
                .command(self, player, LEVEL_DOWN_COMMAND, 0)
                .map_err(|error| FactoryError::Host {
                    address: 0x0042_f85d,
                    error,
                }),
            MenuCommand::AdjustNext if level < LEVEL_LIMIT => {
                if self.pay_player(player, [1, 0, 0, 0])? {
                    self.world_player_mut(player)?.bytes_13c_143[slot] = (level + 1) as u8;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
