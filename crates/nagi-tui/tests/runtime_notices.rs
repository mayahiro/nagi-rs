//! Runtime asynchronous lifecycle notice tests

use std::thread;

use nagi_tui::{
    App, DeliveryPolicy, Effect, Node, Runtime, RuntimeConfig, RuntimeNotice, RuntimeNoticeKind,
    Size, Subscription, ViewContext, VirtualClock,
};

#[derive(Clone, Copy)]
enum NoticeMode {
    EffectPanic,
    StreamCompleted,
    StreamPanicked,
}

struct NoticeApp {
    mode: NoticeMode,
}

impl App for NoticeApp {
    type Message = ();

    fn init(&mut self) -> Effect<Self::Message> {
        match self.mode {
            NoticeMode::EffectPanic => Effect::latest("load", |_| panic!("effect failure")),
            NoticeMode::StreamCompleted | NoticeMode::StreamPanicked => Effect::none(),
        }
    }

    fn update(&mut self, (): Self::Message) -> Effect<Self::Message> {
        Effect::none()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        match self.mode {
            NoticeMode::EffectPanic => Subscription::none(),
            NoticeMode::StreamCompleted => {
                Subscription::stream("events", DeliveryPolicy::reliable(), |_, _| {})
            }
            NoticeMode::StreamPanicked => {
                Subscription::stream("events", DeliveryPolicy::reliable(), |_, _| {
                    panic!("stream failure")
                })
            }
        }
    }

    fn view(&self, _context: ViewContext) -> Node<Self::Message> {
        Node::text("")
    }
}

fn wait_for_notice(
    runtime: &mut Runtime<NoticeApp, VirtualClock>,
    poll_effects: bool,
) -> RuntimeNotice {
    for _ in 0..10_000 {
        if poll_effects {
            runtime.poll_effects();
        }
        if runtime.pending_runtime_notices() > 0 {
            return runtime.drain_runtime_notices().remove(0);
        }
        thread::yield_now();
    }
    panic!("runtime notice was not produced")
}

#[test]
fn effect_panic_notice_contains_latest_identity() {
    let mut runtime = Runtime::with_clock(
        NoticeApp {
            mode: NoticeMode::EffectPanic,
        },
        RuntimeConfig::new(Size::new(1, 1)),
        VirtualClock::new(),
    )
    .unwrap();

    let notice = wait_for_notice(&mut runtime, true);
    assert_eq!(notice.kind(), RuntimeNoticeKind::EffectPanicked);
    let (key, generation) = notice.task().expect("latest task identity");
    assert_eq!(key.as_str(), "load");
    assert_eq!(generation, 1);
    assert_eq!(runtime.effect_diagnostics().task_panics(), 1);
}

#[test]
fn subscription_stream_completion_and_panic_are_distinct() {
    for (mode, expected) in [
        (
            NoticeMode::StreamCompleted,
            RuntimeNoticeKind::SubscriptionStreamCompleted,
        ),
        (
            NoticeMode::StreamPanicked,
            RuntimeNoticeKind::SubscriptionStreamPanicked,
        ),
    ] {
        let mut runtime = Runtime::with_clock(
            NoticeApp { mode },
            RuntimeConfig::new(Size::new(1, 1)),
            VirtualClock::new(),
        )
        .unwrap();

        let notice = wait_for_notice(&mut runtime, false);
        assert_eq!(notice.kind(), expected);
        let (key, generation) = notice.subscription().expect("stream identity");
        assert_eq!(key.as_str(), "events");
        assert_eq!(generation, 1);
    }
}

#[test]
fn zero_notice_capacity_is_rejected() {
    let mut config = RuntimeConfig::new(Size::new(1, 1));
    config.runtime_notice_capacity = 0;
    let result = Runtime::with_clock(
        NoticeApp {
            mode: NoticeMode::EffectPanic,
        },
        config,
        VirtualClock::new(),
    );
    assert!(matches!(
        result,
        Err(nagi_tui::RuntimeError::ZeroRuntimeNoticeCapacity)
    ));
}
