# Account-free bug reporting (0.2.8)

Linear is the authoritative issue tracker. Discord is the private notification and discussion channel. Notion keeps documentation and handoffs only; it is not a second bug-status database.

## Decisions recorded (owner, 2026-11-18)

| Topic | Decision |
|---|---|
| Linear destination | New project **Nebulae Bug Reports** in team **ASI** (project `ed29b395-4bbf-407c-af44-3ee0bbcfdba7`, created). Labels `Bug`, `user-report` (created) and `needs-triage` (proposed). ASI is not a private team, so every workspace member can see reports; the dialog says so. |
| Intake status | The team has no Triage state. Native Triage would change shared team configuration, so it is only proposed. Until the owner approves that change, issues start in **Backlog** with the `needs-triage` label and a saved view. |
| Hosting | ADC server, **separate HTTPS vhost**, dedicated service account, own database and storage. |
| Release | 0.2.8 is held until report submission, screenshot delivery, Linear issue creation and the Discord notification pass end-to-end on macOS and Windows. |
| **Contact e-mail** | Optional. Stored **only** in the restricted intake backend, encrypted, keyed by report reference. It is **never** copied to Linear, Discord, Notion, GitHub, logs, URLs, filenames or analytics. Sanitized descriptions and user-approved screenshots are still delivered to Linear and Discord. |

## Architecture

```
client dialog -> POST /v1/reports (multipart, Idempotency-Key = submission id)
                  |  validate, re-encode screenshots, write blobs (fsync), one DB transaction
                  v
              201 {report_ref, receipt_token}      <- only after durable commit
                  |
        outbox jobs (independent, backoff + jitter, Retry-After respected)
           |                                  |
     linear: create/reconcile issue,     discord: embed + screenshots
     upload screenshots, embed them      (follow-up messages if over the size cap)
           \________ both done ________/
        link_linear / link_discord: cross-link issue and message
```

* Source: `server/bug-intake/` (PHP 8.2, PDO MySQL in production, SQLite in tests).
* Identity of a submission is the client-generated `submission_id` (UUIDv4). A repeat returns the original reference and receipt, so a retry after a timeout cannot create a second report.
* The receipt token is `HMAC(RECEIPT_HMAC_KEY, submission_id)`. Status lookup needs the token in an `Authorization: Bearer` header; a short report reference reveals nothing. Status returns only delivery states, never content, contact data or Linear details.
* Exactly-once across external APIs is not claimed. The service uses unique keys, row-claim locking, per-step persistence and reconciliation:
  * Linear create with an unknown outcome is reconciled by searching the team for the report reference before any second create. If the search finds nothing the service waits two minutes before retrying (search indexing delay).
  * A failed screenshot upload is retried against the same issue; an issue is never re-created because an attachment failed.
  * Discord webhooks cannot be searched, so a timeout during a Discord POST can in rare cases produce a duplicate message. Every message carries the report reference so duplicates are recognizable.
* Possible duplicates are flagged (same error fingerprint) in the Linear text; reports are never merged automatically.

## Contact e-mail handling

* Separate `contacts` table, libsodium `secretbox` (XSalsa20-Poly1305) ciphertext; the key is `/etc/nebulae-reports/contact.key`, outside the database, storage and repository.
* The forwarded payload is built from an explicit allowlist (`Validator` then `Formatter`). The contact value is returned by the validator separately and never enters `payload_json`, jobs, Linear text, Discord embeds or log lines (tests assert this on every outgoing request body and on the log file).
* Only the CLI can read it: `sudo -u nebulae-reports php admin.php contact show NBR-XXXX --reason "…"`. The OS user from `SUDO_USER` must be listed in `/etc/nebulae-reports/maintainers`; denied and successful reads are recorded in `contact_access_log` (maintainer, reference, action, reason, never the address). There is no HTTP route that reads it.
* Retention (proposed, awaiting owner approval): ciphertext is emptied 90 days after receipt (`cleanup`, daily). Backups keep ciphertext for the backup retention window (14 days proposed), so the longest a purged address can still exist is 90 + 14 days; the key is not in the backups. Immediate deletion on request: `admin.php contact purge NBR-XXXX --reason "…"` (removes it from the live database, not from existing backups).
* The service never sends mail and never adds addresses to lists. Any future reply mechanism must keep this handling and use approved sending infrastructure.

## Screenshots

