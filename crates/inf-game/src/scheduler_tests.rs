use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Input,
    Transition,
    SessionTick,
    LinkedTick,
    Linked08,
    Linked0c,
    SessionActivity,
    LinkedActivity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Input,
    Transition,
    SessionTick { owner: u8, tick: SchedulerTick },
    LinkedTick(SchedulerTick),
    Linked08,
    Linked0c,
    SessionActivity(SchedulerActivity),
    LinkedActivity(SchedulerActivity),
}

struct Host {
    selected: bool,
    suspended: bool,
    tick_owner: u8,
    transition_owner: Option<u8>,
    fail_at: Option<Step>,
    events: Vec<Event>,
}

impl Host {
    fn new() -> Self {
        Self {
            selected: true,
            suspended: false,
            tick_owner: 1,
            transition_owner: None,
            fail_at: None,
            events: Vec::new(),
        }
    }

    fn record(&mut self, event: Event, step: Step) -> Result<(), Step> {
        self.events.push(event);
        if self.fail_at == Some(step) {
            Err(step)
        } else {
            Ok(())
        }
    }
}

impl SchedulerHost for Host {
    type Error = Step;

    fn input_is_selected(&self) -> bool {
        self.selected
    }

    fn poll_input(&mut self) -> Result<(), Step> {
        self.record(Event::Input, Step::Input)
    }

    fn transition(&mut self) -> Result<(), Step> {
        self.record(Event::Transition, Step::Transition)?;
        if let Some(owner) = self.transition_owner.take() {
            self.tick_owner = owner;
        }
        Ok(())
    }

    fn session_tick(&mut self, tick: SchedulerTick) -> Result<(), Step> {
        self.record(
            Event::SessionTick {
                owner: self.tick_owner,
                tick,
            },
            Step::SessionTick,
        )
    }

    fn linked_tick(&mut self, tick: SchedulerTick) -> Result<(), Step> {
        self.record(Event::LinkedTick(tick), Step::LinkedTick)
    }

    fn linked_callback_08(&mut self) -> Result<(), Step> {
        self.record(Event::Linked08, Step::Linked08)
    }

    fn linked_callback_0c(&mut self) -> Result<(), Step> {
        self.record(Event::Linked0c, Step::Linked0c)
    }

    fn session_activity(&mut self, activity: SchedulerActivity) -> Result<(), Step> {
        self.record(Event::SessionActivity(activity), Step::SessionActivity)?;
        self.suspended = activity == SchedulerActivity::Suspend;
        Ok(())
    }

    fn linked_activity(&mut self, activity: SchedulerActivity) -> Result<(), Step> {
        self.record(Event::LinkedActivity(activity), Step::LinkedActivity)?;
        self.selected = activity == SchedulerActivity::Resume;
        Ok(())
    }
}

fn tick(delta_micros: i32, elapsed_micros: i32) -> SchedulerTick {
    SchedulerTick {
        delta_micros,
        elapsed_micros,
    }
}

#[test]
fn gate_accumulates_time_and_runs_transition_only_when_admitted() {
    let mut scheduler = Scheduler::new(1_000_000);
    let mut host = Host::new();
    host.transition_owner = Some(2);
    scheduler.second_pass(&mut host).unwrap();
    for now in [1_000_000, 1_003_000, 1_007_999] {
        assert_eq!(scheduler.first_pass(now, &mut host), Ok(None));
        scheduler.second_pass(&mut host).unwrap();
    }
    assert!(host.events.is_empty());
    assert_eq!(scheduler.last_clock_micros(), 1_000_000);
    let admitted = tick(8_000, 8_000);
    assert_eq!(
        scheduler.first_pass(1_008_000, &mut host),
        Ok(Some(admitted))
    );
    assert_eq!(scheduler.last_clock_micros(), 1_008_000);
    assert_eq!(
        host.events,
        [
            Event::Input,
            Event::Transition,
            Event::SessionTick {
                owner: 2,
                tick: admitted,
            },
        ]
    );
    scheduler.second_pass(&mut host).unwrap();
    assert_eq!(
        &host.events[3..],
        [
            Event::LinkedTick(admitted),
            Event::Linked08,
            Event::Linked0c
        ]
    );
}

