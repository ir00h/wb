use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use wb_model::CloseAnswer;

pub const WINDOW_ANSWER_LIMIT: Duration = Duration::from_secs(10);
const IN_FLIGHT_POLL: Duration = Duration::from_millis(20);
const NOT_ASKED: u64 = 0;

pub enum ExitRequest {
    Allow,
    Prevent,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TerminateReply {
    Now,
    Later,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settle {
    Exit,
    ReplyToTerminate(bool),
}

pub struct Closing {
    ask: Box<dyn Fn() + Send + Sync>,
    settle: Box<dyn Fn(Settle) + Send + Sync>,
    answer_limit: Duration,
    leaving: AtomicBool,
    terminate_waits: AtomicBool,
    asked: AtomicU64,
    asks: AtomicU64,
    in_flight: AtomicUsize,
}

pub struct InFlight<'a>(&'a AtomicUsize);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Closing {
    pub fn new(
        ask: impl Fn() + Send + Sync + 'static,
        settle: impl Fn(Settle) + Send + Sync + 'static,
        answer_limit: Duration,
    ) -> Arc<Closing> {
        Arc::new(Closing {
            ask: Box::new(ask),
            settle: Box::new(settle),
            answer_limit,
            leaving: AtomicBool::new(false),
            terminate_waits: AtomicBool::new(false),
            asked: AtomicU64::new(NOT_ASKED),
            asks: AtomicU64::new(NOT_ASKED),
            in_flight: AtomicUsize::new(0),
        })
    }

    pub fn operation(&self) -> InFlight<'_> {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        InFlight(&self.in_flight)
    }

    pub fn exit_requested(self: &Arc<Self>) -> ExitRequest {
        if !self.leaving.load(Ordering::SeqCst) {
            self.ask_window();
            return ExitRequest::Prevent;
        }
        if self.in_flight.load(Ordering::SeqCst) == 0 {
            return ExitRequest::Allow;
        }
        let closing = Arc::clone(self);
        std::thread::spawn(move || closing.leave());
        ExitRequest::Prevent
    }

    pub fn terminate_requested(self: &Arc<Self>) -> TerminateReply {
        if self.leaving.load(Ordering::SeqCst) && self.in_flight.load(Ordering::SeqCst) == 0 {
            return TerminateReply::Now;
        }
        self.terminate_waits.store(true, Ordering::SeqCst);
        self.ask_window();
        TerminateReply::Later
    }

    pub fn ask_window(self: &Arc<Self>) {
        if self.leaving.load(Ordering::SeqCst) {
            return;
        }
        let ask = self.asks.fetch_add(1, Ordering::SeqCst) + 1;
        if self
            .asked
            .compare_exchange(NOT_ASKED, ask, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let closing = Arc::clone(self);
            std::thread::spawn(move || {
                std::thread::sleep(closing.answer_limit);
                if closing
                    .asked
                    .compare_exchange(ask, NOT_ASKED, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    closing.leave();
                }
            });
        }
        (self.ask)();
    }

    pub fn answer(&self, answer: CloseAnswer) {
        self.asked.store(NOT_ASKED, Ordering::SeqCst);
        match answer {
            CloseAnswer::Leave => self.leave(),
            CloseAnswer::Stay => {
                if self.terminate_waits.swap(false, Ordering::SeqCst) {
                    (self.settle)(Settle::ReplyToTerminate(false));
                }
            }
        }
    }

