#!/usr/bin/env bash
# demo-run.sh — Simulates realistic m1nd MCP output for the VHS demo tape.
# Outputs are based on actual JSON response shapes from m1nd-mcp/src/tools.rs
# and m1nd-mcp/src/protocol/layers.rs.

# ANSI color codes
BOLD='\033[1m'
DIM='\033[2m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[0;33m'
MAGENTA='\033[0;35m'
BLUE='\033[0;34m'
RED='\033[0;31m'
WHITE='\033[0;37m'
RESET='\033[0m'

cmd="${1:-}"

case "$cmd" in

  health)
    printf "${DIM}$ ${RESET}${BOLD}m1nd.health${RESET} ${DIM}{ agent_id: \"jimi\" }${RESET}\n"
    sleep 0.3
    printf "${GREEN}{\n"
    printf "  \"status\": \"ok\",\n"
    printf "  \"node_count\": 0,\n"
    printf "  \"edge_count\": 0,\n"
    printf "  \"queries_processed\": 0,\n"
    printf "  \"uptime_seconds\": 0.41,\n"
    printf "  \"memory_usage_bytes\": 2097152,\n"
    printf "  \"plasticity_state\": \"idle\"\n"
    printf "}${RESET}\n"
    ;;

  ingest)
    printf "${DIM}$ ${RESET}${BOLD}m1nd.ingest${RESET} ${DIM}{ path: \"~/clawd/roomanizer-os\", mode: \"replace\" }${RESET}\n"
    sleep 0.2
    printf "${CYAN}[m1nd]${RESET} scanning codebase...\n"
    sleep 0.4
    printf "${CYAN}[m1nd]${RESET} parsing 77 .py files, 31 .ts/.tsx files, 14 .rs files...\n"
    sleep 0.5
    printf "${CYAN}[m1nd]${RESET} resolving cross-file imports and call edges...\n"
    sleep 0.3
    printf "${GREEN}{\n"
    printf "  \"mode\": \"replace\",\n"
    printf "  \"adapter\": \"code\",\n"
    printf "  \"files_scanned\": 335,\n"
    printf "  \"files_parsed\": 311,\n"
    printf "  \"nodes_created\": 9767,\n"
    printf "  \"edges_created\": 26557,\n"
    printf "  \"elapsed_ms\": 912.4,\n"
    printf "  \"node_count\": 9767,\n"
    printf "  \"edge_count\": 26557\n"
    printf "}${RESET}\n"
    printf "${BOLD}${GREEN}  335 files → 9,767 nodes → 26,557 edges in 0.91s${RESET}\n"
    ;;

  activate)
    printf "${DIM}$ ${RESET}${BOLD}m1nd.activate${RESET} ${DIM}{ query: \"rate limiting and provider fallback\", top_k: 8 }${RESET}\n"
    sleep 0.3
    printf "${CYAN}[m1nd]${RESET} spreading activation across 4 dimensions...\n"
    sleep 0.2
    printf "${GREEN}{\n"
    printf "  \"query\": \"rate limiting and provider fallback\",\n"
    printf "  \"activated\": [\n"
    printf "    { \"label\": \"smart_router.py\",          \"type\": \"File\",     \"activation\": ${BOLD}0.9821${RESET}${GREEN}, \"pagerank\": 0.0312 },\n"
    printf "    { \"label\": \"RateLimitGuard\",           \"type\": \"Class\",    \"activation\": ${BOLD}0.9617${RESET}${GREEN}, \"pagerank\": 0.0287 },\n"
    printf "    { \"label\": \"handle_rate_limit_error\",  \"type\": \"Function\", \"activation\": ${BOLD}0.9204${RESET}${GREEN}, \"pagerank\": 0.0201 },\n"
    printf "    { \"label\": \"provider_health_check\",    \"type\": \"Function\", \"activation\": ${BOLD}0.8891${RESET}${GREEN}, \"pagerank\": 0.0198 },\n"
    printf "    { \"label\": \"SmartRouter\",              \"type\": \"Class\",    \"activation\": ${BOLD}0.8743${RESET}${GREEN}, \"pagerank\": 0.0341 },\n"
    printf "    { \"label\": \"fallback_chain\",           \"type\": \"Function\", \"activation\": ${BOLD}0.8512${RESET}${GREEN}, \"pagerank\": 0.0156 },\n"
    printf "    { \"label\": \"metrics_rate_limits.py\",   \"type\": \"File\",     \"activation\": ${BOLD}0.8274${RESET}${GREEN}, \"pagerank\": 0.0143 },\n"
    printf "    { \"label\": \"BackoffStrategy\",          \"type\": \"Class\",    \"activation\": ${BOLD}0.7956${RESET}${GREEN}, \"pagerank\": 0.0119 }\n"
    printf "  ],\n"
    printf "  \"ghost_edges\": [\n"
    printf "    { \"source\": \"RateLimitGuard\", \"target\": \"BackoffStrategy\", \"strength\": 0.74, \"shared_dimensions\": [\"causal\",\"temporal\"] }\n"
    printf "  ],\n"
    printf "  \"plasticity\": { \"edges_strengthened\": 12, \"edges_decayed\": 3, \"ltp_events\": 5, \"priming_nodes\": 8 },\n"
    printf "  \"elapsed_ms\": 18.7\n"
    printf "}${RESET}\n"
    ;;

  learn)
    printf "${DIM}$ ${RESET}${BOLD}m1nd.learn${RESET} ${DIM}{ query: \"rate limiting\", feedback: \"correct\", node_ids: [\"RateLimitGuard\",\"BackoffStrategy\",\"fallback_chain\"] }${RESET}\n"
    sleep 0.2
    printf "${GREEN}{\n"
    printf "  \"query\": \"rate limiting\",\n"
    printf "  \"feedback\": \"correct\",\n"
    printf "  \"nodes_found\": 3,\n"
    printf "  \"nodes_expanded\": 11,\n"
    printf "  \"edges_modified\": ${BOLD}${YELLOW}17${RESET}${GREEN},\n"
    printf "  \"strength\": 0.2\n"
    printf "}${RESET}\n"
    printf "${BOLD}${YELLOW}  17 edges strengthened — Hebbian learning applied${RESET}\n"
    ;;

  hypothesize)
    printf "${DIM}$ ${RESET}${BOLD}m1nd.hypothesize${RESET} ${DIM}{ claim: \"RateLimitGuard always calls BackoffStrategy before retry\", agent_id: \"jimi\" }${RESET}\n"
    sleep 0.3
    printf "${CYAN}[m1nd]${RESET} encoding claim as graph query...\n"
    sleep 0.2
    printf "${CYAN}[m1nd]${RESET} searching for evidence paths (budget: 120 paths)...\n"
    sleep 0.3
    printf "${GREEN}{\n"
    printf "  \"claim\": \"RateLimitGuard always calls BackoffStrategy before retry\",\n"
    printf "  \"claim_type\": \"always_before\",\n"
    printf "  \"subject_nodes\": [\"RateLimitGuard\"],\n"
    printf "  \"object_nodes\": [\"BackoffStrategy\"],\n"
    printf "  \"verdict\": \"${BOLD}${GREEN}likely_true${RESET}${GREEN}\",\n"
    printf "  \"confidence\": ${BOLD}0.871${RESET}${GREEN},\n"
    printf "  \"supporting_evidence\": [\n"
    printf "    { \"type\": \"path_found\",       \"description\": \"direct call edge via handle_rate_limit_error\", \"likelihood_factor\": 1.8 },\n"
    printf "    { \"type\": \"causal_chain\",     \"description\": \"causal chain depth 2 with strength 0.74\",      \"likelihood_factor\": 1.4 },\n"
    printf "    { \"type\": \"ghost_edge\",       \"description\": \"latent coupling on causal+temporal dimensions\", \"likelihood_factor\": 1.2 }\n"
    printf "  ],\n"
    printf "  \"contradicting_evidence\": [],\n"
    printf "  \"paths_explored\": 43,\n"
    printf "  \"elapsed_ms\": 31.2\n"
    printf "}${RESET}\n"
    printf "${BOLD}${GREEN}  verdict: likely_true  confidence: 87.1%%${RESET}\n"
    ;;

  *)
    printf "${RED}Usage: $0 {health|ingest|activate|learn|hypothesize}${RESET}\n"
    exit 1
    ;;
esac
