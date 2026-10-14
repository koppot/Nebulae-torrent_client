# SignPath CI integration (prepared, not yet active)

This document records exactly what is prepared in `.github/workflows/release.yml` and which values the repository owner must supply **after SignPath Foundation onboarding**. No real SignPath identifiers exist yet; none are invented here.

> **Status: SignPath Foundation application declined (2026-10-14, insufficient project history); this integration is preserved but inactive.** The signing steps in the release workflow are gated behind the repository variable `WINDOWS_SIGN_PROVIDER` and stay skipped until a provider is onboarded. Beta releases continue to publish **unsigned** artifacts, and missing SignPath credentials never break Windows builds — with no provider configured, the release pipeline succeeds and publishes unsigned binaries.

## Official references

- Apply / program: <https://signpath.org/apply>
- Conditions for OSS projects: <https://signpath.org/terms.html>
- GitHub trusted build system: <https://docs.signpath.io/trusted-build-systems/github>
- Submit-signing-request action: <https://github.com/SignPath/github-action-submit-signing-request>

## Owner-supplied values (do not exist yet)

| Value | GitHub location | Source |
| :--- | :--- | :--- |
| SignPath **organization ID** (GUID) | Repository variable `SIGNPATH_ORGANIZATION_ID` | SignPath organization settings, after onboarding |
| SignPath **project slug** | Repository variable `SIGNPATH_PROJECT_SLUG` | SignPath project created for Nebulae |
| SignPath **signing policy slug** | Repository variable `SIGNPATH_SIGNING_POLICY_SLUG` | The OSS signing policy SignPath assigns (test/release) |
| SignPath **API token** (submitter permission on the project) | Repository secret `SIGNPATH_API_TOKEN` | Created in SignPath by the owner; never committed to the repository |

The workflow treats signing as configured **only** when all four values are present. If any is missing, the release is built and published exactly as today: unsigned, with `.sha256` checksums, and labeled as unsigned in the release template.

## Trusted build integration

1. SignPath side (during onboarding): add the predefined **Trusted Build System "GitHub.com"** to the organization and link it to the Nebulae project.
2. Install the [SignPath GitHub App](https://github.com/apps/signpath) and grant it access to `koppot/Nebulae-torrent_client` (used for audit-log evaluation).
3. Repository side: no change needed; the workflow submits signing requests using the action above, and SignPath verifies GitHub-provided origin metadata.

## Artifact configuration (to be created with SignPath)

Configured in the SignPath project during onboarding, using SignPath's artifact-configuration reference. Planned shape (exact syntax to be finalized with SignPath):

- **`nebulae.exe`** submitted as a zipped GitHub workflow artifact; metadata restrictions enforce ProductName `Nebulae`, a consistent product version, file description, and original filename `nebulae.exe`.
- **`Nebulae-Setup-x64.exe`** submitted the same way; the NSIS installer is rebuilt in CI so that it contains the already-signed `nebulae.exe`, and is itself signed afterwards.

Open point for SignPath onboarding: the installer is renamed from Tauri's build output (`Nebulae_<version>_x64-setup.exe`) to `Nebulae-Setup-x64.exe` **before** signing, so any `OriginalFilename` restriction for the installer must reflect the pre-rename value. To be confirmed with SignPath; no restriction syntax is guessed here.

## Approval configuration

- The OSS signing policy requires **manual approval of every signing request**. The approver (repository owner `koppot`) approves each release signing request in the SignPath console; the workflow waits for completion (`wait-for-completion: true`, generous timeout because approval is manual).
- MFA must be enabled on the GitHub account and on the SignPath account (SignPath Foundation condition for all team members).

## Pipeline flow (Windows job)

1. Checkout at the tagged commit; CI checks run separately (`ci.yml`) and on the same commit.
2. `pnpm tauri build` produces `nebulae.exe` and the NSIS installer (GitHub-hosted runner).
3. `nebulae.exe` is uploaded with `actions/upload-artifact` and submitted for signing.
4. The signed `nebulae.exe` is copied back over the build output; a SHA-256 guard fails the build if the bundling pass ever replaces it.
5. `pnpm tauri build --bundles nsis` re-packages the installer around the signed executable.
6. The installer is renamed to `Nebulae-Setup-x64.exe`, uploaded, and submitted for signing.
7. Both files are verified with `signtool verify /pa /all` (signature chain + timestamp).
8. SHA-256 checksums are generated **after** signing.
9. Only the final signed artifacts and checksums are uploaded to the GitHub release and to the workflow artifact consumed by the aggregation job.

## What is deliberately not done

- No SignPath IDs, slugs, or tokens are committed or simulated anywhere.
- No third-party/upstream binaries are submitted for signing.
- No secrets (Discord webhook, API tokens) are hardcoded in workflow files; the release announcement webhook moved to a repository secret after a prior leak in `release.yml` history.
