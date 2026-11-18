<?php
declare(strict_types=1);

namespace NebulaeIntake;

/**
 * Delivery worker. Linear and Discord are independent jobs; either can fail and retry without
 * affecting the other, and neither can duplicate work the other already completed.
 */
final class Outbox
{
    public const MAX_ATTEMPTS = 40;
    private const KINDS = ['linear', 'discord', 'link_linear', 'link_discord'];

    public function __construct(
        private Db $db,
        private Config $config,
        private LinearClient $linear,
        private DiscordClient $discord,
        private ImageProcessor $images,
        private Log $log,
    ) {
    }

    /** @return array{ran:int,done:int,failed:int} */
    public function runDue(int $limit = 10): array
    {
        $now = Db::now();
        $rows = $this->db->all(
            "SELECT id FROM jobs WHERE state = 'pending' AND next_attempt_at <= ? AND (locked_until IS NULL OR locked_until < ?) ORDER BY next_attempt_at, id LIMIT " . (int) $limit,
            [$now, $now]
        );
        $stats = ['ran' => 0, 'done' => 0, 'failed' => 0];
        foreach ($rows as $row) {
            $claim = $this->db->run(
                "UPDATE jobs SET locked_until = ? WHERE id = ? AND state = 'pending' AND (locked_until IS NULL OR locked_until < ?)",
                [Db::now(300), $row['id'], $now]
            );
            if ($claim->rowCount() !== 1) {
                continue;
            }
            $stats['ran']++;
            $job = $this->db->one('SELECT * FROM jobs WHERE id = ?', [$row['id']]);
            $ok = $this->runJob($job);
            $stats[$ok ? 'done' : 'failed']++;
        }
        return $stats;
    }

    /** @param array<string,mixed> $job */
    private function runJob(array $job): bool
    {
        $report = $this->db->one('SELECT * FROM reports WHERE id = ?', [$job['report_id']]);
        $ref = (string) $report['report_ref'];
        try {
            $result = match ($job['kind']) {
                'linear' => $this->runLinear($report),
                'discord' => $this->runDiscord($report),
                'link_linear' => $this->runLinkLinear($report),
                'link_discord' => $this->runLinkDiscord($report),
                default => throw new DeliveryError('unknown job kind', false),
            };
            if ($result === 'wait') {
                $this->db->run("UPDATE jobs SET locked_until = NULL, next_attempt_at = ?, updated_at = ? WHERE id = ?", [Db::now(60), Db::now(), $job['id']]);
                return true;
            }
            $this->db->run("UPDATE jobs SET state = 'done', locked_until = NULL, last_error = NULL, completed_at = ?, updated_at = ? WHERE id = ?", [Db::now(), Db::now(), $job['id']]);
            $this->log->info('job_done', ['ref' => $ref, 'kind' => $job['kind']]);
            $this->scheduleCrossLinks((int) $report['id']);
            return true;
        } catch (DeliveryError $e) {
            $this->fail($job, $ref, $e->getMessage(), $e->retryAfter);
        } catch (\Throwable $e) {
            $this->fail($job, $ref, get_class($e) . ': ' . $e->getMessage(), null);
        }
        return false;
    }

    /** @param array<string,mixed> $job */
    private function fail(array $job, string $ref, string $message, ?int $retryAfter): void
    {
        $attempts = (int) $job['attempts'] + 1;
        $state = $attempts >= self::MAX_ATTEMPTS ? 'stuck' : 'pending';
        $this->db->run(
            'UPDATE jobs SET attempts = ?, state = ?, locked_until = NULL, next_attempt_at = ?, last_error = ?, updated_at = ? WHERE id = ?',
            [$attempts, $state, Db::now(Retry::delaySeconds($attempts, $retryAfter)), Log::safeError($message), Db::now(), $job['id']]
        );
        $this->log->error('job_failed', ['ref' => $ref, 'kind' => $job['kind'], 'attempts' => $attempts, 'state' => $state]);
    }

    private function scheduleCrossLinks(int $reportId): void
    {
        $done = $this->db->all("SELECT kind FROM jobs WHERE report_id = ? AND state = 'done' AND kind IN ('linear','discord')", [$reportId]);
        if (count($done) !== 2) {
            return;
        }
        foreach (['link_linear', 'link_discord'] as $kind) {
            self::enqueue($this->db, $reportId, $kind);
        }
    }

