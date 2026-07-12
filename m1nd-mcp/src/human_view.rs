//! human_view — the m1nd voice card (`m1nd-human-view-v0`).
//!
//! The SERVER-composed, ALREADY-MOUNTED human-readable card the `north` packet
//! carries: the m1nd voice for the human in the conversation. Agents render it
//! verbatim (inside a fenced code block) under the negative-default cadence in
//! `M1ND_INSTRUCTIONS` — they never re-compose it.
//!
//! Law of this field (askGOD verdict "human view", 2026-07-12 — the 10
//! amendments; `docs/voice/ASKGOD-VERDICT-HUMAN-VIEW.md`):
//! - the field is `human_view`, never `owner_view` ("owner" = the served owner
//!   process in this codebase) — amendment 1;
//! - MECHANICAL cap: ≤4 lines, ≤80 chars per line, wrap indents +2 inside the
//!   gutter — amendment 2 (Budget Law §C1.3);
//! - composed AFTER reception: under `caller_root_mismatch` the card IS the
//!   warning and carries ZERO statistics (they would describe the wrong
//!   brain) — amendment 3;
//! - the empty/unbound graph gets an honest `needs_ingest` card — amendment 4;
//! - ONE SENTENCE PER FACT: warning lines REUSE the exact strings already
//!   composed into `honest_gaps` (the bell line, the coherence line, the
//!   needs-ingest gap) — never a second wording; a verbatim line that cannot
//!   fit the cap falls WHOLE, never truncated — amendment 5;
//! - brand law G1 as the field's written law: only measured facts already in
//!   the packet — no uncalibrated adjective, no benefit claim — amendment 8;
//! - the mark is the SPINE: the `m1nd` wordmark + `│` (U+2502), both already
//!   accepted; any future mark (the pulse row awaits the owner's explicit
//!   stamp) plugs into `compose_voice_signature` alone — amendment 7;
//! - fail-open: composition is pure and total; `north` never becomes
//!   unavailable over its own voice.

use serde_json::json;

/// Wire schema of the card.
pub const HUMAN_VIEW_SCHEMA: &str = "m1nd-human-view-v0";

/// Mechanical cap (amendment 2): the card never exceeds 4 lines.
pub const MAX_LINES: usize = 4;
/// Mechanical cap (amendment 2): no rendered line exceeds 80 columns.
pub const MAX_COLS: usize = 80;

/// The empty/unbound-graph gap — ONE authoring site (amendment 5: one sentence
/// per fact). `handle_north` pushes this exact string into `honest_gaps`, and
/// the `needs_ingest` card wraps the SAME constant, byte-equal by construction.
pub const NEEDS_INGEST_GAP: &str =
    "The graph is empty or unbound — no codebase context is available until ingest runs.";

/// The brand constant is the WORD `m1nd` (amendment 7 — the wordmark is the
/// mark, always lowercase); the spine `│` (U+2502) is the accepted structural
/// glyph. ASCII fallback is the AGENT's duty (1:1 map `│`→`|`, `·`→`.`,
/// `—`→`-`), documented in `M1ND_INSTRUCTIONS`.
const WORDMARK: &str = "m1nd";
const SPINE: char = '│';
/// Field separator inside the identity line (U+00B7).
const SEP: &str = " · ";

/// Everything the composer needs, lifted from data ALREADY in the packet —
/// the composer performs no reads of its own (pure, total, fail-open).
pub struct HumanViewInput<'a> {
    /// `binding.trust_mode` verbatim (e.g. `full_trust`, `needs_ingest`).
    pub trust_mode: &'a str,
    /// Live graph node count (the packet's ground truth, never reformatted
    /// beyond the thousands separator).
    pub node_count: u64,
    /// `memory_exists` — the on-disk durable L1GHT claim count.
    pub memory_count: usize,
    /// `landing_bell.merge_wait` (0 = silent).
    pub merge_wait: usize,
    /// The bell line EXACTLY as pushed into `honest_gaps` (amendment 5).
    pub bell_line: Option<&'a str>,
    /// The skeleton-coherence line EXACTLY as pushed into `honest_gaps`.
    pub coherence_line: Option<&'a str>,
    /// The packet's `needs == "needs_ingest"` verdict.
    pub needs_ingest: bool,
    /// Present iff `reception.match == "caller_root_mismatch"`.
    pub reception_mismatch: Option<ReceptionMismatch<'a>>,
    /// The packet's `next_move` verbatim (whole-or-nothing on the card).
    pub next_move: &'a str,
}

