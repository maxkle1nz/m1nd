/**
 * M1ndWordmark
 *
 * Visual identity based on the m1nd wiki color system:
 *   ⍌ Signal    →  #00f5ff  (cyan  — activation, flow)
 *   ⍐ Path      →  #7b61ff  (violet — tracing, search)
 *   ⍂ Structure →  #00ff88  (green  — gaps, shape)
 *   𝔻 Dimension →  #ffb700  (amber  — blast radius, prediction)
 *   ⟁ Connection →  #ff00aa (pink   — federation, resonance)
 *
 * The "1" in m1nd is the brand pivot: it is highlighted with the
 * gradient from ⍌ (cyan) → ⍂ (green) because m1nd's core promise
 * is graph activation (Signal) that exposes Structure.
 *
 * The remaining letters stay in clean white so the "1" pops.
 */

interface M1ndWordmarkProps {
  /** Base font size in rem. Default 1.5 (text-2xl). */
  size?: number;
  /** Whether to show the wiki symbol row below. Default false. */
  withGlyphs?: boolean;
  /** CSS class for the outer wrapper. */
  className?: string;
}

const GRADIENT_ID = "m1nd-logo-grad";

export function M1ndWordmark({
  size = 1.5,
  withGlyphs = false,
  className = "",
}: M1ndWordmarkProps) {
  return (
    <span className={`inline-flex flex-col items-start leading-none select-none ${className}`}>
      <span
        style={{
          fontFamily: "Space Grotesk, sans-serif",
          fontSize: `${size}rem`,
          fontWeight: 700,
          letterSpacing: "-0.045em",
          lineHeight: 1,
          display: "inline-flex",
          alignItems: "baseline",
        }}
      >
        <span style={{ color: "#e2e8f0" }}>m</span>
        <span
          style={{
            background: "linear-gradient(135deg, #00f5ff 0%, #00ff88 100%)",
            WebkitBackgroundClip: "text",
            WebkitTextFillColor: "transparent",
            backgroundClip: "text",
            filter: "drop-shadow(0 0 6px rgba(0,245,255,0.55))",
            fontWeight: 800,
          }}
        >
          1
        </span>
        <span style={{ color: "#e2e8f0" }}>nd</span>
      </span>

      {withGlyphs && (
        <span
          style={{
            fontFamily: "Space Mono, monospace",
            fontSize: `${size * 0.38}rem`,
            letterSpacing: "0.06em",
            lineHeight: 1,
            marginTop: 3,
            display: "inline-flex",
            gap: `${size * 0.22}rem`,
          }}
        >
          <GlyphDot char="⍌" color="#00f5ff" title="Signal" />
          <GlyphDot char="⍐" color="#7b61ff" title="Path" />
          <GlyphDot char="⍂" color="#00ff88" title="Structure" />
          <GlyphDot char="𝔻" color="#ffb700" title="Dimension" />
          <GlyphDot char="⟁" color="#ff00aa" title="Connection" />
        </span>
      )}
    </span>
  );
}

function GlyphDot({ char, color, title }: { char: string; color: string; title: string }) {
  return (
    <span
      title={title}
      style={{
        color,
        opacity: 0.65,
        transition: "opacity 0.2s",
        cursor: "default",
      }}
      onMouseEnter={e => ((e.target as HTMLElement).style.opacity = "1")}
      onMouseLeave={e => ((e.target as HTMLElement).style.opacity = "0.65")}
    >
      {char}
    </span>
  );
}

/**
 * Large display wordmark for hero / footer CTAs.
 * Renders "m1nd" with a bolder gradient + stronger glow, as inline text
 * so it can be placed inside a heading.
 */
export function M1ndInline({ glow = false }: { glow?: boolean }) {
  return (
    <>
      <span style={{ color: "inherit" }}>m</span>
      <span
        style={{
          background: "linear-gradient(135deg, #00f5ff 0%, #00ff88 100%)",
          WebkitBackgroundClip: "text",
          WebkitTextFillColor: "transparent",
          backgroundClip: "text",
          filter: glow ? "drop-shadow(0 0 12px rgba(0,245,255,0.5))" : undefined,
        }}
      >
        1
      </span>
      <span style={{ color: "inherit" }}>nd</span>
    </>
  );
}
