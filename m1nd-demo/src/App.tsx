import type { CSSProperties } from 'react';
import { motion } from 'framer-motion';
import { COLORS, GLYPHS } from './lib/colors';

const heroMetrics = [
  { label: 'Warm-graph corpus', value: '50.73%', note: 'less context churn' },
  { label: 'False starts', value: '14 -> 0', note: 'in the recorded corpus' },
  { label: 'Guided follow-throughs', value: '31', note: 'measured workflows' },
  { label: 'Recovery loops', value: '12', note: 'successful guided recoveries' },
];

const capabilities = [
  {
    title: 'Trace failures into the next file',
    body: 'trace does not stop at ranking suspects. It can expose proof_state and hand off the next file worth opening.',
    accent: COLORS.M,
  },
  {
    title: 'Inspect blast radius with state',
    body: 'impact shows affected nodes and whether the seam is still being triaged, actively proven, or ready for edit prep.',
    accent: COLORS.one,
  },
  {
    title: 'Resume work without rediscovery',
    body: 'trail_resume restores the investigation with resume_hints, next_focus_node_id, next_open_question, and the next likely tool.',
    accent: COLORS.N,
  },
  {
    title: 'Prepare safer connected edits',
    body: 'surgical_context_v2 and validate_plan turn connected changes into a guided workflow instead of a blind multi-file jump.',
    accent: COLORS.D,
  },
  {
    title: 'Write with live progress',
    body: 'apply_batch now surfaces phases, progress, SSE events, and follow-up guidance so long-running writes stay understandable.',
    accent: COLORS.M,
  },
  {
    title: 'Recover when the agent gets it wrong',
    body: 'Invalid regex, ambiguous scope, stale route, stale trail, and protected-write failures now teach the next valid move.',
    accent: COLORS.one,
  },
];

const workflow = [
  { step: '01', title: 'Ground the task', body: 'Start with trace, seek, impact, or trail_resume to get structure instead of raw-text drift.' },
  { step: '02', title: 'Read proof state', body: 'Use proof_state to tell whether the agent is still triaging, already proving, or ready to move into edit prep.' },
  { step: '03', title: 'Follow the handoff', body: 'next_suggested_tool, next_suggested_target, and next_step_hint reduce hesitation and retry loops.' },
  { step: '04', title: 'Prepare the edit', body: 'Use surgical_context_v2 and validate_plan to pull connected context and expose risky seams before writing.' },
  { step: '05', title: 'Write and verify', body: 'apply_batch executes with phases, progress, verification verdicts, and runtime-visible completion signals.' },
];

const truths = [
  'm1nd is local-first. It does not need to ship your code to an API to ground navigation.',
  'm1nd is MCP-native. It is built to help agents choose and sequence the next move.',
  'm1nd is not just retrieval. It exposes proof state, continuity, recovery, and execution progress.',
  'm1nd is not for every lookup. Plain tools still win for simple grep, logs, tests, and compiler truth.',
];

const useCases = [
  {
    title: 'Use m1nd when the task is structural',
    items: [
      'ranked retrieval beats raw text hits',
      'blast radius matters',
      'the edit crosses multiple files',
      'you need continuity across steps',
    ],
  },
  {
    title: 'Use plain tools when the task is direct',
    items: [
      'one-file lookups',
      'simple text search',
      'compiler truth and test output',
      'logs and runtime inspection',
    ],
  },
];

function Wordmark() {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 2, fontWeight: 900, letterSpacing: 0 }}>
      <span style={{ color: COLORS.M }}>m</span>
      <span style={{ color: COLORS.one }}>1</span>
      <span style={{ color: COLORS.N }}>n</span>
      <span style={{ color: COLORS.D }}>d</span>
    </div>
  );
}

function SectionEyebrow({ children }: { children: string }) {
  return (
    <div
      style={{
        fontSize: 11,
        letterSpacing: 3,
        textTransform: 'uppercase',
        color: COLORS.textMuted,
        marginBottom: 14,
      }}
    >
      {children}
    </div>
  );
}