PNG, JPEG, WebP only; max 5; 10 MiB each; 25 MiB combined. The server decodes every file, rejects other types (SVG, GIF, HTML, corrupt files), bounds dimensions (10,000 px side, 40 MP), re-encodes (removes EXIF and trailing data), renames safely and stores outside the web root. Linear receives the full re-encoded image through `fileUpload` (private asset storage). Discord receives copies that fit the per-message cap (`DISCORD_MAX_MESSAGE_BYTES`, 8 MB by default; larger sets use follow-up messages and oversize images are downscaled to JPEG for Discord only). Text redaction does not apply to image content and the dialog says so.

## Deployment (exact external changes, none applied yet)

1. **Linear (done, approved):** project `Nebulae Bug Reports`, label `user-report`. **Proposed:** label `needs-triage`, a saved view of that label, and (owner decision) enabling native Triage on team ASI.
2. **Linear API key:** owner creates a dedicated key in Settings, API; it goes only into `/etc/nebulae-reports/env` as `LINEAR_API_KEY`. The Factory Linear connector is an interactive credential and is not reused by the backend. No Linear webhook is required for basic reporting; the optional status sync would need an admin to create a webhook (the connector exposes no webhook tool).
3. **Discord:** owner creates a private channel `#nebulae-bugs` and an incoming webhook; only the channel id and guild id are shared with maintainers. No bot is needed.
4. **Cloudflare DNS:** `nebulae-reports.asiandvdclub.org` A 45.135.163.139, DNS-only until ACME issues the certificate, then proxied (same pattern as nebulae.asiandvdclub.org). A single-level name is used because the zone's universal certificate does not cover `reports.nebulae.…`.
5. **ADC server (45.135.163.139):**
   * `apt install php8.2-fpm` and `a2enmod proxy_fcgi`, then `systemctl reload apache2` (graceful, no restart). The existing vhosts run mod_php as `www-data`; a dedicated PHP-FPM pool is the only way to give the intake service its own user.
   * User `nebulae-reports` (no login), directories `/opt/nebulae-reports` (code, root-owned), `/var/lib/nebulae-reports` (storage, `blobs`, `tmp`), `/var/log/nebulae-reports`, `/etc/nebulae-reports` (`env` 0640 root:nebulae-reports, `contact.key`, `maintainers`).
   * MySQL database and user `nebulae_reports` limited to that database; no tracker database credentials.
   * New files only: `/etc/apache2/sites-available/nebulae-reports.conf`, `/etc/php/8.2/fpm/pool.d/nebulae-reports.conf`, `/etc/cron.d/nebulae-reports`, logrotate entry, a certificate for the new hostname via the existing acme.sh. Templates are in `server/bug-intake/deploy/`.
   * Backups: daily dump of `nebulae_reports` plus storage to `/var/backups/nebulae-reports` (0700, 14 days). The contact key is backed up separately by the owner, never in the same archive.
6. **Rollback:** `a2dissite nebulae-reports && systemctl reload apache2`, remove the cron file, `admin.php disable`. Reports, jobs and storage are left in place, so pending deliveries resume when the service is re-enabled.

## Operations

* `admin.php queue | show REF | requeue REF [--kind K] | metrics | alert-check | cleanup | disable | enable | contact …`
* Alerting: `alert-check` (cron every 15 min) posts one bounded message per 6 hours to the bugs channel when a job is stuck or older than `ALERT_STUCK_MINUTES`.
* Credential rotation: edit `/etc/nebulae-reports/env`, no restart needed (FPM reads it per request, cron per run). Rotate the Linear key in Linear first, then replace it; regenerate the Discord webhook the same way. Rotating `RECEIPT_HMAC_KEY` invalidates status lookups for earlier receipts only.
* Storage full or database unavailable: the endpoint answers with an error body and nothing is acknowledged; the client keeps the draft and screenshots.
* Emergency disable: `INTAKE_DISABLED=1` or `admin.php disable`; the client shows the service message and keeps the draft.
* Reporting does not depend on the torrent engine or the Docker server.

## Tests

* Backend: `php server/bug-intake/tests/run.php` covers redaction, validation, image acceptance/rejection, idempotency, limits, storage failure, status privacy, contact isolation (no contact in any outgoing request, log, payload or status), vault authorization and audit, retention and purge, Linear/Discord failures (5xx, 429, ambiguous create, upload failure), restart recovery, mention suppression, markdown containment and cleanup.
* Client: `tests/bugReportAttachments.test.ts`, `tests/bugReportSubmit.test.ts`.
* Not yet run: native macOS/Windows UI runs, real Linear and Discord delivery, MySQL-specific behavior, PHP-FPM limits, Cloudflare path. Those need the approved external resources above.
