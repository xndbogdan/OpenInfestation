//! Selection/scroll phases of 00452E20. Bounds errors expose missing draw
//! feedback or malformed definitions instead of inventing row heights.

use super::{MenuCommand, MenuRecordsMut, MenuState, MenuStateError, MenuVisibility};

pub(super) fn item_index(index: i32, count: usize) -> Result<usize, MenuStateError> {
    usize::try_from(index)
        .ok()
        .filter(|&index| index < count)
        .ok_or(MenuStateError::ItemOutOfBounds { index, count })
}

/// The flags 00452E20 tests at `index`: an item's, or past the array the
/// adjacent record's that a layout's count reaches. The database's second
/// layout counts two rows from row 1, so Previous wraps to row 2, the
/// definition `0052FA60` itself, whose first DWORD is no row's flags.
fn row_flags(data: &MenuRecordsMut<'_>, index: i32) -> Result<u32, MenuStateError> {
    let count = data.items.len();
    usize::try_from(index)
        .ok()
        .and_then(|row| match data.items.get(row) {
            Some(item) => Some(item.flags),
            None => data.adjacent_flags.get(row - count).copied(),
        })
        .ok_or(MenuStateError::ItemOutOfBounds { index, count })
}

fn layout_for(data: &MenuRecordsMut<'_>, selection: i32) -> Result<(usize, i32), MenuStateError> {
    let mut start = 0_i32;
    for (index, layout) in data.layouts.iter().enumerate() {
        let end = start
            .checked_add(i32::from(layout.item_count))
            .ok_or(MenuStateError::RecordCountOverflow)?;
        if selection < end {
            return Ok((index, start));
        }
        start = end;
    }
    Err(MenuStateError::MissingLayout { selection })
}

fn height(data: &MenuRecordsMut<'_>, index: i32) -> Result<u16, MenuStateError> {
    Ok(data.items[item_index(index, data.items.len())?].row_height)
}

fn reset_age(data: &mut MenuRecordsMut<'_>, index: i32) -> Result<(), MenuStateError> {
    let index = item_index(index, data.items.len())?;
    data.items[index].age = 0;
    Ok(())
}

