//! Human View v2 F11-b — the naming-runner engine (HUMAN-VIEW-V2-F11-TECH §2 + §4bis o5).
//!
//! The zero-touch enabler: a `skeleton_candidate` scan with `naming:"auto"` calls a
//! pinned live naming-runner (via the announced runner daemon's `POST /name`), one
//! packet per block, and lands `named_by:"runner"` / `needs_owner_naming:false` on
//! every block the runner named — the human reads the map and stamps. This module is
//! the OWNER-side engine, deliberately transport-thin and content-paranoid:
//!
//! - **The packet** ([`BlockNamingPacket`]) carries member paths + dominant kinds +
//!   top symbols — NEVER file bodies (§2a).
//! - **The runner's output is HOSTILE input (o5).** [`sanitize_naming`] rejects
//!   control chars, active HTML/Markdown, URLs, emails, path separators, home
//!   tildes, and token/base64/hash-like strings; enforces the declared schema
//!   (name ≤ 40 chars, one-line purpose ≤ 120); strips to plain text. ANY violation
//!   → the block keeps its honest heuristic label. A runner name never enters any
//!   field but a sanitized `name`/`purpose`.
//! - **Partial is normal (§2a).** Every block falls back INDIVIDUALLY; one bad
//!   response never poisons its batch.
//! - **Two consumers, one engine (§2b).** The scan path applies results straight to
//!   the candidate seed BEFORE it is stored ([`apply_naming_results`]); the
//!   in-screen path (F11-c) compiles the same results into `candidate_edit` rename
//!   ops with the runner seat ([`rename_ops_from_results`]) so provenance and OCC
//!   hold — never rewriting the store from outside.
//!
//! The daemon side (the `/name` endpoint, per-block timeout, bounded parallelism)
//! lives in `m1nd-runnerd`; it uses [`parse_and_sanitize_line`] so only sanitized
//! content ever rides the wire — and the owner re-sanitizes on receipt anyway
//! (defense in depth; the trust boundary is the LLM, not the loopback).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::candidate_edit::EditOp;
use crate::skeleton_scan::CandidateBlockReport;
use crate::system_blocks::{NamedBy, SeedFile};

/// The declared response schema (§2c): a block name is at most this many chars.
pub const NAME_MAX_CHARS: usize = 40;
/// The declared response schema (§2c): a one-line purpose is at most this many chars.
pub const PURPOSE_MAX_CHARS: usize = 120;

/// How many member paths a naming packet materializes (the honest total rides
/// beside it). Enough to name a block; never the whole repo listing.
pub const PACKET_MEMBER_PATHS_CAP: usize = 40;
/// How many top symbols a naming packet materializes.
pub const PACKET_SYMBOLS_CAP: usize = 12;

/// The instruction serialized into every naming packet (§2c: the response is ONE
/// JSON line, schema-gated and sanitized on both sides).
pub const PACKET_INSTRUCTION: &str = "Name this code block from the data in this JSON. Reply with EXACTLY one line of JSON and nothing else: {\"name\":\"...\",\"purpose\":\"...\"} — name is a short human module name (max 40 chars), purpose is one plain-text line (max 120 chars). No markdown, no URLs, no paths, no extra keys, no surrounding text.";

/// One block's naming packet (§2a): what the runner sees — member list + dominant
/// kinds + top symbols, NO file bodies. Built by the scan from data it already has.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockNamingPacket {
    /// The task instruction, serialized INTO every packet so a generic LLM-backed
    /// runner works out of the box — field-proven necessary: a real CLI runner
    /// receiving bare data (no task, no format) wandered past its timeout on
    /// every live call. A non-LLM runner may ignore it.
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

/// One block's naming outcome on the `/name` wire (daemon → owner). `ok:false`
/// carries the honest per-block error; partial batches are normal (§2a).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NameCallResult {
    pub block_id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A sanitized, schema-conforming naming (the ONLY shape allowed into a block).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedNaming {
    pub name: String,
    pub purpose: String,
}

/// The raw JSON line a naming runner prints (§2c). Both fields REQUIRED — a
/// missing field is a parse failure and the block falls back honestly.
#[derive(Debug, Deserialize)]
struct RawNamingLine {
    name: String,
    purpose: String,
}

/// Parse ONE runner stdout line (`{"name":"...","purpose":"..."}`) and sanitize it
/// (o5). Any parse or sanitization failure returns the honest reason; the caller
/// falls back to the heuristic label for that block.
pub fn parse_and_sanitize_line(line: &str) -> Result<SanitizedNaming, String> {
    let raw: RawNamingLine =
        serde_json::from_str(line.trim()).map_err(|e| format!("invalid naming JSON line: {e}"))?;
    sanitize_naming(&raw.name, &raw.purpose)
}

