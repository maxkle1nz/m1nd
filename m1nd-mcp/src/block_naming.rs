//! Data-only block naming context and the candidate-edit RUNNER-seat sanitizer.
//!
//! Scan packets retain bounded source metadata without executing a naming client.
//! Untrusted names still pass the same validation before a candidate edit can land.

use serde::Serialize;

/// The declared response schema (§2c): a block name is at most this many chars.
const NAME_MAX_CHARS: usize = 40;
/// The declared response schema (§2c): a one-line purpose is at most this many chars.
const PURPOSE_MAX_CHARS: usize = 120;

/// How many member paths a naming packet materializes (the honest total rides
/// beside it). Enough to name a block; never the whole repo listing.
pub const PACKET_MEMBER_PATHS_CAP: usize = 40;
/// How many top symbols a naming packet materializes.
pub const PACKET_SYMBOLS_CAP: usize = 12;

/// The plain-text instruction serialized into every naming packet. It describes
/// the accepted text shape; a packet never executes the instruction.
pub const PACKET_INSTRUCTION: &str = "Name this code block from the data in this JSON. Reply with EXACTLY one line of JSON and nothing else: {\"name\":\"...\",\"purpose\":\"...\"} — name is a short human module name (max 40 chars), purpose is one plain-text line (max 120 chars). No markdown, no URLs, no paths, no extra keys, no surrounding text.";

/// One block's naming context: member paths, dominant kinds and top symbols.
/// Built by the scan from data it already has; never contains file bodies.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockNamingPacket {
    /// The plain-text naming instruction accompanying the context.
    /// A packet is data only; creating it never calls or starts a runner.
    pub instruction: String,
    pub block_id: String,
    /// The honest total member count (the list below may be capped).
    pub member_count: usize,
    /// Repo-relative member paths, capped at [`PACKET_MEMBER_PATHS_CAP`].
    pub member_paths: Vec<String>,
    /// The block's dominant graph node kinds (e.g. "functions", "structs").
    pub dominant_kinds: Vec<String>,
    /// Top symbol names under the block's files, capped at [`PACKET_SYMBOLS_CAP`].
    pub top_symbols: Vec<String>,
    /// The block's dominant top-level directory (naming context).
    pub dominant_directory: String,
}

/// Validate each present untrusted rename field. Refusals identify the field and
/// violation class without echoing hostile input. Accepted values are left to the
/// caller to store byte-for-byte; this function grants no authority or provenance.
pub(crate) fn sanitize_rename_fields(
    name: Option<&str>,
    purpose: Option<&str>,
) -> Result<(), String> {
    if let Some(n) = name {
        sanitize_field("name", n, NAME_MAX_CHARS)?;
    }
    if let Some(p) = purpose {
        sanitize_field("purpose", p, PURPOSE_MAX_CHARS)?;
    }
    Ok(())
}

fn sanitize_field(field: &str, raw: &str, max_chars: usize) -> Result<String, String> {
    // (1) Control chars first, on the RAW value — a newline/tab is a violation,
    //     never silently collapsed (the schema is single-line plain text).
    if raw.chars().any(|c| c.is_control()) {
        return Err(format!("{field}: control character"));
    }
    // (2) Strip to plain text: trim + collapse whitespace runs to single spaces.
    let stripped = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if stripped.is_empty() {
        return Err(format!("{field}: empty after strip"));
    }
    // (3) The declared schema length (chars, not bytes).
    if stripped.chars().count() > max_chars {
        return Err(format!("{field}: exceeds {max_chars} chars"));
    }
    // (4) Active HTML / Markdown — plain text only.
    if stripped.contains('<') || stripped.contains('>') {
        return Err(format!("{field}: HTML markup"));
    }
    if stripped.contains("](")
        || stripped.contains("![")
        || stripped.contains('`')
        || stripped.starts_with('#')
    {
        return Err(format!("{field}: active Markdown"));
    }
    // (5) URLs.
    let lower = stripped.to_lowercase();
    if lower.contains("http://") || lower.contains("https://") || lower.contains("www.") {
        return Err(format!("{field}: URL"));
    }
    // (6) Path separators and home tildes — no paths of any kind.
    if stripped.contains('/') || stripped.contains('\\') {
        return Err(format!("{field}: path separator"));
    }
    if stripped.contains('~') {
        return Err(format!("{field}: home-path tilde"));
    }
    // (7) Email-like.
    if email_like(&stripped) {
        return Err(format!("{field}: email-like"));
    }
    // (8) Token/base64/hash-like strings.
    if token_like(&stripped) {
        return Err(format!("{field}: token-like string"));
    }
    Ok(stripped)
}

/// `@` with an alphanumeric char on BOTH sides — email-shaped.
fn email_like(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    chars
        .windows(3)
        .any(|w| w[1] == '@' && w[0].is_ascii_alphanumeric() && w[2].is_ascii_alphanumeric())
}

