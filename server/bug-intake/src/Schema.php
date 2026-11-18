<?php
declare(strict_types=1);

namespace NebulaeIntake;

final class Schema
{
    public static function migrate(Db $db): void
    {
        $pk = $db->driver() === 'sqlite' ? 'INTEGER PRIMARY KEY AUTOINCREMENT' : 'BIGINT UNSIGNED PRIMARY KEY AUTO_INCREMENT';
        $big = $db->driver() === 'sqlite' ? 'TEXT' : 'MEDIUMTEXT';
        $db->pdo->exec('CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at VARCHAR(19) NOT NULL)');
        $done = $db->one('SELECT version FROM schema_migrations WHERE version = 1');
        if ($done !== null) {
            return;
        }
        $statements = [
            "CREATE TABLE reports (
                id $pk,
                report_ref VARCHAR(16) NOT NULL,
                submission_id VARCHAR(64) NOT NULL,
                receipt_hash VARCHAR(64) NOT NULL,
                category VARCHAR(64) NOT NULL,
                error_fingerprint VARCHAR(64) NOT NULL DEFAULT '',
                payload_json $big NOT NULL,
                ip_hash VARCHAR(64) NOT NULL,
                possible_duplicate_of VARCHAR(16) NULL,
                linear_create_started_at VARCHAR(19) NULL,
                linear_issue_id VARCHAR(64) NULL,
                linear_identifier VARCHAR(32) NULL,
                linear_url VARCHAR(512) NULL,
                linear_embedded INTEGER NOT NULL DEFAULT 0,
                discord_channel_id VARCHAR(32) NULL,
                discord_link_posted INTEGER NOT NULL DEFAULT 0,
                created_at VARCHAR(19) NOT NULL,
                attachments_purged_at VARCHAR(19) NULL
            )",
            'CREATE UNIQUE INDEX ux_reports_ref ON reports (report_ref)',
            'CREATE UNIQUE INDEX ux_reports_submission ON reports (submission_id)',
            'CREATE UNIQUE INDEX ux_reports_receipt ON reports (receipt_hash)',
            'CREATE INDEX ix_reports_fingerprint ON reports (error_fingerprint)',
            'CREATE INDEX ix_reports_created ON reports (created_at)',
            "CREATE TABLE attachments (
                id $pk,
                report_id BIGINT NOT NULL,
                idx INTEGER NOT NULL,
                public_name VARCHAR(80) NOT NULL,
                blob_path VARCHAR(255) NOT NULL,
                mime VARCHAR(32) NOT NULL,
                bytes BIGINT NOT NULL,
                width INTEGER NOT NULL,
                height INTEGER NOT NULL,
                sha256 VARCHAR(64) NOT NULL,
                linear_asset_url VARCHAR(1024) NULL,
                discord_message_id VARCHAR(32) NULL,
                purged_at VARCHAR(19) NULL
            )",
            'CREATE UNIQUE INDEX ux_attach_report_idx ON attachments (report_id, idx)',
            "CREATE TABLE jobs (
                id $pk,
                report_id BIGINT NOT NULL,
                kind VARCHAR(16) NOT NULL,
                state VARCHAR(16) NOT NULL DEFAULT 'pending',
                attempts INTEGER NOT NULL DEFAULT 0,
                next_attempt_at VARCHAR(19) NOT NULL,
                locked_until VARCHAR(19) NULL,
                last_error VARCHAR(400) NULL,
                created_at VARCHAR(19) NOT NULL,
                updated_at VARCHAR(19) NOT NULL,
                completed_at VARCHAR(19) NULL
            )",
            'CREATE UNIQUE INDEX ux_jobs_report_kind ON jobs (report_id, kind)',
            'CREATE INDEX ix_jobs_due ON jobs (state, next_attempt_at)',
            "CREATE TABLE discord_messages (
                id $pk,
                report_id BIGINT NOT NULL,
                batch INTEGER NOT NULL,
                message_id VARCHAR(32) NOT NULL,
                created_at VARCHAR(19) NOT NULL
            )",
            'CREATE UNIQUE INDEX ux_discord_batch ON discord_messages (report_id, batch)',
            'CREATE TABLE rate_events (
                id ' . $pk . ',
                bucket VARCHAR(80) NOT NULL,
                at_ts BIGINT NOT NULL
            )',
            'CREATE INDEX ix_rate_bucket ON rate_events (bucket, at_ts)',
            'CREATE TABLE kv (k VARCHAR(64) PRIMARY KEY, v VARCHAR(255) NOT NULL)',
            // Contact data is deliberately a separate table, never part of the payload used for forwarding.
            "CREATE TABLE contacts (
                report_id BIGINT PRIMARY KEY,
                email_enc $big NOT NULL,
                created_at VARCHAR(19) NOT NULL,
                purged_at VARCHAR(19) NULL
            )",
            "CREATE TABLE contact_access_log (
                id $pk,
                at VARCHAR(19) NOT NULL,
                maintainer VARCHAR(64) NOT NULL,
                report_ref VARCHAR(16) NOT NULL,
                action VARCHAR(16) NOT NULL,
                reason VARCHAR(200) NOT NULL
            )",
        ];
        // MySQL commits implicitly on DDL, so a wrapping transaction cannot make this atomic. Every statement
        // therefore tolerates "already exists" and the version row is written last, making a partial run re-runnable.
        foreach ($statements as $sql) {
            try {
                $db->pdo->exec($sql);
            } catch (\PDOException $e) {
                if (!in_array((int) ($e->errorInfo[1] ?? 0), [1050, 1061], true)) {
                    throw $e;
                }
            }
        }
        $db->run('INSERT INTO schema_migrations (version, applied_at) VALUES (1, ?)', [Db::now()]);
    }
}
