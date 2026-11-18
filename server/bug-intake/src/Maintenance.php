<?php
declare(strict_types=1);

namespace NebulaeIntake;

final class Maintenance
{
    /** @return array<string,mixed> */
    public static function metrics(Db $db, Config $config): array
    {
        $by = [];
        foreach ($db->all('SELECT kind, state, COUNT(*) AS n FROM jobs GROUP BY kind, state') as $r) {
            $by[$r['kind'] . ':' . $r['state']] = (int) $r['n'];
        }
        $oldest = $db->one("SELECT MIN(created_at) AS t FROM jobs WHERE state NOT IN ('done','abandoned')");
        $age = $oldest && $oldest['t'] ? (int) ((time() - strtotime($oldest['t'] . ' UTC')) / 60) : null;
        return [
            'reports_total' => (int) ($db->one('SELECT COUNT(*) AS n FROM reports')['n'] ?? 0),
            'reports_24h' => (int) ($db->one('SELECT COUNT(*) AS n FROM reports WHERE created_at > ?', [Db::now(-86400)])['n'] ?? 0),
            'jobs' => $by,
            'jobs_stuck' => (int) ($db->one("SELECT COUNT(*) AS n FROM jobs WHERE state = 'stuck'")['n'] ?? 0),
            'oldest_open_job_minutes' => $age,
            'attachment_bytes' => (int) ($db->one('SELECT COALESCE(SUM(bytes),0) AS n FROM attachments WHERE purged_at IS NULL')['n'] ?? 0),
            'storage_quota_bytes' => $config->int('STORAGE_QUOTA_BYTES'),
        ];
    }

    /** Retention and orphan cleanup. Never touches blobs that a pending delivery still needs. */
    public static function cleanup(Db $db, Config $config, ContactVault $vault): array
    {
        $out = ['jobs_abandoned' => 0, 'attachments_purged' => 0, 'orphans_removed' => 0, 'reports_deleted' => 0, 'contacts_purged' => 0];
        $out['contacts_purged'] = $vault->purge();

        // Dead letter: a job that exhausted its retries ('stuck') is kept for RETENTION_STUCK_DAYS so a maintainer can
        // requeue it after fixing the cause, then abandoned. Abandoned jobs no longer pin screenshots or the report.
        $out['jobs_abandoned'] = $db->run(
            "UPDATE jobs SET state = 'abandoned', updated_at = ? WHERE state = 'stuck' AND updated_at < ?",
            [Db::now(), Db::now(-86400 * $config->int('RETENTION_STUCK_DAYS'))]
        )->rowCount();

        $cut = Db::now(-86400 * $config->int('RETENTION_ATTACHMENT_DAYS'));
        $rows = $db->all(
            "SELECT a.id, a.blob_path FROM attachments a JOIN reports r ON r.id = a.report_id
             WHERE a.purged_at IS NULL AND r.created_at < ?
               AND NOT EXISTS (SELECT 1 FROM jobs j WHERE j.report_id = r.id AND j.state NOT IN ('done','abandoned'))",
            [$cut]
        );
        foreach ($rows as $a) {
            @unlink($config->storageDir() . '/' . $a['blob_path']);
            $db->run('UPDATE attachments SET purged_at = ? WHERE id = ?', [Db::now(), $a['id']]);
            $out['attachments_purged']++;
        }

        $known = [];
        foreach ($db->all('SELECT blob_path FROM attachments') as $r) {
            $known[$r['blob_path']] = true;
        }
        $root = $config->storageDir() . '/blobs';
        if (is_dir($root)) {
            foreach (new \RecursiveIteratorIterator(new \RecursiveDirectoryIterator($root, \FilesystemIterator::SKIP_DOTS)) as $f) {
                $rel = 'blobs/' . substr($f->getPathname(), strlen($root) + 1);
                if (!isset($known[$rel]) && $f->getMTime() < time() - 3600) {
                    @unlink($f->getPathname());
                    $out['orphans_removed']++;
                }
            }
        }

        $old = Db::now(-86400 * $config->int('RETENTION_REPORT_DAYS'));
        foreach ($db->all('SELECT id FROM reports WHERE created_at < ?', [$old]) as $r) {
            if ($db->one("SELECT id FROM jobs WHERE report_id = ? AND state NOT IN ('done','abandoned')", [$r['id']]) !== null) {
                continue;
            }
            $db->transaction(function () use ($db, $r): void {
                foreach (['attachments', 'jobs', 'discord_messages', 'contacts'] as $t) {
                    $db->run("DELETE FROM $t WHERE report_id = ?", [$r['id']]);
                }
                $db->run('DELETE FROM reports WHERE id = ?', [$r['id']]);
            });
            $out['reports_deleted']++;
        }
        $db->run('DELETE FROM rate_events WHERE at_ts < ?', [time() - 172800]);
        return $out;
    }
}