/// The o5 gate: a runner name/purpose is UNTRUSTED input. Reject (→ heuristic
/// fallback, marked) on any violation; strip accepted values to plain text
/// (trim + collapse internal whitespace). The rules, per field:
/// - control characters (this also enforces "one line": a newline is a control char);
/// - length over the declared schema (name ≤ 40, purpose ≤ 120 chars);
/// - active HTML (`<`/`>`) and active Markdown (links `](`, images `![`,
///   backticks, a leading `#`);
/// - URLs (`http://`, `https://`, `www.`);
/// - path separators (`/`, `\`) and home tildes (`~`) — no paths of any kind;
/// - email-like text;
/// - token/base64-like strings (≥ 24 chars of base64 charset with digits) and
///   hash-like strings (≥ 32 hex chars).
///
/// The error string names the field and the class, never echoing hostile content.
pub fn sanitize_naming(raw_name: &str, raw_purpose: &str) -> Result<SanitizedNaming, String> {
    let name = sanitize_field("name", raw_name, NAME_MAX_CHARS)?;
    let purpose = sanitize_field("purpose", raw_purpose, PURPOSE_MAX_CHARS)?;
    Ok(SanitizedNaming { name, purpose })
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

/// What [`apply_naming_results`] did, per block — the honest partial ledger.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NamingApplySummary {
    /// Blocks now runner-named (`named_by:"runner"`, `needs_owner_naming:false`).
    pub named: Vec<String>,
    /// Blocks that kept their heuristic label, with the honest reason.
    pub fallback: Vec<(String, String)>,
}

/// Apply `/name` results to a freshly scanned candidate seed (the §2a scan path),
/// BEFORE the seed is stored. Every `ok` result is RE-sanitized here (the owner
/// trusts no wire); on pass the block gets the runner name/purpose and its
/// `candidate_meta` flips to `named_by:"runner"`, `needs_owner_naming:false` — on
/// any violation, error, or unknown id the block keeps its heuristic label and the
/// reason is recorded. `report_blocks` (the scan report) is kept in sync.
pub fn apply_naming_results(
    seed: &mut SeedFile,
    report_blocks: &mut [CandidateBlockReport],
    results: &[NameCallResult],
) -> NamingApplySummary {
    let mut summary = NamingApplySummary::default();
    for result in results {
        let reason: String = if result.ok {
            match sanitize_naming(
                result.name.as_deref().unwrap_or(""),
                result.purpose.as_deref().unwrap_or(""),
            ) {
                Ok(clean) => {
                    let Some(block) = seed
                        .blocks
                        .iter_mut()
                        .find(|b| b.block_id == result.block_id)
                    else {
                        summary
                            .fallback
                            .push((result.block_id.clone(), "unknown block id".to_string()));
                        continue;
                    };
                    let Some(meta) = block.candidate_meta.as_mut() else {
                        summary.fallback.push((
                            result.block_id.clone(),
                            "block has no candidate_meta to stamp provenance on".to_string(),
                        ));
                        continue;
                    };
                    block.name = clean.name;
                    block.purpose = clean.purpose;
                    meta.named_by = NamedBy::Runner;
                    meta.needs_owner_naming = false;
                    let (name, meta) = (block.name.clone(), meta.clone());
                    if let Some(rb) = report_blocks
                        .iter_mut()
                        .find(|rb| rb.block_id == result.block_id)
                    {
                        rb.name = name;
                        rb.candidate_meta = meta;
                    }
                    summary.named.push(result.block_id.clone());
                    continue;
                }
                Err(reason) => reason,
            }
        } else {
            result
                .error
                .clone()
                .unwrap_or_else(|| "runner error".to_string())
        };
        summary.fallback.push((result.block_id.clone(), reason));
    }
    summary
}

/// Compile `/name` results into `candidate_edit` rename ops (the §2b in-screen
/// path — the engine only; the button is F11-c). Each `ok` result is re-sanitized;
/// passes become `rename` ops the caller applies through `candidate_edit` with the
/// RUNNER seat (`by:"runner"`), so provenance (`named_by:"runner"`) and OCC hold —
/// the store is never rewritten from outside. Returns the ops plus the honest
/// per-block fallback list.
pub fn rename_ops_from_results(results: &[NameCallResult]) -> (Vec<EditOp>, Vec<(String, String)>) {
    let mut ops = Vec::new();
    let mut fallback = Vec::new();
    for result in results {
        if !result.ok {
            fallback.push((
                result.block_id.clone(),
                result
                    .error
                    .clone()
                    .unwrap_or_else(|| "runner error".to_string()),
            ));
            continue;
        }
        match sanitize_naming(
            result.name.as_deref().unwrap_or(""),
            result.purpose.as_deref().unwrap_or(""),
        ) {
            Ok(clean) => ops.push(EditOp::Rename {
                block_id: result.block_id.clone(),
                name: Some(clean.name),
                purpose: Some(clean.purpose),
            }),
            Err(reason) => fallback.push((result.block_id.clone(), reason)),
        }
    }
    (ops, fallback)
}

// ===========================================================================
// The loopback `/name` client — owner → runner daemon.
//
// Deliberately a minimal std::net HTTP/1.1 client: the daemon is OUR axum server
// on 127.0.0.1 answering a Content-Length JSON body, and this module must build
// on every dependency edge (m1nd-runnerd consumes this crate with
// default-features = false, so the feature-gated reqwest is not available here).
// `Connection: close` + a read deadline keep it simple and honest.
// ===========================================================================

