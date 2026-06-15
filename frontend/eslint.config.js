import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    rules: {
      // `any` is discouraged but not build-breaking (mostly `catch (err: any)`);
      // surface it as a warning rather than failing the build.
      '@typescript-eslint/no-explicit-any': 'warn',
      // react-hooks v6 RC ships strict advisory rules that flag many valid
      // patterns; keep them as warnings rather than build-breaking errors.
      'react-hooks/set-state-in-effect': 'warn',
      'react-hooks/purity': 'warn',
      'react-hooks/exhaustive-deps': 'warn',
    },
  },
  {
    // Test and end-to-end files have different norms: `any` for mocks/fixtures,
    // intentionally unused fixtures, and helper exports alongside components.
    files: ['**/*.test.{ts,tsx}', '**/*.spec.ts', 'src/test/**/*', 'e2e/**/*'],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unused-vars': 'off',
      'react-refresh/only-export-components': 'off',
      'no-useless-escape': 'off',
      'prefer-const': 'off',
    },
  },
])
