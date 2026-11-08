//! Injectable time source.
//!
//! The real hardware needs short delays between feature reports, while tests
//! must run instantly. Encoding both operations behind a trait keeps service
//! code independent of wall-clock time.

use std::time::{Duration, SystemTime};

/// Time and sleep operations used by the service layer.
pub trait Clock: Send + Sync {
    /// Returns the current wall-clock time.
    fn now(&self) -> SystemTime;

    /// Blocks for at least `duration`.
    fn sleep(&self, duration: Duration);
}

/// Production clock backed by the standard library.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::UNIX_EPOCH;

    use super::*;

    struct FakeClock {
        now: SystemTime,
        slept: Mutex<Vec<Duration>>,
    }

    impl Clock for FakeClock {
        fn now(&self) -> SystemTime {
            self.now
        }

        fn sleep(&self, duration: Duration) {
            self.slept.lock().unwrap().push(duration);
        }
    }

    #[test]
    fn fake_clock_makes_tests_deterministic() {
        let fixed = UNIX_EPOCH + Duration::from_millis(1_700_000_000_000);
        let clock = FakeClock {
            now: fixed,
            slept: Mutex::new(Vec::new()),
        };

        assert_eq!(clock.now(), fixed);
        clock.sleep(Duration::from_millis(40));
        assert_eq!(*clock.slept.lock().unwrap(), [Duration::from_millis(40)]);
    }

    #[test]
    fn system_clock_reports_the_present() {
        let now = SystemClock.now();
        assert!(now.duration_since(UNIX_EPOCH).is_ok());
    }
}
