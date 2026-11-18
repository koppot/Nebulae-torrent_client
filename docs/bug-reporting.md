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
* Retention: see "Retention and failure handling" below (90 days for the ciphertext in the live database; backups hold it for the 14-day backup window; no combined maximum is claimed until backup handling is verified in operation). Immediate deletion on request: admin.php contact purge NBR-XXXX --reason "..." (live database only, not existing backups).
* The service never sends mail and never adds addresses to lists. Any future reply mechanism must keep this handling and use approved sending infrastructure.

## Screenshots

PNG, JPEG, WebP only; max 5; 10 MiB each; 25 MiB combined. The server decodes every file, rejects other types (SVG, GIF, HTML, corrupt files), bounds dimensions (10,000 px side, 40 MP), re-encodes (removes EXIF and trailing data), renames safely and stores outside the web root. Linear receives the full re-encoded image through `fileUpload` (private asset storage). Discord receives copies that fit the per-message cap (`DISCORD_MAX_MESSAGE_BYTES`, 8 MB by default; larger sets use follow-up messages and oversize images are downscaled to JPEG for Discord only). Text redaction does not apply to image content and the dialog says so.

## Deployment (applied 2026-11-18; intake kept disabled and the worker off until the first approved external test)

Applied on the ADC server (45.135.163.139, Ubuntu 22.04). Baseline before any change: `asiandvdclub.org` and `nebulae.asiandvdclub.org` answered 200, Apache and MySQL active. Backup of `/etc/apache2`, `/etc/cron.d`, root crontab, package list and PHP alternatives: `/root/backups/nebulae-reports-deploy.<timestamp>/`. Health re-checked after every Apache reload (still 200).

1. **Linear:** project `Nebulae Bug Reports` (`ed29b395-4bbf-407c-af44-3ee0bbcfdba7`), labels `user-report` (`02159c70-e982-4af2-9cf8-84112f419e6a`) and `needs-triage` (`1287d797-7ca3-4df7-9c1d-4e24ddfd7933`), intake state Backlog. Native Triage and team settings were not changed. The saved view (label `needs-triage`, project `Nebulae Bug Reports`) must be created in the Linear UI, because the connector has no view tool.
2. **Linear key:** a dedicated key is stored only in `/etc/nebulae-reports/env` (root:nebulae-reports 0640). Read-only verification: authenticated, team ASI, project, Backlog state and the three labels all resolve. The key's user is a workspace admin, which is broader than the service needs; a key owned by a non-admin member with access to team ASI is sufficient (create issues, upload files), and preferred.
3. **Discord:** not yet provisioned. Owner creates a private `#nebulae-bugs` channel and an incoming webhook, then installs the URL directly on the server (never in chat, source, issues or logs): `ssh root@HOST` then edit `DISCORD_BUGS_WEBHOOK_URL`, `DISCORD_BUGS_CHANNEL_ID`, `DISCORD_GUILD_ID` in `/etc/nebulae-reports/env`. The channel audience (roles, members, bots with View Channel) must be reviewed before it is called private.
4. **Cloudflare:** `nebulae-reports.asiandvdclub.org` A 45.135.163.139, record `c1f381a6103ee509898fdd206150581b`, proxied (was DNS-only for ACME). Zone SSL mode is Full (strict); the origin presents a Let's Encrypt ECC certificate issued and renewed by the existing acme.sh (reload hook: `apache2ctl configtest && systemctl reload apache2`), stored in `/etc/ssl/nebulae-reports/` (0700).
5. **PHP:** PHP 8.2 on this host is mod_php from the Ondrej PPA. Installing `php8.2-fpm` would have upgraded the tracker's PHP packages (8.2.31 to 8.2.35), so the service runs on a side-by-side **php8.3-fpm** (new packages only; `update-alternatives` stays manual on php8.2; tracker PHP unchanged). The default `www` pool of php8.3-fpm is renamed to `www.conf.disabled-nebulae`; the only pool is `nebulae-reports` (user `nebulae-reports`, socket `/run/php/nebulae-reports.sock`, 0660 www-data).
6. **Apache:** `a2enmod proxy_fcgi`, new `/etc/apache2/sites-available/nebulae-reports.conf` (enabled with `a2ensite`; no existing vhost edited). `CGIPassAuth On` is required so the receipt token reaches PHP. Only `index.php` is routable; `/src`, `/etc/...` return 404.
7. **System:** user `nebulae-reports` (no login), `/opt/nebulae-reports` (root-owned code, `VERSION` holds the deployed commit), `/var/lib/nebulae-reports` (0750), `/var/log/nebulae-reports`, `/etc/nebulae-reports` (0750), `/etc/cron.d/nebulae-reports` (alert-check, cleanup; **worker line commented out**), `/etc/logrotate.d/nebulae-reports` (14 days).
8. **MySQL:** database and user `nebulae_reports` (host 127.0.0.1 and localhost) with privileges only on that database. The tracker database and credentials are untouched.
9. **Backups:** daily dump and storage archive of the service (14 days) is still to be installed; the contact key is backed up separately by the owner.

