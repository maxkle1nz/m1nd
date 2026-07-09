//! F2.5c — the runner daemon as a CLIENT of the OWNER (§5b: "o runnerd é cliente do
//! owner, com ?brain= do alvo"). The daemon never touches the SystemBlockStore or a
//! box directly; it posts mission letters through the owner's `mission_post` verb,
//! reads a block's scope through the owner's `system_blocks_snapshot`, and — for a
//! compose-opened mission — reads the chain head through the owner's `kind=mission`
//! mailbox. All owner-side gates (the §1 contract, the §1e head CAS, the §1d
//! landed-law) therefore still apply to every letter the daemon emits.
//!
//! The trait is the seam the mission engine writes through — the engine is generic
//! over it, so the letter chain + the gate/candidate can be proven against a fake
//! owner (recording letters) with no network.

use serde_json::json;

use m1nd_mcp::mission_letter::MissionLetter;

/// The chain head of one mission, as read from the owner (§2b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadInfo {
    pub seq: u64,
    pub letter_id: String,
}

/// The seam the mission engine posts/reads through (§5b). Async; the engine is
/// generic over it so the real HTTP client and a test fake are interchangeable.
#[allow(async_fn_in_trait)]
pub trait OwnerClient {
    /// Post one mission letter via the owner's `mission_post` verb, scoped to the
    /// target brain by `?brain=` (`None` = the owner's bound box). Returns the
    /// appended letter's `letter_id` — the value the NEXT letter chains on
    /// (`prev_letter_id`). An owner refusal (`stale_head`, contract, read-only)
    /// surfaces as `Err(detail)`; the engine never fabricates a success.
    async fn post_letter(
        &self,
        brain: Option<&str>,
        agent_id: &str,
        letter: &MissionLetter,
    ) -> Result<String, String>;

    /// Read a block's `(boundary_version, contract_version)` from a FRESH owner
    /// snapshot (§5c: the candidate scope comes from the owner, not a guess).
    /// `None` when the block is absent or the snapshot cannot be read — the engine
    /// then falls back to a conservative scope, declared at the call site.
    async fn fetch_block_scope(&self, brain: Option<&str>, block_id: &str) -> Option<(u32, u32)>;

    /// Read the current head of a compose-opened mission (§5b: when `mission_id`
    /// arrives from the compose, seq-1 `judging` already exists). `None` when the
    /// mission has no chain yet (the engine then opens seq-1 itself).
    async fn fetch_head(&self, brain: Option<&str>, mission_id: &str) -> Option<HeadInfo>;
}

/// The real loopback HTTP client to the owner (`http://127.0.0.1:<owner_port>`).
pub struct HttpOwnerClient {
    base_url: String,
    http: reqwest::Client,
}

impl HttpOwnerClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }

    /// Append `?brain=<enc>` to a path when a brain root is given (mirrors the UI
    /// client's `withBrain`); absent = the owner's bound box.
    fn with_brain(&self, path: &str, brain: Option<&str>) -> String {
        match brain.map(str::trim).filter(|s| !s.is_empty()) {
            Some(root) => {
                let sep = if path.contains('?') { '&' } else { '?' };
                format!("{}{}{}brain={}", self.base_url, path, sep, urlencode(root))
            }
            None => format!("{}{}", self.base_url, path),
        }
    }

    /// POST the owner's announce (§5a). Sent once at boot with the shared secret in
    /// the `x-runnerd-secret` header; a non-2xx is a boot failure the caller logs.
    pub async fn announce(
        &self,
        secret: &str,
        runner_ids: &[String],
        port: u16,
        boot_challenge: &str,
    ) -> Result<(), String> {
        let url = format!("{}/api/runnerd/announce", self.base_url);
        let resp = self
            .http
            .post(&url)
            .header(m1nd_mcp::runnerd_owner::RUNNERD_SECRET_HEADER, secret)
            .json(&json!({
                "runner_ids": runner_ids,
                "port": port,
                "boot_challenge": boot_challenge,
            }))
            .send()
            .await
            .map_err(|e| format!("announce transport error: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("owner refused announce: HTTP {}", status.as_u16()));
        }
        // The liveness round-trip (§5a): the owner echoes our boot_challenge.
        let body: serde_json::Value = resp.json().await.unwrap_or_else(|_| json!({}));
        if body.get("echo").and_then(|v| v.as_str()) != Some(boot_challenge) {
            return Err("owner did not echo the boot challenge — liveness unproven".to_string());
        }
        Ok(())
    }
}

