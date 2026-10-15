import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';

// Minimal, high-signal lint gate. rules-of-hooks is the rule that prevents
// "blank screen" crashes: hooks must never run conditionally (after an early
// return, inside a condition/loop), or React unmounts the whole tree at
// runtime with no visible error.
export default tseslint.config(
  { ignores: ['dist/', 'src-tauri/', 'node_modules/'] },
  ...tseslint.configs.recommended,
  {
    files: ['**/*.{ts,tsx}'],
    plugins: { 'react-hooks': reactHooks },
    rules: {
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',
    },
  },
);
