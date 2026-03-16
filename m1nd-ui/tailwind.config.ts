import type { Config } from 'tailwindcss';

const config: Config = {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        m1nd: {
          base: '#09090b',
          surface: '#0c0c10',
          elevated: '#1a1a2e',
          'border-subtle': '#1e1e2e',
          'border-medium': '#2a2a3a',
          'border-strong': '#3b3b5c',
          accent: '#a78bfa',
          indigo: '#6366f1',
          violet: '#7c3aed',
          emerald: '#059669',
          fire: '#ff6b35',
          teal: '#4ecdc4',
        },
      },
      fontFamily: {
        mono: ['JetBrains Mono', 'Fira Code', 'monospace'],
      },
    },
  },
  plugins: [],
};

export default config;
