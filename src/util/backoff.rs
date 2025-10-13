use rand::Rng;

pub const DEFAULT_INTERVAL_MILLIS: u64 = 1_000;
pub const DEFAULT_BACKOFF_FACTOR: f64 = 2.0;
pub const MAX_BACKOFF_MILLIS: u64 = 4 * 60 * 60 * 1_000;
pub const RANDOM_FACTOR: f64 = 0.5;

#[derive(Debug, Clone, Copy)]
pub struct BackoffConfig {
    pub interval_millis: u64,
    pub backoff_factor: f64,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            interval_millis: DEFAULT_INTERVAL_MILLIS,
            backoff_factor: DEFAULT_BACKOFF_FACTOR,
        }
    }
}

pub fn calculate_backoff_millis(backoff_count: u32) -> u64 {
    calculate_backoff_with_rng(
        backoff_count,
        BackoffConfig::default(),
        &mut rand::thread_rng(),
    )
}

fn calculate_backoff_with_rng<R: Rng + ?Sized>(
    backoff_count: u32,
    config: BackoffConfig,
    rng: &mut R,
) -> u64 {
    let base = (config.interval_millis as f64) * config.backoff_factor.powi(backoff_count as i32);
    let jitter = RANDOM_FACTOR * base * rng.gen_range(-1.0..=1.0);
    let value = (base + jitter)
        .round()
        .clamp(0.0, MAX_BACKOFF_MILLIS as f64);
    value as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn deterministic_with_seeded_rng() {
        let mut rng = StdRng::seed_from_u64(42);
        let value = calculate_backoff_with_rng(3, BackoffConfig::default(), &mut rng);
        assert!(value > 0);
        assert!(value <= MAX_BACKOFF_MILLIS);
    }

    #[test]
    fn backoff_grows_with_count() {
        let mut rng = StdRng::seed_from_u64(1);
        let first = calculate_backoff_with_rng(0, BackoffConfig::default(), &mut rng);
        let mut rng = StdRng::seed_from_u64(1);
        let later = calculate_backoff_with_rng(4, BackoffConfig::default(), &mut rng);
        assert!(later >= first);
    }
}