pub(super) fn navigate(
    state: &mut MenuState,
    data: &mut MenuRecordsMut<'_>,
    command: MenuCommand,
    visibility: MenuVisibility,
) -> Result<(), MenuStateError> {
    let count = i32::try_from(data.items.len()).map_err(|_| MenuStateError::RecordCountOverflow)?;
    item_index(state.selection, data.items.len())?;
    let previous = state.selection;
    let (old_layout_index, old_start) = layout_for(data, previous)?;
    let old_layout = data.layouts[old_layout_index];
    let old_end = old_start + i32::from(old_layout.item_count);
    let mut skipped = 0_i32;
    let mut found = false;

    // A valid wrapping layout eventually returns to a selectable row. Bound
    // the malformed all-hidden/all-disabled case, which otherwise loops forever.
    for _ in 0..=data.items.len() + data.adjacent_flags.len() {
        match command {
            MenuCommand::Previous => {
                state.selection -= 1;
                if state.selection < old_start && old_layout.flags & 8 != 0 {
                    state.selection += i32::from(old_layout.item_count);
                }
                if state.selection < 0 {
                    state.selection = previous;
                    return Ok(());
                }
            }
            MenuCommand::Next => {
                state.selection += 1;
                skipped += 1;
                if state.selection >= old_end && old_layout.flags & 8 != 0 {
                    state.selection -= i32::from(old_layout.item_count);
                }
                if state.selection >= count {
                    state.selection = previous;
                    return Ok(());
                }
            }
            _ => unreachable!("navigation accepts only previous/next"),
        }
        let flags = row_flags(data, state.selection)?;
        if flags & visibility.mask() != 0 && flags & 8 == 0 {
            found = true;
            break;
        }
    }
    if !found {
        return Err(MenuStateError::NoSelectableItem);
    }
    // Native would go on to draw and scroll that record as a row.
    item_index(state.selection, data.items.len()).map_err(|_| {
        MenuStateError::SelectsAdjacentRecord {
            index: state.selection,
            count: data.items.len(),
        }
    })?;

    let (layout_index, start) = layout_for(data, state.selection)?;
    let layout = data.layouts[layout_index];
    let visible = i32::from(layout.visible_rows);
    let mut target_scroll = state.scroll_start;

    match command {
        MenuCommand::Previous => {
            if layout.flags & 0x30 != 0 {
                target_scroll = state.selection - start;
                // Upward wrap in these layout modes jumps without animation.
                if state.selection == start + i32::from(layout.item_count) - 1 {
                    state.scroll_start = target_scroll;
                }
            } else {
                if layout_index != old_layout_index {
                    state.scroll_start = i32::from(layout.item_count);
                    let mut rows = 0_i32;
                    loop {
                        state.scroll_start -= 1;
                        rows += i32::from(height(data, start + state.scroll_start)?);
                        if rows >= visible {
                            break;
                        }
                    }
                    target_scroll = state.scroll_start;
                }
                let mut selected = state.selection;
                while selected <= state.scroll_start + start && target_scroll > 0 {
                    selected += 1;
                    target_scroll -= 1;
                }
            }
        }
        MenuCommand::Next => {
            if state.selection == start {
                state.scroll_start = 0;
                target_scroll = 0;
            }
            if layout.flags & 0x30 != 0 {
                target_scroll = state.selection - start;
            } else {
                let mut total_rows = 0_i32;
                let mut rows_before_selection = 0_i32;
                let selected_offset = state.selection - state.scroll_start - start;
                for offset in 0..visible {
                    let index = start + state.scroll_start + offset;
                    // Native reads the WORDs following the logical items as
                    // heights when a layout overcounts them (Options 00539378
                    // counts thirteen slots for eight items). The port stops
                    // at the logical item count like the draw walker instead
                    // of inventing ownership of adjacent authored records.
                    if index >= count {
                        break;
                    }
                    let row_height = height(data, index)?;
                    total_rows += i32::from(row_height);
                    if offset < selected_offset {
                        rows_before_selection = total_rows;
                    } else if row_height > 1 {
                        skipped += 1;
                    }
                }
                while skipped > 0 {
                    if state.selection >= start + i32::from(layout.item_count) - 1
                        || rows_before_selection < visible - 1
                        || total_rows < visible
                    {
                        break;
                    }
                    target_scroll += 1;
                    skipped -= 1;
                }
            }
        }
        _ => unreachable!(),
    }

    animate_scroll(state, data, layout_index, start, target_scroll, count)
}

/// Retail accumulates signed 16-bit animation and clears the entering/leaving
/// rows' ages before publishing +50. The modulo is the whole menu item count,
/// not the current layout count; these asymmetric loops are intentional.
fn animate_scroll(
    state: &mut MenuState,
    data: &mut MenuRecordsMut<'_>,
    layout_index: usize,
    start: i32,
    target: i32,
    count: i32,
) -> Result<(), MenuStateError> {
    let difference = target - state.scroll_start;
    let visible = i32::from(data.layouts[layout_index].visible_rows);
    let mut cursor = start + target;
    if difference > 0 {
        for offset in 0..difference {
            let delta = (height(data, cursor - 1)? as i16).wrapping_mul(200);
            data.layouts[layout_index].scroll_animation = data.layouts[layout_index]
                .scroll_animation
                .wrapping_add(delta);
            cursor -= 1;
            reset_age(data, (visible - offset + start - 1 + target) % count)?;
            reset_age(data, cursor)?;
        }
    } else {
        let mut offset = 0_i32;
        while difference < offset {
            let delta = (height(data, cursor)? as i16).wrapping_mul(-200);
            data.layouts[layout_index].scroll_animation = data.layouts[layout_index]
                .scroll_animation
                .wrapping_add(delta);
            reset_age(data, cursor)?;
            cursor += 1;
            reset_age(data, (visible - offset + start + target) % count)?;
            offset -= 1;
        }
    }
    state.scroll_start = target;
    Ok(())
}