/// POST the naming packets to an announced daemon's `/name` and return the
/// per-block results. `runner_id: None` lets the DAEMON resolve its pinned
/// naming-runner (announce carries no capability, so the owner cannot know which
/// announced id is the naming one — §5a). A non-200 answer or transport failure is
/// an `Err` carrying the daemon's honest keyword; the caller falls back to
/// heuristic naming for the whole batch.
pub fn call_name_endpoint(
    port: u16,
    secret: &str,
    packets: &[BlockNamingPacket],
    timeout: Duration,
) -> Result<Vec<NameCallResult>, String> {
    use std::io::{Read, Write};

    let blocks: Vec<serde_json::Value> = packets
        .iter()
        .map(|p| json!({ "block_id": p.block_id, "packet": p }))
        .collect();
    let body = serde_json::to_string(&json!({ "blocks": blocks })).map_err(|e| e.to_string())?;

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let connect_timeout = timeout.min(Duration::from_secs(5));
    let mut stream = std::net::TcpStream::connect_timeout(&addr, connect_timeout)
        .map_err(|e| format!("connect to the runner daemon on port {port} failed: {e}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(connect_timeout));

    let request = format!(
        "POST /name HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\n{}: {secret}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        crate::runnerd_owner::RUNNERD_SECRET_HEADER,
        body.len(),
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("write to the runner daemon failed: {e}"))?;

    // Read until EOF (connection: close). A mid-read timeout keeps what arrived —
    // if the response is already complete it still parses.
    let mut raw = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
            Err(e) => {
                if raw.is_empty() {
                    return Err(format!("read from the runner daemon failed: {e}"));
                }
                break;
            }
        }
    }

    parse_name_response(&raw)
}

/// Parse the daemon's raw HTTP response: status line + a JSON body (Content-Length
/// honored when present, else the remainder). Split out for direct testing.
fn parse_name_response(raw: &[u8]) -> Result<Vec<NameCallResult>, String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "malformed HTTP response from the runner daemon".to_string())?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let status: u16 = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "malformed HTTP status line from the runner daemon".to_string())?;
    let mut body = &raw[split + 4..];
    // Honor Content-Length when present (our axum daemon always sets it for Json).
    for line in head.lines().skip(1) {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                if let Ok(len) = v.trim().parse::<usize>() {
                    if len <= body.len() {
                        body = &body[..len];
                    }
                }
            }
        }
    }
    if status == 401 {
        return Err("unauthorized (401): the runner daemon refused the shared secret".to_string());
    }
    let value: serde_json::Value = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(body)
            .map_err(|e| format!("invalid JSON from the runner daemon (status {status}): {e}"))?
    };
    if status != 200 {
        let keyword = value
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("runner_error");
        let detail = value
            .get("detail")
            .and_then(|v| v.as_str())
            .unwrap_or("the runner daemon refused the naming call");
        return Err(format!("{keyword}: {detail}"));
    }
    let results = value
        .get("results")
        .cloned()
        .ok_or_else(|| "the runner daemon's answer carries no results".to_string())?;
    serde_json::from_value::<Vec<NameCallResult>>(results)
        .map_err(|e| format!("invalid results from the runner daemon: {e}"))
}

// ===========================================================================
// The scan-path orchestration (§2a) — try each announced daemon, apply results.
// ===========================================================================

/// What the scan-path naming attempt did — the handler folds this into the scan's
/// `NamingReport` honestly.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScanNamingOutcome {
    /// Blocks now runner-named.
    pub named: Vec<String>,
    /// Blocks that kept their heuristic label, with the honest per-block reason.
    pub fallback: Vec<(String, String)>,
    /// Set when NO announced daemon completed the call (transport failure or an
    /// honest refusal like `no_naming_runner`) — every block stays heuristic.
    pub transport_error: Option<String>,
}

/// The owner-side wait budget for one whole `/name` batch, capped under the tool
/// dispatch timeout so a slow runner degrades to the honest heuristic fallback
/// instead of killing the scan. Field-measured: a real CLI-backed naming runner
/// takes ~50s per call (process startup dominates), and the daemon's per-block
/// timeout is operator-configurable up to that scale — a 25s-per-wave budget cut
/// live batches mid-read (EAGAIN at the exact budget) while the daemon was still
/// legitimately working. One wave now gets the full sub-dispatch window; larger
/// batches keep the same cap and surface the partial result honestly.
fn scan_naming_timeout(block_count: usize) -> Duration {
    let waves = block_count.div_ceil(4).max(1) as u64;
    Duration::from_secs((10 + 95 * waves).min(110))
}

/// Run the whole §2a scan-path naming attempt: try each announced daemon port in
/// order (announce carries no capability, so the daemon resolves its own naming
/// pin — a daemon without one refuses honestly and the next port is tried), and
/// apply the first completed call's results to the seed + report. Partial results
/// are applied per block; a total transport failure returns the honest error and
/// touches nothing.
pub fn run_scan_naming(
    handle: &crate::runnerd_owner::NamingRunnerHandle,
    secret: &str,
    packets: &[BlockNamingPacket],
    seed: &mut SeedFile,
    report_blocks: &mut [CandidateBlockReport],
) -> ScanNamingOutcome {
    let ports = handle.registry.live_ports();
    let timeout = scan_naming_timeout(packets.len());
    let mut last_err = "no announced runner daemon".to_string();
    for port in ports {
        match call_name_endpoint(port, secret, packets, timeout) {
            Ok(results) => {
                let applied = apply_naming_results(seed, report_blocks, &results);
                return ScanNamingOutcome {
                    named: applied.named,
                    fallback: applied.fallback,
                    transport_error: None,
                };
            }
            Err(e) => last_err = e,
        }
    }
    ScanNamingOutcome {
        named: Vec::new(),
        fallback: Vec::new(),
        transport_error: Some(last_err),
    }
}