    fn leave(&self) {
        self.leaving.store(true, Ordering::SeqCst);
        while self.in_flight.load(Ordering::SeqCst) > 0 {
            std::thread::sleep(IN_FLIGHT_POLL);
        }
        (self.settle)(if self.terminate_waits.swap(false, Ordering::SeqCst) {
            Settle::ReplyToTerminate(true)
        } else {
            Settle::Exit
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::time::Duration;

    use wb_model::CloseAnswer;

    use super::{Closing, ExitRequest, Settle, TerminateReply, WINDOW_ANSWER_LIMIT};

    const ANSWER_LIMIT: Duration = Duration::from_millis(100);

    fn counted() -> (Arc<AtomicUsize>, impl Fn() + Send + Sync + 'static) {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&count);
        (count, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
    }

    #[derive(Clone, Default)]
    struct Settled(Arc<Mutex<Vec<Settle>>>);

    impl Settled {
        fn signal(&self) -> impl Fn(Settle) + Send + Sync + 'static {
            let settled = self.clone();
            move |settle| {
                settled
                    .0
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(settle);
            }
        }

        fn take(&self) -> Vec<Settle> {
            std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
        }
    }

    #[test]
    fn a_request_to_quit_asks_the_window_and_waits_for_its_answer() {
        let (asked, ask) = counted();
        let settled = Settled::default();
        let closing = Closing::new(ask, settled.signal(), WINDOW_ANSWER_LIMIT);
        assert!(matches!(closing.exit_requested(), ExitRequest::Prevent));
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        closing.answer(CloseAnswer::Stay);
        assert_eq!(settled.take(), []);
        assert!(matches!(closing.exit_requested(), ExitRequest::Prevent));
        closing.answer(CloseAnswer::Leave);
        assert_eq!(settled.take(), [Settle::Exit]);
        assert!(matches!(closing.exit_requested(), ExitRequest::Allow));
    }

    #[test]
    fn the_app_does_not_exit_while_an_operation_is_in_flight() {
        let (_, ask) = counted();
        let settled = Settled::default();
        let closing = Closing::new(ask, settled.signal(), ANSWER_LIMIT);
        let in_flight = closing.operation();
        let leaving = {
            let closing = Arc::clone(&closing);
            std::thread::spawn(move || closing.answer(CloseAnswer::Leave))
        };
        std::thread::sleep(Duration::from_secs(6));
        assert_eq!(settled.take(), []);
        assert!(matches!(closing.exit_requested(), ExitRequest::Prevent));
        drop(in_flight);
        leaving.join().expect("left");
        assert!(settled.take().contains(&Settle::Exit));
        assert!(matches!(closing.exit_requested(), ExitRequest::Allow));
    }

    #[test]
    fn the_app_stays_after_the_window_answered_stay() {
        let (_, ask) = counted();
        let settled = Settled::default();
        let closing = Closing::new(ask, settled.signal(), ANSWER_LIMIT);
        assert!(matches!(closing.exit_requested(), ExitRequest::Prevent));
        closing.answer(CloseAnswer::Stay);
        std::thread::sleep(ANSWER_LIMIT * 3);
        assert_eq!(settled.take(), []);
    }

    #[test]
    fn the_app_leaves_when_the_window_gives_no_answer() {
        let (_, ask) = counted();
        let settled = Settled::default();
        let closing = Closing::new(ask, settled.signal(), ANSWER_LIMIT);
        assert!(matches!(closing.exit_requested(), ExitRequest::Prevent));
        std::thread::sleep(ANSWER_LIMIT * 3);
        assert_eq!(settled.take(), [Settle::Exit]);
    }

    #[test]
    fn a_terminate_request_is_answered_once_the_window_has_answered() {
        let (asked, ask) = counted();
        let settled = Settled::default();
        let closing = Closing::new(ask, settled.signal(), WINDOW_ANSWER_LIMIT);
        assert_eq!(closing.terminate_requested(), TerminateReply::Later);
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        closing.answer(CloseAnswer::Stay);
        assert_eq!(settled.take(), [Settle::ReplyToTerminate(false)]);
        assert_eq!(closing.terminate_requested(), TerminateReply::Later);
        closing.answer(CloseAnswer::Leave);
        assert_eq!(settled.take(), [Settle::ReplyToTerminate(true)]);
        assert_eq!(closing.terminate_requested(), TerminateReply::Now);
        assert_eq!(settled.take(), []);
    }

    #[test]
    fn a_terminate_request_waits_for_an_operation_in_flight() {
        let (_, ask) = counted();
        let settled = Settled::default();
        let closing = Closing::new(ask, settled.signal(), WINDOW_ANSWER_LIMIT);
        closing.answer(CloseAnswer::Leave);
        assert_eq!(settled.take(), [Settle::Exit]);
        let in_flight = closing.operation();
        assert_eq!(closing.terminate_requested(), TerminateReply::Later);
        drop(in_flight);
        assert_eq!(closing.terminate_requested(), TerminateReply::Now);
    }
}
