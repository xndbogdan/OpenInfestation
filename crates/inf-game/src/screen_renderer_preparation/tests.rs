use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    Lookup(u8),
    Arena,
    Batch,
    Publication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fault {
    point: Point,
    native_code: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Lookup(u8),
    Arena(u8, usize),
    Batch(u8),
    Publication(u8),
}

const EVENTS: [Event; 6] = [
    Event::Lookup(1),
    Event::Arena(11, 0xc8000),
    Event::Lookup(2),
    Event::Batch(22),
    Event::Lookup(3),
    Event::Publication(33),
];

struct Host {
    events: Vec<Event>,
    lookups: u8,
    failure: Option<Fault>,
    arena: (u8, usize),
    batch: u8,
    publication: u8,
}

impl Host {
    fn new(failure: Option<Fault>) -> Self {
        Self {
            events: Vec::new(),
            lookups: 0,
            failure,
            arena: (71, 0x100),
            batch: 72,
            publication: 73,
        }
    }

    fn reached(&self, point: Point) -> Result<(), Fault> {
        match self.failure {
            Some(fault) if fault.point == point => Err(fault),
            _ => Ok(()),
        }
    }
}

impl ScreenRendererPreparationHost<Fault> for Host {
    type Renderer = u8;

    fn renderer(&mut self) -> Result<u8, Fault> {
        self.lookups += 1;
        self.events.push(Event::Lookup(self.lookups));
        self.reached(Point::Lookup(self.lookups))?;
        Ok([11, 22, 33][usize::from(self.lookups - 1)])
    }

    fn replace_command_arena(&mut self, renderer: u8, bytes: usize) -> Result<(), Fault> {
        self.events.push(Event::Arena(renderer, bytes));
        self.reached(Point::Arena)?;
        self.arena = (renderer, bytes);
        Ok(())
    }

    fn begin_batch(&mut self, renderer: u8) -> Result<(), Fault> {
        self.events.push(Event::Batch(renderer));
        self.reached(Point::Batch)?;
        self.batch = renderer;
        Ok(())
    }

    fn publish_current_renderer(&mut self, renderer: u8) -> Result<(), Fault> {
        self.events.push(Event::Publication(renderer));
        self.reached(Point::Publication)?;
        self.publication = renderer;
        Ok(())
    }
}

#[test]
fn each_preparation_effect_uses_its_fresh_renderer_and_native_arena_size() {
    let mut host = Host::new(None);
    assert_eq!(prepare_screen_renderer(&mut host), Ok(()));
    assert_eq!(host.events, EVENTS);
    assert_eq!(host.lookups, 3);
    assert_eq!(host.arena, (11, 0xc8000));
    assert_eq!(host.batch, 22);
    assert_eq!(host.publication, 33);
}

#[test]
fn all_six_failures_keep_completed_effects_and_the_original_error() {
    let cases = [
        (Point::Lookup(1), (71, 0x100), 72),
        (Point::Arena, (71, 0x100), 72),
        (Point::Lookup(2), (11, 0xc8000), 72),
        (Point::Batch, (11, 0xc8000), 72),
        (Point::Lookup(3), (11, 0xc8000), 22),
        (Point::Publication, (11, 0xc8000), 22),
    ];
    for (index, (point, arena, batch)) in cases.into_iter().enumerate() {
        let fault = Fault {
            point,
            native_code: 0x301,
        };
        let mut host = Host::new(Some(fault));
        assert_eq!(prepare_screen_renderer(&mut host), Err(fault));
        assert_eq!(host.events, EVENTS[..=index]);
        assert_eq!(host.arena, arena);
        assert_eq!(host.batch, batch);
        assert_eq!(host.publication, 73);
    }
}
