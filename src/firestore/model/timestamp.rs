use std::cmp::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Timestamp {
    pub seconds: i64,
    pub nanos: i32,
}

impl Timestamp {
    pub fn new(seconds: i64, nanos: i32) -> Self {
        let mut timestamp = Self { seconds, nanos };
        timestamp.normalize();
        timestamp
    }

    pub fn now() -> Self {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0));
        Self {
            seconds: duration.as_secs() as i64,
            nanos: duration.subsec_nanos() as i32,
        }
    }

    pub fn from_system_time(time: SystemTime) -> Self {
        match time.duration_since(UNIX_EPOCH) {
            Ok(duration) => Self {
                seconds: duration.as_secs() as i64,
                nanos: duration.subsec_nanos() as i32,
            },
            Err(err) => {
                let duration = err.duration();
                Self {
                    seconds: -(duration.as_secs() as i64),
                    nanos: -(duration.subsec_nanos() as i32),
                }
            }
        }
    }

    pub fn to_system_time(&self) -> SystemTime {
        if self.seconds >= 0 {
            UNIX_EPOCH + Duration::from_secs(self.seconds as u64) + Duration::from_nanos(self.nanos as u64)
        } else {
            UNIX_EPOCH
                - Duration::from_secs((-self.seconds) as u64)
                - Duration::from_nanos(self.nanos.unsigned_abs() as u64)
        }
    }

    fn normalize(&mut self) {
        let extra_seconds = self.nanos.div_euclid(1_000_000_000);
        self.seconds += extra_seconds as i64;
        self.nanos = self.nanos.rem_euclid(1_000_000_000);
        if self.seconds > 0 && self.nanos < 0 {
            self.seconds -= 1;
            self.nanos += 1_000_000_000;
        }
    }
}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.seconds.cmp(&other.seconds) {
            Ordering::Equal => self.nanos.cmp(&other.nanos),
            ordering => ordering,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_nanoseconds() {
        let timestamp = Timestamp::new(1, 1_500_000_000);
        assert_eq!(timestamp.seconds, 2);
        assert_eq!(timestamp.nanos, 500_000_000);
    }

    #[test]
    fn ordering() {
        let earlier = Timestamp::new(1, 0);
        let later = Timestamp::new(2, 0);
        assert!(earlier < later);
    }
}
