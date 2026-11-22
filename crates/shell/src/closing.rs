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

type Signal = Box<dyn Fn() + Send + Sync>;

pub struct Closing {
    ask: Signal,
    exit: Signal,
    answer_limit: Duration,
    leaving: AtomicBool,
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
        exit: impl Fn() + Send + Sync + 'static,
        answer_limit: Duration,
    ) -> Arc<Closing> {
        Arc::new(Closing {
            ask: Box::new(ask),
            exit: Box::new(exit),
            answer_limit,
            leaving: AtomicBool::new(false),
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
            CloseAnswer::Stay => {}
        }
    }

    fn leave(&self) {
        self.leaving.store(true, Ordering::SeqCst);
        while self.in_flight.load(Ordering::SeqCst) > 0 {
            std::thread::sleep(IN_FLIGHT_POLL);
        }
        (self.exit)();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use wb_model::CloseAnswer;

    use super::{Closing, ExitRequest, WINDOW_ANSWER_LIMIT};

    const ANSWER_LIMIT: Duration = Duration::from_millis(100);

    fn counted() -> (Arc<AtomicUsize>, impl Fn() + Send + Sync + 'static) {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&count);
        (count, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
    }

    #[test]
    fn a_request_to_quit_asks_the_window_and_waits_for_its_answer() {
        let (asked, ask) = counted();
        let (exited, exit) = counted();
        let closing = Closing::new(ask, exit, WINDOW_ANSWER_LIMIT);
        assert!(matches!(closing.exit_requested(), ExitRequest::Prevent));
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        closing.answer(CloseAnswer::Stay);
        assert_eq!(exited.load(Ordering::SeqCst), 0);
        assert!(matches!(closing.exit_requested(), ExitRequest::Prevent));
        closing.answer(CloseAnswer::Leave);
        assert_eq!(exited.load(Ordering::SeqCst), 1);
        assert!(matches!(closing.exit_requested(), ExitRequest::Allow));
    }

    #[test]
    fn the_app_does_not_exit_while_an_operation_is_in_flight() {
        let (_, ask) = counted();
        let (exited, exit) = counted();
        let closing = Closing::new(ask, exit, ANSWER_LIMIT);
        let in_flight = closing.operation();
        let leaving = {
            let closing = Arc::clone(&closing);
            std::thread::spawn(move || closing.answer(CloseAnswer::Leave))
        };
        std::thread::sleep(Duration::from_secs(6));
        assert_eq!(exited.load(Ordering::SeqCst), 0);
        assert!(matches!(closing.exit_requested(), ExitRequest::Prevent));
        drop(in_flight);
        leaving.join().expect("left");
        assert!(exited.load(Ordering::SeqCst) >= 1);
        assert!(matches!(closing.exit_requested(), ExitRequest::Allow));
    }

    #[test]
    fn the_app_stays_after_the_window_answered_stay() {
        let (_, ask) = counted();
        let (exited, exit) = counted();
        let closing = Closing::new(ask, exit, ANSWER_LIMIT);
        assert!(matches!(closing.exit_requested(), ExitRequest::Prevent));
        closing.answer(CloseAnswer::Stay);
        std::thread::sleep(ANSWER_LIMIT * 3);
        assert_eq!(exited.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn the_app_leaves_when_the_window_gives_no_answer() {
        let (_, ask) = counted();
        let (exited, exit) = counted();
        let closing = Closing::new(ask, exit, ANSWER_LIMIT);
        assert!(matches!(closing.exit_requested(), ExitRequest::Prevent));
        std::thread::sleep(ANSWER_LIMIT * 3);
        assert_eq!(exited.load(Ordering::SeqCst), 1);
    }
}