    public static function enqueue(Db $db, int $reportId, string $kind): void
    {
        if ($db->one('SELECT id FROM jobs WHERE report_id = ? AND kind = ?', [$reportId, $kind]) !== null) {
            return;
        }
        $now = Db::now();
        $db->run('INSERT INTO jobs (report_id, kind, state, attempts, next_attempt_at, created_at, updated_at) VALUES (?, ?, ?, 0, ?, ?, ?)', [$reportId, $kind, 'pending', $now, $now, $now]);
    }

    /** @param array<string,mixed> $report */
    private function payload(array $report): array
    {
        return json_decode((string) $report['payload_json'], true, 8, JSON_THROW_ON_ERROR);
    }

    /** @return list<array<string,mixed>> */
    private function attachments(int $reportId): array
    {
        return $this->db->all('SELECT * FROM attachments WHERE report_id = ? ORDER BY idx', [$reportId]);
    }

    private function discordLink(array $report): ?string
    {
        $m = $this->db->one('SELECT message_id FROM discord_messages WHERE report_id = ? AND batch = 0', [$report['id']]);
        $guild = $this->config->str('DISCORD_GUILD_ID');
        if ($m === null || $guild === '' || $this->config->str('DISCORD_BUGS_CHANNEL_ID') === '') {
            return null;
        }
        return 'https://discord.com/channels/' . $guild . '/' . $this->config->str('DISCORD_BUGS_CHANNEL_ID') . '/' . $m['message_id'];
    }

    /** @param array<string,mixed> $report */
    private function runLinear(array $report): string
    {
        $id = (int) $report['id'];
        $ref = (string) $report['report_ref'];
        $p = $this->payload($report);
        $atts = $this->attachments($id);

        if ($report['linear_issue_id'] === null) {
            if ($report['linear_create_started_at'] !== null) {
                // A previous create attempt had an unknown outcome: look for the issue before creating again.
                $found = $this->linear->findIssueByRef($ref);
                if ($found === null && strtotime($report['linear_create_started_at'] . ' UTC') > time() - 120) {
                    return 'wait';
                }
            } else {
                $found = null;
            }
            if ($found === null) {
                $this->db->run('UPDATE reports SET linear_create_started_at = ? WHERE id = ?', [Db::now(), $id]);
                try {
                    $found = $this->linear->createIssue(
                        Formatter::linearTitle($ref, $p),
                        Formatter::linearBody($ref, $p, $atts, $report['possible_duplicate_of'], null)
                    );
                } catch (DeliveryError $e) {
                    if (!$e->ambiguous) {
                        // Provably not processed (rate limit, auth, connect failure): safe to create again later.
                        $this->db->run('UPDATE reports SET linear_create_started_at = NULL WHERE id = ?', [$id]);
                    }
                    throw $e;
                }
            }
            $this->db->run('UPDATE reports SET linear_issue_id = ?, linear_identifier = ?, linear_url = ? WHERE id = ?', [$found['id'], $found['identifier'], $found['url'], $id]);
            $report['linear_issue_id'] = $found['id'];
        }

        $pending = false;
        foreach ($atts as $i => $a) {
            if ($a['linear_asset_url'] !== null || $a['purged_at'] !== null) {
                continue;
            }
            $bytes = @file_get_contents($this->config->storageDir() . '/' . $a['blob_path']);
            if ($bytes === false) {
                throw new DeliveryError('stored screenshot is missing from disk', false, false, 3600);
            }
            $url = $this->linear->uploadFile($a['public_name'], $a['mime'], $bytes);
            $this->db->run('UPDATE attachments SET linear_asset_url = ? WHERE id = ?', [$url, $a['id']]);
            $atts[$i]['linear_asset_url'] = $url;
        }
        if ($atts !== [] && !$report['linear_embedded']) {
            $this->linear->updateDescription($report['linear_issue_id'], Formatter::linearBody($ref, $p, $atts, $report['possible_duplicate_of'], $this->discordLink($report)));
        }
        $this->db->run('UPDATE reports SET linear_embedded = 1 WHERE id = ?', [$id]);
        return $pending ? 'wait' : 'done';
    }

