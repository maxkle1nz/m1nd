import { create } from 'zustand';

export interface Toast {
  id: string;
  message: string;
  detail?: string;
  type: 'info' | 'success' | 'learn';
  timestamp: number;
}

interface ToastStore {
  toasts: Toast[];
  addToast: (message: string, detail?: string, type?: Toast['type']) => void;
  removeToast: (id: string) => void;
}

let toastCounter = 0;

export const useToastStore = create<ToastStore>((set, get) => ({
  toasts: [],

  addToast: (message, detail, type = 'info') => {
    const id = `toast-${++toastCounter}`;
    const toast: Toast = { id, message, detail, type, timestamp: Date.now() };

    set((s) => ({ toasts: [...s.toasts, toast].slice(-5) })); // Max 5 visible

    // Auto-dismiss after 4 seconds
    setTimeout(() => {
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    }, 4000);
  },

  removeToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));