impl OwnerClient for HttpOwnerClient {
    async fn post_letter(
        &self,
        brain: Option<&str>,
        agent_id: &str,
        letter: &MissionLetter,
    ) -> Result<String, String> {
        let url = self.with_brain("/api/tools/mission_post", brain);
        let resp = self
            .http
            .post(&url)
            .json(&json!({ "agent_id": agent_id, "letter": letter }))
            .send()
            .await
            .map_err(|e| format!("mission_post transport error: {e}"))?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_else(|_| json!({}));
        if !status.is_success() {
            let detail = body
                .get("detail")
                .or_else(|| body.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("mission_post refused");
            return Err(detail.to_string());
        }
        body.get("result")
            .and_then(|r| r.get("letter_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "mission_post returned no letter_id".to_string())
    }

    async fn fetch_block_scope(&self, brain: Option<&str>, block_id: &str) -> Option<(u32, u32)> {
        let url = self.with_brain("/api/tools/system_blocks_snapshot", brain);
        let resp = self
            .http
            .post(&url)
            .json(&json!({ "agent_id": "runnerd" }))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body: serde_json::Value = resp.json().await.ok()?;
        let blocks = body
            .get("result")?
            .get("store")?
            .get("blocks")?
            .as_array()?;
        let block = blocks
            .iter()
            .find(|b| b.get("block_id").and_then(|v| v.as_str()) == Some(block_id))?;
        let boundary = block.get("boundary_version").and_then(|v| v.as_u64())? as u32;
        let contract = block.get("contract_version").and_then(|v| v.as_u64())? as u32;
        Some((boundary, contract))
    }

    async fn fetch_head(&self, brain: Option<&str>, mission_id: &str) -> Option<HeadInfo> {
        let url = self.with_brain("/api/mailbox?kind=mission", brain);
        let resp = self.http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body: serde_json::Value = resp.json().await.ok()?;
        let missions = body.get("missions")?.as_array()?;
        let m = missions
            .iter()
            .find(|m| m.get("mission_id").and_then(|v| v.as_str()) == Some(mission_id))?;
        let letter_id = m
            .get("head_letter_id")
            .and_then(|v| v.as_str())?
            .to_string();
        let seq = m.get("head")?.get("mission_seq").and_then(|v| v.as_u64())?;
        Some(HeadInfo { seq, letter_id })
    }
}

/// Minimal percent-encoding for a project_root used as a `?brain=` value — encodes
/// exactly the delimiters that would break the query, nothing exotic (loopback,
/// owner-trusted paths). Avoids pulling a URL crate for one query param.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_brain_appends_selector_or_leaves_bound() {
        let c = HttpOwnerClient::new("http://127.0.0.1:1338");
        assert_eq!(
            c.with_brain("/api/tools/mission_post", None),
            "http://127.0.0.1:1338/api/tools/mission_post"
        );
        assert_eq!(
            c.with_brain("/api/tools/mission_post", Some("/repo root")),
            "http://127.0.0.1:1338/api/tools/mission_post?brain=/repo%20root"
        );
        assert_eq!(
            c.with_brain("/api/mailbox?kind=mission", Some("/r")),
            "http://127.0.0.1:1338/api/mailbox?kind=mission&brain=/r"
        );
    }
}
