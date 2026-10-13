## Summary of Changes
Provide an objective explanation of the changes introduced in this pull request and the problems they solve.

## Type of Change
- [ ] Bug fix (non-breaking change fixing an issue)
- [ ] New feature (non-breaking change adding functionality)
- [ ] Refactor / Performance enhancement (code improvement without behavior regression)
- [ ] Documentation update
- [ ] Testing & CI automation

## Verification and Testing
Describe tests performed to ensure functionality and prevent regressions.

- [ ] `cargo test` passes locally without failures.
- [ ] `cargo fmt --check` and `cargo clippy -- -D warnings` pass with zero diagnostics.
- [ ] `pnpm run build` succeeds without TypeScript or Vite compilation errors.
- [ ] Tested on target operating systems:
  - [ ] macOS (Apple Silicon / Intel)
  - [ ] Windows (x64)

## Quality & Privacy Checklist
- [ ] **No credentials or secrets committed**: No API keys, passkeys, VPN credentials, private IP addresses, or internal tokens.
- [ ] **No runtime databases, logs, or cache committed**: SQLite files (`*.db`), log files (`*.log`), and build artifacts are strictly excluded.
- [ ] **Proper branding and naming adhered to**: Uses `Nebulae`, `nebulae`, and approved product conventions.
- [ ] **Observational boundaries respected**: Any engine/networking hooks not yet exposed by the core engine are explicitly marked as observational.
