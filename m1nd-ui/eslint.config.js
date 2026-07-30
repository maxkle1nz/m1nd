import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'

// Flat config (ESLint 10). Vite + React + TypeScript, syntax-only linting
// (no type-aware rules) to keep it fast and dependency-light.

// ESLint 10's peer range admits exactly one published eslint-plugin-react-hooks
// (7.1.1), and that major moved the flat configs under `.flat` AND folded the
// React Compiler rule family into `recommended` — 15 rules on top of the two
// this project has enforced since the config was written. Those 15 are not a
// dependency concern: they report 31 findings in existing source
// (set-state-in-effect x21, purity x5, refs x5) whose fixes change render
// behaviour of a bundle compiled into every m1nd-mcp binary. So this pins the
// enforced set to the pre-migration strength, stated positively rather than as
// a wall of disables. Adopting the compiler family is its own change, with its
// own proof — not a silent rider on a version bump.
const reactHooksAtProjectStrength = {
  plugins: { 'react-hooks': reactHooks },
  rules: {
    'react-hooks/rules-of-hooks': 'error',
    'react-hooks/exhaustive-deps': 'warn',
  },
}

export default tseslint.config(
  // Replaces .eslintignore — build output and deps are never linted.
  { ignores: ['dist', 'node_modules'] },
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      ...tseslint.configs.recommended,
      reactHooksAtProjectStrength,
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
