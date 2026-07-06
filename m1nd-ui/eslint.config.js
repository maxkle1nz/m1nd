import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'

// Flat config (ESLint 9). Vite + React + TypeScript, syntax-only linting
// (no type-aware rules) to keep it fast and dependency-light.
export default tseslint.config(
  // Replaces .eslintignore — build output and deps are never linted.
  { ignores: ['dist', 'node_modules'] },
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      ...tseslint.configs.recommended,
      reactHooks.configs['recommended-latest'],
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    rules: {
      // Fast Refresh is a dev-only concern; a few modules here legitimately
      // co-locate a helper/registry with components (e.g. lib/icons/registry.tsx).
      // Keep it as a signal, not a lint failure.
      'react-refresh/only-export-components': 'warn',
    },
  },
)
