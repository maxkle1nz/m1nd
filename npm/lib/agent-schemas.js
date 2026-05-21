"use strict";

const AGENT_CLI_SCHEMA = "m1nd-agent-cli-v0";

function agentNonClaims() {
  return [
    "m1nd agent does not refresh an already-open MCP host or cached tool list.",
    "m1nd agent does not prove semantic retrieval correctness.",
    "m1nd agent does not replace direct source reads, tests, compilers, logs, or runtime probes.",
    "m1nd agent does not mutate tracked repository code.",
    "m1nd agent does not guarantee every possible agent host is configured.",
    "m1nd agent is a local operating layer, not a production unattended agent orchestrator.",
  ];
}

function baseAgentEnvelope({
  command,
  repo,
  agentId,
  runtime,
  scopeAlignment,
  graphState = null,
  trust = null,
}) {
  return {
    schema: AGENT_CLI_SCHEMA,
    ok: true,
    command,
    repo,
    agent_id: agentId,
    runtime,
    scope_alignment: scopeAlignment,
    graph_state: graphState,
    trust,
    calls: [],
    results: [],
    next_actions: [],
    non_claims: agentNonClaims(),
  };
}

module.exports = {
  AGENT_CLI_SCHEMA,
  agentNonClaims,
  baseAgentEnvelope,
};
