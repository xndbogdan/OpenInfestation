//! The sixteen resource handler pools of the dispatch table at 0053AA14, as
//! retail's CRT initializers build them: each initializer calls a thunk that
//! loads the pool into ECX and jumps to its constructor, which installs an
//! intermediate and then the final table. The final table's +4 names the
//! handler, its +8 loads an overlay's resources and its +0C frees them when
//! the overlay is released (004EE6D0, the port's `unload_overlay`).
//! ([`LOADING_RUNTIME.md`](../../../docs/re/LOADING_RUNTIME.md))

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeHandlerPool {
    /// The handler's name, as the final table's +4 returns it.
    pub name: &'static str,
    pub pool: u32,
    pub final_table: u32,
    /// The final table's +4 (the name) and +8 (the overlay load).
    pub name_slot: u32,
    pub loader: u32,
    /// The final table's +0C, the release's cleanup.
    pub cleanup: u32,
    /// The CRT initializer, its construct thunk and the constructor.
    pub initializer: u32,
    pub construct: u32,
    pub constructor: u32,
    /// The static destructor the initializer registers with `atexit`: it
    /// releases the handler's module and runs the base destructor
    /// `004EFBD0`. The port drops each pool with its resident runtime.
    pub destructor: u32,
}

const fn pool(
    name: &'static str,
    pool: u32,
    final_table: u32,
    [name_slot, loader, cleanup]: [u32; 3],
    [initializer, construct, constructor, destructor]: [u32; 4],
) -> NativeHandlerPool {
    NativeHandlerPool {
        name,
        pool,
        final_table,
        name_slot,
        loader,
        cleanup,
        initializer,
        construct,
        constructor,
        destructor,
    }
}

/// Indexed by handler.
pub const NATIVE_HANDLER_POOLS: [NativeHandlerPool; 16] = [
    pool(
        "Text",
        0x0055_ee50,
        0x0052_7598,
        [0x0047_42e0, 0x0050_ecd0, 0x0050_ed60],
        [0x0047_4010, 0x0047_4020, 0x0047_40a0, 0x0047_4050],
    ),
    pool(
        "Position",
        0x0055_ee80,
        0x0052_75c0,
        [0x0047_4a20, 0x0050_ed90, 0x0050_ed60],
        [0x0047_4100, 0x0047_4110, 0x0047_4190, 0x0047_4140],
    ),
    pool(
        "Number",
        0x0055_ee08,
        0x0052_75e8,
        [0x0047_4a90, 0x0050_f3f0, 0x0050_ed60],
        [0x0047_41f0, 0x0047_4200, 0x0047_4280, 0x0047_4230],
    ),
    pool(
        "Graphic",
        0x0055_eda8,
        0x0052_7610,
        [0x0047_4f50, 0x0050_ee80, 0x0050_f150],
        [0x0047_42f0, 0x0047_4300, 0x0047_4380, 0x0047_4330],
    ),
    pool(
        "Font",
        0x0055_ee20,
        0x0052_7638,
        [0x0047_4f60, 0x0050_f250, 0x0050_ed60],
        [0x0047_43e0, 0x0047_43f0, 0x0047_4470, 0x0047_4420],
    ),
    pool(
        "Display",
        0x0055_edc0,
        0x0052_7660,
        [0x0047_4f70, 0x0050_f340, 0x0050_ed60],
        [0x0047_44d0, 0x0047_44e0, 0x0047_4560, 0x0047_4510],
    ),
    pool(
        "Shade",
        0x0055_eec8,
        0x0052_7688,
        [0x0047_4f80, 0x0050_f3f0, 0x0050_ed60],
        [0x0047_45c0, 0x0047_45d0, 0x0047_4650, 0x0047_4600],
    ),
    pool(
        "Colour",
        0x0055_ee68,
        0x0052_76b0,
        [0x0047_4f90, 0x0050_f490, 0x0050_f560],
        [0x0047_46c0, 0x0047_46d0, 0x0047_4750, 0x0047_4700],
    ),
    pool(
        "Shape",
        0x0055_eee0,
        0x0052_76d8,
        [0x0047_4fa0, 0x0050_f5a0, 0x0050_f720],
        [0x0047_47b0, 0x0047_47c0, 0x0047_4840, 0x0047_47f0],
    ),
    pool(
        "SoundFX",
        0x0055_ee98,
        0x0052_7700,
        [0x0047_4fb0, 0x0050_f760, 0x0050_f830],
        [0x0047_48a0, 0x0047_48b0, 0x0047_4930, 0x0047_48e0],
    ),
    pool(
        "Table",
        0x0055_eeb0,
        0x0052_7728,
        [0x0047_4fc0, 0x0050_f890, 0x0050_ed60],
        [0x0047_4990, 0x0047_49a0, 0x0047_4a30, 0x0047_49d0],
    ),
    pool(
        "Furniture",
        0x0055_eef8,
        0x0052_7750,
        [0x0047_4fd0, 0x0043_7b50, 0x0050_ed60],
        [0x0047_4aa0, 0x0047_4ab0, 0x0047_4b30, 0x0047_4ae0],
    ),
    pool(
        "Map",
        0x0055_ee38,
        0x0052_7778,
        [0x0047_4ff0, 0x0045_07d0, 0x0050_ed60],
        [0x0047_4b90, 0x0047_4ba0, 0x0047_4c20, 0x0047_4bd0],
    ),
    pool(
        "Creature",
        0x0055_ed90,
        0x0052_77a0,
        [0x0047_5000, 0x0044_80b0, 0x0050_ed60],
        [0x0047_4c80, 0x0047_4c90, 0x0047_4d10, 0x0047_4cc0],
    ),
    pool(
        "Level",
        0x0055_edd8,
        0x0052_77c8,
        [0x0047_4fe0, 0x0044_80b0, 0x0050_ed60],
        [0x0047_4d70, 0x0047_4d80, 0x0047_4e00, 0x0047_4db0],
    ),
    pool(
        "Surface",
        0x0055_edf0,
        0x0052_77f0,
        [0x0047_5010, 0x0048_1090, 0x0050_ed60],
        [0x0047_4e60, 0x0047_4e70, 0x0047_4ef0, 0x0047_4ea0],
    ),
];

/// A handler's native pool, or `None` past the sixteen.
pub fn native_handler_pool(handler: u8) -> Option<&'static NativeHandlerPool> {
    NATIVE_HANDLER_POOLS.get(usize::from(handler))
}

#[cfg(test)]
#[path = "native_pools_tests.rs"]
mod tests;