/// The reception block's mismatch facts, lifted verbatim (S3 card material).
pub struct ReceptionMismatch<'a> {
    /// `reception.honest` verbatim ("this graph does NOT cover your repo").
    pub honest: &'a str,
    /// `reception.caller_root` verbatim.
    pub caller_root: &'a str,
    /// `reception.bound_workspace` verbatim.
    pub bound_workspace: &'a str,
}

/// Compose the card. Returns `None` only when nothing can be said honestly
/// (fail-open: the caller omits the field; `north` never errors here).
pub fn compose_human_view(input: &HumanViewInput) -> Option<serde_json::Value> {
    let state_sig = compose_state_sig(input);
    let (state, lines) = if let Some(mismatch) = &input.reception_mismatch {
        // Amendment 3: the card IS the warning — zero statistics, they would
        // describe the wrong brain.
        ("mismatch", compose_mismatch_lines(mismatch))
    } else if input.needs_ingest {
        // Amendment 4: the honest "I don't know this repo yet" card.
        ("needs_ingest", compose_needs_ingest_lines(input))
    } else {
        let state = if input.merge_wait > 0 {
            "bell"
        } else if input.coherence_line.is_some() {
            "coherence"
        } else {
            "clean"
        };
        (state, compose_identity_and_signal_lines(input))
    };
    if lines.is_empty() {
        return None;
    }
    Some(json!({
        "schema": HUMAN_VIEW_SCHEMA,
        "state": state,
        "state_sig": state_sig,
        "lines": lines,
    }))
}

/// The mechanical anti-repetition key (design §2): `trust | bell | coherence |
/// reception`. Equal state ⇒ equal signature; agents use it to never render
/// the same card twice in a session.
fn compose_state_sig(input: &HumanViewInput) -> String {
    let coh = if input.coherence_line.is_some() {
        "sick"
    } else {
        "ok"
    };
    let recv = if input.reception_mismatch.is_some() {
        "mismatch"
    } else {
        "match"
    };
    format!(
        "{}|bell:{}|coh:{}|recv:{}",
        input.trust_mode, input.merge_wait, coh, recv
    )
}

/// Line 1 — THE pluggable-mark seam (amendment 7). The signature prefix is the
/// wordmark hung on the margin with the spine beside it; a future mark (e.g.
/// the pulse row — awaiting the owner's explicit stamp) changes THIS function
/// only, never the composition laws.
fn compose_voice_signature(content: &str) -> Vec<String> {
    render_wrapped(&format!("{WORDMARK} {SPINE} "), content)
}

/// Continuation-line prefix: the gutter, spine fixed at column 6.
fn gutter_prefix() -> String {
    format!("     {SPINE} ")
}

/// Wrap-continuation prefix: the gutter plus the +2 wrap indent.
fn wrap_prefix() -> String {
    format!("     {SPINE}   ")
}

/// S0/S1/S2 — identity line + signal lines in priority order (bell before
/// coherence), each verbatim line whole-or-nothing within the 4-line cap.
fn compose_identity_and_signal_lines(input: &HumanViewInput) -> Vec<String> {
    // Identity segments: each one a measured fact. A zero-valued segment is
    // OMITTED, never rendered as ornament (the zero only speaks in S4).
    let mut segments: Vec<String> = vec![input.trust_mode.replace('_', " ")];
    if input.node_count > 0 {
        segments.push(format!("{} nodes", thousands(input.node_count)));
    }
    if input.memory_count > 0 {
        segments.push(format!("{} memories", thousands(input.memory_count as u64)));
    }
    let mut lines = compose_voice_signature(&segments.join(SEP));

    // Signal lines — the verbatim honest_gaps strings, priority bell >
    // coherence, whole-or-nothing (amendment 5: never truncated).
    for signal in [input.bell_line, input.coherence_line].iter().flatten() {
        push_whole_or_nothing(&mut lines, signal);
    }
    lines.truncate(MAX_LINES);
    lines
}

