import React from 'react';
import { useToastStore } from '../stores/toastStore';

const TYPE_STYLES = {
  info: 'border-m1nd-accent/50 text-m1nd-accent',
  success: 'border-emerald-500/50 text-emerald-400',
  learn: 'border-violet-500/50 text-violet-400',
} as const;

const TYPE_ICONS = {
  info: '\u25C8',   // diamond
  success: '\u2713', // checkmark
  learn: '\u2731',   // heavy asterisk
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
          className={`pointer-events-auto bg-m1nd-surface/95 backdrop-blur-sm border rounded-lg px-3 py-2 shadow-xl text-xs font-mono max-w-[320px] animate-slide-in ${TYPE_STYLES[toast.type]}`}
          onClick={() => removeToast(toast.id)}
          role="status"
        >
          <div className="flex items-center gap-2">
            <span className="text-sm">{TYPE_ICONS[toast.type]}</span>
            <span className="text-slate-200">{toast.message}</span>
          </div>
          {toast.detail && (
            <div className="text-[10px] text-slate-500 mt-0.5 ml-5 truncate" title={toast.detail}>
              {toast.detail}
            </div>
          )}
        </div>
      ))}
    </div>
  );
});

export default ToastContainer;