#[test]
fn capped_update_discards_excess_but_preserves_raw_elapsed_argument() {
    for elapsed in [124_999, 125_000, 125_001, 900_000] {
        let mut scheduler = Scheduler::new(30_000);
        let mut host = Host::new();
        let expected = tick(elapsed.min(125_000), elapsed);
        let now = 30_000 + elapsed as u64;
        assert_eq!(scheduler.first_pass(now, &mut host), Ok(Some(expected)));
        scheduler.second_pass(&mut host).unwrap();
        assert_eq!(host.events[3], Event::LinkedTick(expected));
        host.events.clear();
        assert_eq!(scheduler.first_pass(now + 7_999, &mut host), Ok(None));
        scheduler.second_pass(&mut host).unwrap();
        assert!(host.events.is_empty());
        assert_eq!(
            scheduler.first_pass(now + 8_000, &mut host),
            Ok(Some(tick(8_000, 8_000)))
        );
    }
}

#[test]
fn a_gated_iteration_clears_previous_second_pass_eligibility() {
    let mut scheduler = Scheduler::new(0);
    let mut host = Host::new();
    scheduler.first_pass(8_000, &mut host).unwrap();
    scheduler.second_pass(&mut host).unwrap();
    host.events.clear();
    assert_eq!(scheduler.first_pass(9_000, &mut host), Ok(None));
    scheduler.second_pass(&mut host).unwrap();
    assert!(host.events.is_empty());
    assert_eq!(scheduler.last_clock_micros(), 8_000);
}

#[test]
fn callback_time_belongs_to_the_next_pre_callback_sample() {
    let mut scheduler = Scheduler::new(0);
    let mut host = Host::new();
    scheduler.first_pass(8_000, &mut host).unwrap();
    // Simulate work lasting 12 ms by supplying the next sampled clock. The
    // committed timestamp must remain 8 ms, not a separately sampled 20 ms.
    scheduler.second_pass(&mut host).unwrap();
    assert_eq!(scheduler.last_clock_micros(), 8_000);
    assert_eq!(
        scheduler.first_pass(20_000, &mut host),
        Ok(Some(tick(12_000, 12_000)))
    );
}

#[test]
fn low_word_wrap_and_high_words_match_the_native_subtraction() {
    let mut host = Host::new();
    let start = u64::from(u32::MAX) - 3_999;
    let mut scheduler = Scheduler::new(start);
    assert_eq!(
        scheduler.first_pass(start + 8_000, &mut host),
        Ok(Some(tick(8_000, 8_000)))
    );
    assert_eq!(scheduler.last_clock_micros(), start + 8_000);

    let mut scheduler = Scheduler::new(1_000);
    assert_eq!(
        scheduler.first_pass((9_u64 << 32) + 9_000, &mut host),
        Ok(Some(tick(8_000, 8_000)))
    );
    assert_eq!(scheduler.last_clock_micros(), (9_u64 << 32) + 9_000);
}

#[test]
fn signed_elapsed_rejects_backwards_and_half_range_ambiguous_clocks() {
    let mut host = Host::new();
    let mut scheduler = Scheduler::new(50_000);
    for now in [49_999, 50_000 + i32::MAX as u64 + 1, 50_000 + (1_u64 << 32)] {
        assert_eq!(scheduler.first_pass(now, &mut host), Ok(None));
        scheduler.second_pass(&mut host).unwrap();
    }
    assert!(host.events.is_empty());
    assert_eq!(scheduler.last_clock_micros(), 50_000);
    assert_eq!(
        scheduler.first_pass(50_000 + i32::MAX as u64, &mut host),
        Ok(Some(tick(125_000, i32::MAX)))
    );
}

