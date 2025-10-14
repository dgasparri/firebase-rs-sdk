use rand::Rng;
use std::time::{Duration, Instant};

/// Configuration for exponential backoff when issuing storage requests.
#[derive(Clone, Debug)]
pub struct BackoffConfig {
    /// Initial delay applied before the second attempt.
    pub initial_delay: Duration,
    /// Maximum delay between attempts.
    pub max_delay: Duration,
    /// Total time budget for the request, including all retries.
    pub total_timeout: Duration,
    /// Maximum number of attempts before giving up.
    pub max_attempts: usize,
}

impl BackoffConfig {
    /// Creates a configuration using Storage defaults (1s base, up to 64s, 2m timeout).
    pub fn standard_operation() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(64),
            total_timeout: Duration::from_secs(2 * 60),
            max_attempts: 8,
        }
    }

    /// Configuration tuned for long running uploads (same defaults as the JS SDK).
    pub fn upload_operation(max_retry_time: Duration) -> Self {
        Self {
            total_timeout: max_retry_time,
            ..Self::standard_operation()
        }
    }

    pub fn with_total_timeout(mut self, timeout: Duration) -> Self {
        self.total_timeout = timeout;
        self
    }
}

/// Tracks the evolving backoff state across attempts.
#[derive(Debug)]
pub struct BackoffState {
    config: BackoffConfig,
    attempt: usize,
    deadline: Instant,
}

impl BackoffState {
    pub fn new(config: BackoffConfig) -> Self {
        let deadline = Instant::now() + config.total_timeout;
        Self {
            config,
            attempt: 0,
            deadline,
        }
    }

    pub fn attempts(&self) -> usize {
        self.attempt
    }

    pub fn has_time_remaining(&self) -> bool {
        Instant::now() < self.deadline
    }

    pub fn can_retry(&self) -> bool {
        self.attempt < self.config.max_attempts && self.has_time_remaining()
    }

    pub fn next_delay(&mut self) -> Duration {
        if self.attempt == 0 {
            self.attempt += 1;
            return Duration::from_millis(0);
        }

        let exp = 2u64.pow((self.attempt - 1) as u32);
        let base = self.config.initial_delay.mul_f64(exp as f64);
        self.attempt += 1;

        let capped = if base > self.config.max_delay {
            self.config.max_delay
        } else {
            base
        };

        let jitter: f64 = rand::thread_rng().gen();
        let jittered = capped.mul_f64(1.0 + jitter);
        if jittered > self.config.max_delay {
            self.config.max_delay
        } else {
            jittered
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_delay_is_zero() {
        let mut backoff = BackoffState::new(BackoffConfig::standard_operation());
        assert_eq!(backoff.next_delay(), Duration::from_millis(0));
    }

    #[test]
    fn delays_increase_with_jitter() {
        let mut backoff = BackoffState::new(BackoffConfig::standard_operation());
        backoff.next_delay();
        let d1 = backoff.next_delay();
        backoff.next_delay();
        let d2 = backoff.next_delay();
        assert!(d2 >= d1);
    }
}
