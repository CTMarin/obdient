use std::time::Duration;

const DEFAULT_INITIAL_BACKOFF_MS: u64 = 100;
const DEFAULT_MAX_BACKOFF_MS: u64 = 1000;

pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(DEFAULT_INITIAL_BACKOFF_MS),
            max_backoff: Duration::from_millis(DEFAULT_MAX_BACKOFF_MS),
        }
    }
}

impl RetryPolicy {
    pub fn backoff_for(&self, retry: u32) -> Duration {
        self.initial_backoff
            .saturating_mul(2u32.saturating_pow(retry))
            .min(self.max_backoff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_default_policy_when_retry_zero_then_return_100ms() {
        let policy: RetryPolicy = RetryPolicy::default();
        let retry: u32 = 0;

        let expected: Duration = Duration::from_millis(100);
        let actual: Duration = policy.backoff_for(retry);

        assert_eq!(actual, expected)
    }

    #[test]
    fn given_default_policy_when_retry_one_then_return_200ms() {
        let policy: RetryPolicy = RetryPolicy::default();
        let retry: u32 = 1;

        let expected: Duration = Duration::from_millis(200);
        let actual: Duration = policy.backoff_for(retry);

        assert_eq!(actual, expected)
    }

    #[test]
    fn given_default_policy_when_retry_two_then_return_400ms() {
        let policy: RetryPolicy = RetryPolicy::default();
        let retry: u32 = 2;

        let expected: Duration = Duration::from_millis(400);
        let actual: Duration = policy.backoff_for(retry);

        assert_eq!(actual, expected)
    }

    #[test]
    fn given_default_policy_when_retry_four_then_return_1s() {
        let policy: RetryPolicy = RetryPolicy::default();
        let retry: u32 = 4;

        let expected: Duration = Duration::from_millis(1000);
        let actual: Duration = policy.backoff_for(retry);

        assert_eq!(actual, expected)
    }

    #[test]
    fn given_default_policy_when_retry_u32_max_then_return_1s() {
        let policy: RetryPolicy = RetryPolicy::default();
        let retry: u32 = u32::MAX;

        let expected: Duration = Duration::from_millis(1000);
        let actual: Duration = policy.backoff_for(retry);

        assert_eq!(actual, expected)
    }

    #[test]
    fn given_zero_initial_backoff_policy_when_retry_three_then_return_0() {
        let policy: RetryPolicy = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::from_millis(1000),
        };
        let retry: u32 = 3;

        let expected: Duration = Duration::from_millis(0);
        let actual: Duration = policy.backoff_for(retry);

        assert_eq!(actual, expected)
    }
}
