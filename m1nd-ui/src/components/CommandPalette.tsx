import React, { useCallback, useEffect, useRef, useState } from 'react';
import type { ToolId } from '../types';
import { parseCommand, suggestCommands } from '../lib/commandParser';

interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  onExecute: (tool: ToolId, params: Record<string, unknown>) => void;
}

const TOOL_HINTS: Partial<Record<ToolId, string>> = {
  activate: 'activate <query>  — spreading activation query',
  seek: 'seek <query>  — semantic node search',
  impact: 'impact <node_id>  — blast radius of a change',
  missing: 'missing <query>  — find structural holes',
  why: 'why <node_a> <node_b>  — path between two nodes',
  trace: 'trace <node_a> <node_b>  — trace influence path',
  differential: 'diff <node_a> <node_b>  — compare two nodes',
  predict: 'predict <node_id>  — co-change prediction',
  counterfactual: 'cf <node_id>  — simulate removal',
  ingest: 'ingest <path>  — load codebase into graph',
  health: 'health  — server diagnostics',
  drift: 'drift  — what changed since baseline',
};

export default function CommandPalette({ isOpen, onClose, onExecute }: CommandPaletteProps) {
  const [input, setInput] = useState('');
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [selectedIdx, setSelectedIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus input when opened
  useEffect(() => {
    if (isOpen) {
      setInput('');
      setSuggestions([]);
      setSelectedIdx(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    setInput(val);
    setSelectedIdx(0);
    if (val.trim()) {
      setSuggestions(suggestCommands(val.split(' ')[0]));
    } else {
      setSuggestions([]);
    }
  }, []);

  const execute = useCallback((rawInput: string) => {
    const parsed = parseCommand(rawInput);
    onExecute(parsed.tool, parsed.params);
    onClose();
  }, [onExecute, onClose]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      onClose();
      return;
    }
    if (e.key === 'Enter') {
      if (suggestions.length > 0 && selectedIdx < suggestions.length) {
        const parts = input.split(' ');
        const rest = parts.slice(1).join(' ');
        const completed = rest ? `${suggestions[selectedIdx]} ${rest}` : suggestions[selectedIdx];
        execute(completed);
      } else {
        execute(input);
      }
      return;
    }
    if (e.key === 'Tab' && suggestions.length > 0) {
      e.preventDefault();
      const parts = input.split(' ');
      const rest = parts.slice(1).join(' ');
      const completed = rest ? `${suggestions[selectedIdx]} ${rest}` : `${suggestions[selectedIdx]} `;
      setInput(completed);
      setSuggestions([]);
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIdx((i) => Math.min(i + 1, suggestions.length - 1));
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIdx((i) => Math.max(i - 1, 0));
    }
  }, [input, suggestions, selectedIdx, execute, onClose]);

  if (!isOpen) return null;

  const parsedPreview = input.trim() ? parseCommand(input) : null;
  const hintText = parsedPreview ? TOOL_HINTS[parsedPreview.tool] : null;

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-black/60 z-40"
        onClick={onClose}
        aria-hidden
      />

      {/* Palette */}
      <div className="fixed top-1/4 left-1/2 -translate-x-1/2 z-50 w-full max-w-xl">
        <div className="mx-4 bg-m1nd-surface border border-m1nd-border-strong rounded-lg shadow-2xl overflow-hidden">
          {/* Input */}
          <div className="flex items-center gap-3 px-4 py-3 border-b border-m1nd-border-subtle">
            <span className="text-m1nd-accent text-sm">◈</span>
            <input
              ref={inputRef}
              type="text"
              value={input}
              onChange={handleChange}
              onKeyDown={handleKeyDown}
              placeholder="activate http_server  |  impact session.rs  |  why A B"
              className="flex-1 bg-transparent text-slate-200 text-sm font-mono placeholder-slate-700 outline-none"
              spellCheck={false}
              autoComplete="off"
            />
            <kbd className="text-[10px] text-slate-600 border border-m1nd-border-medium rounded px-1">esc</kbd>
          </div>

          {/* Suggestions */}
          {suggestions.length > 0 && (
            <div className="border-b border-m1nd-border-subtle">
              {suggestions.map((s, i) => (
                <button
                  key={s}
                  className={`w-full text-left px-4 py-2 text-sm font-mono flex items-center justify-between transition-colors ${
                    i === selectedIdx
                      ? 'bg-m1nd-elevated text-m1nd-accent'
                      : 'text-slate-400 hover:bg-m1nd-elevated hover:text-slate-200'
                  }`}
                  onClick={() => {
                    const parts = input.split(' ');
                    const rest = parts.slice(1).join(' ');
                    execute(rest ? `${s} ${rest}` : s);
                  }}
                >
                  <span>{s}</span>
                  {TOOL_HINTS[s as ToolId] && (
                    <span className="text-[11px] text-slate-600 ml-4 truncate">
                      {TOOL_HINTS[s as ToolId]?.split('—')[1]?.trim()}
                    </span>
                  )}
                </button>
              ))}
            </div>
          )}

          {/* Hint / preview */}
          {hintText && suggestions.length === 0 && (
            <div className="px-4 py-2 text-[11px] text-slate-600 font-mono">
              {hintText}
            </div>
          )}

          {/* Quick actions when empty */}
          {!input && (
            <div className="px-4 py-3">
              <div className="text-[10px] text-slate-700 uppercase tracking-wide mb-2">Quick actions</div>
              <div className="grid grid-cols-2 gap-1">
                {(['activate', 'seek', 'impact', 'missing', 'drift', 'health'] as ToolId[]).map((tool) => (
                  <button
                    key={tool}
                    className="text-left px-2 py-1.5 text-xs text-slate-500 hover:text-slate-300 hover:bg-m1nd-elevated rounded transition-colors font-mono"
                    onClick={() => setInput(`${tool} `)}
                  >
                    {tool}
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </>
  );
}
