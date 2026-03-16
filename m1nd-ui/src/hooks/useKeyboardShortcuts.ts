import { useEffect } from 'react';

export interface ShortcutHandlers {
  onCommandPalette?: () => void;  // Cmd+K / Ctrl+K
  onEscape?: () => void;          // Escape
  onClearGraph?: () => void;      // Ctrl+Backspace
  onIngest?: () => void;          // Ctrl+I
}

/**
 * Register global keyboard shortcuts.
 * Handlers are stable refs — safe to pass inline functions (updated each render).
 */
export function useKeyboardShortcuts(handlers: ShortcutHandlers) {
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const meta = e.metaKey || e.ctrlKey;

      if (meta && e.key === 'k') {
        e.preventDefault();
        handlers.onCommandPalette?.();
        return;
      }

      if (e.key === 'Escape') {
        handlers.onEscape?.();
        return;
      }

      if (meta && e.key === 'Backspace') {
        e.preventDefault();
        handlers.onClearGraph?.();
        return;
      }

      if (meta && e.key === 'i') {
        e.preventDefault();
        handlers.onIngest?.();
        return;
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  });
}