/// S4 — `needs_ingest`: the zero IS the message (`0 nodes`), the gap string
/// verbatim, and `next:` only when the whole wrapped line still fits.
fn compose_needs_ingest_lines(input: &HumanViewInput) -> Vec<String> {
    let identity = format!("needs_ingest{SEP}{} nodes", thousands(input.node_count));
    let mut lines = compose_voice_signature(&identity);
    push_whole_or_nothing(&mut lines, NEEDS_INGEST_GAP);
    if !input.next_move.is_empty() {
        push_whole_or_nothing(&mut lines, &format!("next: {}", input.next_move));
    }
    lines.truncate(MAX_LINES);
    lines
}

/// S3 — the card IS the warning (amendment 3). Line 1 = the reception's
/// `honest` string verbatim; then bound/yours; then the literal repair call.
/// ZERO statistics — they would describe the wrong brain.
fn compose_mismatch_lines(m: &ReceptionMismatch) -> Vec<String> {
    let mut lines = compose_voice_signature(m.honest);
    push_whole_or_nothing(
        &mut lines,
        &format!("bound: {}{SEP}yours: {}", m.bound_workspace, m.caller_root),
    );
    push_whole_or_nothing(
        &mut lines,
        &format!("next: ingest project_root={}", m.caller_root),
    );
    lines.truncate(MAX_LINES);
    lines
}

/// Append one content line (wrapped) ONLY if the whole wrapped form fits the
/// remaining budget — the whole-or-nothing law (amendment 5): a line falls
/// entirely rather than ever being truncated.
fn push_whole_or_nothing(lines: &mut Vec<String>, content: &str) {
    let rendered = render_wrapped(&gutter_prefix(), content);
    if lines.len() + rendered.len() <= MAX_LINES {
        lines.extend(rendered);
    }
}

/// Greedy word-wrap of `content` behind `first_prefix`; continuations carry
/// the wrap prefix (+2 inside the gutter). Widths are counted in CHARS, so the
/// non-ASCII spine/separators (each 1 column wide, multi-byte) never skew the
/// 80-column law. A single word longer than a whole line is hard-broken as the
/// last resort — the cap is mechanical law and always wins.
fn render_wrapped(first_prefix: &str, content: &str) -> Vec<String> {
    let first_width = MAX_COLS.saturating_sub(first_prefix.chars().count()).max(1);
    let cont_prefix = wrap_prefix();
    let cont_width = MAX_COLS.saturating_sub(cont_prefix.chars().count()).max(1);

    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for word in content.split(' ').filter(|w| !w.is_empty()) {
        let word_len = word.chars().count();
        let budget = if chunks.is_empty() {
            first_width
        } else {
            cont_width
        };
        let needed = if current_len == 0 {
            word_len
        } else {
            current_len + 1 + word_len
        };
        if needed <= budget {
            if current_len > 0 {
                current.push(' ');
                current_len += 1;
            }
            current.push_str(word);
            current_len += word_len;
            continue;
        }
        if current_len > 0 {
            chunks.push(std::mem::take(&mut current));
            current_len = 0;
        }
        // The word alone may still exceed a whole line: hard-break it.
        let mut rest: Vec<char> = word.chars().collect();
        loop {
            let budget = if chunks.is_empty() {
                first_width
            } else {
                cont_width
            };
            if rest.len() <= budget {
                current = rest.iter().collect();
                current_len = rest.len();
                break;
            }
            chunks.push(rest[..budget].iter().collect());
            rest = rest[budget..].to_vec();
        }
    }
    if current_len > 0 {
        chunks.push(current);
    }

    chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            if i == 0 {
                format!("{first_prefix}{chunk}")
            } else {
                format!("{cont_prefix}{chunk}")
            }
        })
        .collect()
}

