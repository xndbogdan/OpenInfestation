use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    model_values::ModelValues,
    render_resources::{RenderResourceKind, RenderResourceRequest, RenderResources},
    Installation, ResidencyRequest, ResidentRuntime,
};

#[derive(Default)]
struct Host {
    queries: usize,
    addresses: usize,
    sorting: bool,
    address: u32,
}
impl ModelValueCallback for Host {
    type Error = &'static str;
    fn packed_value(&mut self, _: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
        Err("unexpected packed callback")
    }
}
impl<'a> PrimitiveHost<'a> for Host {
    fn texture(&mut self, _: i32) -> Result<Option<RenderResource<'a>>, Self::Error> {
        Err("unexpected resource lookup")
    }
    fn material(&mut self, _: i32) -> Result<u32, Self::Error> {
        Err("unexpected material lookup")
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.queries += 1;
        Ok(self.sorting)
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
        self.addresses += 1;
        Ok(TextureSortIdentity::NativeEntryAddress(self.address))
    }
    fn submit(&mut self, _: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        Err("unexpected submission")
    }
}

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}
fn resident() -> ResidentRuntime {
    let mut runtime = ResidentRuntime::new(Installation::open(&root()).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    runtime
}

#[test]
fn portable_opaque_sort_keys_keep_selector_order_and_validate_band_capacity() {
    assert_eq!(
        TextureSortIdentity::PortableEntryOrdinal(0).sort_word(0),
        Ok(0x4000_0100)
    );
    assert_eq!(
        TextureSortIdentity::PortableEntryOrdinal(7).sort_word(9),
        Ok(0x4000_0809)
    );
    assert_eq!(
        TextureSortIdentity::PortableEntryOrdinal(MAX_PORTABLE_TEXTURE_ENTRY_ORDINAL)
            .sort_word(255),
        Ok(0x7fff_ffff)
    );
    assert_eq!(
        TextureSortIdentity::PortableEntryOrdinal(MAX_PORTABLE_TEXTURE_ENTRY_ORDINAL + 1)
            .sort_word(0),
        Err(TextureSortError {
            ordinal: MAX_PORTABLE_TEXTURE_ENTRY_ORDINAL + 1
        })
    );
    // The exact native branch retains signed SAR and wrapping descriptor adds.
    assert_eq!(
        TextureSortIdentity::NativeEntryAddress(0xffff_fffc).sort_word(0),
        Ok(0x4000_0000)
    );
    assert_eq!(
        TextureSortIdentity::NativeEntryAddress(0x8000_0000).sort_word(0),
        Ok(2)
    );
}

#[test]
fn payload_retains_unknown_padding_and_borrowed_texture_relocation() {
    let runtime = resident();
    let resource = RenderResources::new(&runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 1805,
        })
        .unwrap();
    let mut command = PrimitiveCommand::new(PrimitiveCallback::TexturedTriangleNear, 123, 5);
    assert_eq!(command.payload.len(), 80);
    assert!(command.payload.iter().all(Option::is_none));
    command.write_u32(0, 0x1234_5678);
    command.write_rgb(0x20, [1, 2, 3]);
    command.write_texture(0x1c, Some(resource));
    assert_eq!(command.read_u32(0), Some(0x1234_5678));
    assert_eq!(command.read_u32(0x20), None);
    assert_eq!(
        command.payload[0x20..0x24],
        [Some(1), Some(2), Some(3), None]
    );
    assert_eq!(command.read_u32(0x1c), None);
    assert_eq!(
        command.texture.unwrap().1.source.registration,
        resource.source.registration
    );
    command.write_texture(0x1c, None);
    assert!(command.texture.is_none());
    assert_eq!(command.read_u32(0x1c), Some(0));
    assert_eq!(command.callback.native_address(), 0x004a_38f0);
}

#[test]
fn original_tables_keep_file_authored_lut_sine_and_clip_constants() {
    let assets = FrontendAssets::open(&root()).unwrap();
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(exe).unwrap();
    assert_eq!(*tables.sine_table, assets.sine_table().values);
    assert_eq!(
        tables.clip_acceptance.as_slice(),
        exe.mapped(0x0052_8bd0, 256).unwrap().bytes
    );
    assert_eq!(tables.quad_u, [0, 65535, 65535, 0]);
    assert_eq!(tables.quad_v, [0, 0, 65535, 65535]);
    assert_eq!(tables.clip_projection_bits, [0x3c80_0000, 0xbc80_0000]);
    assert_ne!(tables.clip_acceptance[0x12], 0);
    assert_eq!(tables.clip_acceptance[0x09], 0);
}

#[test]
fn late_sorting_queries_preserve_depth_pointer_signedness_and_mode_selectors() {
    let runtime = resident();
    let resource = RenderResources::new(&runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 1805,
        })
        .unwrap();
    let RenderResourceData::Sprite(entry) = resource.data else {
        unreachable!()
    };
    let mut entry = entry.clone();
    let mut first = entry.variants[0].clone();
    first.flags = inf_formats::SpriteFlags::new(0);
    let mut second = first.clone();
    second.flags = inf_formats::SpriteFlags::new(0x20);
    entry.variants = vec![first, second];
    entry.selectors.mode0 = 0;
    entry.selectors.mode1 = 1;
    let texture = RenderResource {
        data: RenderResourceData::Sprite(&entry),
        ..resource
    };
    let mut host = Host {
        sorting: true,
        address: 0xffff_fffc,
        ..Host::default()
    };
    // Logical base+4 wraps tozero before SAR. Pointer order is not slot order.
    assert_eq!(
        textured_sort_depth(0xf000_0001, 0, Some(texture), &mut host).unwrap(),
        0x4000_0000
    );
    host.address = 0x8000_0000;
    assert_eq!(
        textured_sort_depth(123, 0, Some(texture), &mut host).unwrap(),
        2
    );
    assert_eq!(
        textured_sort_depth(0xf000_0001, 1, Some(texture), &mut host).unwrap(),
        0x3800_0000
    );
    assert_eq!((host.queries, host.addresses), (3, 2));
    entry.selectors.mode0 = 255;
    let texture = RenderResource {
        data: RenderResourceData::Sprite(&entry),
        ..resource
    };
    assert_eq!(
        textured_sort_depth(100, 0, Some(texture), &mut host).unwrap(),
        0xc000_0032
    );
    assert_eq!(textured_sort_depth(100, 0, None, &mut host).unwrap(), 100);
    host.sorting = false;
    assert_eq!(
        textured_sort_depth(101, 0, Some(texture), &mut host).unwrap(),
        101
    );
    assert_eq!((host.queries, host.addresses), (6, 2));
    assert_eq!(flat_sort_depth(100, 0, &mut host).unwrap(), 100);
    host.sorting = true;
    assert_eq!(flat_sort_depth(100, 0, &mut host).unwrap(), 0x4000_0000);
    assert_eq!(flat_sort_depth(100, 2, &mut host).unwrap(), 0xc000_0032);
}
