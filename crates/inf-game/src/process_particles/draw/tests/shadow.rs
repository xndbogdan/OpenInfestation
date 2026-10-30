use super::*;

#[derive(Deserialize)]
struct ShadowReceipt {
    cases: Vec<ShadowCase>,
}
#[derive(Deserialize)]
struct ShadowCase {
    #[serde(flatten)]
    draw: Case,
    shadow_color: u32,
    shadow_amount: u32,
    scale: u32,
    terrain_height: Option<i8>,
}

#[test]
fn combat_shadow_native_packets_preserve_signed_scaling_floor_and_sorting() {
    // Original464C80/466CD0/465810, including real465C96 shadow suffix.
    // H3 sprite, H7 converted color and renderer queue are supplied services.
    let fixture: ShadowReceipt = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/combat_particle_shadow_native.json"
    ))
    .unwrap();
    assert_eq!(fixture.cases.len(), 12);
    let (runtime, assets) = setup();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let template = RenderResources::new(&runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 1805,
        })
        .unwrap();
    let RenderResourceData::Sprite(source) = template.data else {
        unreachable!()
    };
    for row in fixture.cases {
        let case = &row.draw;
        let mut entry = source.clone();
        entry.selectors.mode0 = case.selector;
        entry.variants = vec![source.variants[0].clone(); 4];
        for variant in &mut entry.variants {
            variant.width = case.dimensions[0];
            variant.height = case.dimensions[1];
            variant.flags = inf_formats::SpriteFlags::new(case.texture_flags);
        }
        let mut host = Host {
            resource: RenderResource {
                data: RenderResourceData::Sprite(&entry),
                ..template
            },
            sorting: case.sorting,
            queries: 0,
            packets: Vec::new(),
            lights: Vec::new(),
            emitters: Vec::new(),
            fail_light: false,
            shadow_color: Some(row.shadow_color),
        };
        let mut particles = construct(case, executable);
        let supplied = supplied_data(case, &mut particles, assets.executable_bytes());
        let executable = FrontendExecutable::parse(&supplied).unwrap();
        for record in &case.records {
            let record = particles.record_mut(ParticleId::Pool(record.slot)).unwrap();
            record.write_dword(0x2c, row.scale).unwrap();
            record.write_dword(0x30, row.shadow_amount).unwrap();
        }
        let terrain = flat_terrain_height(row.terrain_height.unwrap_or(0));
        let mut request = request(case);
        request.terrain = Some(&terrain);
        let report = particles.draw_particles(request, executable, &mut host);
        assert!(
            report.omissions.is_empty(),
            "{}: {:?}",
            case.name,
            report.omissions
        );
        assert_eq!(report.commands, case.packets.len(), "{} count", case.name);
        assert_eq!(host.queries, case.sort_queries, "{} sorting", case.name);
        for (actual, expected) in host.packets.iter().zip(&case.packets) {
            assert_eq!(actual.depth, expected.depth, "{} sort key", case.name);
            assert_eq!(
                actual.callback.native_address(),
                expected.callback,
                "{} callback",
                case.name
            );
            assert_eq!(actual.payload, expected.payload, "{} payload", case.name);
        }
        for expected in &case.records {
            let record = particles.record(ParticleId::Pool(expected.slot)).unwrap();
            assert_eq!(
                record.read_word(0x12).unwrap(),
                expected.flags,
                "{} flags",
                case.name
            );
            assert_eq!(
                record.read_word(0x24).unwrap(),
                expected.floor.unwrap(),
                "{} floor",
                case.name
            );
        }
    }
}
