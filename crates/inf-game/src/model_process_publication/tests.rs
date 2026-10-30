use super::*;
use crate::frontend_assets::FrontendAssets;
#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("declared backend failure")]
    Backend,
    #[error(transparent)]
    Process(#[from] ProcessModelGraphicsError),
}
#[derive(Default)]
struct Host {
    events: Vec<&'static str>,
    fail: bool,
}
impl ModelValueCallback for Host {
    type Error = Error;
    fn packed_value(&mut self, _: &mut ModelValues, _: u32) -> Result<u16, Error> {
        self.events.push("value");
        Ok(9)
    }
}
impl ModelExecutionHost for Host {
    fn publish_transform(&mut self, _: &ModelContext, _: ModelTransform) -> Result<(), Error> {
        self.events.push("publish");
        if self.fail {
            Err(Error::Backend)
        } else {
            Ok(())
        }
    }
    fn select_projection(&mut self, _: ModelDispatch) -> Result<(), Error> {
        self.events.push("select");
        if self.fail {
            Err(Error::Backend)
        } else {
            Ok(())
        }
    }
    fn primitive(
        &mut self,
        _: ModelPrimitiveRequest<'_>,
        _: &mut ModelValues,
    ) -> Result<(), Error> {
        self.events.push("primitive");
        Ok(())
    }
    fn begin_depth_group(&mut self, _: i32, _: bool) -> Result<(), Error> {
        self.events.push("begin");
        Ok(())
    }
    fn end_depth_group(&mut self) -> Result<(), Error> {
        self.events.push("end");
        Ok(())
    }
}
fn process() -> ProcessModelGraphics {
    let assets = FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    ProcessModelGraphics::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap()
}
#[test]
fn forwarding_backend_failure_preserves_real_prefix_before_any_process_store() {
    let process = process();
    let world = process.world().unwrap();
    let mut host = Host {
        fail: true,
        ..Host::default()
    };
    let mut adapter = ModelProcessPublication {
        inner: &mut host,
        process: &process,
        table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
    };
    let context = ModelContext::new([1, 2, 3], crate::menu::MenuBasis::INITIAL);
    assert!(matches!(
        adapter.publish_transform(
            &context,
            context.publish(process.view().unwrap(), process.shift().unwrap())
        ),
        Err(Error::Backend)
    ));
    assert_eq!(process.world().unwrap(), world);
    assert!(matches!(
        adapter.select_projection(ModelDispatch::Far),
        Err(Error::Backend)
    ));
    assert_eq!(
        process
            .selected(
                crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
                crate::process_model_graphics::SelectedProjectionSlot::Local1c
            )
            .unwrap(),
        crate::process_model_graphics::ProjectionEntry::NativeNull
    );
    assert_eq!(host.events, ["publish", "select"]);
}
#[test]
fn success_records_current_owner_while_mismatched_snapshot_stops_after_backend_prefix() {
    let process = process();
    process
        .publish_view(crate::frontend_scene::SceneView {
            eye: [0; 3],
            basis: crate::menu::MenuBasis::INITIAL,
        })
        .unwrap();
    let mut host = Host::default();
    let context = ModelContext::new([1, 2, 300], crate::menu::MenuBasis::INITIAL);
    let mut adapter = ModelProcessPublication {
        inner: &mut host,
        process: &process,
        table: crate::frontend_scene::ModelSurfaceTableOrigin::TemplateDefault,
    };
    let transform = context.publish(process.view().unwrap(), 0);
    adapter.publish_transform(&context, transform).unwrap();
    let retained = process.world().unwrap();
    assert_eq!(retained.position, context.position);
    process
        .publish_view(crate::frontend_scene::SceneView {
            eye: [5, 6, 7],
            basis: crate::menu::MenuBasis::INITIAL,
        })
        .unwrap();
    assert!(matches!(
        adapter.publish_transform(&context, transform),
        Err(Error::Process(
            ProcessModelGraphicsError::PublicationSourceMismatch
        ))
    ));
    assert_eq!(process.world().unwrap(), retained);
    assert_eq!(host.events, ["publish", "publish"]);
}
