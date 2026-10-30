use super::*;
use crate::render_resources::{RenderResourceKind, RenderResourceRequest, RenderResources};
use crate::{Installation, ResidencyRequest, ResidentRuntime};
use std::path::Path;

#[test]
fn every_resident_frontend_descriptor_uploads_with_proven_native_dimensions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let resources = RenderResources::new(&runtime);
    let mut store = CpuTextureStore::new();
    let mut flags = std::collections::BTreeSet::new();
    let mut descriptors = 0;
    let mut largest = [0; 2];
    for slot in 0..resources.pool_capacity(RenderResourceKind::Sprite) {
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: slot as i32,
        };
        let Ok(resource) = resources.resolve(request) else {
            continue;
        };
        for mode in [SpriteRenderMode::Mode0, SpriteRenderMode::Mode1] {
            let Some(texture) = store.resolve(resource, mode, state()).unwrap() else {
                continue;
            };
            flags.insert(texture.flags);
            largest = std::array::from_fn(|i| largest[i].max(texture.dimensions[i]));
            assert!(texture.texel(1, 1).is_some());
            assert!(texture
                .texel(
                    u32::from(texture.dimensions[0]) + 1,
                    u32::from(texture.dimensions[1]) + 1
                )
                .is_some());
            let identity = texture.allocation_id;
            assert_eq!(
                store
                    .resolve(resource, mode, state())
                    .unwrap()
                    .unwrap()
                    .allocation_id,
                identity
            );
            descriptors += 1;
        }
    }
    assert_eq!(
        flags.into_iter().collect::<Vec<_>>(),
        [0x0004, 0x0012, 0x0014, 0x0024, 0x8014, 0x8024]
    );
    assert_eq!(largest, [254, 254]);
    assert!(descriptors > 400);
    assert!(store.len() > 200);
}