/// Thousands separator (`9024` → `"9,024"`): the one number formatting the
/// signature line applies; the value itself is the packet's ground truth.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The design doc's bell line for merge_wait=3 — 73 content chars, fitting
    /// the 73-column gutter budget exactly (80 total).
    const BELL_3: &str =
        "3 mission(s) in merge_wait await the human landing — the tray is the door";
    const COHERENCE_LINE: &str = "Skeleton coherence sickness: serving brain expects slug `spine-north`, but the SystemBlock store carries `spine-north-v0` — signal only; reads and writes remain available.";

    fn clean_input<'a>() -> HumanViewInput<'a> {
        HumanViewInput {
            trust_mode: "full_trust",
            node_count: 9024,
            memory_count: 30,
            merge_wait: 0,
            bell_line: None,
            coherence_line: None,
            needs_ingest: false,
            reception_mismatch: None,
            next_move:
                "Call `surgical_context` on the top focus node to ground the task before editing.",
        }
    }

    fn assert_cap(lines: &[serde_json::Value]) {
        assert!(
            lines.len() <= MAX_LINES,
            "cap law: never more than {MAX_LINES} lines, got {}",
            lines.len()
        );
        for line in lines {
            let s = line.as_str().expect("line is a string");
            assert!(
                s.chars().count() <= MAX_COLS,
                "cap law: no line over {MAX_COLS} chars, got {} in {s:?}",
                s.chars().count()
            );
        }
    }

    fn lines_of(card: &serde_json::Value) -> Vec<serde_json::Value> {
        card["lines"].as_array().expect("lines array").clone()
    }

    /// S0 — the clean state is ONE line: the better the world, the smaller the
    /// voice. Segments render exactly as the design pins them.
    #[test]
    fn clean_state_is_one_line() {
        let card = compose_human_view(&clean_input()).expect("card composes");
        assert_eq!(card["schema"], HUMAN_VIEW_SCHEMA);
        assert_eq!(card["state"], "clean");
        let lines = lines_of(&card);
        assert_eq!(lines.len(), 1, "clean state = one line");
        assert_eq!(
            lines[0], "m1nd │ full trust · 9,024 nodes · 30 memories",
            "the signature line renders the measured facts (maps segment omitted — not in the packet)"
        );
        assert_cap(&lines);
    }

    /// S1 — the bell card is TWO lines and line 2 is the honest_gaps string
    /// VERBATIM behind the gutter (amendment 5: one sentence per fact).
    #[test]
    fn bell_state_reuses_the_verbatim_line() {
        let mut input = clean_input();
        input.merge_wait = 3;
        input.bell_line = Some(BELL_3);
        let card = compose_human_view(&input).expect("card composes");
        assert_eq!(card["state"], "bell");
        let lines = lines_of(&card);
        assert_eq!(lines.len(), 2, "bell state = identity + the bell line");
        let line2 = lines[1].as_str().unwrap();
        assert_eq!(
            line2,
            format!("     │ {BELL_3}"),
            "line 2 must be the bell string verbatim behind the gutter"
        );
        assert_eq!(
            line2.chars().count(),
            80,
            "the 73-char bell line fills the 80-column budget exactly"
        );
        assert_cap(&lines);
    }

    /// S2 — the coherence card wraps the long verbatim string with the +2
    /// indent inside the gutter and stays within the 4-line cap.
    #[test]
    fn coherence_wraps_with_indent_within_cap() {
        let mut input = clean_input();
        input.coherence_line = Some(COHERENCE_LINE);
        let card = compose_human_view(&input).expect("card composes");
        assert_eq!(card["state"], "coherence");
        let lines = lines_of(&card);
        assert!(lines.len() >= 3, "the long coherence string must wrap");
        assert_cap(&lines);
        for (i, line) in lines.iter().enumerate().skip(1) {
            let s = line.as_str().unwrap();
            assert!(
                s.starts_with("     │ "),
                "gutter fixed at column 6 on line {i}: {s:?}"
            );
        }
        // Wrap continuations indent +2 inside the content column.
        assert!(
            lines[2].as_str().unwrap().starts_with("     │   "),
            "wrap continuation must indent +2, got {:?}",
            lines[2]
        );
        // Reassembling the wrapped content yields the verbatim string — wrapped,
        // never reworded, never truncated.
        let mut rebuilt = String::new();
        for line in lines.iter().skip(1) {
            let content = line
                .as_str()
                .unwrap()
                .trim_start_matches("     │")
                .trim_start();
            if !rebuilt.is_empty() {
                rebuilt.push(' ');
            }
            rebuilt.push_str(content);
        }
        assert_eq!(
            rebuilt, COHERENCE_LINE,
            "wrap preserves the verbatim string"
        );
    }

    /// S3 — under caller_root_mismatch the card IS the warning (amendment 3):
    /// the reception strings verbatim, the literal repair call, and ZERO
    /// statistics — they would describe the wrong brain.
    #[test]
    fn mismatch_card_is_the_warning_with_zero_statistics() {
        let mut input = clean_input();
        input.merge_wait = 3;
        input.bell_line = Some(BELL_3);
        input.reception_mismatch = Some(ReceptionMismatch {
            honest: "this graph does NOT cover your repo",
            caller_root: "/tmp/repo-beta",
            bound_workspace: "/tmp/repo-alpha",
        });
        let card = compose_human_view(&input).expect("card composes");
        assert_eq!(card["state"], "mismatch");
        let lines = lines_of(&card);
        assert_eq!(
            lines[0], "m1nd │ this graph does NOT cover your repo",
            "line 1 IS the warning, verbatim"
        );
        assert_eq!(
            lines[1], "     │ bound: /tmp/repo-alpha · yours: /tmp/repo-beta",
            "line 2 names both roots"
        );
        assert_eq!(
            lines[2], "     │ next: ingest project_root=/tmp/repo-beta",
            "line 3 is the literal repair call"
        );
        for line in &lines {
            let s = line.as_str().unwrap();
            assert!(
                !s.contains("nodes") && !s.contains("memories") && !s.contains("trust"),
                "zero statistics under mismatch (they describe the wrong brain): {s:?}"
            );
            assert!(
                !s.contains("merge_wait"),
                "the bound brain's bell never rings on a mismatch card: {s:?}"
            );
        }
        assert_cap(&lines);
    }

    /// S4 — the honest needs_ingest card (amendment 4): the zero IS the
    /// message, the gap string rides verbatim (wrapped), and the long real
    /// next_move falls WHOLE because it cannot fit the cap.
    #[test]
    fn needs_ingest_form_is_honest_and_whole_or_nothing() {
        let mut input = clean_input();
        input.trust_mode = "needs_ingest";
        input.node_count = 0;
        input.memory_count = 0;
        input.needs_ingest = true;
        input.next_move =
            "Run ingest for the intended repo, then call north again to get grounded context.";
        let card = compose_human_view(&input).expect("card composes");
        assert_eq!(card["state"], "needs_ingest");
        let lines = lines_of(&card);
        assert_eq!(
            lines[0], "m1nd │ needs_ingest · 0 nodes",
            "S4 line 1: the zero is the message"
        );
        // The gap string rides verbatim, wrapped.
        let mut rebuilt = String::new();
        for line in lines.iter().skip(1) {
            let content = line
                .as_str()
                .unwrap()
                .trim_start_matches("     │")
                .trim_start();
            if !rebuilt.is_empty() {
                rebuilt.push(' ');
            }
            rebuilt.push_str(content);
        }
        assert_eq!(
            rebuilt, NEEDS_INGEST_GAP,
            "the gap rides verbatim; the wrapped next_move did not fit and fell WHOLE"
        );
        assert_cap(&lines);
    }

    /// The cap is mechanical under adversarial input: absurdly long fields can
    /// never push a line past 80 chars or the card past 4 lines.
    #[test]
    fn cap_holds_under_adversarial_input() {
        let long_word = "x".repeat(500);
        let mut input = clean_input();
        input.trust_mode = "degraded_host_tool_surface_with_an_absurdly_long_qualifier";
        input.node_count = 18_446_744_073_709_551_615;
        input.memory_count = usize::MAX;
        input.merge_wait = 12_345;
        let bell = format!(
            "12345 mission(s) in merge_wait await the human landing — the tray is the door and {long_word}"
        );
        input.bell_line = Some(&bell);
        input.coherence_line = Some(COHERENCE_LINE);
        let card = compose_human_view(&input).expect("card composes");
        assert_cap(&lines_of(&card));

        // Mismatch shape with absurd paths obeys the same cap.
        let long_path = format!("/tmp/{}", "d/".repeat(200));
        let mut input = clean_input();
        input.reception_mismatch = Some(ReceptionMismatch {
            honest: "this graph does NOT cover your repo",
            caller_root: &long_path,
            bound_workspace: &long_path,
        });
        let card = compose_human_view(&input).expect("card composes");
        assert_cap(&lines_of(&card));
    }

    /// Whole-or-nothing (amendment 5): a verbatim line that cannot fit the
    /// remaining budget falls ENTIRELY — never a partial render, never `…`.
    #[test]
    fn verbatim_line_falls_whole_never_truncated() {
        let mut input = clean_input();
        input.merge_wait = 3;
        input.bell_line = Some(BELL_3);
        // The coherence line wraps to 3 gutter lines; after identity + bell
        // only 2 slots remain → it must fall whole.
        input.coherence_line = Some(COHERENCE_LINE);
        let card = compose_human_view(&input).expect("card composes");
        let lines = lines_of(&card);
        assert_eq!(
            lines.len(),
            2,
            "identity + bell only — coherence fell whole"
        );
        for line in &lines {
            let s = line.as_str().unwrap();
            assert!(
                !s.contains("Skeleton coherence") && !s.contains('…'),
                "no partial coherence render, no ellipsis: {s:?}"
            );
        }
        // The state still names the TOP signal (bell) and the sig carries the
        // coherence truth mechanically.
        assert_eq!(card["state"], "bell");
        assert_eq!(card["state_sig"], "full_trust|bell:3|coh:sick|recv:match");
    }

    /// The state_sig is stable for equal state and matches the pinned example.
    #[test]
    fn state_sig_is_stable_and_mechanical() {
        let mut input = clean_input();
        input.merge_wait = 3;
        input.bell_line = Some(BELL_3);
        let a = compose_human_view(&input).expect("card composes");
        let b = compose_human_view(&input).expect("card composes");
        assert_eq!(a["state_sig"], b["state_sig"], "equal state ⇒ equal sig");
        assert_eq!(a["state_sig"], "full_trust|bell:3|coh:ok|recv:match");
        let clean = compose_human_view(&clean_input()).expect("card composes");
        assert_eq!(clean["state_sig"], "full_trust|bell:0|coh:ok|recv:match");
        assert_ne!(
            a["state_sig"], clean["state_sig"],
            "state change ⇒ sig change"
        );
    }

    /// Zero-valued identity segments are OMITTED, never rendered as ornament
    /// (the zero only speaks in S4).
    #[test]
    fn zero_segments_are_omitted_outside_s4() {
        let mut input = clean_input();
        input.memory_count = 0;
        let card = compose_human_view(&input).expect("card composes");
        assert_eq!(
            lines_of(&card)[0],
            "m1nd │ full trust · 9,024 nodes",
            "a zero memories segment is omitted, never `0 memories`"
        );
    }

    /// The thousands separator matches the design's number rendering.
    #[test]
    fn thousands_separator_renders_like_the_design() {
        assert_eq!(thousands(9024), "9,024");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(thousands(0), "0");
    }

    /// The geometry law: line 1 hangs the wordmark; every later line keeps the
    /// spine fixed at column 6 (5 spaces, `│`, space).
    #[test]
    fn gutter_is_fixed_at_column_six() {
        let mut input = clean_input();
        input.merge_wait = 3;
        input.bell_line = Some(BELL_3);
        let card = compose_human_view(&input).expect("card composes");
        let lines = lines_of(&card);
        assert!(lines[0].as_str().unwrap().starts_with("m1nd │ "));
        for line in lines.iter().skip(1) {
            let s = line.as_str().unwrap();
            let chars: Vec<char> = s.chars().collect();
            assert_eq!(&chars[..5], &[' ', ' ', ' ', ' ', ' ']);
            assert_eq!(chars[5], '│', "spine at column 6: {s:?}");
        }
    }
}