    /** @param array<string,mixed> $report */
    private function runDiscord(array $report): string
    {
        $id = (int) $report['id'];
        $ref = (string) $report['report_ref'];
        $p = $this->payload($report);
        $atts = $this->attachments($id);
        $limit = $this->config->int('DISCORD_MAX_MESSAGE_BYTES');

        $copies = [];
        $batches = [[]];
        $size = 0;
        foreach ($atts as $a) {
            if ($a['purged_at'] !== null) {
                continue;
            }
            $bytes = @file_get_contents($this->config->storageDir() . '/' . $a['blob_path']);
            if ($bytes === false) {
                throw new DeliveryError('stored screenshot is missing from disk', false, false, 3600);
            }
            $fit = $this->images->fitForDiscord($bytes, (string) $a['mime'], $limit);
            $ext = ['image/png' => 'png', 'image/jpeg' => 'jpg', 'image/webp' => 'webp'][$fit['mime']];
            $copy = ['idx' => (int) $a['idx'], 'attachment_id' => $a['id'], 'bytes' => $fit['bytes'], 'mime' => $fit['mime'], 'name' => 'screenshot-' . ((int) $a['idx'] + 1) . '.' . $ext];
            if ($size + strlen($fit['bytes']) > $limit || count($batches[array_key_last($batches)]) >= 10) {
                $batches[] = [];
                $size = 0;
            }
            $batches[array_key_last($batches)][] = $copy;
            $size += strlen($fit['bytes']);
        }

        $linearUrl = $report['linear_url'];
        $embed = Formatter::discordEmbed($ref, $p, count($atts), $linearUrl, $report['linear_identifier']);
        $total = count($batches);
        foreach ($batches as $n => $batch) {
            if ($this->db->one('SELECT id FROM discord_messages WHERE report_id = ? AND batch = ?', [$id, $n]) !== null) {
                continue;
            }
            $tmp = [];
            $files = [];
            foreach ($batch as $c) {
                $path = tempnam(sys_get_temp_dir(), 'nbr');
                file_put_contents($path, $c['bytes']);
                $tmp[] = $path;
                $files[] = ['name' => $c['name'], 'path' => $path, 'mime' => $c['mime']];
            }
            try {
                $mid = $n === 0
                    ? $this->discord->post(null, $embed, $files)
                    : $this->discord->post("$ref: additional screenshots (" . ($n + 1) . " of $total)", null, $files);
            } finally {
                foreach ($tmp as $t) {
                    @unlink($t);
                }
            }
            $this->db->run('INSERT INTO discord_messages (report_id, batch, message_id, created_at) VALUES (?, ?, ?, ?)', [$id, $n, $mid, Db::now()]);
            foreach ($batch as $c) {
                $this->db->run('UPDATE attachments SET discord_message_id = ? WHERE id = ?', [$mid, $c['attachment_id']]);
            }
            $this->db->run('UPDATE reports SET discord_channel_id = ? WHERE id = ?', [$this->config->str('DISCORD_BUGS_CHANNEL_ID') ?: null, $id]);
        }
        return 'done';
    }

    /** Adds the Discord message link to the Linear issue. */
    private function runLinkLinear(array $report): string
    {
        $link = $this->discordLink($report);
        if ($link === null) {
            return 'done'; // no guild configured: nothing meaningful to link
        }
        $p = $this->payload($report);
        $this->linear->updateDescription(
            (string) $report['linear_issue_id'],
            Formatter::linearBody((string) $report['report_ref'], $p, $this->attachments((int) $report['id']), $report['possible_duplicate_of'], $link)
        );
        return 'done';
    }

    /** Adds the Linear issue link to the Discord message. */
    private function runLinkDiscord(array $report): string
    {
        $m = $this->db->one('SELECT message_id FROM discord_messages WHERE report_id = ? AND batch = 0', [$report['id']]);
        if ($m === null) {
            return 'wait';
        }
        $embed = Formatter::discordEmbed((string) $report['report_ref'], $this->payload($report), count($this->attachments((int) $report['id'])), $report['linear_url'], $report['linear_identifier']);
        $this->discord->editEmbed($m['message_id'], $embed);
        $this->db->run('UPDATE reports SET discord_link_posted = 1 WHERE id = ?', [$report['id']]);
        return 'done';
    }

    /** Reports whose Linear and Discord delivery are both complete, as a summary for status lookups. */
    public static function stateFor(Db $db, int $reportId, string $kind): string
    {
        $j = $db->one('SELECT state, attempts FROM jobs WHERE report_id = ? AND kind = ?', [$reportId, $kind]);
        if ($j === null) {
            return 'pending';
        }
        return match ($j['state']) {
            'done' => 'done',
            'stuck' => 'delayed',
            default => (int) $j['attempts'] > 0 ? 'retrying' : 'pending',
        };
    }
}