#[test]
fn suspension_deselects_input_without_pausing_or_rebasing_the_clock() {
    let mut scheduler = Scheduler::new(0);
    let mut host = Host::new();
    scheduler
        .notify_activity(SchedulerActivity::Suspend, &mut host)
        .unwrap();
    assert!(host.suspended);
    assert!(!host.selected);
    assert_eq!(
        host.events,
        [
            Event::SessionActivity(SchedulerActivity::Suspend),
            Event::LinkedActivity(SchedulerActivity::Suspend),
        ]
    );
    host.events.clear();
    let suspended_tick = tick(8_000, 8_000);
    assert_eq!(
        scheduler.first_pass(8_000, &mut host),
        Ok(Some(suspended_tick))
    );
    scheduler.second_pass(&mut host).unwrap();
    assert_eq!(
        host.events,
        [
            Event::Transition,
            Event::SessionTick {
                owner: 1,
                tick: suspended_tick,
            },
            Event::LinkedTick(suspended_tick),
            Event::Linked08,
            Event::Linked0c,
        ]
    );
    scheduler
        .notify_activity(SchedulerActivity::Resume, &mut host)
        .unwrap();
    assert!(!host.suspended);
    assert!(host.selected);
    assert_eq!(scheduler.last_clock_micros(), 8_000);
    host.events.clear();
    assert_eq!(
        scheduler.first_pass(1_008_000, &mut host),
        Ok(Some(tick(125_000, 1_000_000)))
    );
    assert_eq!(host.events[0], Event::Input);
}

#[test]
fn first_pass_failure_stops_later_owners_and_does_not_commit_clock() {
    for (failure, event_count) in [
        (Step::Input, 1),
        (Step::Transition, 2),
        (Step::SessionTick, 3),
    ] {
        let mut scheduler = Scheduler::new(0);
        let mut host = Host::new();
        scheduler.first_pass(8_000, &mut host).unwrap();
        host.events.clear();
        host.fail_at = Some(failure);
        assert_eq!(scheduler.first_pass(20_000, &mut host), Err(failure));
        scheduler.second_pass(&mut host).unwrap();
        assert_eq!(host.events.len(), event_count);
        assert_eq!(scheduler.last_clock_micros(), 8_000);
        host.fail_at = None;
        assert_eq!(
            scheduler.first_pass(24_000, &mut host),
            Ok(Some(tick(16_000, 16_000)))
        );
    }
}

#[test]
fn second_pass_failure_stops_later_callbacks_but_keeps_committed_time() {
    for (failure, event_count) in [
        (Step::LinkedTick, 1),
        (Step::Linked08, 2),
        (Step::Linked0c, 3),
    ] {
        let mut scheduler = Scheduler::new(0);
        let mut host = Host::new();
        scheduler.first_pass(8_000, &mut host).unwrap();
        host.events.clear();
        host.fail_at = Some(failure);
        assert_eq!(scheduler.second_pass(&mut host), Err(failure));
        assert_eq!(host.events.len(), event_count);
        assert_eq!(scheduler.last_clock_micros(), 8_000);
    }
}

#[test]
fn separate_first_passes_do_not_dispatch_second_pass_callbacks_early() {
    let mut schedulers = [Scheduler::new(0), Scheduler::new(10_000)];
    let mut hosts = [Host::new(), Host::new()];
    for (scheduler, host) in schedulers.iter_mut().zip(&mut hosts) {
        scheduler.first_pass(20_000, host).unwrap();
        assert!(matches!(
            host.events.last(),
            Some(Event::SessionTick { .. })
        ));
    }
    assert!(hosts.iter().all(|host| host.events.len() == 3));
    for (scheduler, host) in schedulers.iter().zip(&mut hosts) {
        scheduler.second_pass(host).unwrap();
    }
    assert_eq!(hosts[0].events[3], Event::LinkedTick(tick(20_000, 20_000)));
    assert_eq!(hosts[1].events[3], Event::LinkedTick(tick(10_000, 10_000)));
}
