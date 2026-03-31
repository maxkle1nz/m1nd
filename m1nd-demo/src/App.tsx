import type { CSSProperties } from 'react';
import { motion } from 'framer-motion';
import { COLORS, GLYPHS } from './lib/colors';

const heroMetrics = [
  { label: 'Context churn', value: '-47.05%', note: 'in the recorded corpus' },
  { label: 'False starts', value: '14 -> 0', note: 'on measured workflows' },
  { label: 'Guided handoffs', value: '39', note: 'recorded follow-throughs' },
  { label: 'Recovery loops', value: '12', note: 'successful recoveries' },
];

const capabilities = [
  {
    title: 'Find the authority before the model drifts',
    body: 'm1nd gets an agent to the file, symbol, or seam that actually matters before it burns time rediscovering structure through blind reads.',
    accent: COLORS.M,
  },
  {
    title: 'Preflight blast radius before the edit',
    body: 'impact surfaces real consumers and connected seams early, so the agent can narrow scope before it patches the wrong place.',
    accent: COLORS.one,
  },
  {
    title: 'Prepare narrower, safer changes',
    body: 'surgical_context_v2 and validate_plan turn connected edits into a scoped plan instead of a blind multi-file jump.',
    accent: COLORS.D,
  },
  {
    title: 'Work with connected context',
    body: 'm1nd can connect code with docs, RFCs, papers, articles, and memory so the agent does not reason from code alone.',
    accent: COLORS.N,
  },
];

const workflow = [
  { step: '01', title: 'Locate the cut', body: 'Start with trace, seek, impact, or trail_resume to get structure before the model disappears into repo reading.' },
  { step: '02', title: 'Check impact', body: 'Use impact and proof_state to confirm whether the seam still needs proof or is ready for edit prep.' },
  { step: '03', title: 'Plan the narrow change', body: 'Use surgical_context_v2 and validate_plan to pull connected context and expose blast radius before writing.' },
  { step: '04', title: 'Write with visibility', body: 'apply_batch executes with phases, progress, verification verdicts, and runtime-visible completion signals.' },
];

const useCases = [
  {
    title: 'Use m1nd when repo reading would be expensive',
    items: [
      'you need authority, not just string matches',
      'blast radius matters before the edit',
      'the change crosses multiple files or surfaces',
      'continuity matters across several agent steps',
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
              <SectionEyebrow>Map first, cut second</SectionEyebrow>
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
                Before the model finishes reading, m1nd has already found the cut.
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
                m1nd reduces token burn on structural work by finding authority, blast radius, and connected edit
                context before an agent disappears into read-search-drift loops.
              </p>
              <div className="hero-actions" style={{ display: 'flex', gap: 12, flexWrap: 'wrap', marginBottom: 26 }}>
                <a href="/wiki/" style={primaryButton}>
                  Read the docs
                </a>
                <a href="/wiki/benchmarks.html" style={secondaryButton}>
                  See measured benchmarks
                </a>
              </div>
              <div className="pill-row" style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
                {['authority discovery', 'blast-radius preflight', 'connected edit context', 'apply_batch'].map((pill, index) => (
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
                <div className="hero-visual-core-label">map lands first</div>
                <div className="hero-visual-core-value">m1nd</div>
              </div>
              <div className="hero-visual-node hero-node-trace">
                <span>trace</span>
                <strong>authority located</strong>
              </div>
              <div className="hero-visual-node hero-node-validate">
                <span>validate_plan</span>
                <strong>blast radius checked</strong>
              </div>
              <div className="hero-visual-node hero-node-batch">
                <span>apply_batch</span>
                <strong>narrow write + progress</strong>
              </div>
              <div className="hero-visual-node hero-node-resume">
                <span>trail_resume</span>
                <strong>continuity preserved</strong>
              </div>
              <div className="hero-visual-snippet">
                <div className="hero-visual-snippet-line">$ m1nd.trace("AuthError: stale session in middleware")</div>
                <div className="hero-visual-snippet-line">authority=middleware/session.py</div>
                <div className="hero-visual-snippet-line">blast_radius=3 consumers</div>
                <div className="hero-visual-snippet-line">next_suggested_tool=validate_plan</div>
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
              <SectionEyebrow>Measured benchmark truth</SectionEyebrow>
              <div style={{ color: COLORS.textMuted, fontSize: 14, lineHeight: 1.8 }}>
                The current corpus tracks where m1nd changes real workflow behavior: less context churn, fewer
                false starts, stronger handoff, and faster recovery on structural tasks.
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
          <SectionEyebrow>Use it honestly</SectionEyebrow>
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr',
              gap: 18,
              marginBottom: 30,
            }}
            className="use-grid"
          >
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

        <section style={{ marginBottom: 74 }}>
          <SectionEyebrow>Why it matters</SectionEyebrow>
          <div>
            <h2 style={sectionTitle}>Stop paying tokens to rediscover repo structure.</h2>
            <p style={{ ...sectionBody, maxWidth: 920 }}>
              Models read. m1nd locates. That difference shows up as lower spend, faster orientation, less
              wandering, and narrower edits before the model has burned half the budget reopening files.
            </p>
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
          <SectionEyebrow>How it works</SectionEyebrow>
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
              <h2 style={sectionTitle}>A workflow that lands before the drift begins</h2>
              <p style={sectionBody}>
                A grounded flow looks like <span style={{ color: COLORS.M }}>{'trace -> view -> surgical_context_v2 -> validate_plan -> apply_batch'}</span>.
              </p>
            </div>
            <div>
              {workflow.map((step) => (
                <WorkflowStep key={step.step} {...step} />
              ))}
            </div>
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
              m1nd is strongest when the task is structural, connected, stateful, or risky. That is where less spend,
              faster orientation, narrower cuts, and grounded continuity change how an agent operates.
            </p>
          </div>
          <div className="cta-actions" style={{ display: 'flex', gap: 12, justifyContent: 'flex-end', flexWrap: 'wrap' }}>
            <a href="https://crates.io/crates/m1nd-mcp" target="_blank" rel="noreferrer" style={primaryButton}>
              Install from crates.io
            </a>
            <a href="https://github.com/maxkle1nz/m1nd#readme" target="_blank" rel="noreferrer" style={secondaryButton}>
              Read README
            </a>
            <a href="/wiki/" style={secondaryButton}>
              Open wiki
            </a>
            <a href="/wiki/benchmarks.html" style={secondaryButton}>
              Benchmark corpus
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
