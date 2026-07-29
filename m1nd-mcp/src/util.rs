use std::time::{SystemTime, UNIX_EPOCH};

/// Unix epoch milliseconds; 0 if the clock is before the epoch.
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Lowercase hex of a byte slice.
///
/// `digest` 0.11 returns `hybrid_array::Array`, which — unlike the old
/// `generic_array::GenericArray` — does not implement `LowerHex`, so the
/// former `format!("{:x}", ..)` spelling no longer compiles. This is the
/// workspace's existing hex idiom, the single shared copy for this crate.
pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
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
