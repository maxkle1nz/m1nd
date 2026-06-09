"use strict";

const fs = require("fs");
const path = require("path");
const { McpRuntimeClient, callToolSafely } = require("./mcp-runtime-client");
const { AGENT_CLI_SCHEMA, agentNonClaims, baseAgentEnvelope } = require("./agent-schemas");

const AGENT_ACTION_SCHEMA = "m1nd-agent-action-envelope-v0";
const ORIENTATION_TOOLS = new Set(["auto", "search", "seek", "activate", "audit", "glob"]);
const RETROBUILDER_TOOLS = [
  "ghost_edges",
  "taint_trace",
  "twins",
  "refactor_plan",
  "runtime_overlay",
];

function safeJsonParse(text) {
  try {
    return JSON.parse(text);
  } catch (_) {
    return null;
  }
}

function shellQuote(value) {
  const text = String(value);
  if (/^[A-Za-z0-9_./:=+-]+$/.test(text)) return text;
  return `'${text.replace(/'/g, `'\\''`)}'`;
}

function buildAgentCliCommand(subcommand, repo, flags = [], binary = null) {
  const parts = ["m1nd", "agent", subcommand, "--repo", shellQuote(repo)];
  if (binary) {
    parts.push("--binary", shellQuote(binary));
  }
  for (const [key, value] of flags) {
    if (value === undefined || value === null || value === false) continue;
    parts.push(`--${key}`);
    if (value !== true) parts.push(shellQuote(value));
  }
  parts.push("--json");
  return parts.join(" ");
}

function buildActionEnvelope({
  trigger,
  route,
  action,
  proofRequirements,
  switchToDirectProof,
}) {
  const envelope = {
    schema: AGENT_ACTION_SCHEMA,
    trigger,
    route,
    action,
    proof_requirements: proofRequirements,
    non_claims: agentNonClaims(),
  };
  if (switchToDirectProof === true) envelope.switch_to_direct_proof = true;
  return envelope;
}

function proofRequirementsForRoute(routeKind) {
  switch (routeKind) {
    case "first_minute":
      return [
        "Use the first-minute output as repo orientation, not final proof.",
        "Read the listed anchors directly before behavior or architecture claims.",
        "After one bounded graph pass, switch to source reads, tests, compiler/runtime output, or focused probes.",
      ];
    case "recover":
      return [
        "Run the emitted recovery path before relying on retrieval again.",
        "After recovery, re-run trust or orient on the requested repo.",
        "Do not treat restart/rebind/update steps as proof of code behavior.",
      ];
    case "context":
      return [
        "Read the selected file directly before final claims.",
        "Use the capsule to narrow proof, then verify behavior with focused tests or runtime output.",
      ];
    case "direct_proof":
      return [
        "Read source directly before final claims.",
        "Use tests, compiler output, logs, or focused probes for behavior-sensitive answers.",
      ];
    default:
      return [
        "Treat m1nd output as orientation, not final proof.",
        "Read source directly and run focused tests or probes before behavioral claims.",
      ];
  }
}

function uniqueList(values) {
  return [...new Set(values.filter(Boolean))];
}

function capabilityGuidanceForQuery(query) {
  const raw = String(query || "");
  const text = raw.toLowerCase();
  if (!text.trim()) return null;

  const signals = [];
  const tools = [];
  const supportingTools = [];

  const add = (signal, toolNames, support = []) => {
    signals.push(signal);
    tools.push(...toolNames);
    supportingTools.push(...support);
  };

  if (/\b(retrobuilder|advanced graph|superpower|superpowers)\b/i.test(raw)) {
    add("explicit_retrobuilder_request", RETROBUILDER_TOOLS, [
      "layers",
      "impact",
      "validate_plan",
    ]);
  }
  if (/\b(security|privacy|taint|trust boundary|trust-boundary|sink|source|user input|credential|secret|sanitize|sanitizer|auth)\b/i.test(raw)) {
    add("taint_or_trust_boundary", ["taint_trace"], [
      "trust",
      "type_trace",
      "validate_plan",
    ]);
  }
  if (/\b(duplicate|duplication|clone|twins?|near-equivalent|equivalent|consolidat|extract|refactor|cleanup|spaghetti)\b/i.test(raw)) {
    add("duplication_or_refactor_surface", ["twins", "refactor_plan"], [
      "impact",
      "validate_plan",
      "surgical_context_v2",
    ]);
  }
  if (/\b(runtime|production|prod|otel|opentelemetry|span|distributed trace|runtime trace|latency|error rate|hot path|perf|performance|logs?|deploy)\b/i.test(raw)) {
    add("runtime_truth_overlay", ["runtime_overlay"], [
      "trace",
      "impact",
      "panoramic",
    ]);
  }
  if (/\b(co-?change|changed together|git history|hidden coupling|coupling|blast radius|temporal coupling|history)\b/i.test(raw)) {
    add("hidden_temporal_coupling", ["ghost_edges"], [
      "timeline",
      "impact",
    ]);
  }
  if (/\b(architecture|arquitetura|audit|map|overview|understand|entenda|entender|system|sistema|dependency|dependencies|boundary|quality|spaghetti)\b/i.test(raw)) {
    add("deep_structure_or_architecture", ["ghost_edges", "twins", "refactor_plan"], [
      "layers",
      "layer_inspect",
      "scan_all",
      "heuristics_surface",
    ]);
  }

  const selectedTools = uniqueList(tools);
  if (selectedTools.length === 0) return null;

  let primaryIntent = "deep_architecture";
  if (signals.includes("taint_or_trust_boundary")) {
    primaryIntent = "security_taint_audit";
  } else if (signals.includes("runtime_truth_overlay")) {
    primaryIntent = "runtime_overlay_audit";
  } else if (signals.includes("duplication_or_refactor_surface")) {
    primaryIntent = "duplication_refactor";
  } else if (signals.includes("hidden_temporal_coupling")) {
    primaryIntent = "hidden_coupling_audit";
  }

  const riskLevel = signals.some((signal) =>
    ["taint_or_trust_boundary", "runtime_truth_overlay", "duplication_or_refactor_surface"].includes(signal)
  ) ? "high" : "medium";

  return {
    task_profile: {
      schema: "m1nd-agent-task-profile-v0",
      primary_intent: primaryIntent,
      risk_level: riskLevel,
      signals: uniqueList(signals),
      evidence_mode: "graph_orientation_then_direct_proof",
    },
    capability_suggestions: [{
      schema: "m1nd-agent-capability-suggestion-v0",
      family_id: "retrobuilder",
      priority: riskLevel === "high" ? "high" : "medium",
      tools: selectedTools,
      supporting_tools: uniqueList(supportingTools),
      use_when: "Use after scope/trust when the task asks for hidden coupling, taint/security paths, duplication, refactor seams, runtime heat, or deep architecture quality.",
      stop_rule: "Stop once the tools return concrete files, node ids, paths, or hypotheses; final claims still need direct source reads, tests, compiler/runtime output, logs, or focused probes.",
      non_claims: [
        "RETROBUILDER output is structural orientation, not proof of a bug by itself.",
        "runtime_overlay needs real span/log/runtime evidence to prove runtime behavior.",
        "refactor_plan does not execute edits; validate and prove before applying changes.",
      ],
    }],
    playbook: {
      schema: "m1nd-agent-playbook-suggestion-v0",
      id: "retrobuilder-deep-structure-v0",
      steps: [
        "Establish scope/trust or use m1nd agent first-minute for the repo.",
        "Run only the RETROBUILDER tools whose signals match the task.",
        "Convert returned nodes/files into a short direct-proof read/test list.",
        "Use impact/validate_plan before edits or risky recommendations.",
        "Report what was graph orientation versus what was directly proved.",
      ],
    },
  };
}

function attachCapabilityGuidance(envelope, query) {
  const guidance = capabilityGuidanceForQuery(query);
  if (!guidance) return envelope;
  envelope.task_profile = guidance.task_profile;
  envelope.capability_suggestions = guidance.capability_suggestions;
  envelope.playbook = guidance.playbook;
  if (envelope.action && typeof envelope.action === "object") {
    envelope.action.task_profile = guidance.task_profile;
    envelope.action.capability_suggestions = guidance.capability_suggestions;
    envelope.action.playbook = guidance.playbook;
  }
  return envelope;
}

function realPathOrResolved(target) {
  if (!target) return null;
  try {
    return fs.realpathSync.native(target);
  } catch (_) {
    return path.resolve(target);
  }
}

function pathContains(parent, child) {
  if (!parent || !child) return false;
  const relative = path.relative(parent, child);
  return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
}

function classifyScopeBinding(repo, activeRoot, ingestRoots = []) {
  const repoPath = realPathOrResolved(repo);
  const activePath = activeRoot ? realPathOrResolved(activeRoot) : null;
  const roots = ingestRoots.map((root) => realPathOrResolved(root)).filter(Boolean);
  const allRoots = activePath ? [activePath, ...roots] : roots;

  if (allRoots.length === 0) {
    return {
      binding_kind: "ambiguous_scope",
      partial_scope: true,
      scope_reliability: "unknown",
      recommended_usage_mode: "scope_before_retrieval",
      reason: "no active workspace or ingest root is known",
    };
  }

  const fileLevel = allRoots.find((root) => {
    try {
      return fs.existsSync(root) && fs.statSync(root).isFile();
    } catch (_) {
      return false;
    }
  });
  if (fileLevel && pathContains(repoPath, fileLevel)) {
    return {
      binding_kind: "file_level_binding",
      partial_scope: true,
      scope_reliability: "document_or_file_only",
      recommended_usage_mode: "partial_scope_orientation",
      reason: "active binding points at a file inside the requested repo",
    };
  }

  if (allRoots.some((root) => root === repoPath || pathContains(root, repoPath))) {
    return {
      binding_kind: "full_repo_binding",
      partial_scope: false,
      scope_reliability: "repo_wide",
      recommended_usage_mode: "full_repo_truth",
      reason: "active workspace contains the requested repo",
    };
  }

  if (allRoots.some((root) => pathContains(repoPath, root))) {
    return {
      binding_kind: "nested_workspace_binding",
      partial_scope: true,
      scope_reliability: "subtree_only",
      recommended_usage_mode: "partial_scope_orientation",
      reason: "active binding is a subdirectory of the requested repo",
    };
  }

  return {
    binding_kind: "wrong_workspace_binding",
    partial_scope: true,
    scope_reliability: "wrong_repo",
    recommended_usage_mode: "rebind_or_isolated_cli",
    reason: "active binding is outside the requested repo",
  };
}

function extractGraphState(payload) {
  if (payload && typeof payload.graph_state === "object") return payload.graph_state;
  const contract = payload && typeof payload.agent_runtime_contract === "object"
    ? payload.agent_runtime_contract
    : null;
  if (contract && typeof contract.graph_identity === "object") return contract.graph_identity;
  return {};
}

function candidateCount(payload) {
  const countLike = (value) => {
    if (Array.isArray(value)) return value.length;
    if (Number.isInteger(value)) return value;
    if (value === true) return 1;
    if (value && typeof value === "object") {
      if (Number.isInteger(value.count)) return value.count;
      const keys = Object.keys(value);
      return keys.length > 0 ? keys.length : 0;
    }
    return 0;
  };
  for (const key of ["results", "matches", "items", "candidates", "activated"]) {
    const count = countLike(payload && payload[key]);
    if (count > 0) return count;
  }
  for (const key of ["total_matches", "total_candidates", "count", "activated_count"]) {
    if (Number.isInteger(payload && payload[key])) return payload[key];
  }
  return 0;
}

function proofState(payload) {
  if (payload && typeof payload.proof_state === "string") return payload.proof_state;
  const contract = payload && typeof payload.agent_runtime_contract === "object"
    ? payload.agent_runtime_contract
    : null;
  if (contract && typeof contract.proof_state === "string") return contract.proof_state;
  return null;
}

function payloadHasWrongWorkspaceBinding(payload) {
  if (!payload || typeof payload !== "object") return false;
  if (payload.context_guard && payload.context_guard.wrong_workspace_binding === true) return true;
  if (payload.recovery && payload.recovery.binding_issue === "wrong_workspace_binding") return true;
  if (payload.binding_issue === "wrong_workspace_binding") return true;
  const contract = payload.agent_runtime_contract && typeof payload.agent_runtime_contract === "object"
    ? payload.agent_runtime_contract
    : null;
  if (contract && contract.trust_mode === "wrong_workspace_binding") return true;
  if (contract && contract.workspace_binding && contract.workspace_binding.mismatch) return true;
  return false;
}

function payloadDict(result) {
  return result && result.payload && typeof result.payload === "object" && !Array.isArray(result.payload)
    ? result.payload
    : {};
}

function callSummary(tool, result) {
  const payload = payloadDict(result);
  const graphState = extractGraphState(payload);
  const summary = {
    tool,
    isError: Boolean(result && result.isError),
    schema: payload.schema,
    verdict: payload.verdict,
    status: payload.status,
    proof_state: proofState(payload),
    candidate_count: candidateCount(payload),
  };
  if (Object.keys(graphState).length > 0) {
    summary.graph_state = {
      node_count: graphState.node_count,
      edge_count: graphState.edge_count,
      finalized: graphState.finalized,
      graph_generation: graphState.graph_generation,
      ingest_root_count: graphState.ingest_root_count,
      workspace_root: graphState.workspace_root,
      runtime_root: graphState.runtime_root,
    };
  }
  return Object.fromEntries(Object.entries(summary).filter(([, value]) => value !== undefined && value !== null));
}

function trustNeedsIngest(result) {
  const payload = payloadDict(result);
  if (payload.verdict === "needs_ingest" || payload.verdict === "cold_graph") return true;
  const checks = payload.checks && typeof payload.checks === "object" ? payload.checks : {};
  if (checks.needs_ingest === true) return true;
  const graphState = extractGraphState(payload);
  return Number.isInteger(graphState.node_count) && graphState.node_count === 0;
}

function runtimeInfo(binary, deps) {
  return {
    binary: binary || null,
    version: binary ? deps.runtimeVersion(binary) : null,
    runtime_root: null,
  };
}

function buildScopeAlignment(repo) {
  const ambientRoot = process.env.M1ND_WORKSPACE_ROOT || process.env.OLDPWD || null;
  const ambient = classifyScopeBinding(repo, ambientRoot, []);
  const agentRuntime = classifyScopeBinding(repo, repo, [repo]);
  return {
    binding_kind: agentRuntime.binding_kind,
    requested_repo: repo,
    ambient_workspace_root: ambientRoot,
    ambient_binding_kind: ambient.binding_kind,
    ambient_recommended_usage_mode: ambient.recommended_usage_mode,
    agent_runtime_binding_kind: agentRuntime.binding_kind,
    partial_scope: agentRuntime.partial_scope,
    scope_reliability: agentRuntime.scope_reliability,
    recommended_usage_mode: agentRuntime.recommended_usage_mode,
    reason: "m1nd agent commands launch an isolated runtime bound to the requested repo",
  };
}

function queryHasGlobPattern(query) {
  return /[*?[\]{}]/.test(String(query || ""));
}

function queryHasPathSignal(query) {
  const text = String(query || "").trim();
  if (!text) return false;
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(text)) return false;
  if (/^@[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(text)) return false;
  if (text.startsWith("./") || text.startsWith("../") || text.startsWith("~/")) return true;
  if (text.includes("/") || text.includes("\\")) return true;
  return /^[^/\s]+\.(c|cc|cpp|css|go|h|hpp|html|java|js|json|jsx|md|mjs|py|rb|rs|scss|sh|sql|toml|ts|tsx|txt|yaml|yml)$/i.test(text);
}

function queryLooksLikeExactIdentifier(query) {
  const text = String(query || "").trim();
  if (!text || /\s/.test(text)) return false;
  if (queryHasPathSignal(text) || queryHasGlobPattern(text)) return false;
  return /^[$A-Za-z_][$\w]*(?:(?:\.|::|#)[$A-Za-z_][$\w]*)*$/.test(text);
}

function queryLooksBroadTask(query) {
  const text = String(query || "").trim();
  if (!text) return false;
  const wordCount = text.split(/\s+/).filter(Boolean).length;
  if (wordCount < 3) return false;
  return /\b(audit|architecture|overview|map|review|investigate|debug|trace|refactor|explore|understand|system|subsystem|pipeline|dependency|dependencies|release|bug|issue)\b/i.test(text);
}

function queryLooksLikeFirstContact(query) {
  const text = String(query || "").trim();
  if (!text) return false;
  if (queryHasPathSignal(text) || queryLooksLikeExactIdentifier(text)) return false;
  return /\b(understand|entenda|entender|explain|explica|map|overview|audit|architecture|arquitetura|repo|codebase|system|sistema|own code|proprio codigo|próprio código)\b/i.test(text);
}

function strongIdentifierCandidatesFromQuery(query) {
  const weak = new Set([
    "audit", "trace", "flow", "chat", "repo", "code", "system", "review", "debug",
    "understand", "entenda", "entender", "architecture", "arquitetura", "session",
    "boundary", "behavior", "comportamento", "context", "m1nd", "mind",
  ]);
  return identifierCandidatesFromQuery(query).filter((term) => {
    const lower = term.toLowerCase();
    if (weak.has(lower)) return false;
    if (/[.:#]/.test(term)) return true;
    if (/[a-z][A-Z]/.test(term) || /[A-Z][a-z]/.test(term)) return true;
    if (term.includes("_") || term.includes("$")) return true;
    return false;
  });
}

function chooseOrientationTool(tool, query, mode) {
  if (tool && tool !== "auto") return tool;
  const text = String(query || "").trim();
  if (mode === "deep" && /audit|architecture|overview|map|quality/i.test(text)) return "audit";
  if (queryHasGlobPattern(text) || queryHasPathSignal(text)) return "glob";
  if (queryLooksLikeExactIdentifier(text)) return "search";
  return mode === "deep" ? "activate" : "seek";
}

function orientationArgs(tool, { agentId, repo, query, topK }) {
  if (tool === "audit") return { agent_id: agentId, path: repo };
  if (tool === "glob") return { agent_id: agentId, pattern: query, scope: repo, top_k: topK };
  return { agent_id: agentId, query, scope: repo, top_k: topK };
}

function extractResultList(payload) {
  for (const key of ["results", "matches", "items", "candidates"]) {
    if (Array.isArray(payload && payload[key])) return payload[key];
  }
  return [];
}

function extractFileFromEntry(entry) {
  if (!entry || typeof entry !== "object") return null;
  for (const key of ["file_path", "path", "file", "filepath"]) {
    if (typeof entry[key] === "string") return entry[key];
  }
  if (entry.location && typeof entry.location.file === "string") return entry.location.file;
  for (const key of ["node_id", "id"]) {
    if (typeof entry[key] === "string") {
      const match = entry[key].match(/(?:file::)?([^:#]+(?:\.[A-Za-z0-9]+))(?:[:#]|$)/);
      if (match) return match[1];
    }
  }
  return null;
}

function clipValue(value, maxChars) {
  if (typeof value === "string") {
    if (value.length <= maxChars) return value;
    return `${value.slice(0, maxChars)}\n...[truncated by m1nd agent context]`;
  }
  if (Array.isArray(value)) return value.map((entry) => clipValue(entry, maxChars));
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, entry]) => [key, clipValue(entry, maxChars)])
    );
  }
  return value;
}

function compactContextPayload(payload, maxChars) {
  const compact = {
    schema: payload.schema,
    file_path: payload.file_path,
    selected_file: payload.selected_file,
    graph_state: payload.graph_state,
    elapsed_ms: payload.elapsed_ms,
  };
  if (payload.file_contents) compact.file_contents = clipValue(payload.file_contents, maxChars);
  if (payload.context) compact.context = clipValue(payload.context, maxChars);
  if (Array.isArray(payload.connected_files)) {
    compact.connected_files = payload.connected_files.slice(0, 10);
    compact.connected_files_truncated = payload.connected_files.length > compact.connected_files.length;
  }
  if (Number.isInteger(payload.total_lines)) compact.total_lines = payload.total_lines;
  compact.capsule_truncated = JSON.stringify(payload).length > JSON.stringify(compact).length;
  return compact;
}

function queryLooksLikePath(repo, query) {
  if (!query) return null;
  for (const candidate of pathCandidatesFromQuery(query)) {
    const resolved = path.isAbsolute(candidate) ? candidate : path.resolve(repo, candidate);
    if (!pathContains(repo, resolved)) continue;
    if (fs.existsSync(resolved) && fs.statSync(resolved).isFile()) return resolved;
  }
  return null;
}

function pathCandidatesFromQuery(query) {
  const text = String(query || "").trim();
  if (!text) return [];
  const candidates = [text];
  for (const raw of text.split(/\s+/)) {
    const stripped = raw
      .replace(/^[`"'([{<]+/, "")
      .replace(/[`"',;)\]}>]+$/, "")
      .replace(/:\d+(?::\d+)?$/, "");
    if (stripped && stripped !== text) candidates.push(stripped);
  }
  return [...new Set(candidates)];
}

function identifierCandidatesFromQuery(query) {
  const text = String(query || "");
  const candidates = [];
  for (const match of text.matchAll(/[$A-Za-z_][$\w]*(?:(?:\.|::|#)[$A-Za-z_][$\w]*)*/g)) {
    const term = match[0];
    if (term.length < 3) continue;
    if (queryLooksLikeExactIdentifier(term)) candidates.push(term);
  }
  return [...new Set(candidates)].filter((term) => term !== text.trim());
}

function defaultAutoMode(query, requestedMode) {
  if (requestedMode) return requestedMode;
  return queryLooksBroadTask(query) ? "deep" : "short";
}

function observedText(raw, parsed) {
  if (typeof raw === "string" && raw.trim()) return raw;
  if (parsed && typeof parsed === "object") {
    try {
      return JSON.stringify(parsed);
    } catch (_) {
      return "";
    }
  }
  return "";
}

function classifyRecoveryType(raw, parsed) {
  const text = observedText(raw, parsed).toLowerCase();
  if (text.includes("transport closed")) return "transport_closed";
  const directPayload = parsed && typeof parsed === "object" ? parsed : null;
  const resultPayloads = Array.isArray(directPayload && directPayload.results) ? directPayload.results : [];
  if (
    payloadHasWrongWorkspaceBinding(directPayload) ||
    resultPayloads.some(payloadHasWrongWorkspaceBinding) ||
    (!directPayload && (text.includes("wrong_workspace_binding") || text.includes("wrong workspace")))
  ) {
    return "wrong_workspace_binding";
  }
  if (proofState(directPayload) === "blocked" || resultPayloads.some((entry) => proofState(entry) === "blocked")) {
    return "blocked_retrieval";
  }
  if (text.includes("blocked") || text.includes("zero")) return "blocked_retrieval";
  if (text.includes("missing")) return "missing_runtime_or_graph";
  if (text.includes("stale")) return "stale_runtime_or_graph";
  return "generic_recovery";
}

function payloadSwitchesToDirectProof(parsed) {
  if (!parsed || typeof parsed !== "object") return false;
  if (parsed.switch_to_direct_proof === true) return true;
  return Boolean(parsed.action && parsed.action.switch_to_direct_proof === true);
}

function buildRecoverAction(repo, recoveryType, rawSource, sourceMode, binary = null) {
  const useStdin = sourceMode === "stdin" || rawSource.length > 160 || rawSource.includes("\n") || rawSource.trim().startsWith("{");
  const fromValue = useStdin ? "stdin" : rawSource;
  return buildActionEnvelope({
    trigger: {
      kind: recoveryType,
      source: sourceMode === "stdin" ? "stdin" : "observed_input",
    },
    route: {
      kind: "recover",
      recovery_type: recoveryType,
    },
    action: {
      kind: "run_command",
      subcommand: "recover",
      command: buildAgentCliCommand("recover", repo, [["from", fromValue]], binary),
      summary: "Classify the observed failure and emit the isolated recovery path.",
    },
    proofRequirements: proofRequirementsForRoute("recover"),
  });
}

function buildOrientAction(repo, query, mode, tool, triggerKind, binary = null) {
  return buildActionEnvelope({
    trigger: {
      kind: triggerKind,
      source: "query",
      query,
    },
    route: {
      kind: "orient",
      tool,
      mode,
    },
    action: {
      kind: "run_command",
      subcommand: "orient",
      command: buildAgentCliCommand("orient", repo, [
        ["query", query],
        ["mode", mode],
        ["tool", tool],
      ], binary),
      summary: `Run one bounded orientation pass with ${tool}.`,
    },
    proofRequirements: proofRequirementsForRoute("orient"),
  });
}

function buildContextAction(repo, query, tokens, triggerKind, binary = null) {
  return buildActionEnvelope({
    trigger: {
      kind: triggerKind,
      source: "query",
      query,
    },
    route: {
      kind: "context",
    },
    action: {
      kind: "run_command",
      subcommand: "context",
      command: buildAgentCliCommand("context", repo, [
        ["query", query],
        ["tokens", tokens || 4000],
      ], binary),
      summary: "Capture a bounded context capsule for the named file or path.",
    },
    proofRequirements: proofRequirementsForRoute("context"),
  });
}

function buildFirstMinuteAction(repo, query, mode, triggerKind, binary = null, reason = "first_contact_broad_task") {
  return buildActionEnvelope({
    trigger: {
      kind: triggerKind,
      source: "query",
      query,
    },
    route: {
      kind: "first_minute",
      mode,
      reason,
    },
    action: {
      kind: "run_command",
      subcommand: "first-minute",
      command: buildAgentCliCommand("first-minute", repo, [
        ["query", query],
        ["mode", mode],
      ], binary),
      summary: "Run the agent first-minute loop: scope, trust, one orientation pass, then direct proof.",
    },
    proofRequirements: proofRequirementsForRoute("first_minute"),
  });
}

function buildDirectProofAction(reason) {
  return buildActionEnvelope({
    trigger: {
      kind: reason,
      source: "prior_agent_payload",
    },
    route: {
      kind: "direct_proof",
      reason,
    },
    action: {
      kind: "handoff",
      summary: "The prior payload already requested a direct-proof handoff.",
      command: "Switch to direct source reads, targeted tests, compiler/runtime output, or focused probes before final claims.",
    },
    proofRequirements: proofRequirementsForRoute("direct_proof"),
    switchToDirectProof: true,
  });
}

function autoActionForQuery(args, repo, binary = null) {
  const query = args.query;
  if (!query) throw new Error("agent auto requires --query <text> when --from is not provided");
  const requestedTool = args.tool || "auto";
  const mode = defaultAutoMode(query, args.mode);
  if (!ORIENTATION_TOOLS.has(requestedTool)) throw new Error(`unsupported agent auto tool '${requestedTool}'`);
  if (requestedTool !== "auto") {
    return buildOrientAction(repo, query, mode, requestedTool, "explicit_tool_override", binary);
  }
  if (queryLooksLikeFirstContact(query)) {
    return buildFirstMinuteAction(repo, query, mode, "first_contact_broad_task", binary);
  }
  if (queryLooksLikePath(repo, query)) {
    return buildContextAction(repo, query, args.tokens, "exact_path", binary);
  }
  if (queryHasGlobPattern(query)) {
    return buildOrientAction(repo, query, mode, "glob", "glob_query", binary);
  }
  if (queryHasPathSignal(query)) {
    return buildOrientAction(repo, query, mode, "glob", "needs_orientation_before_context", binary);
  }
  const tool = chooseOrientationTool("auto", query, mode);
  const triggerKind = queryLooksLikeExactIdentifier(query)
    ? "exact_identifier"
    : queryLooksBroadTask(query)
      ? "broad_task"
      : "natural_language";
  return buildOrientAction(repo, query, mode, tool, triggerKind, binary);
}

function autoActionForObserved(args, repo, binary = null) {
  const from = args.from;
  const raw = from === "stdin" ? fs.readFileSync(0, "utf8") : from;
  const parsed = safeJsonParse(raw);
  const recoveryType = classifyRecoveryType(raw, parsed);
  if (recoveryType !== "generic_recovery") {
    return {
      observed: parsed || { text: raw },
      action: buildRecoverAction(repo, recoveryType, String(raw || ""), from === "stdin" ? "stdin" : "inline", binary),
    };
  }
  if (payloadSwitchesToDirectProof(parsed)) {
    return {
      observed: parsed,
      action: buildDirectProofAction("prior_switch_to_direct_proof"),
    };
  }
  return {
    observed: parsed || { text: raw },
    action: buildRecoverAction(repo, "generic_recovery", String(raw || ""), from === "stdin" ? "stdin" : "inline", binary),
  };
}

function ensureWithinRepo(repo, target) {
  const resolved = path.isAbsolute(target) ? target : path.resolve(repo, target);
  const repoPath = path.resolve(repo);
  const targetPath = fs.existsSync(resolved) ? realPathOrResolved(resolved) : resolved;
  const safeRepoPath = fs.existsSync(repoPath) ? realPathOrResolved(repoPath) : repoPath;
  if (!pathContains(repoPath, resolved) && !pathContains(safeRepoPath, targetPath)) {
    throw new Error(`path escapes repo: ${target}`);
  }
  return resolved;
}

function anchorGroupForFile(filePath) {
  const normalized = String(filePath || "").replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  if (parts.length >= 3) return `${parts[0]}/${parts[1]}`;
  if (parts.length >= 2) return parts[0];
  return path.dirname(normalized) === "." ? "root" : path.dirname(normalized);
}

function anchorsFromPayload(payload, repo, limit = 10) {
  const seen = new Set();
  const anchors = [];
  for (const entry of extractResultList(payload)) {
    const file = extractFileFromEntry(entry);
    if (!file) continue;
    let resolved;
    try {
      resolved = ensureWithinRepo(repo, file);
    } catch (_) {
      continue;
    }
    const relative = path.relative(repo, resolved) || path.basename(resolved);
    if (seen.has(relative)) continue;
    seen.add(relative);
    anchors.push({
      path: relative,
      absolute_path: resolved,
      group: anchorGroupForFile(relative),
      source: "orientation_result",
    });
    if (anchors.length >= limit) break;
  }
  return anchors;
}

function groupAnchors(anchors) {
  const groups = {};
  for (const anchor of anchors) {
    const group = anchor.group || "root";
    if (!groups[group]) groups[group] = [];
    groups[group].push(anchor.path);
  }
  return groups;
}

function operatingContract(maxGraphCalls = 2) {
  return {
    schema: "m1nd-agent-operating-contract-v0",
    role: "orientation_and_routing",
    max_graph_calls_before_direct_proof: maxGraphCalls,
    proven_by_m1nd: [
      "repo scope and isolated runtime binding",
      "trust/graph health envelope",
      "candidate anchors from one bounded graph pass",
    ],
    requires_direct_proof: [
      "behavioral claims",
      "bug findings",
      "architecture conclusions beyond listed anchors",
      "runtime, compiler, browser, deploy, or production behavior",
    ],
    stop_conditions: [
      "orientation returned concrete anchors",
      "retrieval is blocked or empty unexpectedly",
      "the task needs execution truth",
    ],
  };
}

async function withClient(args, deps, repo, fn) {
  const binary = args.binary ? path.resolve(args.binary) : deps.findRuntimeBinary() || deps.defaultRuntimePath();
  const client = new McpRuntimeClient({
    binary,
    repo,
    sharedRuntime: Boolean(args["shared-runtime"]),
  });
  try {
    await client.start();
    return await fn(client, binary);
  } finally {
    client.close();
  }
}

async function runTrustSequence(client, repo, agentId, ensureIngest) {
  const calls = [];
  const trustBefore = await callToolSafely(client, "trust_selftest", { agent_id: agentId });
  calls.push(callSummary("trust_selftest", trustBefore));
  let ingest = null;
  let handshake = null;
  if (ensureIngest && trustNeedsIngest(trustBefore)) {
    ingest = await callToolSafely(client, "ingest", { agent_id: agentId, path: repo });
    calls.push(callSummary("ingest", ingest));
    handshake = await callToolSafely(client, "session_handshake", { agent_id: agentId, scope: repo });
    calls.push(callSummary("session_handshake", handshake));
  } else {
    handshake = await callToolSafely(client, "session_handshake", { agent_id: agentId, scope: repo });
    calls.push(callSummary("session_handshake", handshake));
  }
  return { trustBefore, ingest, handshake, calls };
}

async function agentScope(args, deps, repo, agentId) {
  const binary = args.binary ? path.resolve(args.binary) : deps.findRuntimeBinary() || deps.defaultRuntimePath();
  const envelope = baseAgentEnvelope({
    command: "scope",
    repo,
    agentId,
    runtime: runtimeInfo(binary, deps),
    scopeAlignment: buildScopeAlignment(repo),
  });
  envelope.ok = Boolean(binary);
  envelope.package_version = deps.readPackageVersion();
  envelope.git_root = findGitRoot(repo);
  envelope.host_hints = {
    env_workspace_root: process.env.M1ND_WORKSPACE_ROOT || null,
    selected_binary: binary || null,
    default_runtime_path: deps.defaultRuntimePath(),
  };
  envelope.action = buildActionEnvelope({
    trigger: {
      kind: "scope_ready",
      source: "agent_scope",
    },
    route: {
      kind: "trust",
    },
    action: {
      kind: "run_command",
      subcommand: "trust",
      command: buildAgentCliCommand("trust", repo, [["ensure-ingest", true]], binary),
      summary: "Establish trust on the isolated repo-bound runtime before retrieval.",
    },
    proofRequirements: proofRequirementsForRoute("orient"),
  });
  envelope.next_actions.push("Run m1nd agent trust --repo <repo> --ensure-ingest --json before relying on retrieval.");
  return envelope;
}

async function agentTrust(args, deps, repo, agentId) {
  return withClient(args, deps, repo, async (client, binary) => {
    const sequence = await runTrustSequence(client, repo, agentId, Boolean(args["ensure-ingest"]));
    const payload = payloadDict(sequence.handshake || sequence.trustBefore);
    const graphState = extractGraphState(payload);
    const envelope = baseAgentEnvelope({
      command: "trust",
      repo,
      agentId,
      runtime: { ...runtimeInfo(binary, deps), runtime_root: client.runtimeDir || null },
      scopeAlignment: buildScopeAlignment(repo),
      graphState,
      trust: {
        verdict: payload.trust_mode || payload.verdict || payload.status || "unknown",
        needs_ingest: trustNeedsIngest(sequence.trustBefore),
      },
    });
    envelope.calls = sequence.calls;
    envelope.results = [payload];
    if (trustNeedsIngest(sequence.trustBefore) && !args["ensure-ingest"]) {
      envelope.action = buildActionEnvelope({
        trigger: {
          kind: "needs_ingest",
          source: "trust_sequence",
        },
        route: {
          kind: "trust",
        },
        action: {
          kind: "run_command",
          subcommand: "trust",
          command: buildAgentCliCommand("trust", repo, [["ensure-ingest", true]], binary),
          summary: "Re-run trust with ingest enabled before relying on retrieval.",
        },
        proofRequirements: proofRequirementsForRoute("recover"),
      });
      envelope.next_actions.push("Re-run with --ensure-ingest or call ingest before retrieval.");
    } else {
      envelope.action = buildActionEnvelope({
        trigger: {
          kind: "trust_ready",
          source: "trust_sequence",
        },
        route: {
          kind: "orient",
          tool: "seek",
          mode: "short",
        },
        action: {
          kind: "run_command",
          subcommand: "orient",
          command: buildAgentCliCommand("orient", repo, [
            ["query", "<focused task>"],
            ["mode", "short"],
            ["tool", "seek"],
          ], binary),
          summary: "Run one bounded orientation pass, then switch back to direct proof.",
        },
        proofRequirements: proofRequirementsForRoute("orient"),
      });
      envelope.next_actions.push("Use m1nd agent orient for one bounded orientation pass, then prove directly.");
    }
    return envelope;
  });
}

async function agentOrient(args, deps, repo, agentId) {
  const query = args.query;
  if (!query) throw new Error("agent orient requires --query <text>");
  const mode = args.mode || "short";
  const requestedTool = args.tool || "auto";
  if (!ORIENTATION_TOOLS.has(requestedTool)) throw new Error(`unsupported agent orient tool '${requestedTool}'`);
  const tool = chooseOrientationTool(requestedTool, query, mode);
  const topK = Number(args["top-k"] || args.topK || 5);
  return withClient(args, deps, repo, async (client, binary) => {
    const sequence = await runTrustSequence(client, repo, agentId, !args["skip-ingest"]);
    const orientation = await callToolSafely(
      client,
      tool,
      orientationArgs(tool, { agentId, repo, query, topK })
    );
    const orientationPayload = payloadDict(orientation);
    const candidateTotal = candidateCount(orientationPayload);
    const orientationBlocked = proofState(orientationPayload) === "blocked";
    const useful = !orientation.isError && !orientationBlocked && candidateTotal > 0;
    const graphState = extractGraphState(orientationPayload);
    const envelope = baseAgentEnvelope({
      command: "orient",
      repo,
      agentId,
      runtime: { ...runtimeInfo(binary, deps), runtime_root: client.runtimeDir || null },
      scopeAlignment: buildScopeAlignment(repo),
      graphState,
      trust: {
        verdict: payloadDict(sequence.handshake).trust_mode || payloadDict(sequence.trustBefore).verdict || "unknown",
      },
    });
    envelope.query = query;
    envelope.mode = mode;
    envelope.orientation_tool = tool;
    envelope.m1nd_usage_mode = useful ? "short_audit_orientation" : "recovery_overhead";
    envelope.proof_boundary = {
      m1nd_proved: useful
        ? "m1nd found candidate orientation anchors for the requested scope"
        : "m1nd proved only the trust/recovery state for this orientation attempt",
      still_needs_direct_proof: [
        "direct source reads",
        "focused tests or compiler/runtime output",
        "manual verification of any selected anchor before final claims",
      ],
    };
    envelope.switch_to_direct_proof = mode === "short" || !useful;
    envelope.calls = [...sequence.calls, callSummary(tool, orientation)];
    envelope.results = [orientationPayload];
    if (envelope.switch_to_direct_proof) {
      envelope.action = buildDirectProofAction(useful ? "orientation_short_handoff" : "orientation_not_useful");
      envelope.next_actions.push("Switch to direct source reads, tests, compiler/runtime output, or focused probes for final truth.");
    } else {
      envelope.action = buildActionEnvelope({
        trigger: {
          kind: "orientation_useful",
          source: "orientation_result",
          query,
        },
        route: {
          kind: "context",
        },
        action: {
          kind: "run_command",
          subcommand: "context",
          command: buildAgentCliCommand("context", repo, [["query", query]], binary),
          summary: "Pull a bounded source capsule before moving to direct proof.",
        },
        proofRequirements: proofRequirementsForRoute("context"),
      });
    }
    if (!useful) {
      envelope.next_actions.push("If retrieval looked suspicious, run m1nd agent recover with the observed payload or error.");
    }
    return attachCapabilityGuidance(envelope, query);
  });
}

async function agentFirstMinute(args, deps, repo, agentId) {
  const query = args.query || "understand this repo";
  const mode = args.mode || "short";
  const requestedTool = args.tool || "auto";
  if (!ORIENTATION_TOOLS.has(requestedTool)) throw new Error(`unsupported agent first-minute tool '${requestedTool}'`);
  const tool = chooseOrientationTool(requestedTool, query, mode === "short" ? "normal" : mode);
  const topK = Number(args["top-k"] || args.topK || 8);
  return withClient(args, deps, repo, async (client, binary) => {
    const sequence = await runTrustSequence(client, repo, agentId, true);
    const orientation = await callToolSafely(
      client,
      tool,
      orientationArgs(tool, { agentId, repo, query, topK })
    );
    const orientationPayload = payloadDict(orientation);
    const candidateTotal = candidateCount(orientationPayload);
    const orientationBlocked = proofState(orientationPayload) === "blocked";
    const useful = !orientation.isError && !orientationBlocked && candidateTotal > 0;
    const anchors = useful ? anchorsFromPayload(orientationPayload, repo, 10) : [];
    const graphState = extractGraphState(orientationPayload);
    const envelope = baseAgentEnvelope({
      command: "first-minute",
      repo,
      agentId,
      runtime: { ...runtimeInfo(binary, deps), runtime_root: client.runtimeDir || null },
      scopeAlignment: buildScopeAlignment(repo),
      graphState,
      trust: {
        verdict: payloadDict(sequence.handshake).trust_mode || payloadDict(sequence.trustBefore).verdict || "unknown",
      },
    });
    envelope.query = query;
    envelope.mode = mode;
    envelope.orientation_tool = tool;
    envelope.operating_contract = operatingContract(mode === "deep" ? 3 : 2);
    envelope.m1nd_usage_mode = useful ? "first_minute_orientation" : "recovery_overhead";
    envelope.switch_to_direct_proof = true;
    envelope.calls = [...sequence.calls, callSummary(tool, orientation)];
    envelope.results = [orientationPayload];
    envelope.anchors = anchors;
    envelope.anchor_groups = groupAnchors(anchors);
    envelope.do_not = [
      "do not call agent context on a broad narrative query without a concrete anchor",
      "do not treat graph candidates as final code truth",
      "do not spend more graph calls before reading source if anchors are present",
    ];
    envelope.proof_boundary = {
      m1nd_proved: useful
        ? "scope/trust are established and one bounded orientation pass returned candidate anchors"
        : "scope/trust were checked, but orientation did not produce useful anchors",
      still_needs_direct_proof: [
        "read the selected anchors directly",
        "run focused tests, compiler/runtime checks, logs, browser smoke, or probes before final claims",
      ],
    };
    envelope.action = buildDirectProofAction(useful ? "first_minute_handoff" : "first_minute_recovery_or_direct_proof");
    if (anchors.length > 0) {
      envelope.recommended_next_command = `Read one anchor directly, for example: ${anchors[0].path}`;
      envelope.next_actions.push(envelope.recommended_next_command);
    } else {
      envelope.recommended_next_command = buildAgentCliCommand("recover", repo, [["from", "stdin"]], binary);
      envelope.next_actions.push("If orientation looked suspicious, pipe this JSON to m1nd agent recover --from stdin.");
    }
    envelope.next_actions.push("After direct proof, report what was read/tested and what remains a non-claim.");
    return attachCapabilityGuidance(envelope, query);
  });
}

async function agentRecover(args, deps, repo, agentId) {
  const from = args.from || "unknown";
  const raw = from === "stdin" ? fs.readFileSync(0, "utf8") : from;
  const parsed = safeJsonParse(raw);
  const text = typeof raw === "string" ? raw : JSON.stringify(raw);
  const recoveryType = classifyRecoveryType(text, parsed);

  const binary = args.binary ? path.resolve(args.binary) : deps.findRuntimeBinary() || deps.defaultRuntimePath();
  const envelope = baseAgentEnvelope({
    command: "recover",
    repo,
    agentId,
    runtime: runtimeInfo(binary, deps),
    scopeAlignment: buildScopeAlignment(repo),
  });
  envelope.recovery_type = recoveryType;
  envelope.observed = parsed || { text };
  envelope.recovery_plan = recoveryPlan(recoveryType, repo, binary);
  envelope.action = buildRecoverAction(repo, recoveryType, text, from === "stdin" ? "stdin" : "inline", binary);
  envelope.next_actions = envelope.recovery_plan.map((step) => step.command || step.action);
  envelope.ok = true;
  return envelope;
}

async function agentContext(args, deps, repo, agentId) {
  const query = args.query;
  if (!query) throw new Error("agent context requires --query <text>");
  const maxOutputChars = Math.max(1000, Number(args.tokens || 4000) * 4);
  const anchor = args.anchor || null;
  const directFile = anchor ? ensureWithinRepo(repo, anchor) : queryLooksLikePath(repo, query);
  const allowDiscovery = Boolean(args["allow-discovery"]);
  const strongIdentifiers = strongIdentifierCandidatesFromQuery(query);
  const hasStrongIdentifier = queryLooksLikeExactIdentifier(query) || strongIdentifiers.length > 0;
  return withClient(args, deps, repo, async (client, binary) => {
    const sequence = await runTrustSequence(client, repo, agentId, !args["skip-ingest"]);
    let selectedFile = directFile;
    let discovery = null;
    const discoveryCalls = [];
    const envelope = baseAgentEnvelope({
      command: "context",
      repo,
      agentId,
      runtime: { ...runtimeInfo(binary, deps), runtime_root: client.runtimeDir || null },
      scopeAlignment: buildScopeAlignment(repo),
      trust: {
        verdict: payloadDict(sequence.handshake).trust_mode || payloadDict(sequence.trustBefore).verdict || "unknown",
      },
    });
    envelope.query = query;
    envelope.max_output_chars = maxOutputChars;
    envelope.calls = [...sequence.calls];
    if (!selectedFile && !allowDiscovery && !hasStrongIdentifier) {
      envelope.ok = false;
      envelope.needs_orientation_first = true;
      envelope.context_confidence = "needs_orientation_first";
      envelope.switch_to_direct_proof = false;
      envelope.proof_boundary = {
        m1nd_proved: "context refused a broad query without a concrete source anchor",
        still_needs_direct_proof: [
          "run first-minute or orient to identify anchors",
          "then call context with --anchor <file> or read the anchor directly",
        ],
      };
      envelope.action = queryLooksLikeFirstContact(query)
        ? buildFirstMinuteAction(repo, query, args.mode || "short", "needs_orientation_before_context", binary, "needs_orientation_before_context")
        : buildOrientAction(repo, query, args.mode || "short", chooseOrientationTool(args.tool || "auto", query, args.mode || "short"), "needs_orientation_before_context", binary);
      if (envelope.action && envelope.action.action && envelope.action.action.command) {
        envelope.next_actions.push(envelope.action.action.command);
      }
      envelope.next_actions.push("After orientation, use --anchor <file> or direct source reads before final claims.");
      return attachCapabilityGuidance(envelope, query);
    }
    if (!selectedFile) {
      const discoveryQueries = [query, ...strongIdentifiers];
      for (const searchQuery of discoveryQueries) {
        discovery = await callToolSafely(client, "search", { agent_id: agentId, query: searchQuery, scope: repo, top_k: 3 });
        discoveryCalls.push(["search", discovery]);
        const first = extractResultList(payloadDict(discovery)).map(extractFileFromEntry).find(Boolean);
        if (first) {
          selectedFile = ensureWithinRepo(repo, first);
          break;
        }
      }
      if (!selectedFile) {
        discovery = await callToolSafely(client, "seek", { agent_id: agentId, query, scope: repo, top_k: 3 });
        discoveryCalls.push(["seek", discovery]);
        const first = extractResultList(payloadDict(discovery)).map(extractFileFromEntry).find(Boolean);
        if (first) selectedFile = ensureWithinRepo(repo, first);
      }
    }
    for (const [tool, result] of discoveryCalls) envelope.calls.push(callSummary(tool, result));
    if (!selectedFile) {
      envelope.ok = false;
      envelope.switch_to_direct_proof = true;
      envelope.context_confidence = allowDiscovery ? "discovery_allowed_no_anchor" : "identifier_anchor_not_found";
      envelope.next_actions.push("No source anchor was found; use search/glob/direct file reads to identify a concrete file first.");
      return attachCapabilityGuidance(envelope, query);
    }
    const context = await callToolSafely(client, "surgical_context_v2", {
      agent_id: agentId,
      file_path: selectedFile,
      include_tests: true,
      radius: 1,
      max_connected_files: 5,
      max_lines_per_file: Math.max(20, Math.floor(maxOutputChars / 600)),
    });
    envelope.selected_file = selectedFile;
    envelope.context_confidence = anchor || directFile
      ? "direct_anchor"
      : allowDiscovery
        ? "discovery_allowed"
        : "identifier_anchor";
    envelope.calls.push(callSummary("surgical_context_v2", context));
    envelope.results = [compactContextPayload(payloadDict(context), maxOutputChars)];
    envelope.action = buildDirectProofAction("context_capsule_ready");
    envelope.proof_boundary = {
      m1nd_proved: "m1nd built a bounded context capsule for a concrete source anchor",
      still_needs_direct_proof: [
        "read the selected file directly",
        "run focused tests or runtime probes before final claims",
      ],
    };
    envelope.next_actions.push("Use this context capsule for planning only; final claims still need direct proof.");
    return attachCapabilityGuidance(envelope, query);
  });
}

async function agentAuto(args, deps, repo, agentId, requestedCommand = "auto") {
  const binary = args.binary ? path.resolve(args.binary) : deps.findRuntimeBinary() || deps.defaultRuntimePath();
  const commandName = requestedCommand === "next" ? "next" : "auto";
  const envelope = baseAgentEnvelope({
    command: commandName,
    repo,
    agentId,
    runtime: runtimeInfo(binary, deps),
    scopeAlignment: buildScopeAlignment(repo),
  });
  if (commandName !== "auto") {
    envelope.resolved_command = "auto";
  }
  if (args.from) {
    const observed = autoActionForObserved(args, repo, binary);
    envelope.observed = observed.observed;
    envelope.action = observed.action;
  } else {
    envelope.query = args.query;
    envelope.mode = defaultAutoMode(args.query, args.mode);
    envelope.action = autoActionForQuery(args, repo, binary);
  }
  if (envelope.action && envelope.action.action && envelope.action.action.command) {
    envelope.next_actions.push(envelope.action.action.command);
  }
  if (envelope.action && envelope.action.route) {
    envelope.route_reason = envelope.action.route.reason || envelope.action.trigger.kind || null;
  }
  if (envelope.action && envelope.action.switch_to_direct_proof) {
    envelope.switch_to_direct_proof = true;
  }
  envelope.operating_contract = {
    first_move: "scope/trust before retrieval; one bounded orientation before direct proof",
    context_rule: "agent context requires a concrete anchor unless --allow-discovery is explicit",
    final_truth: "source reads, tests, compiler/runtime output, logs, browser smoke, or focused probes",
  };
  return attachCapabilityGuidance(envelope, args.query);
}

async function agentHandoff(args, deps, repo, agentId) {
  const binary = args.binary ? path.resolve(args.binary) : deps.findRuntimeBinary() || deps.defaultRuntimePath();
  const envelope = baseAgentEnvelope({
    command: "handoff",
    repo,
    agentId,
    runtime: runtimeInfo(binary, deps),
    scopeAlignment: buildScopeAlignment(repo),
  });
  envelope.handoff = {
    schema: "m1nd-agent-handoff-v0",
    source: args.from || "last-run",
    summary: "No durable CLI mission state exists yet; this handoff records scope and recommended resume path.",
    verified_claims: [],
    open_hypotheses: [],
    dead_paths: [],
    resume_hint: "Run m1nd agent scope, then m1nd agent orient --mode short for the next concrete task.",
  };
  envelope.next_actions.push(envelope.handoff.resume_hint);
  return envelope;
}

async function agentDoctor(args, deps, repo, agentId) {
  const binary = args.binary ? path.resolve(args.binary) : deps.findRuntimeBinary() || deps.defaultRuntimePath();
  const envelope = baseAgentEnvelope({
    command: "doctor",
    repo,
    agentId,
    runtime: runtimeInfo(binary, deps),
    scopeAlignment: buildScopeAlignment(repo),
  });
  envelope.package_doctor = deps.doctor();
  envelope.hosts = deps.hostStatus({ ...args, host: args.host || "all", project: repo, binary });
  envelope.update = deps.selfUpdate({ ...args, _: ["update", "status"], channel: args.channel || "beta", binary, "no-kill": true });
  envelope.pack = deps.assertPackShape();
  envelope.next_actions.push("If any host/runtime surface needs attention, apply the emitted update/hosts plan and restart/rebind the host.");
  envelope.next_actions.push("After host rebind, call trust_selftest or run m1nd agent trust --ensure-ingest.");
  return envelope;
}

function recoveryPlan(type, repo, binary) {
  const base = [
    {
      action: "prove_scope_with_agent_cli",
      command: buildAgentCliCommand("scope", repo, [], binary),
    },
    {
      action: "run_isolated_trust",
      command: buildAgentCliCommand("trust", repo, [["ensure-ingest", true]], binary),
    },
  ];
  if (type === "transport_closed") {
    return [
      {
        action: "verify_local_runtime_outside_dead_host",
        command: buildAgentCliCommand("doctor", repo, [], binary),
      },
      {
        action: "restart_or_rebind_host",
        command: "Restart/rebind the MCP host or open a fresh session.",
      },
      ...base,
    ];
  }
  if (type === "wrong_workspace_binding") {
    return [
      {
        action: "rebind_host_workspace",
        command: `Set M1ND_WORKSPACE_ROOT=${repo} in the host MCP config.`,
      },
      {
        action: "use_isolated_cli_bypass_now",
        command: `M1ND_WORKSPACE_ROOT=${repo} ${buildAgentCliCommand("orient", repo, [
          ["query", "<focused task>"],
          ["mode", "short"],
        ], binary)}`,
      },
      ...base,
    ];
  }
  if (type === "stale_runtime_or_graph" || type === "missing_runtime_or_graph") {
    return [
      {
        action: "inspect_install",
        command: "m1nd update status --channel beta --json",
      },
      {
        action: "plan_update_if_needed",
        command: "m1nd update plan --channel beta --json",
      },
      {
        action: "check_host_wiring",
        command: `m1nd hosts status --host all --project ${repo} --binary ${binary || "<m1nd-mcp>"} --json`,
      },
      ...base,
    ];
  }
  return [
    ...base,
    {
      action: "switch_to_direct_proof",
      command: "Use source reads, tests, compiler/runtime output, or focused probes before final claims.",
    },
  ];
}

function findGitRoot(repo) {
  let current = path.resolve(repo);
  while (true) {
    if (fs.existsSync(path.join(current, ".git"))) return current;
    const parent = path.dirname(current);
    if (parent === current) return null;
    current = parent;
  }
}

const KICKSTART_SCHEMA = "m1nd-kickstart-v0";

async function agentKickstart(args, deps) {
  const repo = path.resolve(args.repo || args.project || process.cwd());
  if (!args.repo && !args.project) throw new Error("agentKickstart requires --repo <path>");
  const auditPath = args["audit-path"] ? path.resolve(args["audit-path"]) : repo;
  const agentId = args["agent-id"] || "m1nd-kickstart";
  const t0 = Date.now();
  let trustMs = 0;
  let ingestMs = 0;
  let auditMs = 0;

  try {
    return await withClient(args, deps, repo, async (client) => {
      const tTrust0 = Date.now();
      const sequence = await runTrustSequence(client, repo, agentId, true);
      trustMs = Date.now() - tTrust0;

      const trustPayload = payloadDict(sequence.trustBefore);
      const handshakePayload = payloadDict(sequence.handshake);
      const trustVerdict = handshakePayload.trust_mode || trustPayload.verdict || "unknown";
      const graphState = extractGraphState(handshakePayload) || extractGraphState(trustPayload) || {};
      const nodeCount = Number(graphState.node_count || 0);
      const edgeCount = Number(graphState.edge_count || 0);

      let ingestPerformed = false;
      let filesParsed = 0;
      if (sequence.ingest) {
        ingestMs = 0; // already baked into trust sequence; report 0 for separate timing
        const ingestPayload = payloadDict(sequence.ingest);
        ingestPerformed = true;
        filesParsed = Number(ingestPayload.files_parsed || ingestPayload.files_indexed || ingestPayload.count || 0);
      }

      const tAudit0 = Date.now();
      const auditResult = await callToolSafely(client, "audit", {
        agent_id: agentId,
        path: auditPath,
        profile: "auto",
      });
      auditMs = Date.now() - tAudit0;

      const auditPayload = payloadDict(auditResult);
      const auditBlocked = proofState(auditPayload) === "blocked" || auditResult.isError;
      const auditSummary = auditPayload.summary ||
        (auditPayload.next_action ? `Audit completed. Next: ${auditPayload.next_action}` : null) ||
        (auditBlocked ? "Audit was blocked or returned an error." : "Audit completed successfully.");

      const trustOk = trustVerdict !== "irrecoverable" && trustVerdict !== "blocked";
      const ok = trustOk && !auditBlocked;

      let nextAction = "ready_to_query";
      if (!trustOk) {
        nextAction = "recovery_required";
      } else if (!ingestPerformed && nodeCount === 0) {
        nextAction = "needs_reingest";
      } else if (auditBlocked) {
        nextAction = "needs_reingest";
      }

      const totalMs = Date.now() - t0;
      return {
        schema: KICKSTART_SCHEMA,
        ok,
        trust_verdict: trustVerdict,
        node_count: nodeCount,
        edge_count: edgeCount,
        ingest: {
          performed: ingestPerformed,
          files_parsed: filesParsed,
        },
        audit_summary: auditSummary,
        next_action: nextAction,
        non_claims: [
          "does not prove runtime or production behavior",
          "graph counts are orientation estimates, not code truth",
          "kickstart does not replace direct source reads or focused tests",
          "audit summary is derived from graph state, not static analysis output",
        ],
        timing_ms: {
          trust: trustMs,
          ingest: ingestMs,
          audit: auditMs,
          total: totalMs,
        },
      };
    });
  } catch (err) {
    const totalMs = Date.now() - t0;
    return {
      schema: KICKSTART_SCHEMA,
      ok: false,
      trust_verdict: "error",
      node_count: 0,
      edge_count: 0,
      ingest: { performed: false, files_parsed: 0 },
      audit_summary: "Kickstart failed before audit could run.",
      next_action: "recovery_required",
      non_claims: [
        "does not prove runtime or production behavior",
        "graph counts are orientation estimates, not code truth",
        "kickstart does not replace direct source reads or focused tests",
        "audit summary is derived from graph state, not static analysis output",
      ],
      timing_ms: { trust: trustMs, ingest: ingestMs, audit: auditMs, total: totalMs },
      error: err.message || String(err),
    };
  }
}

async function agentCommand(args, deps) {
  const subcommand = args._[1] || "scope";
  const repo = path.resolve(args.repo || args.project || process.cwd());
  const normalizedSubcommand = subcommand === "next" ? "auto" : subcommand;
  const agentId = args["agent-id"] || `m1nd-agent-${subcommand === "next" ? "next" : normalizedSubcommand}`;
  switch (normalizedSubcommand) {
    case "scope":
      return agentScope(args, deps, repo, agentId);
    case "trust":
      return agentTrust(args, deps, repo, agentId);
    case "orient":
    case "short-audit":
      return agentOrient(args, deps, repo, agentId);
    case "first-minute":
      return agentFirstMinute(args, deps, repo, agentId);
    case "auto":
      return agentAuto(args, deps, repo, agentId, subcommand);
    case "recover":
      return agentRecover(args, deps, repo, agentId);
    case "context":
      return agentContext(args, deps, repo, agentId);
    case "handoff":
      return agentHandoff(args, deps, repo, agentId);
    case "doctor":
      return agentDoctor(args, deps, repo, agentId);
    default:
      throw new Error(`unknown agent subcommand '${subcommand}'`);
  }
}

module.exports = {
  AGENT_CLI_SCHEMA,
  KICKSTART_SCHEMA,
  agentCommand,
  agentKickstart,
  classifyScopeBinding,
  chooseOrientationTool,
  extractFileFromEntry,
  agentNonClaims,
  candidateCount,
};
