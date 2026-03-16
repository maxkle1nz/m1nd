import type { ToolId } from '../types';

export interface ParsedCommand {
  tool: ToolId;
  params: Record<string, unknown>;
  raw: string;
}

/** Known tool aliases and keywords. */
const TOOL_ALIASES: Record<string, ToolId> = {
  activate: 'activate',
  act: 'activate',
  a: 'activate',
  seek: 'seek',
  find: 'seek',
  scan: 'scan',
  missing: 'missing',
  gap: 'missing',
  diff: 'differential',
  differential: 'differential',
  impact: 'impact',
  blast: 'impact',
  why: 'why',
  trace: 'trace',
  counterfactual: 'counterfactual',
  cf: 'counterfactual',
  predict: 'predict',
  hypothesize: 'hypothesize',
  hypo: 'hypothesize',
  validate: 'validate_plan',
  fingerprint: 'fingerprint',
  fp: 'fingerprint',
  resonate: 'resonate',
  drift: 'drift',
  timeline: 'timeline',
  diverge: 'diverge',
  warmup: 'warmup',
  health: 'health',
  ingest: 'ingest',
  learn: 'learn',
  trails: 'trail.list',
  perspective: 'perspective.start',
};

/**
 * Parse freeform command palette input into tool + params.
 *
 * Formats supported:
 *   "activate http_server"        → activate(query="http_server")
 *   "impact src/session.rs"       → impact(node_id="src/session.rs")
 *   "why A B"                     → why(source="A", target="B")
 *   "ingest /path/to/repo"        → ingest(path="/path/to/repo")
 *   "health"                      → health({})
 */
export function parseCommand(input: string): ParsedCommand {
  const trimmed = input.trim();
  if (!trimmed) {
    return { tool: 'activate', params: {}, raw: input };
  }

  const parts = trimmed.split(/\s+/);
  const keyword = parts[0].toLowerCase();
  const rest = parts.slice(1).join(' ');

  const tool: ToolId = TOOL_ALIASES[keyword] ?? 'activate';

  let params: Record<string, unknown> = {};

  switch (tool) {
    case 'activate':
    case 'seek':
    case 'scan':
    case 'missing':
    case 'resonate':
    case 'warmup':
      // First positional = query
      params = rest
        ? { query: rest, agent_id: 'gui', top_k: 30 }
        : { query: trimmed, agent_id: 'gui', top_k: 30 };
      break;

    case 'differential':
      // "diff A B"
      if (parts.length >= 3) {
        params = { node_a: parts[1], node_b: parts[2], agent_id: 'gui' };
      } else {
        params = { query: rest || trimmed, agent_id: 'gui' };
      }
      break;

    case 'impact':
    case 'predict':
    case 'counterfactual':
    case 'timeline':
      params = { node_id: rest || trimmed, agent_id: 'gui' };
      break;

    case 'why':
    case 'trace':
      if (parts.length >= 3) {
        params = { source: parts[1], target: parts[2], agent_id: 'gui' };
      } else {
        params = { query: rest || trimmed, agent_id: 'gui' };
      }
      break;

    case 'ingest':
      params = rest
        ? { path: rest, agent_id: 'gui', incremental: true }
        : { agent_id: 'gui', incremental: true };
      break;

    case 'learn':
      params = { feedback: rest || 'correct', agent_id: 'gui' };
      break;

    case 'health':
      params = { agent_id: 'gui' };
      break;

    default:
      params = rest ? { query: rest, agent_id: 'gui' } : { agent_id: 'gui' };
  }

  return { tool, params, raw: input };
}

/** Generate autocomplete suggestions for a partial input. */
export function suggestCommands(partial: string): string[] {
  const lower = partial.toLowerCase();
  return Object.keys(TOOL_ALIASES)
    .filter((k) => k.startsWith(lower))
    .slice(0, 8);
}
