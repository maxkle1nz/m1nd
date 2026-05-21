"use strict";

const fs = require("fs");
const path = require("path");
const { McpRuntimeClient, callToolSafely } = require("./mcp-runtime-client");
const { AGENT_CLI_SCHEMA, agentNonClaims, baseAgentEnvelope } = require("./agent-schemas");

const AGENT_ACTION_SCHEMA = "m1nd-agent-action-envelope-v0";
const ORIENTATION_TOOLS = new Set(["auto", "search", "seek", "activate", "audit", "glob"]);

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
  if (wordCount < 4) return false;
  return /\b(audit|architecture|overview|map|review|investigate|debug|trace|refactor|explore|understand|system|subsystem|pipeline|dependency|dependencies|release|bug|issue)\b/i.test(text);
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
  if (queryLooksLikePath(repo, query)) {
    return buildContextAction(repo, query, args.tokens, "exact_path", binary);
  }
  if (queryHasGlobPattern(query)) {
    return buildOrientAction(repo, query, mode, "glob", "glob_query", binary);
  }
  if (queryHasPathSignal(query)) {
    return buildContextAction(repo, query, args.tokens, "path_query", binary);
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
    return envelope;
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
  const directFile = queryLooksLikePath(repo, query);
  return withClient(args, deps, repo, async (client, binary) => {
    const sequence = await runTrustSequence(client, repo, agentId, !args["skip-ingest"]);
    let selectedFile = directFile;
    let discovery = null;
    const discoveryCalls = [];
    if (!selectedFile) {
      const discoveryQueries = [query, ...identifierCandidatesFromQuery(query)];
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
    for (const [tool, result] of discoveryCalls) envelope.calls.push(callSummary(tool, result));
    if (!selectedFile) {
      envelope.ok = false;
      envelope.switch_to_direct_proof = true;
      envelope.next_actions.push("No source anchor was found; use search/glob/direct file reads to identify a concrete file first.");
      return envelope;
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
    envelope.calls.push(callSummary("surgical_context_v2", context));
    envelope.results = [compactContextPayload(payloadDict(context), maxOutputChars)];
    envelope.action = buildDirectProofAction("context_capsule_ready");
    envelope.next_actions.push("Use this context capsule for planning only; final claims still need direct proof.");
    return envelope;
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
  if (envelope.action && envelope.action.switch_to_direct_proof) {
    envelope.switch_to_direct_proof = true;
  }
  return envelope;
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
  agentCommand,
  classifyScopeBinding,
  chooseOrientationTool,
  extractFileFromEntry,
  agentNonClaims,
  candidateCount,
};