// ===========================================================================
// The candidate_naming route core (F11-c §2b — the in-screen "Name with runner").
// ===========================================================================

/// What a `candidate_naming` call did — the wire result the screen reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateNamingOutcome {
    /// The store version AFTER the call (bumped once iff any rename applied).
    pub store_version: u64,
    /// Blocks now runner-named (through `candidate_edit`, runner seat).
    pub named: Vec<String>,
    /// Blocks that kept their label, with the honest per-block reason.
    pub fell_back: Vec<(String, String)>,
    /// The honest whole-call refusal when NO naming call could run (no daemon, no
    /// naming runner pinned, transport failure) — the screen disables/tells why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
}

/// Select the blocks a `candidate_naming` call targets: explicit ids (each must
/// exist — no silent skip), or absent → every block still carrying an untouched
/// provisional name (`needs_owner_naming == true`).
pub fn select_naming_targets<'a>(
    store: &'a crate::system_blocks::SystemBlockStore,
    block_ids: Option<&[String]>,
) -> Result<Vec<&'a crate::system_blocks::SystemBlock>, crate::system_blocks::SeedError> {
    match block_ids {
        Some(ids) => ids
            .iter()
            .map(|id| {
                store
                    .blocks
                    .iter()
                    .find(|b| b.block_id.as_str() == id.as_str())
                    .ok_or_else(|| crate::system_blocks::SeedError::BlockNotFound {
                        block_id: id.clone(),
                    })
            })
            .collect(),
        None => Ok(store
            .blocks
            .iter()
            .filter(|b| {
                b.candidate_meta
                    .as_ref()
                    .is_some_and(|m| m.needs_owner_naming)
            })
            .collect()),
    }
}

