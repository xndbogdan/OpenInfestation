//! Runtime menu records retain executable provenance without inventing a PE
//! definition for derived objects. Rows and layouts have separate lifetimes:
//! 00449410 allocates profile rows but binds process-owned layout records.

use std::{
    cell::{Ref, RefCell, RefMut},
    ops::{Deref, DerefMut},
    rc::Rc,
};

use inf_formats::{ByteSpan, FrontendMenu, FrontendMenuItem, FrontendMenuLayout};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSource {
    AuthoredDefinition { address: u32, span: ByteSpan },
    Derived { constructor: u32, vtable: u32 },
}

impl MenuSource {
    pub fn authored_address(self) -> Option<u32> {
        match self {
            Self::AuthoredDefinition { address, .. } => Some(address),
            Self::Derived { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuLayoutSource {
    pub address: u32,
    pub terminator: ByteSpan,
}

/// Clones bind the same mutable layouts, including their scroll animation.
/// The source describes the original array, not the owner currently using it.
#[derive(Clone, Debug)]
pub struct MenuLayouts {
    source: Option<MenuLayoutSource>,
    records: Rc<RefCell<Vec<FrontendMenuLayout>>>,
}

impl MenuLayouts {
    pub fn new(address: u32, terminator: ByteSpan, records: Vec<FrontendMenuLayout>) -> Self {
        Self {
            source: Some(MenuLayoutSource {
                address,
                terminator,
            }),
            records: Rc::new(RefCell::new(records)),
        }
    }

    fn absent() -> Self {
        Self {
            source: None,
            records: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// A chain the port lays out for a menu it lengthens: a copy of the
    /// authored records that claims none of them, so no other definition
    /// binds it.
    pub fn own(records: &[FrontendMenuLayout]) -> Self {
        let unclaimed = ByteSpan { start: 0, end: 0 };
        Self {
            source: None,
            records: Rc::new(RefCell::new(
                records
                    .iter()
                    .map(|record| FrontendMenuLayout {
                        span: unclaimed,
                        ..*record
                    })
                    .collect(),
            )),
        }
    }

    pub fn source(&self) -> Option<MenuLayoutSource> {
        self.source
    }
    pub fn records(&self) -> Ref<'_, Vec<FrontendMenuLayout>> {
        self.records.borrow()
    }
    pub fn records_mut(&self) -> RefMut<'_, Vec<FrontendMenuLayout>> {
        self.records.borrow_mut()
    }
}

/// Instance-owned copies keep each original template's address/span on the
/// row itself. None is the native NULL binding before rebuild; Some(empty)
/// remains a bound zero-length array and does not select that behavior.
pub struct MenuDerivedData {
    pub constructor: u32,
    pub vtable: u32,
    pub items: Option<Vec<FrontendMenuItem>>,
    pub layouts: Option<MenuLayouts>,
    pub initial_selection: i32,
    pub flags: u32,
}

#[derive(Debug)]
pub struct MenuMetadata {
    pub source: MenuSource,
    pub initial_selection: i32,
    pub flags: u32,
    items_present: bool,
}

impl MenuMetadata {
    pub fn has_items(&self) -> bool {
        self.items_present
    }
}

/// An item array two authored definitions share whole: the network host
/// menus `00538650` and `00538668` both bind `00538510`, whose row flags
/// `0045F670` changes for both.
#[derive(Clone, Debug)]
pub struct MenuItems(Rc<RefCell<Vec<FrontendMenuItem>>>);

#[derive(Debug)]
struct MenuStorage {
    metadata: RefCell<MenuMetadata>,
    items: MenuItems,
    layouts: MenuLayouts,
    /// The flags DWORDs of the 32-byte records past the item array, as far
    /// as the layouts count rows: authored executable data a layout census
    /// (`00452B60`) reads when its layout counts more rows than the menu has.
    adjacent_flags: Vec<u32>,
}

/// Clones share the actual record bindings. Authored definitions are interned
/// once; every derived rebuild creates new rows and retains its layout handle.
#[derive(Clone, Debug)]
pub struct MenuData(Rc<MenuStorage>);

impl MenuData {
    pub fn new(definition: FrontendMenu) -> Self {
        Self::with_adjacent_flags(definition, Vec::new())
    }

    /// [`Self::new`] with the flags of the records past the item array.
    pub fn with_adjacent_flags(definition: FrontendMenu, adjacent_flags: Vec<u32>) -> Self {
        Self::with_shared_layouts(definition, None, adjacent_flags)
    }

    /// [`Self::with_adjacent_flags`] bound to `layouts`, a layout chain
    /// another definition already binds (the Options submenus share
    /// `00538D58`), instead of a fresh copy of the definition's.
    pub fn with_shared_layouts(
        definition: FrontendMenu,
        layouts: Option<MenuLayouts>,
        adjacent_flags: Vec<u32>,
    ) -> Self {
        Self::with_shared_records(definition, layouts, None, adjacent_flags)
    }

    /// [`Self::with_shared_layouts`] also bound to `items`, an item array
    /// another definition already binds.
    pub fn with_shared_records(
        definition: FrontendMenu,
        layouts: Option<MenuLayouts>,
        items: Option<MenuItems>,
        adjacent_flags: Vec<u32>,
    ) -> Self {
        let layouts = layouts.unwrap_or_else(|| {
            if definition.layouts_address == 0 {
                MenuLayouts::absent()
            } else {
                MenuLayouts::new(
                    definition.layouts_address,
                    definition.layout_terminator,
                    definition.layouts.clone(),
                )
            }
        });
        Self(Rc::new(MenuStorage {
            adjacent_flags,
            metadata: RefCell::new(MenuMetadata {
                source: MenuSource::AuthoredDefinition {
                    address: definition.virtual_address,
                    span: definition.definition_span,
                },
                initial_selection: definition.initial_selection,
                flags: definition.flags,
                items_present: definition.items_address != 0,
            }),
            items: items.unwrap_or_else(|| MenuItems(Rc::new(RefCell::new(definition.items)))),
            layouts,
        }))
    }

    /// The bound layout chain's handle.
    pub fn layouts(&self) -> MenuLayouts {
        self.0.layouts.clone()
    }

    /// The bound item array's handle.
    pub fn items(&self) -> MenuItems {
        self.0.items.clone()
    }

    pub fn derived(request: MenuDerivedData) -> Self {
        Self(Rc::new(MenuStorage {
            metadata: RefCell::new(MenuMetadata {
                source: MenuSource::Derived {
                    constructor: request.constructor,
                    vtable: request.vtable,
                },
                initial_selection: request.initial_selection,
                flags: request.flags,
                items_present: request.items.is_some(),
            }),
            items: MenuItems(Rc::new(RefCell::new(request.items.unwrap_or_default()))),
            layouts: request.layouts.unwrap_or_else(MenuLayouts::absent),
            adjacent_flags: Vec::new(),
        }))
    }

    /// The flags a layout census reads for `index`: the item's, or past the
    /// array the adjacent record's. None beyond both.
    pub fn census_flags(&self, index: usize) -> Option<u32> {
        let items = self.0.items.0.borrow();
        match items.get(index) {
            Some(item) => Some(item.flags),
            None => self.0.adjacent_flags.get(index - items.len()).copied(),
        }
    }

    pub fn source(&self) -> MenuSource {
        self.0.metadata.borrow().source
    }

    pub fn layout_source(&self) -> Option<MenuLayoutSource> {
        self.0.layouts.source()
    }

    pub fn records(&self) -> MenuRecords<'_> {
        MenuRecords {
            metadata: self.0.metadata.borrow(),
            items: self.0.items.0.borrow(),
            layouts: self.0.layouts.records(),
        }
    }

    /// No record borrow may survive a callback. Heights, ages and layout
    /// changes are visible to all owners sharing the corresponding storage.
    pub fn records_mut(&self) -> MenuRecordsMut<'_> {
        MenuRecordsMut {
            metadata: self.0.metadata.borrow_mut(),
            items: self.0.items.0.borrow_mut(),
            layouts: self.0.layouts.records_mut(),
            adjacent_flags: &self.0.adjacent_flags,
        }
    }
}

pub struct MenuRecords<'a> {
    metadata: Ref<'a, MenuMetadata>,
    pub items: Ref<'a, Vec<FrontendMenuItem>>,
    pub layouts: Ref<'a, Vec<FrontendMenuLayout>>,
}

impl Deref for MenuRecords<'_> {
    type Target = MenuMetadata;
    fn deref(&self) -> &Self::Target {
        &self.metadata
    }
}

pub struct MenuRecordsMut<'a> {
    metadata: RefMut<'a, MenuMetadata>,
    pub items: RefMut<'a, Vec<FrontendMenuItem>>,
    pub layouts: RefMut<'a, Vec<FrontendMenuLayout>>,
    /// [`MenuData::census_flags`] past the item array.
    pub adjacent_flags: &'a [u32],
}

impl Deref for MenuRecordsMut<'_> {
    type Target = MenuMetadata;
    fn deref(&self) -> &Self::Target {
        &self.metadata
    }
}

impl DerefMut for MenuRecordsMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.metadata
    }
}

#[cfg(test)]
#[path = "menu_data_tests.rs"]
mod tests;