/// A whitespace-delimited token that looks like a secret/base64 blob (≥ 24 chars of
/// the base64ish charset with digits OR `=` padding — base64 encodes digits as
/// letters, but its padding is a dead giveaway) or a hash (≥ 32 hex chars). Plain
/// long words ("internationalization") pass — no digits, no padding, under 32 hex.
fn token_like(s: &str) -> bool {
    s.split(' ').any(|tok| {
        let n = tok.chars().count();
        let base64ish = tok
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '=' | '_' | '-'));
        let has_digit = tok.chars().any(|c| c.is_ascii_digit());
        let all_hex = tok.chars().all(|c| c.is_ascii_hexdigit());
        (n >= 24 && base64ish && (has_digit || tok.ends_with('='))) || (n >= 32 && all_hex)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_rejects_every_injection_class_table_driven() {
        let clean_purpose = "A clean one-line purpose.";
        // (field-under-test-as-name, expected reason substring, case label)
        let hostile_names: &[(&str, &str, &str)] = &[
            ("Auth\x07Service", "control character", "control char (BEL)"),
            ("Auth\nService", "control character", "newline"),
            ("Auth\tService", "control character", "tab"),
            ("<script>alert(1)</script>", "HTML markup", "script tag"),
            ("Auth <b>bold</b>", "HTML markup", "html tag"),
            ("[click me](evil)", "active Markdown", "markdown link"),
            ("![img](x)", "active Markdown", "markdown image"),
            ("`rm -rf`", "active Markdown", "backticks"),
            ("# Heading", "active Markdown", "leading heading"),
            ("see http://evil.example", "URL", "http url"),
            ("see https://evil.example", "URL", "https url"),
            ("visit www.evil.example", "URL", "www url"),
            ("admin@example.com", "email-like", "email"),
            ("/etc/passwd", "path separator", "absolute path"),
            ("src/auth.rs", "path separator", "relative path slash"),
            ("C:\\Windows\\System32", "path separator", "backslash path"),
            ("~ home", "home-path tilde", "home tilde"),
            // The token/base64/hash fixtures are deliberately LOW-ENTROPY synthetic
            // shapes (repeated chars + a digit tail): they trip THIS sanitizer's
            // token-like classes without resembling any real credential pattern.
            ("tokAAAAAAAAAAAAAAAAAA1234", "token-like", "api-token-like"),
            ("QUFBQUFBQUFBQUFBQUFBQTEyMzQ=", "token-like", "base64-like"),
            (
                "abcdef0123456789abcdef0123456789",
                "token-like",
                "hash-like hex",
            ),
            (
                "This Name Is Way Way Way Too Long For The Schema",
                "exceeds 40 chars",
                "over-long name",
            ),
            ("   ", "empty after strip", "whitespace-only"),
            ("", "empty after strip", "empty"),
        ];
        for (name, want, label) in hostile_names {
            let err = sanitize_rename_fields(Some(name), Some(clean_purpose))
                .expect_err(&format!("case '{label}' must be rejected"));
            assert!(
                err.contains(want),
                "case '{label}': expected reason containing '{want}', got '{err}'"
            );
            assert!(err.starts_with("name:"), "reason names the field: {err}");
        }

        // Purpose-specific classes: over-length and multi-line.
        let long_purpose = "p".repeat(PURPOSE_MAX_CHARS + 1);
        let err = sanitize_rename_fields(Some("Clean Name"), Some(&long_purpose))
            .expect_err("over-long purpose");
        assert!(err.contains("exceeds 120 chars"), "got '{err}'");
        let err = sanitize_rename_fields(Some("Clean Name"), Some("line one\nline two"))
            .expect_err("multi-line purpose");
        assert!(err.contains("control character"), "got '{err}'");
    }

    #[test]
    fn sanitizer_accepts_clean_input_with_correct_strip() {
        let name = sanitize_field("name", "  Auth   Service  ", NAME_MAX_CHARS)
            .expect("clean name passes");
        let purpose = sanitize_field(
            "purpose",
            "  Owns   login and sessions.  ",
            PURPOSE_MAX_CHARS,
        )
        .expect("clean purpose passes");
        assert_eq!(name, "Auth Service", "whitespace collapsed + trimmed");
        assert_eq!(purpose, "Owns login and sessions.");

        // Boundary lengths pass exactly.
        let name40 = "n".repeat(NAME_MAX_CHARS);
        let purpose120 = "p".repeat(PURPOSE_MAX_CHARS);
        sanitize_rename_fields(Some(&name40), Some(&purpose120)).expect("boundary lengths pass");
        assert_eq!(
            sanitize_field("name", &name40, NAME_MAX_CHARS)
                .unwrap()
                .chars()
                .count(),
            NAME_MAX_CHARS
        );
        assert_eq!(
            sanitize_field("purpose", &purpose120, PURPOSE_MAX_CHARS)
                .unwrap()
                .chars()
                .count(),
            PURPOSE_MAX_CHARS
        );

        // Plain long words survive the token heuristic; an internal # is not a heading.
        sanitize_rename_fields(
            Some("Internationalization Layer"),
            Some("Long words are fine."),
        )
        .expect("long plain words pass");
        sanitize_rename_fields(Some("C# Bindings"), Some("Interop surface."))
            .expect("internal # passes");
    }
}
