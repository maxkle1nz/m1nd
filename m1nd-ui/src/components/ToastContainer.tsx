import React from 'react';
import { useToastStore } from '../stores/toastStore';

// SOFT PROOF skin — quiet bone cards, verdict-toned accents (no neon, no glow).
const TYPE_STYLES = {
  info: 'border-ink/15 text-ink-soft',
  success: 'border-verdict-act/50 text-ink',
  learn: 'border-verdict-reverify/50 text-ink',
} as const;

const TYPE_ICONS = {
  info: '◈', // diamond
  success: '✓', // checkmark
  learn: '✱', // heavy asterisk
} as const;

const ToastContainer = React.memo(function ToastContainer() {
  const toasts = useToastStore((s) => s.toasts);
  const removeToast = useToastStore((s) => s.removeToast);

  if (toasts.length === 0) return null;

  return (
    <div className="fixed top-14 right-4 z-50 flex flex-col gap-2 pointer-events-none">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`pointer-events-auto bg-bone/95 backdrop-blur-sm border rounded-lg px-3 py-2 shadow-card text-xs font-sans max-w-[320px] animate-soft-slide-in ${TYPE_STYLES[toast.type]}`}
          onClick={() => removeToast(toast.id)}
          role="status"
        >
          <div className="flex items-center gap-2">
            <span className="text-sm">{TYPE_ICONS[toast.type]}</span>
            <span className="text-ink">{toast.message}</span>
          </div>
          {toast.detail && (
            <div className="text-[10px] text-ink-soft mt-0.5 ml-5 truncate" title={toast.detail}>
              {toast.detail}
            </div>
          )}
        </div>
      ))}
    </div>
  );
});

export default ToastContainer;
