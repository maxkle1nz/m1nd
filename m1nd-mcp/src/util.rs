use std::time::{SystemTime, UNIX_EPOCH};

/// Unix epoch milliseconds; 0 if the clock is before the epoch.
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::now_ms;

    #[test]
    fn now_ms_returns_recent_epoch_millis() {
        // A timestamp after 2023-11-14 (1_700_000_000_000 ms) and no later than
        // a fresh reading proves the helper returns plausible, monotonic-ish
        // wall-clock epoch millis rather than 0 or a bogus value.
        let observed = now_ms();
        let upper = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the unix epoch")
            .as_millis() as u64;
        assert!(
            observed > 1_700_000_000_000,
            "now_ms() should be a recent epoch-millis value, got {observed}"
        );
        assert!(
            observed <= upper,
            "now_ms() ({observed}) must not exceed a fresh reading ({upper})"
        );
    }
}
