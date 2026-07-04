#!/usr/bin/env bash
# demo-replay.sh — deterministic replay of a REAL m1nd agent session for the README demo.
#
# Every line of output below was CAPTURED VERBATIM (then trimmed for width) from a live
# m1nd owner process on 2026-07-04: binary m1nd-mcp 1.3.0 (8a36034), a 6,453-node /
# 20,808-edge graph over the m1nd repo itself, full_trust binding, embeddings ON.
# The three calls are the agent's real front door → verdict → memory loop:
#   north(task)  →  seek(query)  →  memorize(claim)
# Nothing here is invented: these are the actual JSON responses, replayed for the tape.
# Regenerate the GIF from this + demo.tape with:  vhs docs/assets/demo.tape
set -u

# — calm palette (soft-proof): dim label, warm accent, muted verdict colors —
DIM=$'\033[38;5;245m'      # secondary text
INK=$'\033[38;5;238m'      # primary ink
KEY=$'\033[38;5;131m'      # muted rose — the verb / prompt
VAL=$'\033[38;5;108m'      # sage — values that mean "good"
WARN=$'\033[38;5;179m'     # ochre — honest caveat / reverify
OFF=$'\033[0m'
BOLD=$'\033[1m'

p() { printf '%b\n' "$1"; }        # print a line
c() { printf '%b'   "$1"; }        # print, no newline
pause() { sleep "${1:-0.4}"; }

clear
p ""
p "  ${DIM}the agent's loop, one real session — ${INK}${BOLD}m1nd${OFF}${DIM} 1.3.0 · graph over this repo${OFF}"
p ""
pause 0.6

# ─────────────────────────────────────────────────────────────────────────────
# 1. BEFORE — north(task): born oriented, with honest gaps
# ─────────────────────────────────────────────────────────────────────────────
p "  ${DIM}# BEFORE it acts — one call returns the whole oriented packet${OFF}"
c "  ${KEY}north${OFF}(task: ${INK}\"harden the JWT auth token validation flow\"${OFF})"
pause 0.5 ; p "" ; pause 0.3
p "  ${DIM}{${OFF}"
p "    ${DIM}\"binding\":${OFF}   { \"trust_mode\": ${VAL}\"full_trust\"${OFF}, \"ok\": ${VAL}true${OFF} },   ${DIM}// verdict before retrieval${OFF}"
p "    ${DIM}\"context\":${OFF}   { \"focus_nodes\": ["
p "      { \"label\": ${INK}\"validation\"${OFF}, \"kind\": \"Module\", \"activation\": 0.73 },"
p "      { \"label\": ${INK}\"tokenize\"${OFF},   \"kind\": \"Function\", \"activation\": 0.80 },"
p "      { \"label\": ${INK}\"taint_with_boundary_detects_validation\"${OFF}, \"activation\": 0.70 } ] },"
p "    ${DIM}\"honest_gaps\":${OFF} [ ${WARN}\"No durable memory yet — no prior cross-session claim to carry.\"${OFF} ],"
p "    ${DIM}\"sufficiency\":${OFF} { \"state\": ${WARN}\"gathering\"${OFF}, \"top_score\": 0.43 },"
p "    ${DIM}\"next_move\":${OFF}  ${INK}\"Call surgical_context on the top focus node to ground the task.\"${OFF}"
p "  ${DIM}}${OFF}"
pause 1.4

# ─────────────────────────────────────────────────────────────────────────────
# 2. DURING — seek(query): a verdict-carrying answer (reverify, not a fake 'act')
# ─────────────────────────────────────────────────────────────────────────────
p ""
p "  ${DIM}# DURING — every answer arrives wearing how much to trust it${OFF}"
c "  ${KEY}seek${OFF}(query: ${INK}\"token validation and boundary detection for auth flow\"${OFF})"
pause 0.5 ; p "" ; pause 0.3
p "  ${DIM}results:${OFF}  ${VAL}embeddings_used: true${OFF}   ${DIM}· 6,453 candidates scanned${OFF}"
p "    ${VAL}0.47${OFF}  ${INK}taint_with_boundary_detects_validation${OFF}  ${DIM}m1nd-core/src/taint.rs${OFF}"
p "    ${VAL}0.41${OFF}  ${INK}detect_boundary_validates${OFF}              ${DIM}m1nd-core/src/taint.rs${OFF}"
p "            ${DIM}excerpt: patterns = [\"validate\", \"sanitize\", \"auth\"]${OFF}"
p "  ${DIM}trust_envelope:${OFF} {"
p "    \"calibrated\": ${WARN}false${OFF},          ${DIM}// no calibration row measured yet${OFF}"
p "    \"verdict\": ${WARN}${BOLD}\"reverify\"${OFF},        ${DIM}// 'act' unreachable — it won't overclaim${OFF}"
p "    \"next_repair_call\": ${INK}\"trust_selftest\"${OFF} }"
pause 1.6

# ─────────────────────────────────────────────────────────────────────────────
# 3. AFTER — memorize(claim): written down with evidence, anchored to code
# ─────────────────────────────────────────────────────────────────────────────
p ""
p "  ${DIM}# AFTER — the finding is written down with the evidence that backs it${OFF}"
c "  ${KEY}memorize${OFF}(node: ${INK}TokenValidator${OFF}, evidence: ${INK}[m1nd-core/src/taint.rs]${OFF})"
pause 0.5 ; p "" ; pause 0.3
p "  ${DIM}{${OFF}"
p "    \"ok\": ${VAL}true${OFF}, \"claims_written\": ${VAL}1${OFF}, \"light_evidence_resolved\": ${VAL}66${OFF},"
p "    \"path\": ${DIM}\".../agent-memory/tokenvalidator.light.md\"${OFF},"
p "    \"node_count\": ${VAL}6453 → 6456${OFF}, \"edge_count\": ${VAL}20808 → 20813${OFF},   ${DIM}// graph grew${OFF}"
p "    \"next_action\": ${INK}\"anchored to code; cross_verify flags it if that file moves.\"${OFF}"
p "  ${DIM}}${OFF}"
pause 1.2
p ""
p "  ${VAL}${BOLD}↳${OFF} ${DIM}the next session — any host, fresh process — starts already knowing this.${OFF}"
p ""
pause 2.0
