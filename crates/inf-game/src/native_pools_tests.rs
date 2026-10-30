use super::*;

fn word(exe: &[u8], address: u32) -> u32 {
    u32::from_le_bytes(code(exe, address, 4).try_into().unwrap())
}

/// Code and initialized data alike: every section's raw offset equals its
/// RVA in this executable, as the native guards read it.
fn code(exe: &[u8], address: u32, length: usize) -> &[u8] {
    &exe[(address - 0x0040_0000) as usize..][..length]
}

fn relative_target(exe: &[u8], instruction: u32) -> u32 {
    (instruction + 5).wrapping_add(word(exe, instruction + 1))
}

#[test]
fn pools_are_what_the_initializers_construct_and_dispatch() {
    use inf_formats::FrontendExecutable;
    let exe = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&exe).unwrap();
    for (handler, pool) in NATIVE_HANDLER_POOLS.iter().enumerate() {
        let handler = handler as u32;
        // The dispatch table 0053AA14 names the pool.
        assert_eq!(
            executable.word(0x0053_aa14 + 4 * handler).unwrap(),
            pool.pool
        );
        // call construct; jmp (the destructor's registration)
        assert_eq!(code(&exe, pool.initializer, 1), [0xe8], "H{handler}");
        assert_eq!(relative_target(&exe, pool.initializer), pool.construct);
        assert_eq!(code(&exe, pool.initializer + 5, 1), [0xe9], "H{handler}");
        // The registrar: push thunk; call atexit (00511D89). The thunk: mov
        // ecx, pool; jmp destructor, which calls the base destructor 004EFBD0.
        let registrar = relative_target(&exe, pool.initializer + 5);
        assert_eq!(code(&exe, registrar, 1), [0x68], "H{handler}");
        assert_eq!(
            relative_target(&exe, registrar + 5),
            0x0051_1d89,
            "H{handler}"
        );
        let thunk = word(&exe, registrar + 1);
        assert_eq!(code(&exe, thunk, 1), [0xb9], "H{handler}");
        assert_eq!(word(&exe, thunk + 1), pool.pool, "H{handler}");
        assert_eq!(
            relative_target(&exe, thunk + 5),
            pool.destructor,
            "H{handler}"
        );
        let body = code(&exe, pool.destructor, 0x50);
        assert!(
            (0..body.len() - 5).any(|at| body[at] == 0xe8
                && relative_target(&exe, pool.destructor + at as u32) == 0x004e_fbd0),
            "H{handler}"
        );
        // mov ecx, pool; jmp constructor
        assert_eq!(code(&exe, pool.construct, 1), [0xb9], "H{handler}");
        assert_eq!(word(&exe, pool.construct + 1), pool.pool, "H{handler}");
        assert_eq!(relative_target(&exe, pool.construct + 5), pool.constructor);
        // The constructor's last table store (mov dword ptr [esi], table)
        // installs the final table, after the intermediate one.
        let body = code(&exe, pool.constructor, 0x60);
        let stores: Vec<u32> = body
            .windows(6)
            .filter(|bytes| bytes[..2] == [0xc7, 0x06])
            .map(|bytes| u32::from_le_bytes(bytes[2..].try_into().unwrap()))
            .collect();
        assert_eq!(stores.len(), 2, "H{handler}");
        assert_eq!(stores[1], pool.final_table, "H{handler}");
        // +4: mov eax, name; ret. +8: the loader.
        assert_eq!(
            executable.word(pool.final_table + 4).unwrap(),
            pool.name_slot
        );
        assert_eq!(code(&exe, pool.name_slot, 1), [0xb8], "H{handler}");
        let name = executable
            .mapped(word(&exe, pool.name_slot + 1), 16)
            .unwrap()
            .bytes;
        let name = &name[..name.iter().position(|&byte| byte == 0).unwrap()];
        assert_eq!(name, pool.name.as_bytes(), "H{handler}");
        assert_eq!(executable.word(pool.final_table + 8).unwrap(), pool.loader);
        assert_eq!(
            executable.word(pool.final_table + 0xc).unwrap(),
            pool.cleanup
        );
    }
}

#[test]
fn render_resource_kinds_name_their_handlers_pools() {
    use crate::render_resources::RenderResourceKind as Kind;
    for kind in [
        Kind::String,
        Kind::Sprite,
        Kind::Font,
        Kind::Style,
        Kind::Material,
        Kind::Model,
    ] {
        assert_eq!(
            kind.native_pool(),
            native_handler_pool(kind.handler()).unwrap().pool,
            "{kind:?}"
        );
    }
}