### Rollback (reversible, reports preserved)

1. `admin.php disable` (refuses new reports, keeps drafts client-side).
2. Cloudflare: delete DNS record `c1f381a6103ee509898fdd206150581b`.
3. `a2dissite nebulae-reports && a2dismod proxy_fcgi` (only if nothing else uses it) `&& apachectl configtest && systemctl reload apache2`; remove `/etc/apache2/sites-available/nebulae-reports.conf`.
4. Remove `/etc/cron.d/nebulae-reports`, `/etc/logrotate.d/nebulae-reports`, the pool file, then `systemctl disable --now php8.3-fpm`; optionally `apt remove php8.3-*` (does not affect php8.2).
5. `acme.sh --remove -d nebulae-reports.asiandvdclub.org --ecc` and delete `/etc/ssl/nebulae-reports`.
6. Data (`/var/lib/nebulae-reports`, database `nebulae_reports`, `/etc/nebulae-reports`) is kept so pending deliveries resume if re-enabled; deleting it is a separate decision. Full config backup for the Apache/cron state: `/root/backups/nebulae-reports-deploy.*`.

## Retention and failure handling

| Data | Where | Kept | Mechanism |
| --- | --- | --- | --- |
| Contact e-mail (ciphertext) | intake DB `contacts` | 90 days | daily `cleanup` empties it; `contact purge` on request |
| Intake DB backups | server backup dir | 14 days | backup rotation (contact ciphertext inside is not purged earlier) |
| Screenshots (intake copy) | `/var/lib/nebulae-reports/blobs` | 30 days after the report, and only after neither delivery is pending | `cleanup` |
| Report record (text, no contact) | intake DB | 365 days | `cleanup`; reports with open jobs are kept |
| Dead-lettered jobs | intake DB | `RETENTION_STUCK_DAYS` (14) after a job exhausts its 40 attempts | `cleanup` marks `stuck` jobs `abandoned`; abandoned jobs no longer pin screenshots or the report, and `alert-check` reports `stuck` jobs until then |

* A purged address can exist in backups for up to the 14-day backup window after the 90 days. No total "maximum" is claimed beyond that until backup rotation and restore handling are verified in operation.
* Deleting or expiring intake copies does **not** delete anything already delivered. Linear issues, Linear uploaded files and Discord messages and attachments have **no automatic expiry** and are retained until a maintainer deletes them there; no downstream item is ever deleted without owner approval. Linear is the authoritative record.
* A permanently failing destination never blocks the other: Linear and Discord jobs are independent. A report abandoned for one destination keeps its other delivery.

### Who can read the contact encryption key, and why

`/etc/nebulae-reports/contact.key` is `root:nebulae-reports 0640`. It is readable by root and by the `nebulae-reports` user, which runs the PHP-FPM pool (needs the key to encrypt the address at submission time) and the cron jobs. `www-data`, the tracker and other accounts cannot read it (verified). Consequently a compromise of the intake PHP process could decrypt stored addresses; it cannot be avoided with symmetric encryption while the same process encrypts. A public-key (sealed box) design would make the live service unable to decrypt and is the recommended hardening if contact retrieval is ever enabled.

`contact show` is disabled in production: `/etc/nebulae-reports/maintainers` does not exist, so every retrieval is denied (verified). No maintainer OS accounts have been named, so none were invented. Scheduled and manual purge still work.

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
* The same suite passes against MySQL 8.0 on the ADC host (PHP 8.3): set NEBULAE_TEST_MYSQL_DSN (no dbname), NEBULAE_TEST_MYSQL_USER, NEBULAE_TEST_MYSQL_PASS; each test environment gets and drops its own throwaway database.
* Staging on the production host (worker off, no external writes): durable receipt, idempotent replay, status via Cloudflare, FPM restart recovery, contact ciphertext only, permission isolation.
* Not yet run: native macOS/Windows UI runs, real Linear and Discord delivery (awaiting owner approval of the synthetic test plan and the Discord webhook).
