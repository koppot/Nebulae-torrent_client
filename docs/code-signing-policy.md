# Code signing policy — Nebulae

> **Status: SignPath Foundation application declined due to insufficient project history. Windows beta builds are currently unsigned unless another signing provider is configured.**
>
> The application (submitted 2026-10-14, see [signpath-application.md](signpath-application.md)) was declined; no signing provider is active, no certificate has been issued, and nothing in current releases is signed. This page describes the provider-neutral policy that will govern signed releases under whichever signing provider is onboarded in the future. It does not imply that SignPath or SignPath Foundation currently sponsors, endorses, or signs this project. The prepared SignPath integration is preserved but inactive: [signpath-ci-integration.md](signpath-ci-integration.md).

## Project identity

- **Product name:** Nebulae
- **Subtitle:** Torrent Client by Asian DVD Club
- **Repository:** <https://github.com/koppot/Nebulae-torrent_client>
- **Signed artifact scope (requested):** the project-owned Windows executable `nebulae.exe` and the project-built NSIS installer `Nebulae-Setup-x64.exe`
- **License:** MIT ([LICENSE](../LICENSE))

## Signing service

**Current provider: none.** Windows artifacts are published unsigned, and no signature claim is made for any published artifact.

A provider will be onboarded once the project has more verifiable history. If and when releases are signed through SignPath (not currently the case), the required attribution will be displayed here and in release notes:

**Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/)**

Once signing of any kind is enabled, signatures will confirm that a binary is a valid, automated build resulting from the source code in this repository. Until then, no such claim can be made.

## Team roles

Nebulae is a single-maintainer project. The roles below list the actual members; no other team members exist.

| Role | Members | Responsibility |
| :--- | :--- | :--- |
| **Authors / Committers** | [koppot](https://github.com/koppot) (repository owner) | Trusted to modify the source code in this repository without additional review. |
| **Reviewers** | [koppot](https://github.com/koppot) | Reviews all pull requests from contributors who are not committers before merge. |
| **Signing approvers** | [koppot](https://github.com/koppot) | Approves each individual signing request with the signing provider before a release is signed. |

All release history and commit history are visible in the public repository: <https://github.com/koppot/Nebulae-torrent_client/commits>

## Rules for reviewed releases and verifiable builds

1. **Verifiable builds only.** Only artifacts produced by the automated [GitHub Actions release workflow](https://github.com/koppot/Nebulae-torrent_client/blob/main/.github/workflows/release.yml) from a commit or tag in this repository are eligible for signing. Locally built or manually uploaded binaries are never signed.
2. **Build provenance.** Builds run on GitHub-hosted runners from tagged commits. The signing provider verifies that the binary submitted for signing was produced by this repository's workflow (for SignPath, via its GitHub trusted-build integration).
3. **Manual approval per release.** Every signing request requires a manual approval by the signing approver in the SignPath console. General project authorization does not replace this per-release approval.
4. **Project-owned binaries only.** Only `nebulae.exe` (built from this repository's Rust/TypeScript source) and the NSIS installer built from it are signed. Third-party upstream binaries are never signed under this project's certificate; permissive-licensed upstream components are included unsigned, as permitted by the conditions of the open-source signing programs this policy was drafted against (SignPath Foundation OSS terms).
5. **No secrets or runtime data in signed packages.** Signed installers contain only program files, application resources, and license texts. No API tokens, webhook URLs, credentials, passkeys, private keys, database files, logs, or user data are included in build artifacts.
6. **Checksums after signing.** SHA-256 checksums are generated from the final signed files and published alongside the release (`SHA256SUMS.txt` and per-file `.sha256`).
7. **Consistent metadata.** Product name (`Nebulae`), file descriptions, original executable filename (`nebulae.exe`), and a consistent product version are embedded in every signed build and are not modified after signing. Provider file-metadata restrictions (for SignPath: its artifact configuration) are enforced from onboarding onward.

## Privacy policy

Nebulae is local-first and does not include analytics or telemetry: [Privacy & Security Architecture](privacy.md).

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it (for example: connecting to a tracker, downloading a torrent, fetching a poster image the user added, or checking for signed updates when enabled).

## Software updates & silent-update conditions

The updater is built to satisfy the silent-update conditions used by open-source code-signing programs — drafted against the SignPath Foundation OSS terms (*"Applications that update themselves silently must allow users to disable this behavior. A notification must be displayed to the user that an update will be applied, e.g. in the installer"*) and enforced regardless of which provider, if any, is onboarded:

1. **Installer Notification**: The Windows NSIS installer displays a visible disclosure during setup notifying users that Nebulae applies signed updates silently in the background, and states where this behavior can be adjusted.
2. **User Control & One-Time Consent**: A clear toggle labeled *"Automatically keep Nebulae updated (recommended)"* is provided in **Settings → Options (Behavior)**. Users can disable auto-updates at any time with a single click.
3. **Zero Phone-Home When Disabled**: When disabled, the update subsystem executes an immediate return before creating any network socket or issuing any DNS lookup. Zero network requests or telemetry packets are transmitted.
4. **Staged Quiet Installation**: Updates download in the background without popups or transfer interruptions, and stage to apply strictly upon natural application quit.
5. **Cryptographic Signatures**: All updater bundles are cryptographically signed with Minisign (`.sig`) covering final artifact bytes and verified prior to extraction.

## Uninstallation

Nebulae provides an automated uninstaller on Windows (NSIS) and documented uninstallation instructions for all platforms in the [Installation Guide](installation.md#uninstalling-nebulae-on-windows).

## Questions

Questions or complaints about this policy or signed binaries: open an issue at <https://github.com/koppot/Nebulae-torrent_client/issues> or contact the maintainer (`koppot`) on GitHub.