/// The whole `candidate_naming` transaction (F11-c §2b): call the announced
/// daemon's `/name` with the given packets, compile the sanitized results into
/// `candidate_edit` rename ops, and apply them UNDER THE GIVEN OCC KEY with the
/// RUNNER seat — provenance and OCC hold; the store is never rewritten from
/// outside the verb. Honest outcomes:
/// - stale `expected_store_version` → [`SeedError::Conflict`] BEFORE any network
///   call (nothing ran, nothing applied);
/// - no daemon / no naming runner / transport failure → `Ok` with `refusal`
///   (every block keeps its label; the screen says why);
/// - partial results → the ok blocks land in ONE `candidate_edit` batch; the rest
///   ride `fell_back` with their reasons;
/// - zero ok results → no write at all (`store_version` unchanged).
pub fn name_candidate_blocks(
    handle: &crate::runnerd_owner::NamingRunnerHandle,
    dir: &std::path::Path,
    expected_store_version: u64,
    packets: &[BlockNamingPacket],
) -> Result<CandidateNamingOutcome, crate::system_blocks::SeedError> {
    use crate::system_blocks::{SeedError, SystemBlockStore};

    let store = SystemBlockStore::load(dir)?.ok_or(SeedError::NoStore)?;
    // OCC pre-check: a stale caller conflicts BEFORE any runner is invoked.
    if expected_store_version != store.store_version {
        return Err(SeedError::Conflict {
            expected: expected_store_version,
            actual: store.store_version,
        });
    }
    let unchanged = |refusal: Option<String>, fell_back: Vec<(String, String)>| {
        Ok(CandidateNamingOutcome {
            store_version: store.store_version,
            named: Vec::new(),
            fell_back,
            refusal,
        })
    };
    if packets.is_empty() {
        // Nothing needs naming — an honest no-op, never an error.
        return unchanged(None, Vec::new());
    }
    let Some(secret) = crate::runnerd_owner::read_secret(&handle.owner_runtime_root) else {
        return unchanged(
            Some(
                "no_naming_runner: no runner daemon has booted here (no shared secret on disk)"
                    .to_string(),
            ),
            Vec::new(),
        );
    };
    let ports = handle.registry.live_ports();
    if ports.is_empty() {
        return unchanged(
            Some("no_naming_runner: no runner daemon announced — start m1nd-runnerd with a pinned naming-runner".to_string()),
            Vec::new(),
        );
    }

    let timeout = scan_naming_timeout(packets.len());
    let mut last_err = String::new();
    for port in ports {
        match call_name_endpoint(port, &secret, packets, timeout) {
            Ok(results) => {
                let (ops, fell_back) = rename_ops_from_results(&results);
                if ops.is_empty() {
                    // Every block fell back — nothing to write, versions untouched.
                    return unchanged(None, fell_back);
                }
                let named: Vec<String> = ops
                    .iter()
                    .filter_map(|op| match op {
                        EditOp::Rename { block_id, .. } => Some(block_id.clone()),
                        _ => None,
                    })
                    .collect();
                let new_store = crate::system_blocks::candidate_edit_in_dir(
                    dir,
                    expected_store_version,
                    &ops,
                    crate::candidate_edit::EditSeat::Runner,
                )?;
                return Ok(CandidateNamingOutcome {
                    store_version: new_store.store_version,
                    named,
                    fell_back,
                    refusal: None,
                });
            }
            Err(e) => last_err = e,
        }
    }
    unchanged(Some(format!("no_naming_runner: {last_err}")), Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate_edit::EditSeat;
    use crate::system_blocks::{
        candidate_edit_in_dir, CandidateMeta, Layout, MembershipEntry, MembershipRole,
        MembershipSource, ReceiptContract, SeedRatification, SeedRepo, SeedSkeleton,
        SeedSkeletonState, Sockets, SystemBlock, SystemBlockKind, SystemBlockState,
        SystemBlockStore, UnmappedDefaultAction, UnmappedPolicy, SYSTEM_BLOCK_SEED_SCHEMA,
    };

    fn heuristic_block(id: &str) -> SystemBlock {
        SystemBlock {
            block_id: id.to_string(),
            name: format!("Heuristic {id}"),
            purpose: "Provisional.".to_string(),
            kind: SystemBlockKind::Scanned,
            state: SystemBlockState::Candidate,
            boundary_version: 1,
            contract_version: 1,
            membership_source: MembershipSource::Proposed,
            membership: vec![MembershipEntry {
                path: format!("src/{id}.rs"),
                role: MembershipRole::Primary,
                optional: false,
            }],
            sockets: Sockets {
                inputs: Vec::new(),
                outputs: Vec::new(),
                external: Vec::new(),
            },
            receipt_contract: ReceiptContract {
                version: 1,
                required: Vec::new(),
                optional: Vec::new(),
                waived: Vec::new(),
                declared_by: None,
                declared_at: None,
            },
            receipts: Vec::new(),
            layout: Layout {
                x: None,
                y: None,
                locked: false,
                algorithm_seed: None,
                version: 1,
            },
            unmapped_residue: Vec::new(),
            membership_fingerprint: None,
            resolved_members: Vec::new(),
            pre_archive_state: None,
            candidate_meta: Some(CandidateMeta {
                named_by: NamedBy::Heuristic,
                needs_owner_naming: true,
                graph_cohesion: None,
                edge_sample_size: 0,
                directory_support: 1.0,
                coverage_ratio: 1.0,
                shared_member_count: 0,
            }),
        }
    }

    fn seed_of(blocks: Vec<SystemBlock>) -> SeedFile {
        SeedFile {
            schema: SYSTEM_BLOCK_SEED_SCHEMA.to_string(),
            repo: SeedRepo {
                repo_id: "r".to_string(),
                root: ".".to_string(),
                source_commit: "c".to_string(),
            },
            skeleton: SeedSkeleton {
                skeleton_id: "sk".to_string(),
                version: 1,
                state: SeedSkeletonState::Candidate,
                ratification: SeedRatification {
                    method: String::new(),
                    ratifier: String::new(),
                    ratified_at: String::new(),
                    commit: String::new(),
                },
            },
            blocks,
            unmapped_policy: UnmappedPolicy {
                visible: true,
                default_action: UnmappedDefaultAction::LeaveUnmappedUntilRatified,
            },
        }
    }

    fn ok_result(block_id: &str, name: &str, purpose: &str) -> NameCallResult {
        NameCallResult {
            block_id: block_id.to_string(),
            ok: true,
            name: Some(name.to_string()),
            purpose: Some(purpose.to_string()),
            error: None,
        }
    }

    fn err_result(block_id: &str, error: &str) -> NameCallResult {
        NameCallResult {
            block_id: block_id.to_string(),
            ok: false,
            name: None,
            purpose: None,
            error: Some(error.to_string()),
        }
    }

    // --- o5: the table-driven sanitizer — one case per injection class ---------

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
            let err = sanitize_naming(name, clean_purpose)
                .expect_err(&format!("case '{label}' must be rejected"));
            assert!(
                err.contains(want),
                "case '{label}': expected reason containing '{want}', got '{err}'"
            );
            assert!(err.starts_with("name:"), "reason names the field: {err}");
        }

        // Purpose-specific classes: over-length and multi-line.
        let long_purpose = "p".repeat(PURPOSE_MAX_CHARS + 1);
        let err = sanitize_naming("Clean Name", &long_purpose).expect_err("over-long purpose");
        assert!(err.contains("exceeds 120 chars"), "got '{err}'");
        let err =
            sanitize_naming("Clean Name", "line one\nline two").expect_err("multi-line purpose");
        assert!(err.contains("control character"), "got '{err}'");
    }

    #[test]
    fn sanitizer_accepts_clean_input_with_correct_strip() {
        let out = sanitize_naming("  Auth   Service  ", "  Owns   login and sessions.  ")
            .expect("clean input passes");
        assert_eq!(out.name, "Auth Service", "whitespace collapsed + trimmed");
        assert_eq!(out.purpose, "Owns login and sessions.");

        // Boundary lengths pass exactly.
        let name40: String = "n".repeat(NAME_MAX_CHARS);
        let purpose120: String = "p".repeat(PURPOSE_MAX_CHARS);
        let out = sanitize_naming(&name40, &purpose120).expect("boundary lengths pass");
        assert_eq!(out.name.chars().count(), NAME_MAX_CHARS);
        assert_eq!(out.purpose.chars().count(), PURPOSE_MAX_CHARS);

        // Plain long words survive the token heuristic; a "C# Bindings" name passes
        // the leading-# rule (only a LEADING heading marker is active Markdown).
        sanitize_naming("Internationalization Layer", "Long words are fine.")
            .expect("long plain words pass");
        sanitize_naming("C# Bindings", "Interop surface.").expect("internal # passes");
    }

    #[test]
    fn parse_line_rejects_invalid_json_and_missing_field() {
        // Invalid JSON → honest parse failure.
        let err = parse_and_sanitize_line("not json at all").expect_err("garbage rejected");
        assert!(err.contains("invalid naming JSON line"), "got '{err}'");
        // Missing field → the same fallback path.
        let err = parse_and_sanitize_line(r#"{"name":"Only A Name"}"#)
            .expect_err("missing purpose rejected");
        assert!(err.contains("missing field"), "got '{err}'");
        // A valid line parses AND sanitizes.
        let out = parse_and_sanitize_line(r#"{"name":"  Auth ","purpose":"Owns login."}"#)
            .expect("valid line passes");
        assert_eq!(out.name, "Auth");
        // A valid line with hostile content still falls to the sanitizer.
        let err = parse_and_sanitize_line(r#"{"name":"<script>x</script>","purpose":"p"}"#)
            .expect_err("hostile content rejected after parse");
        assert!(err.contains("HTML markup"), "got '{err}'");
    }

    // --- §2a: partial application to a scanned seed ----------------------------

    #[test]
    fn apply_naming_results_partial_marks_only_ok_blocks() {
        let mut seed = seed_of(vec![
            heuristic_block("sb_a"),
            heuristic_block("sb_b"),
            heuristic_block("sb_c"),
        ]);
        let mut report_blocks: Vec<CandidateBlockReport> = seed
            .blocks
            .iter()
            .map(|b| CandidateBlockReport {
                block_id: b.block_id.clone(),
                name: b.name.clone(),
                member_count: 1,
                membership_count: 1,
                dominant_directory: "src".to_string(),
                candidate_meta: b.candidate_meta.clone().unwrap(),
            })
            .collect();

        let results = vec![
            ok_result("sb_a", "Auth", "Owns login and sessions."),
            err_result("sb_b", "naming runner timed out after 20s"),
            ok_result("sb_c", "<script>evil</script>", "hostile"),
        ];
        let summary = apply_naming_results(&mut seed, &mut report_blocks, &results);

        assert_eq!(summary.named, vec!["sb_a".to_string()], "one runner-named");
        assert_eq!(summary.fallback.len(), 2, "two honest fallbacks");

        // sb_a: runner-named, provenance stamped, report in sync.
        let a = &seed.blocks[0];
        assert_eq!(a.name, "Auth");
        let meta = a.candidate_meta.as_ref().unwrap();
        assert_eq!(meta.named_by, NamedBy::Runner);
        assert!(!meta.needs_owner_naming);
        assert_eq!(report_blocks[0].name, "Auth");
        assert_eq!(report_blocks[0].candidate_meta.named_by, NamedBy::Runner);

        // sb_b (timeout) and sb_c (hostile): heuristic, still needing the owner.
        for i in [1usize, 2] {
            let b = &seed.blocks[i];
            let meta = b.candidate_meta.as_ref().unwrap();
            assert_eq!(
                meta.named_by,
                NamedBy::Heuristic,
                "{} stays heuristic",
                b.block_id
            );
            assert!(
                meta.needs_owner_naming,
                "{} still needs the owner",
                b.block_id
            );
            assert!(b.name.starts_with("Heuristic"), "name untouched");
        }
        // The hostile fallback carries the sanitizer's honest class.
        let (id, reason) = &summary.fallback[1];
        assert_eq!(id, "sb_c");
        assert!(reason.contains("HTML markup"), "got '{reason}'");
    }

    // --- §2b: the in-screen engine — results → rename ops → candidate_edit -----

    #[test]
    fn rename_ops_apply_via_candidate_edit_with_runner_seat() {
        let results = vec![
            ok_result("sb_a", "Auth", "Owns login."),
            ok_result("sb_b", "see https://evil.example", "hostile"),
            err_result("sb_c", "parse failed"),
        ];
        let (ops, fallback) = rename_ops_from_results(&results);
        assert_eq!(ops.len(), 1, "only the sanitized-ok result becomes an op");
        assert_eq!(fallback.len(), 2);
        assert!(matches!(&ops[0], EditOp::Rename { block_id, .. } if block_id == "sb_a"));

        // The ops land through candidate_edit with the RUNNER seat — provenance and
        // OCC hold; the store is never rewritten from outside.
        let dir = tempfile::tempdir().expect("tempdir");
        let seed = seed_of(vec![heuristic_block("sb_a")]);
        SystemBlockStore::from_seed(seed)
            .save(dir.path())
            .expect("save");
        let store =
            candidate_edit_in_dir(dir.path(), 1, &ops, EditSeat::Runner).expect("ops apply");
        assert_eq!(store.store_version, 2, "one OCC bump");
        let a = &store.blocks[0];
        assert_eq!(a.name, "Auth");
        let meta = a.candidate_meta.as_ref().unwrap();
        assert_eq!(meta.named_by, NamedBy::Runner, "runner provenance stamped");
        assert!(
            !meta.needs_owner_naming,
            "runner-named blocks are ratifiable"
        );
    }

    // --- the loopback /name client against a canned fake daemon ----------------

    /// A minimal fake daemon: accepts ONE connection, reads the request, asserts
    /// the secret header, answers with the canned body. Never a real runner.
    fn spawn_fake_daemon(
        status_line: &'static str,
        body: String,
        expect_secret: &'static str,
    ) -> (u16, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            let mut req = Vec::new();
            let mut chunk = [0u8; 4096];
            // Read until the full headers + declared content-length arrive.
            loop {
                let n = sock.read(&mut chunk).expect("read");
                req.extend_from_slice(&chunk[..n]);
                if let Some(pos) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&req[..pos]).to_string();
                    let want: usize = head
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.trim()
                                .eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse().ok())?
                        })
                        .unwrap_or(0);
                    if req.len() >= pos + 4 + want {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let request_text = String::from_utf8_lossy(&req).to_string();
            assert!(
                request_text.contains(&format!("x-runnerd-secret: {expect_secret}")),
                "the client must send the shared secret header: {request_text}"
            );
            let response = format!(
                "{status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(response.as_bytes()).expect("write");
            request_text
        });
        (port, handle)
    }

    #[test]
    fn call_name_endpoint_round_trips_results_and_sends_the_secret() {
        let body = serde_json::to_string(&json!({
            "runner_id": "namer-1",
            "results": [
                {"block_id": "sb_a", "ok": true, "name": "Auth", "purpose": "Owns login."},
                {"block_id": "sb_b", "ok": false, "error": "timed out"},
            ]
        }))
        .unwrap();
        let (port, daemon) = spawn_fake_daemon("HTTP/1.1 200 OK", body, "s3cr3t");

        let packets = vec![BlockNamingPacket {
            instruction: PACKET_INSTRUCTION.to_string(),
            block_id: "sb_a".to_string(),
            member_count: 1,
            member_paths: vec!["src/a.rs".to_string()],
            dominant_kinds: vec!["functions".to_string()],
            top_symbols: vec!["login".to_string()],
            dominant_directory: "src".to_string(),
        }];
        let results =
            call_name_endpoint(port, "s3cr3t", &packets, Duration::from_secs(5)).expect("call ok");
        assert_eq!(results.len(), 2);
        assert!(results[0].ok);
        assert_eq!(results[0].name.as_deref(), Some("Auth"));
        assert!(!results[1].ok);
        assert_eq!(results[1].error.as_deref(), Some("timed out"));

        // The request the daemon saw carried the packets (block_id on the wire).
        let request_text = daemon.join().expect("daemon thread");
        assert!(request_text.contains("\"block_id\":\"sb_a\""));
        assert!(
            !request_text.contains("runner_id"),
            "the owner lets the daemon resolve its pinned naming runner"
        );
    }

    #[test]
    fn call_name_endpoint_surfaces_daemon_refusals_and_401() {
        // A 403 refusal surfaces the daemon's keyword verbatim.
        let body =
            json!({"error": "no_naming_runner", "detail": "no naming-runner pinned"}).to_string();
        let (port, _daemon) = spawn_fake_daemon("HTTP/1.1 403 Forbidden", body, "s");
        let err = call_name_endpoint(port, "s", &[], Duration::from_secs(5))
            .expect_err("a refusal maps to Err");
        assert!(err.contains("no_naming_runner"), "got '{err}'");

        // A bare 401 is the secret law.
        let (port, _daemon) = spawn_fake_daemon("HTTP/1.1 401 Unauthorized", String::new(), "s");
        let err = call_name_endpoint(port, "s", &[], Duration::from_secs(5))
            .expect_err("401 maps to Err");
        assert!(err.contains("401"), "got '{err}'");

        // A dead port is a transport error, never a panic.
        let err = call_name_endpoint(1, "s", &[], Duration::from_millis(300))
            .expect_err("dead port errs");
        assert!(err.contains("failed"), "got '{err}'");
    }

    // --- F11-c: the candidate_naming route core (mirrors the F11-b tests) ------

    use crate::runnerd_owner::{secret_path, NamingRunnerHandle, RunnerdRegistry};
    use crate::system_blocks::SeedError;

    /// A tempdir store + an owner runtime root with the shared secret + a registry.
    fn naming_fixture(
        blocks: Vec<SystemBlock>,
    ) -> (tempfile::TempDir, std::path::PathBuf, NamingRunnerHandle) {
        let temp = tempfile::tempdir().expect("tempdir");
        let store_dir = temp.path().join("brain");
        std::fs::create_dir_all(&store_dir).expect("store dir");
        SystemBlockStore::from_seed(seed_of(blocks))
            .save(&store_dir)
            .expect("save");
        let owner_rt = temp.path().join("owner-rt");
        std::fs::create_dir_all(&owner_rt).expect("owner rt");
        std::fs::write(secret_path(&owner_rt), "route-secret").expect("secret");
        let handle = NamingRunnerHandle {
            registry: std::sync::Arc::new(RunnerdRegistry::default()),
            owner_runtime_root: owner_rt,
        };
        (temp, store_dir, handle)
    }

    fn packet_for(block_id: &str) -> BlockNamingPacket {
        BlockNamingPacket {
            instruction: PACKET_INSTRUCTION.to_string(),
            block_id: block_id.to_string(),
            member_count: 1,
            member_paths: vec![format!("src/{block_id}.rs")],
            dominant_kinds: vec!["functions".to_string()],
            top_symbols: Vec::new(),
            dominant_directory: "src".to_string(),
        }
    }

    #[test]
    fn candidate_naming_names_targets_via_candidate_edit() {
        let (_temp, dir, handle) =
            naming_fixture(vec![heuristic_block("sb_a"), heuristic_block("sb_b")]);
        // A fake daemon that names sb_a and fails sb_b — partial is normal.
        let body = serde_json::to_string(&json!({
            "runner_id": "namer-1",
            "results": [
                {"block_id": "sb_a", "ok": true, "name": "Auth", "purpose": "Owns login."},
                {"block_id": "sb_b", "ok": false, "error": "timed out"},
            ]
        }))
        .unwrap();
        let (port, _daemon) = spawn_fake_daemon("HTTP/1.1 200 OK", body, "route-secret");
        handle.registry.register(&["namer-1".to_string()], port, 1);

        let outcome =
            name_candidate_blocks(&handle, &dir, 1, &[packet_for("sb_a"), packet_for("sb_b")])
                .expect("the naming call lands");
        assert_eq!(outcome.named, vec!["sb_a".to_string()]);
        assert_eq!(outcome.fell_back.len(), 1);
        assert_eq!(
            outcome.store_version, 2,
            "one candidate_edit batch, one bump"
        );
        assert!(outcome.refusal.is_none());

        // The PERSISTED store carries the runner provenance through candidate_edit.
        let store = SystemBlockStore::load(&dir).unwrap().unwrap();
        let a = store.blocks.iter().find(|b| b.block_id == "sb_a").unwrap();
        assert_eq!(a.name, "Auth");
        let meta = a.candidate_meta.as_ref().unwrap();
        assert_eq!(meta.named_by, NamedBy::Runner);
        assert!(!meta.needs_owner_naming);
        let b = store.blocks.iter().find(|b| b.block_id == "sb_b").unwrap();
        assert!(
            b.candidate_meta.as_ref().unwrap().needs_owner_naming,
            "the failed block keeps its provisional label"
        );
    }

    #[test]
    fn candidate_naming_refuses_honestly_without_runnerd() {
        let (_temp, dir, handle) = naming_fixture(vec![heuristic_block("sb_a")]);
        // No daemon announced (empty registry) → the honest refusal, nothing touched.
        let before = std::fs::read(dir.join("system_blocks.json")).expect("read before");
        let outcome = name_candidate_blocks(&handle, &dir, 1, &[packet_for("sb_a")])
            .expect("a refusal is a result, not an exception");
        assert!(outcome.named.is_empty());
        assert_eq!(outcome.store_version, 1, "no version churn");
        let refusal = outcome.refusal.expect("carries the honest refusal");
        assert!(refusal.contains("no_naming_runner"), "got '{refusal}'");
        let after = std::fs::read(dir.join("system_blocks.json")).expect("read after");
        assert_eq!(before, after, "the store is byte-identical");
    }

    #[test]
    fn candidate_naming_occ_conflict_before_any_network_call() {
        let (_temp, dir, handle) = naming_fixture(vec![heuristic_block("sb_a")]);
        // NO daemon is registered and none is contacted: the stale OCC key must
        // conflict FIRST (a runner is never invoked for a stale view).
        let before = std::fs::read(dir.join("system_blocks.json")).expect("read before");
        let err = name_candidate_blocks(&handle, &dir, 99, &[packet_for("sb_a")])
            .expect_err("a stale OCC key conflicts");
        assert!(matches!(
            err,
            SeedError::Conflict {
                expected: 99,
                actual: 1
            }
        ));
        let after = std::fs::read(dir.join("system_blocks.json")).expect("read after");
        assert_eq!(before, after, "nothing was applied");
    }

    #[test]
    fn select_naming_targets_picks_needs_naming_and_validates_ids() {
        let mut named = heuristic_block("sb_named");
        named.candidate_meta.as_mut().unwrap().needs_owner_naming = false;
        let store = SystemBlockStore::from_seed(seed_of(vec![
            heuristic_block("sb_a"),
            named,
            heuristic_block("sb_b"),
        ]));

        // Absent ids → only the blocks still needing a name.
        let targets = select_naming_targets(&store, None).expect("selects");
        let ids: Vec<&str> = targets.iter().map(|b| b.block_id.as_str()).collect();
        assert_eq!(ids, vec!["sb_a", "sb_b"]);

        // Explicit ids resolve (even already-named ones — a re-name is legal).
        let targets =
            select_naming_targets(&store, Some(&["sb_named".to_string()])).expect("explicit");
        assert_eq!(targets[0].block_id, "sb_named");

        // An unknown id is a hard error, never a silent skip.
        assert!(matches!(
            select_naming_targets(&store, Some(&["sb_ghost".to_string()])),
            Err(SeedError::BlockNotFound { .. })
        ));
    }
}