function MetricCard({ label, value, note }: { label: string; value: string; note: string }) {
  return (
    <div
      style={{
        background: 'linear-gradient(180deg, rgba(20,27,45,0.82), rgba(15,20,32,0.74))',
        border: `1px solid ${COLORS.border}`,
        borderRadius: 18,
        padding: '22px 20px',
        minHeight: 148,
        boxShadow: '0 20px 60px rgba(0,0,0,0.22)',
      }}
    >
      <div style={{ fontSize: 12, color: COLORS.textMuted, letterSpacing: 2, textTransform: 'uppercase', marginBottom: 16 }}>
        {label}
      </div>
      <div style={{ fontSize: 34, lineHeight: 1.05, color: COLORS.text, fontWeight: 800, marginBottom: 10 }}>
        {value}
      </div>
      <div style={{ fontSize: 13, color: COLORS.textMuted, lineHeight: 1.6 }}>{note}</div>
    </div>
  );
}

function CapabilityCard({ title, body, accent }: { title: string; body: string; accent: string }) {
  return (
    <div
      style={{
        position: 'relative',
        background: 'rgba(20,27,45,0.72)',
        border: `1px solid ${accent}25`,
        borderRadius: 20,
        padding: '24px 22px 22px',
        minHeight: 220,
        overflow: 'hidden',
      }}
    >
      <div
        style={{
          position: 'absolute',
          inset: 0,
          background: `radial-gradient(circle at top right, ${accent}20, transparent 45%)`,
          pointerEvents: 'none',
        }}
      />
      <div style={{ position: 'relative' }}>
        <div style={{ color: accent, fontSize: 12, letterSpacing: 2, textTransform: 'uppercase', marginBottom: 14 }}>
          {GLYPHS.structure} Product truth
        </div>
        <div style={{ color: COLORS.text, fontSize: 22, lineHeight: 1.22, fontWeight: 700, marginBottom: 14 }}>
          {title}
        </div>
        <div style={{ color: COLORS.textMuted, fontSize: 14, lineHeight: 1.75 }}>{body}</div>
      </div>
    </div>
  );
}

function WorkflowStep({ step, title, body }: { step: string; title: string; body: string }) {
  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: '88px 1fr',
        gap: 18,
        alignItems: 'start',
        padding: '22px 0',
        borderTop: `1px solid rgba(107,127,163,0.18)`,
      }}
    >
      <div style={{ color: COLORS.M, fontSize: 14, letterSpacing: 2, textTransform: 'uppercase' }}>{step}</div>
      <div>
        <div style={{ color: COLORS.text, fontSize: 22, fontWeight: 700, marginBottom: 10 }}>{title}</div>
        <div style={{ color: COLORS.textMuted, fontSize: 15, lineHeight: 1.8 }}>{body}</div>
      </div>
    </div>
  );
}

function TruthList() {
  return (
    <div
      style={{
        display: 'grid',
        gap: 14,
      }}
    >
      {truths.map((truth) => (
        <div
          key={truth}
          style={{
            display: 'flex',
            gap: 12,
            alignItems: 'flex-start',
            padding: '16px 18px',
            background: 'rgba(15,20,32,0.7)',
            border: `1px solid rgba(107,127,163,0.16)`,
            borderRadius: 16,
          }}
        >
          <div style={{ color: COLORS.D, marginTop: 2 }}>{GLYPHS.convergence}</div>
          <div style={{ color: COLORS.textMuted, fontSize: 14, lineHeight: 1.75 }}>{truth}</div>
        </div>
      ))}
    </div>
  );
}

