/*
 * packet — the PURE MissionPacket compositor (HUMAN-VIEW-V2 F12–F13, §7).
 *
 * `composePacket` turns a block (or block + sub-path) plus the operator's message
 * into the exact Markdown the clipboard receives. It is a pure function: no I/O, no
 * engine call, no side effect — the clipboard mode is a READ-ONLY compositor
 * (F0-TECH §9), so composing a packet writes NOTHING to the engine. `delegate`'s
 * registry write is spawn/direct behaviour (F2.5), never this path.
 *
 * Copy law (PRD §13): scoped, auditable claims only — never "proven/done/correct".
 * The packet carries repo-relative paths and declared sockets — no absolute paths,
 * no secrets. Agents propose; a human lands the change.
 */
import type { BlockRollup, MembershipRole, Socket, SystemBlock } from './buildMap';
import { STATE_LABEL, domainTag as toDomainTag, membershipByRole } from './buildMap';

/** What the packet includes (F12). All ON by default except `screenshot`, which is
 *  OFF by default — screenshot capture + redaction is a spawn concern (F0-TECH §9),
 *  so the clipboard packet is text-only in F2. */
export interface PacketToggles {
  blockDetails: boolean;
  likelyFiles: boolean;
  receipts: boolean;
  impact: boolean;
  screenshot: boolean;
}

export const DEFAULT_TOGGLES: PacketToggles = {
  blockDetails: true,
  likelyFiles: true,
  receipts: true,
  impact: true,
  screenshot: false,
};

export interface PacketInput {
  block: SystemBlock;
  rollup: BlockRollup;
  repoId: string | null;
  /** The operator's "what should change?" — verbatim, may be empty while drafting. */
  message: string;
  /** Optional sub-path scope (block + a member path). */
  subPath?: string | null;
  toggles: PacketToggles;
}

/** Human labels for membership roles (operator language). */
const ROLE_LABEL: Record<MembershipRole, string> = {
  primary: 'primary',
  shared: 'shared',
  generated: 'generated',
  test: 'tests',
  docs: 'docs',
  external_socket: 'external sockets',
};

/** The block's scope line: `CORE GRAPH · Core Graph Kernel[ ▸ <sub-path>]`. */
export function packetScope(block: SystemBlock, repoId: string | null, subPath?: string | null): string {
  const tag = toDomainTag(block.block_id, repoId);
  const base = `${tag} · ${block.name}`;
  return subPath ? `${base} ▸ ${subPath}` : base;
}

/** The likely-files section: the membership grouped by role, repo-relative paths. */
function likelyFilesSection(block: SystemBlock, subPath?: string | null): string {
  const roles = membershipByRole(block);
  const lines: string[] = [`## Likely files (${block.membership.length})`];
  if (subPath) lines.push(`focused on: \`${subPath}\``);
  for (const { role, count } of roles) {
    lines.push('', `**${ROLE_LABEL[role]} (${count})**`);
    for (const entry of block.membership) {
      if (entry.role === role) lines.push(`- \`${entry.path}\``);
    }
  }
  return lines.join('\n');
}

/** The receipts section: M/N required earned-fresh, each required type's state. */
function receiptsSection(block: SystemBlock, rollup: BlockRollup): string {
  const lines: string[] = [
    '## Receipts & evidence state',
    `${rollup.receiptsEarned}/${rollup.receiptsRequired} required receipt types earned-fresh (${STATE_LABEL[rollup.state]}).`,
  ];
  if (rollup.requiredTypes.length === 0) {
    lines.push('- no required contract declared — not scanned yet');
  } else {
    for (const type of rollup.requiredTypes) {
      const earned = rollup.earnedTypes.includes(type);
      lines.push(`- \`${type}\` — ${earned ? 'earned-fresh' : 'not earned yet'}`);
    }
  }
  const optional = block.receipt_contract.optional.map((o) => o.type);
  if (optional.length > 0) {
    lines.push(`_optional axes (never counted): ${optional.join(' · ')}_`);
  }
  return lines.join('\n');
}

/** One socket rendered as an arrow line (`→ <target> · <type>`). */
function socketLine(s: Socket): string {
  const target = s.to ?? s.alias ?? s.class ?? '—';
  const kind = s.type ?? s.class ?? '';
  return kind ? `- → ${target} · ${kind}` : `- → ${target}`;
}

/** The impact section: the block's DECLARED sockets (structural neighbours). No
 *  invented numbers — the honest note says these come from declared sockets. */
function impactSection(block: SystemBlock): string {
  const { inputs, outputs, external } = block.sockets;
  const lines: string[] = ['## Connected systems (declared sockets)'];
  if (inputs.length + outputs.length + external.length === 0) {
    lines.push('- no declared sockets — this block connects to nothing in the skeleton yet');
    return lines.join('\n');
  }
  if (outputs.length > 0) {
    lines.push('outputs:');
    for (const s of outputs) lines.push(socketLine(s));
  }
  if (inputs.length > 0) {
    lines.push('inputs:');
    for (const s of inputs) lines.push(socketLine(s));
  }
  if (external.length > 0) {
    lines.push('external:');
    for (const s of external) lines.push(socketLine(s));
  }
  lines.push('_structural neighbours from declared sockets — not a computed blast radius._');
  return lines.join('\n');
}

/** The block-details section: the internal identity, versions, purpose. */
function blockDetailsSection(block: SystemBlock): string {
  return [
    '## Block details',
    `- id: \`${block.block_id}\``,
    `- boundary v${block.boundary_version} · contract v${block.contract_version}`,
    `- membership source: ${block.membership_source}`,
    `- purpose: ${block.purpose}`,
  ].join('\n');
}

/**
 * Compose the MissionPacket Markdown for the clipboard (F12–F13). Pure — the exact
 * text `navigator.clipboard.writeText` receives, and the exact text the preview
 * shows. Sections are gated by the toggles; an OFF toggle drops its whole section.
 */
export function composePacket(input: PacketInput): string {
  const { block, rollup, repoId, message, subPath, toggles } = input;
  const scope = packetScope(block, repoId, subPath);
  const parts: string[] = [
    `# Mission packet — ${block.name}${subPath ? ` ▸ ${subPath}` : ''}`,
    `scope: ${scope}\nstate: ${STATE_LABEL[rollup.state]} (receipts ${rollup.receiptsEarned}/${rollup.receiptsRequired} required earned)`,
    `## What should change?\n${message.trim() || '_(no message yet)_'}`,
  ];
  if (toggles.likelyFiles) parts.push(likelyFilesSection(block, subPath));
  if (toggles.receipts) parts.push(receiptsSection(block, rollup));
  if (toggles.impact) parts.push(impactSection(block));
  if (toggles.blockDetails) parts.push(blockDetailsSection(block));
  parts.push(
    '---\n_Composed read-only from the m1nd Build Map — repo-relative paths, no secrets. Agents propose; a human lands the change._',
  );
  return parts.join('\n\n') + '\n';
}
