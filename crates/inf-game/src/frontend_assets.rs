//! Original executable-owned frontend definitions, separate from mutable menus.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::frontend_bindings::ScreenAnalogDescriptors;
use crate::menu::MenuData;
use crate::profile_menu::{ProfileMenuTemplates, ProfileTemplateError};

use inf_formats::{
    parse_frontend_attract_selection_table, parse_frontend_camera_presets,
    parse_frontend_input_tables, parse_frontend_menu, parse_frontend_scene_models,
    parse_frontend_sine_table, parse_key_name_table, parse_phase12_input_tables,
    parse_phase3_input_tables, parse_phase5_input_tables, parse_phase7_input_tables,
    parse_phase8_input_tables, parse_startup_input_tables, parse_text_entry_input_tables,
    parse_title_input_tables, ByteSpan, FrontendAttractSelectionTable, FrontendCameraPreset,
    FrontendData, FrontendError, FrontendExecutable, FrontendExecutableError, FrontendMenu,
    FrontendSceneModel, FrontendSineTable, InputTableError, KeyNameError, KeyNameTable,
    ScreenInputTables, FRONTEND_ATTRACT_SELECTION_ADDRESS, FRONTEND_CAMERA_PRESETS_ADDRESS,
    FRONTEND_ROOT_MENU_ADDRESS, FRONTEND_SCENE_MODELS_ADDRESS, FRONTEND_SINE_TABLE_ADDRESS,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FrontendAssetError {
    #[error("cannot read frontend data from {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error(transparent)]
    Table(#[from] FrontendError),
    #[error(transparent)]
    InputTable(#[from] InputTableError),
    #[error(transparent)]
    KeyName(#[from] KeyNameError),
    #[error(transparent)]
    ProfileTemplate(#[from] ProfileTemplateError),
    #[error(transparent)]
    NetworkTemplate(#[from] crate::network_menu::NetworkTemplateError),
    #[error("frontend model count is negative: {0}")]
    NegativeModelCount(i32),
    #[error(transparent)]
    PlayerAnalog(#[from] crate::player_input_source::PlayerInputSourceError),
    #[error("menu {address:#010x} aliases mutable records owned by definition {existing:#010x}; cross-definition table sharing is not implemented")]
    SharedMenuRecords { address: u32, existing: u32 },
    #[error("data address {0:#010x} is outside the executable's data section")]
    DataAddress(u32),
}

/// Process-lifetime authored menu records. Screen destruction does not discard
/// runtime row heights, and reopening a definition reuses the same tables.
/// Definitions that name the same layout chain share its records, as the
/// Options submenus share `00538D58`; any other aliasing of mutable arrays
/// would need a separate table owner, so it is rejected rather than copied.
#[derive(Debug)]
pub struct FrontendMenuCatalog {
    assets: FrontendAssets,
    menus: BTreeMap<u32, MenuData>,
    profile_templates: Option<ProfileMenuTemplates>,
    briefing_templates: Option<crate::briefing_controls::BriefingMenuTemplates>,
    network_templates: Option<crate::network_menu::NetworkMenuTemplates>,
    /// The Video menus carry the port's Vulkan rows.
    modern_rows: bool,
}

impl FrontendMenuCatalog {
    pub fn new(assets: FrontendAssets) -> Self {
        Self {
            assets,
            menus: BTreeMap::new(),
            profile_templates: None,
            briefing_templates: None,
            network_templates: None,
            modern_rows: false,
        }
    }

    /// Whether the Video menus (the frontend's and the in-world Options'),
    /// when first built, carry the port's Vulkan rows.
    pub fn set_modern_rows(&mut self, on: bool) {
        self.modern_rows = on;
    }

    /// `0045F9C0`'s template rows.
    pub fn network_templates(
        &mut self,
    ) -> Result<crate::network_menu::NetworkMenuTemplates, FrontendAssetError> {
        if self.network_templates.is_none() {
            self.network_templates = Some(crate::network_menu::NetworkMenuTemplates::parse(
                self.assets.executable_bytes(),
            )?);
        }
        Ok(self
            .network_templates
            .as_ref()
            .expect("parsed network templates")
            .clone())
    }

    pub fn assets(&self) -> &FrontendAssets {
        &self.assets
    }

    /// Briefing's detail/list scratch rows have explicit alias ownership in
    /// their screen controller, so they cannot use the disjoint-menu catalog.
    pub fn briefing_templates(
        &mut self,
    ) -> Result<crate::briefing_controls::BriefingMenuTemplates, FrontendAssetError> {
        if self.briefing_templates.is_none() {
            self.briefing_templates = Some(crate::briefing_controls::BriefingMenuTemplates::parse(
                &self.assets,
            )?);
        }
        Ok(self
            .briefing_templates
            .as_ref()
            .expect("parsed briefing templates")
            .clone())
    }

    /// Instances copy row templates and retain process-shared native layouts.
    pub fn profile_templates(&mut self) -> Result<ProfileMenuTemplates, FrontendAssetError> {
        if self.profile_templates.is_none() {
            self.profile_templates =
                Some(ProfileMenuTemplates::parse(self.assets.executable_bytes())?);
        }
        Ok(self
            .profile_templates
            .as_ref()
            .expect("loaded templates")
            .clone())
    }

    pub fn menu_data(&mut self, address: u32) -> Result<MenuData, FrontendAssetError> {
        if let Some(menu) = self.menus.get(&address) {
            return Ok(menu.clone());
        }
        let mut definition = self.assets.menu_definition(address)?;
        let video = [
            crate::frontend_video::VIDEO_MENU,
            crate::frontend_video::IN_WORLD_VIDEO_MENU,
        ]
        .contains(&address);
        let sound = [
            crate::frontend_sound::SOUND_MENU,
            crate::frontend_sound::IN_WORLD_SOUND_MENU,
        ]
        .contains(&address);
        let modern = self.modern_rows && (video || sound);
        if modern && video {
            crate::frontend_video::append_modern_rows(&mut definition);
        }
        if modern && sound {
            crate::frontend_sound::append_ambience_row(&mut definition);
        }
        let spans = |menu: &FrontendMenu| {
            menu.items
                .iter()
                .map(|row| row.span)
                .chain(menu.layouts.iter().map(|layout| layout.span))
                .collect::<Vec<_>>()
        };
        // A layout chain another definition already binds is shared whole.
        // The Video and Sound menus lay their port rows out on a chain of
        // their own: the in-world one's (00538D58) is also the in-world
        // Options' and Controls', which count their own rows.
        let shared_layouts = if modern {
            Some(crate::menu::MenuLayouts::own(&definition.layouts))
        } else {
            (definition.layouts_address != 0)
                .then(|| {
                    self.menus.values().map(MenuData::layouts).find(|layouts| {
                        layouts.source().map(|source| source.address)
                            == Some(definition.layouts_address)
                    })
                })
                .flatten()
        };
        // So is an item array: the network host menus 00538650 and 00538668
        // both bind 00538510.
        let shared_items = self.menus.values().find_map(|menu| {
            let records = menu.records();
            (definition.items_address != 0
                && records.items.len() == definition.items.len()
                && records.items.first().map(|row| row.virtual_address)
                    == Some(definition.items_address))
            .then(|| menu.items())
        });
        let incoming: Vec<_> = match (shared_layouts.is_some(), shared_items.is_some()) {
            (true, true) => Vec::new(),
            (true, false) => definition.items.iter().map(|row| row.span).collect(),
            (false, true) => definition
                .layouts
                .iter()
                .map(|layout| layout.span)
                .collect(),
            (false, false) => spans(&definition),
        };
        for (&existing, menu) in &self.menus {
            let records = menu.records();
            let occupied = records
                .items
                .iter()
                .map(|row| row.span)
                .chain(records.layouts.iter().map(|layout| layout.span))
                .collect::<Vec<_>>();
            if incoming
                .iter()
                .any(|a| occupied.iter().any(|b| a.start < b.end && b.start < a.end))
            {
                return Err(FrontendAssetError::SharedMenuRecords { address, existing });
            }
        }
        // A layout may count more rows than the menu has items (the in-game
        // Options 00539390 has six under a thirteen-row layout); its census
        // reads the flags of the records that follow the array.
        let counted: usize = definition
            .layouts
            .iter()
            .map(|layout| usize::from(layout.item_count))
            .sum();
        let adjacent = (definition.items.len()..counted)
            .map_while(|index| {
                self.assets
                    .data_dword(definition.items_address.wrapping_add(0x20 * index as u32))
            })
            .collect();
        let menu =
            MenuData::with_shared_records(definition, shared_layouts, shared_items, adjacent);
        self.menus.insert(address, menu.clone());
        Ok(menu)
    }
}

/// Immutable authored definitions. Live menus must share their mutable table
/// owner rather than decoding a fresh snapshot every time they are reopened.
#[derive(Debug)]
pub struct FrontendAssets {
    executable: Box<[u8]>,
    data_file_span: ByteSpan,
    data_address: u32,
    root_menu: FrontendMenu,
    models: Vec<FrontendSceneModel>,
    cameras: Vec<FrontendCameraPreset>,
    sine_table: FrontendSineTable,
    attract_selections: FrontendAttractSelectionTable,
    /// 004838A0 frontend tables.
    frontend_input_tables: ScreenInputTables,
    /// 00459060 phase 9/10/11 text-entry tables.
    text_entry_input_tables: ScreenInputTables,
    /// 00404620 phase-1 title tables (digital only; every analog cell is zero).
    title_input_tables: ScreenInputTables,
    /// 00432500 startup (language) tables: keyboard, joystick buttons and rows.
    startup_input_tables: ScreenInputTables,
    /// 00444BD0 key configuration tables: keyboard and joystick buttons.
    phase12_input_tables: ScreenInputTables,
    /// 0040A420 phase-3 briefing tables (digital roots plus the shared
    /// analog cells; unwritten analog slot 7 stays empty).
    phase3_input_tables: ScreenInputTables,
    /// 00451020 phase-7 map tables (digital only; its analog slots read the
    /// shared cells, through the screen's own consumers).
    phase5_input_tables: ScreenInputTables,
    phase7_input_tables: ScreenInputTables,
    /// 0042AB00 phase-8 results tables (digital only; every analog cell
    /// is zero and no shared cell is read).
    phase8_input_tables: ScreenInputTables,
    /// Gameplay's authored roots, with the menu axes of the shared cells.
    gameplay_input_tables: ScreenInputTables,
    /// The gameplay +0xFC sub-object's authored roots (00534570 family) and
    /// its analog tables, the vehicle controls the first player's input
    /// references share.
    gameplay_sub_input_tables: ScreenInputTables,
    /// 00535E54 names read by the 00445840 key translator.
    key_names: KeyNameTable,
    screen_analog_descriptors: ScreenAnalogDescriptors,
    position_scale: i32,
    motion_scale: i32,
}

impl FrontendAssets {
    pub fn open(data_root: &Path) -> Result<Self, FrontendAssetError> {
        let path = crate::retail_path(data_root, "Infest.exe");
        let bytes = fs::read(&path).map_err(|source| FrontendAssetError::Read { path, source })?;
        Self::from_executable(bytes)
    }

    pub fn from_executable(bytes: Vec<u8>) -> Result<Self, FrontendAssetError> {
        let mapped = FrontendExecutable::parse(&bytes)?;
        let source = mapped.data();
        let root_menu = parse_frontend_menu(source, FRONTEND_ROOT_MENU_ADDRESS)?;
        let count = mapped.word(0x0053_bfb0)? as i32;
        if count < 0 {
            return Err(FrontendAssetError::NegativeModelCount(count));
        }
        let models =
            parse_frontend_scene_models(source, FRONTEND_SCENE_MODELS_ADDRESS, count as u32)?;
        // 00484AC0's authored preset consumers address records 0 through 9.
        let cameras = parse_frontend_camera_presets(source, FRONTEND_CAMERA_PRESETS_ADDRESS, 10)?;
        let sine_table = parse_frontend_sine_table(source, FRONTEND_SINE_TABLE_ADDRESS)?;
        let attract_selections =
            parse_frontend_attract_selection_table(source, FRONTEND_ATTRACT_SELECTION_ADDRESS)?;
        let frontend_input_tables = parse_frontend_input_tables(mapped)?;
        let text_entry_input_tables = parse_text_entry_input_tables(mapped)?;
        let title_input_tables = parse_title_input_tables(mapped)?;
        let startup_input_tables = parse_startup_input_tables(mapped)?;
        let phase12_input_tables = parse_phase12_input_tables(mapped)?;
        let phase3_input_tables = parse_phase3_input_tables(mapped)?;
        let phase5_input_tables = parse_phase5_input_tables(mapped)?;
        let phase7_input_tables = parse_phase7_input_tables(mapped)?;
        let phase8_input_tables = parse_phase8_input_tables(mapped)?;
        let gameplay_input_tables = inf_formats::parse_screen_input_tables(
            mapped,
            inf_formats::GAMEPLAY_DIGITAL_TABLE_ADDRESSES,
            inf_formats::GAMEPLAY_ANALOG_TABLE_POINTERS,
        )?;
        // The sub-object's analog descriptors lie in .data's zero fill.
        crate::player_input_source::validate_zero_tail(&bytes)?;
        let gameplay_sub_input_tables = inf_formats::parse_screen_input_tables_direct(
            mapped,
            inf_formats::GAMEPLAY_SUB_DIGITAL_TABLE_ADDRESSES,
            inf_formats::GAMEPLAY_SUB_ANALOG_TABLE_ADDRESSES,
            &crate::player_input_source::ANALOG_IDENTITIES,
        )?;
        let key_names = parse_key_name_table(mapped)?;
        let screen_analog_descriptors = ScreenAnalogDescriptors::from_executable(mapped)?;
        let position_scale = mapped.word(0x0053_bfb4)? as i32;
        let motion_scale = mapped.word(0x0053_bfb8)? as i32;
        let data_file_span = mapped.file_span();
        let data_address = source.virtual_address;
        Ok(Self {
            executable: bytes.into_boxed_slice(),
            data_file_span,
            data_address,
            root_menu,
            models,
            cameras,
            sine_table,
            attract_selections,
            frontend_input_tables,
            text_entry_input_tables,
            title_input_tables,
            startup_input_tables,
            phase12_input_tables,
            phase3_input_tables,
            phase5_input_tables,
            phase7_input_tables,
            phase8_input_tables,
            gameplay_input_tables,
            gameplay_sub_input_tables,
            key_names,
            screen_analog_descriptors,
            position_scale,
            motion_scale,
        })
    }

    pub fn executable_bytes(&self) -> &[u8] {
        &self.executable
    }
    pub fn root_menu(&self) -> &FrontendMenu {
        &self.root_menu
    }
    pub fn scene_models(&self) -> &[FrontendSceneModel] {
        &self.models
    }
    pub fn camera_presets(&self) -> &[FrontendCameraPreset] {
        &self.cameras
    }
    pub fn sine_table(&self) -> &FrontendSineTable {
        &self.sine_table
    }
    pub fn attract_selections(&self) -> &FrontendAttractSelectionTable {
        &self.attract_selections
    }
    pub fn frontend_input_tables(&self) -> &ScreenInputTables {
        &self.frontend_input_tables
    }
    pub fn text_entry_input_tables(&self) -> &ScreenInputTables {
        &self.text_entry_input_tables
    }
    pub fn startup_input_tables(&self) -> &ScreenInputTables {
        &self.startup_input_tables
    }

    pub fn phase12_input_tables(&self) -> &ScreenInputTables {
        &self.phase12_input_tables
    }

    pub fn title_input_tables(&self) -> &ScreenInputTables {
        &self.title_input_tables
    }
    pub fn phase3_input_tables(&self) -> &ScreenInputTables {
        &self.phase3_input_tables
    }
    /// 00443190's tables: the in-world menu over the live world.
    pub fn phase5_input_tables(&self) -> &ScreenInputTables {
        &self.phase5_input_tables
    }

    pub fn phase7_input_tables(&self) -> &ScreenInputTables {
        &self.phase7_input_tables
    }
    pub fn phase8_input_tables(&self) -> &ScreenInputTables {
        &self.phase8_input_tables
    }
    pub fn gameplay_input_tables(&self) -> &ScreenInputTables {
        &self.gameplay_input_tables
    }
    pub fn gameplay_sub_input_tables(&self) -> &ScreenInputTables {
        &self.gameplay_sub_input_tables
    }
    pub fn key_names(&self) -> &KeyNameTable {
        &self.key_names
    }
    pub fn screen_analog_descriptors(&self) -> ScreenAnalogDescriptors {
        self.screen_analog_descriptors
    }
    pub fn position_scale(&self) -> i32 {
        self.position_scale
    }
    pub fn motion_scale(&self) -> i32 {
        self.motion_scale
    }

    /// A DWORD of the executable's data section at `address`, if inside it.
    pub fn data_dword(&self, address: u32) -> Option<u32> {
        let data = &self.executable[self.data_file_span.start..self.data_file_span.end];
        let offset = address.checked_sub(self.data_address)? as usize;
        let bytes = data.get(offset..offset.checked_add(4)?)?;
        Some(u32::from_le_bytes(bytes.try_into().ok()?))
    }

    /// Materialize a definition from original bytes once for a live table
    /// catalog. Action arguments are not followed as pointers automatically.
    pub fn menu_definition(&self, address: u32) -> Result<FrontendMenu, FrontendAssetError> {
        let source = FrontendData {
            bytes: &self.executable[self.data_file_span.start..self.data_file_span.end],
            virtual_address: self.data_address,
        };
        Ok(parse_frontend_menu(source, address)?)
    }

    /// A menu `00452310` builds from an on-stack spec: `count` item records
    /// at `items` and the layout chain at `layouts`, without a definition.
    pub fn menu_parts(
        &self,
        items: u32,
        count: u32,
        layouts: u32,
    ) -> Result<crate::menu::MenuData, FrontendAssetError> {
        let source = FrontendData {
            bytes: &self.executable[self.data_file_span.start..self.data_file_span.end],
            virtual_address: self.data_address,
        };
        Ok(menu_parts(source, items, count, layouts)?)
    }
}

/// [`FrontendAssets::menu_parts`] over the executable's data.
pub fn menu_parts(
    source: FrontendData<'_>,
    items: u32,
    count: u32,
    layouts: u32,
) -> Result<crate::menu::MenuData, inf_formats::FrontendError> {
    let records = inf_formats::parse_frontend_menu_items(source, items, count)?;
    let (layout_records, terminator) = inf_formats::parse_frontend_menu_layouts(source, layouts)?;
    Ok(crate::menu::MenuData::derived(
        crate::menu::MenuDerivedData {
            // 004135D0 fills the spec; 00452310 installs base vtable 00525608.
            constructor: 0x0041_35d0,
            vtable: 0x0052_5608,
            items: Some(records),
            layouts: Some(crate::menu::MenuLayouts::new(
                layouts,
                terminator,
                layout_records,
            )),
            initial_selection: 0,
            flags: 0,
        },
    ))
}
