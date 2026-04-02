import { useEffect, useRef, useMemo } from "react";

export interface ScriptLine {
  text: string;
  startAt: number;
  type: "prompt" | "output" | "success" | "warning" | "dim" | "separator" | "blank" | "error";
}

interface TerminalPanelProps {
  title: string;
  subtitle: string;
  accent: string;
  script: ScriptLine[];
  completeAt: number;
  animationTime: number;
  isLeft?: boolean;
  displayTimeMax?: number;
}

function getLineColor(type: ScriptLine["type"]): string {
  switch (type) {
    case "prompt":   return "#e2e8f0";
    case "output":   return "#7a8faa";
    case "success":  return "#00ff88";
    case "warning":  return "#ffb700";
    case "error":    return "#ff00aa";
    case "dim":      return "#3a4a5c";
    default:         return "#7a8faa";
  }
}

function getLinePrefix(type: ScriptLine["type"]): string {
  switch (type) {
    case "prompt":  return "";
    default:        return "  ";
  }
}

export function TerminalPanel({
  title,
  subtitle,
  accent,
  script,
  completeAt,
  animationTime,
  isLeft = false,
  displayTimeMax,
}: TerminalPanelProps) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const isComplete = animationTime >= completeAt && animationTime > 0;
  const isStarted = animationTime > 200;

  const visibleLines = useMemo(
    () => script.filter((l) => l.startAt <= animationTime),
    [script, animationTime]
  );

  useEffect(() => {
    if (bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight;
    }
  }, [visibleLines.length]);

  let displayTime: string;
  if (!isStarted) {
    displayTime = "—";
  } else if (displayTimeMax != null) {
    if (isComplete) {
      displayTime = `${(displayTimeMax / 1000).toFixed(3)}s`;
    } else {
      const mapped = Math.round((animationTime / completeAt) * displayTimeMax);
      displayTime = `${Math.min(mapped, displayTimeMax)}ms`;
    }
  } else {
    if (isComplete) {
      displayTime = `${(completeAt / 1000).toFixed(2)}s`;
    } else {
      displayTime = `${(animationTime / 1000).toFixed(2)}s`;
    }
  }

  return (
    <div
      className="flex flex-col h-full rounded-xl overflow-hidden border"
      style={{ borderColor: `${accent}28`, background: "#05050f" }}
    >
      <div
        className="flex items-center gap-2 px-4 py-2.5 border-b flex-shrink-0"
        style={{ borderColor: `${accent}18`, background: "#080818" }}
      >
        <div className="flex gap-1.5">
          <div className="w-2.5 h-2.5 rounded-full bg-red-500/50" />
          <div className="w-2.5 h-2.5 rounded-full bg-yellow-500/50" />
          <div className="w-2.5 h-2.5 rounded-full bg-green-500/50" />
        </div>
        <div className="flex-1 text-center">
          <span className="text-xs font-mono" style={{ color: `${accent}88` }}>
            {title}
          </span>
        </div>
      </div>

      <div
        className="px-4 py-1.5 flex items-center justify-between border-b flex-shrink-0"
        style={{ borderColor: `${accent}12`, background: "#060614" }}
      >
        <span className="text-[10px] font-mono text-muted-foreground/40">{subtitle}</span>
        <div className="flex items-center gap-2">
          <span
            className="text-[10px] font-mono tabular-nums"
            style={{ color: isComplete && displayTimeMax != null ? "#00ff88" : "rgba(148,163,184,0.5)" }}
          >
            {displayTime}
          </span>
          {isStarted && (
            isComplete ? (
              <span
                className="text-[10px] font-mono font-bold px-2 py-0.5 rounded"
                style={{
                  background: "#00ff8814",
                  color: "#00ff88",
                  border: "1px solid #00ff8830",
                }}
              >
                DONE ✓
              </span>
            ) : (
              <span
                className="text-[10px] font-mono px-2 py-0.5 rounded animate-pulse"
                style={{
                  background: `${accent}12`,
                  color: accent,
                  border: `1px solid ${accent}28`,
                }}
              >
                {isLeft ? "STILL RUNNING..." : "RUNNING"}
              </span>
            )
          )}
        </div>
      </div>

      <div
        ref={bodyRef}
        className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden p-4 font-mono text-[11px] leading-5 space-y-0.5"
        style={{ scrollbarWidth: "none", wordBreak: "break-word", scrollBehavior: "smooth" }}
      >
        {visibleLines.map((line, i) => {
          if (line.type === "separator") {
            return (
              <div
                key={i}
                className="my-2 border-t"
                style={{ borderColor: `${accent}18` }}
              />
            );
          }
          if (line.type === "blank") {
            return <div key={i} className="h-1.5" />;
          }
          return (
            <div key={i} style={{ color: getLineColor(line.type) }}>
              {getLinePrefix(line.type)}
              {line.text}
            </div>
          );
        })}

        {isStarted && !isComplete && (
          <span
            className="inline-block w-1.5 h-3 ml-0.5"
            style={{
              background: accent,
              animation: "blink 1s step-end infinite",
            }}
          />
        )}
      </div>

      <style>{`@keyframes blink { 50% { opacity: 0; } }`}</style>
    </div>
  );
}
