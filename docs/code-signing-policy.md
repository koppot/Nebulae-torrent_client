# Code signing policy — Nebulae

> **Status: SignPath Foundation application pending; signing is not yet enabled.**
>
> All Windows artifacts currently published in [Releases](https://github.com/koppot/Nebulae-torrent_client/releases) are **unsigned** beta builds. This page describes the policy that will govern signed releases once the SignPath Foundation application is accepted. It does not imply that SignPath or SignPath Foundation currently sponsors, endorses, or signs this project.

## Project identity

- **Product name:** Nebulae
- **Subtitle:** Torrent Client by Asian DVD Club
- **Repository:** <https://github.com/koppot/Nebulae-torrent_client>
- **Signed artifact scope (requested):** the project-owned Windows executable `nebulae.exe` and the project-built NSIS installer `Nebulae-Setup-x64.exe`
- **License:** MIT ([LICENSE](../LICENSE))

## Free code signing service

Once the application is accepted and signing is enabled:

**Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/)**

Signatures will confirm that a binary is a valid, automated build resulting from the source code in this repository. Until acceptance, no such claim can be made for any published artifact.

## Team roles

Nebulae is a single-maintainer project. The roles below list the actual members; no other team members exist.

| Role | Members | Responsibility |
| :--- | :--- | :--- |
| **Authors / Committers** | [koppot](https://github.com/koppot) (repository owner) | Trusted to modify the source code in this repository without additional review. |
| **Reviewers** | [koppot](https://github.com/koppot) | Reviews all pull requests from contributors who are not committers before merge. |
| **Signing approvers** | [koppot](https://github.com/koppot) | Approves each individual signing request in SignPath before a release is signed. |

All release history and commit history are visible in the public repository: <https://github.com/koppot/Nebulae-torrent_client/commits>

## Rules for reviewed releases and verifiable builds

1. **Verifiable builds only.** Only artifacts produced by the automated [GitHub Actions release workflow](https://github.com/koppot/Nebulae-torrent_client/blob/main/.github/workflows/release.yml) from a commit or tag in this repository are eligible for signing. Locally built or manually uploaded binaries are never signed.
2. **Build provenance.** Builds run on GitHub-hosted runners. SignPath verifies through its GitHub trusted-build integration that the binary submitted for signing was produced by this repository's workflow.
3. **Manual approval per release.** Every signing request requires a manual approval by the signing approver in the SignPath console. General project authorization does not replace this per-release approval.
4. **Project-owned binaries only.** Only `nebulae.exe` (built from this repository's Rust/TypeScript source) and the NSIS installer built from it are signed. Third-party upstream binaries are never signed under this project's certificate; permissive-licensed upstream components are included unsigned, as permitted by the SignPath Foundation conditions.
5. **No secrets or runtime data in signed packages.** Signed installers contain only program files, application resources, and license texts. No API tokens, webhook URLs, credentials, passkeys, private keys, database files, logs, or user data are included in build artifacts.
6. **Checksums after signing.** SHA-256 checksums are generated from the final signed files and published alongside the release (`SHA256SUMS.txt` and per-file `.sha256`).
7. **Consistent metadata.** Product name (`Nebulae`), file descriptions, original executable filename (`nebulae.exe`), and a consistent product version are embedded in every signed build and are not modified after signing. SignPath file metadata restrictions are enforced through the project's SignPath artifact configuration once onboarded.

## Privacy policy

Nebulae is local-first and does not include analytics or telemetry: [Privacy & Security Architecture](privacy.md).

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it (for example: connecting to a tracker, downloading a torrent, or fetching a poster image the user added).

## Uninstallation

Nebulae provides an automated uninstaller on Windows (NSIS) and documented uninstallation instructions for all platforms in the [Installation Guide](installation.md#uninstalling-nebulae-on-windows).

## Questions

Questions or complaints about this policy or signed binaries: open an issue at <https://github.com/koppot/Nebulae-torrent_client/issues> or contact the maintainer (`koppot`) on GitHub.