export default function App() {
  return (
    <div
      className="landing-shell"
      style={{
        minHeight: '100vh',
        background: `
          radial-gradient(circle at top left, rgba(0,212,255,0.15), transparent 26%),
          radial-gradient(circle at top right, rgba(255,0,170,0.12), transparent 28%),
          linear-gradient(180deg, #070B12 0%, #090F19 38%, #0B1220 100%)
        `,
        color: COLORS.text,
      }}
    >
      <div
        className="landing-container"
        style={{
          maxWidth: 1220,
          margin: '0 auto',
          padding: '28px 28px 80px',
        }}
      >
        <motion.header
          className="landing-header"
          initial={{ opacity: 0, y: -18 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5 }}
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            padding: '8px 0 34px',
          }}
        >
          <div className="landing-brand" style={{ display: 'flex', alignItems: 'center', gap: 14, fontSize: 24 }}>
            <Wordmark />
            <span style={{ color: COLORS.textMuted, fontSize: 12, letterSpacing: 2, textTransform: 'uppercase' }}>
              Guided runtime for MCP agents
            </span>
          </div>
          <div className="header-actions" style={{ display: 'flex', gap: 12, flexWrap: 'wrap' }}>
            <a href="https://github.com/maxkle1nz/m1nd" target="_blank" rel="noreferrer" style={secondaryButton}>
              GitHub
            </a>
            <a href="https://crates.io/crates/m1nd-mcp" target="_blank" rel="noreferrer" style={primaryButton}>
              Install m1nd-mcp
            </a>
          </div>
        </motion.header>

        <section
          className="hero-grid"
          style={{
            display: 'grid',
            gridTemplateColumns: '1.2fr 0.8fr',
            gap: 28,
            alignItems: 'stretch',
            marginBottom: 68,
          }}
        >
          <motion.div
            className="hero-panel"
            initial={{ opacity: 0, y: 18 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.55, delay: 0.05 }}
            style={{
              background: 'linear-gradient(180deg, rgba(20,27,45,0.76), rgba(15,20,32,0.64))',
              border: `1px solid rgba(0,212,255,0.22)`,
              borderRadius: 28,
              padding: '34px 32px 30px',
              boxShadow: '0 30px 80px rgba(0,0,0,0.34)',
              position: 'relative',
              overflow: 'hidden',
            }}
          >
            <div
              style={{
                position: 'absolute',
                inset: 0,
                background: `
                  radial-gradient(circle at 12% 18%, rgba(0,212,255,0.22), transparent 28%),
                  radial-gradient(circle at 92% 16%, rgba(255,0,170,0.18), transparent 24%),
                  radial-gradient(circle at 85% 82%, rgba(0,255,136,0.12), transparent 22%)
                `,
                pointerEvents: 'none',
              }}
            />
            <div style={{ position: 'relative' }}>
              <SectionEyebrow>Local-first agent infrastructure</SectionEyebrow>
              <h1
                style={{
                  fontSize: 'clamp(40px, 7vw, 78px)',
                  lineHeight: 0.96,
                  letterSpacing: -2.4,
                  fontWeight: 800,
                  maxWidth: 760,
                  marginBottom: 20,
                }}
              >
                A local code graph engine for MCP agents.
              </h1>
              <p
                style={{
                  fontSize: 18,
                  color: COLORS.textMuted,
                  lineHeight: 1.85,
                  maxWidth: 760,
                  marginBottom: 28,
                }}
              >
                m1nd helps agents trace failures, inspect impact, resume investigations, prepare safer edits,
                and recover from mistakes with less context churn. It does not just return results. It exposes
                proof state, next-step guidance, and execution progress.
              </p>
              <div className="hero-actions" style={{ display: 'flex', gap: 12, flexWrap: 'wrap', marginBottom: 26 }}>
                <a href="https://github.com/maxkle1nz/m1nd#readme" target="_blank" rel="noreferrer" style={primaryButton}>
                  Read the docs
                </a>
                <a href="https://github.com/maxkle1nz/m1nd/tree/main/docs/benchmarks" target="_blank" rel="noreferrer" style={secondaryButton}>
                  See benchmark truth
                </a>
              </div>
              <div className="pill-row" style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
                {['proof_state', 'next-step guidance', 'trail_resume', 'apply_batch progress', 'recovery loops', 'SSE handoff'].map((pill, index) => (
                  <div
                    key={pill}
                    style={{
                      padding: '10px 14px',
                      borderRadius: 999,
                      fontSize: 12,
                      letterSpacing: 1.4,
                      textTransform: 'uppercase',
                      color: [COLORS.M, COLORS.one, COLORS.N, COLORS.D][index % 4],
                      border: `1px solid ${[COLORS.M, COLORS.one, COLORS.N, COLORS.D][index % 4]}30`,
                      background: 'rgba(7,11,18,0.45)',
                    }}
                  >
                    {pill}
                  </div>
                ))}
              </div>
            </div>
          </motion.div>

          <motion.aside
            className="hero-side"
            initial={{ opacity: 0, y: 18 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.55, delay: 0.15 }}
            style={{
              display: 'grid',
              gap: 16,
            }}
          >
            <div className="hero-visual">
              <div className="hero-visual-grid" />
              <div className="hero-visual-orbit hero-visual-orbit-a" />
              <div className="hero-visual-orbit hero-visual-orbit-b" />
              <div className="hero-visual-orbit hero-visual-orbit-c" />
              <div className="hero-visual-core">
                <div className="hero-visual-core-label">runtime</div>
                <div className="hero-visual-core-value">m1nd</div>
              </div>
              <div className="hero-visual-node hero-node-trace">
                <span>trace</span>
                <strong>proof_state=triaging</strong>
              </div>
              <div className="hero-visual-node hero-node-validate">
                <span>validate_plan</span>
                <strong>next=heuristics_surface</strong>
              </div>
              <div className="hero-visual-node hero-node-batch">
                <span>apply_batch</span>
                <strong>progress + handoff</strong>
              </div>
              <div className="hero-visual-node hero-node-resume">
                <span>trail_resume</span>
                <strong>next focus + next tool</strong>
              </div>
            </div>
            <div
              style={{
                background: 'linear-gradient(180deg, rgba(11,18,32,0.92), rgba(20,27,45,0.78))',
                border: `1px solid rgba(0,212,255,0.18)`,
                borderRadius: 24,
                padding: 22,
              }}
            >
              <SectionEyebrow>Current benchmark truth</SectionEyebrow>
              <div style={{ color: COLORS.textMuted, fontSize: 14, lineHeight: 1.8 }}>
                The recorded warm-graph corpus now measures not only token proxy, but also guidance,
                recovery, false starts, and workflow follow-through.
              </div>
            </div>
            <div className="metrics-grid" style={{ display: 'grid', gap: 14, gridTemplateColumns: '1fr 1fr' }}>
              {heroMetrics.map((metric) => (
                <MetricCard key={metric.label} {...metric} />
              ))}
            </div>
          </motion.aside>
        </section>

        <section style={{ marginBottom: 74 }}>
          <SectionEyebrow>Why it matters</SectionEyebrow>
          <div className="split-grid" style={{ display: 'grid', gridTemplateColumns: '0.92fr 1.08fr', gap: 28, alignItems: 'start' }}>
            <div>
              <h2 style={sectionTitle}>Without structure, agents keep rediscovering the repo.</h2>
              <p style={sectionBody}>
                Raw text search can find strings, but it does not tell an agent whether it is still triaging,
                actively proving, or already safe to move into edit prep. The real product win is less hesitation,
                less re-reading, and less time spent reconstructing state from scratch.
              </p>
            </div>
            <TruthList />
          </div>
        </section>

        <section style={{ marginBottom: 74 }}>
          <SectionEyebrow>What m1nd actually changes</SectionEyebrow>
          <div className="capability-grid" style={{ display: 'grid', gap: 18, gridTemplateColumns: 'repeat(3, minmax(0, 1fr))' }}>
            {capabilities.map((capability) => (
              <CapabilityCard key={capability.title} {...capability} />
            ))}
          </div>
        </section>

        <section style={{ marginBottom: 74 }}>
          <SectionEyebrow>Guided workflow</SectionEyebrow>
          <div
            className="workflow-grid"
            style={{
              display: 'grid',
              gridTemplateColumns: '0.9fr 1.1fr',
              gap: 34,
              alignItems: 'start',
              background: 'linear-gradient(180deg, rgba(20,27,45,0.72), rgba(15,20,32,0.62))',
              border: `1px solid rgba(0,212,255,0.18)`,
              borderRadius: 28,
              padding: '30px 28px',
            }}
          >
            <div>
              <h2 style={sectionTitle}>A guided agent workflow</h2>
              <p style={sectionBody}>
                The product is strongest when it changes the sequence of work, not just the answer format.
                A grounded flow now looks like <span style={{ color: COLORS.M }}>{'trace -> view -> surgical_context_v2 -> validate_plan -> apply_batch'}</span>.
              </p>
            </div>
            <div>
              {workflow.map((step) => (
                <WorkflowStep key={step.step} {...step} />
              ))}
            </div>
          </div>
        </section>

        <section style={{ marginBottom: 74 }}>
          <SectionEyebrow>Use it honestly</SectionEyebrow>
          <div className="use-grid" style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 18 }}>
            {useCases.map((group, groupIndex) => (
              <div
                key={group.title}
                style={{
                  background: 'rgba(15,20,32,0.74)',
                  border: `1px solid ${groupIndex === 0 ? `${COLORS.D}22` : 'rgba(107,127,163,0.16)'}`,
                  borderRadius: 22,
                  padding: '24px 22px',
                }}
              >
                <div style={{ fontSize: 22, color: COLORS.text, fontWeight: 700, marginBottom: 14 }}>{group.title}</div>
                <div style={{ display: 'grid', gap: 10 }}>
                  {group.items.map((item) => (
                    <div key={item} style={{ display: 'flex', gap: 10, alignItems: 'flex-start' }}>
                      <span style={{ color: groupIndex === 0 ? COLORS.D : COLORS.textDim }}>{groupIndex === 0 ? GLYPHS.activate : GLYPHS.edge}</span>
                      <span style={{ color: COLORS.textMuted, fontSize: 14, lineHeight: 1.7 }}>{item}</span>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </section>

        <section
          className="final-cta-grid"
          style={{
            background: 'linear-gradient(135deg, rgba(0,212,255,0.1), rgba(255,0,170,0.08), rgba(0,255,136,0.08))',
            border: `1px solid rgba(0,212,255,0.2)`,
            borderRadius: 30,
            padding: '34px 30px',
            display: 'grid',
            gridTemplateColumns: '1.1fr 0.9fr',
            gap: 28,
            alignItems: 'center',
          }}
        >
          <div>
            <SectionEyebrow>v0.6.1</SectionEyebrow>
            <h2 style={sectionTitle}>Ship the runtime your agents can actually work with.</h2>
            <p style={sectionBody}>
              m1nd is strongest when the work is structural, connected, stateful, or risky. That is where proof state,
              handoff, continuity, recovery, and execution visibility change how an agent operates.
            </p>
          </div>
          <div className="cta-actions" style={{ display: 'flex', gap: 12, justifyContent: 'flex-end', flexWrap: 'wrap' }}>
            <a href="https://crates.io/crates/m1nd-mcp" target="_blank" rel="noreferrer" style={primaryButton}>
              Install from crates.io
            </a>
            <a href="https://github.com/maxkle1nz/m1nd/releases/tag/v0.6.1" target="_blank" rel="noreferrer" style={secondaryButton}>
              View release
            </a>
          </div>
        </section>
      </div>
    </div>
  );
}

const primaryButton: CSSProperties = {
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  minHeight: 46,
  padding: '0 18px',
  borderRadius: 999,
  border: '1px solid rgba(0,212,255,0.28)',
  background: 'linear-gradient(135deg, rgba(0,212,255,0.18), rgba(0,255,136,0.14))',
  color: COLORS.text,
  textDecoration: 'none',
  fontSize: 13,
  letterSpacing: 1.2,
  textTransform: 'uppercase',
};

const secondaryButton: CSSProperties = {
  ...primaryButton,
  background: 'rgba(15,20,32,0.75)',
  border: '1px solid rgba(107,127,163,0.2)',
};

const sectionTitle: CSSProperties = {
  fontSize: 'clamp(30px, 4vw, 52px)',
  lineHeight: 1.02,
  letterSpacing: -1.5,
  fontWeight: 780,
  marginBottom: 18,
};

const sectionBody: CSSProperties = {
  color: COLORS.textMuted,
  fontSize: 16,
  lineHeight: 1.85,
};
