<?php
declare(strict_types=1);

use NebulaeIntake\{Db, DiscordClient, Maintenance, Outbox};

$s = require __DIR__ . '/bootstrap.php';
/** @var NebulaeIntake\Config $config */
$config = $s['config'];
/** @var Db $db */
$db = $s['db'];
$cmd = $argv[1] ?? 'help';
$arg = $argv[2] ?? null;

function opt(array $argv, string $name): ?string
{
    foreach ($argv as $i => $a) {
        if ($a === "--$name" && isset($argv[$i + 1])) {
            return $argv[$i + 1];
        }
    }
    return null;
}

function actor(): string
{
    $u = getenv('SUDO_USER') ?: '';
    if ($u === '' && function_exists('posix_getpwuid')) {
        $u = (string) (posix_getpwuid(posix_geteuid())['name'] ?? '');
    }
    return $u;
}

switch ($cmd) {
    case 'queue': // inspect jobs; never prints report content
        $rows = $db->all("SELECT r.report_ref, j.kind, j.state, j.attempts, j.next_attempt_at, j.last_error FROM jobs j JOIN reports r ON r.id = j.report_id WHERE j.state <> 'done' ORDER BY j.next_attempt_at LIMIT 100");
        foreach ($rows as $r) {
            echo implode("\t", array_map(fn ($v) => (string) $v, $r)), "\n";
        }
        echo count($rows), " open job(s)\n";
        break;
    case 'show':
        $r = $db->one('SELECT id, report_ref, category, created_at, linear_identifier, linear_url, discord_channel_id FROM reports WHERE report_ref = ?', [$arg]);
        if (!$r) {
            fwrite(STDERR, "unknown report\n");
            exit(1);
        }
        $jobs = $db->all('SELECT kind, state, attempts, last_error FROM jobs WHERE report_id = ?', [$r['id']]);
        $att = $db->all('SELECT idx, mime, bytes, linear_asset_url IS NOT NULL AS in_linear, discord_message_id FROM attachments WHERE report_id = ?', [$r['id']]);
        echo json_encode(['report' => $r, 'jobs' => $jobs, 'attachments' => $att], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES), "\n";
        break;
    case 'requeue': // controlled reprocessing: only resets scheduling, never creates duplicates (delivery is idempotent per step)
        $kind = opt($argv, 'kind');
        $sql = "UPDATE jobs SET state = 'pending', attempts = 0, locked_until = NULL, next_attempt_at = ?, updated_at = ? WHERE state <> 'done' AND report_id IN (SELECT id FROM reports WHERE report_ref = ?)";
        $params = [Db::now(), Db::now(), $arg];
        if ($kind !== null) {
            $sql .= ' AND kind = ?';
            $params[] = $kind;
        }
        echo $db->run($sql, $params)->rowCount(), " job(s) requeued\n";
        break;
    case 'metrics':
        echo json_encode(Maintenance::metrics($db, $s['config']), JSON_PRETTY_PRINT), "\n";
        break;
    case 'alert-check':
        $m = Maintenance::metrics($db, $config);
        $bad = $m['jobs_stuck'] > 0 || ($m['oldest_open_job_minutes'] ?? 0) > $config->int('ALERT_STUCK_MINUTES');
        $last = (int) ($db->one("SELECT v FROM kv WHERE k = 'last_alert'")['v'] ?? 0);
        if ($bad && time() - $last > 21600) {
            (new DiscordClient($config, $s['http']))->post(
                'Bug report delivery needs attention: ' . $m['jobs_stuck'] . ' stuck job(s), oldest open job ' . (int) ($m['oldest_open_job_minutes'] ?? 0) . ' min. Run `admin.php queue` on the intake host.',
                null,
                []
            );
            $db->run("DELETE FROM kv WHERE k = 'last_alert'");
            $db->run("INSERT INTO kv (k, v) VALUES ('last_alert', ?)", [(string) time()]);
            echo "alert sent\n";
        } else {
            echo $bad ? "alert suppressed (sent recently)\n" : "ok\n";
        }
        exit($bad ? 2 : 0);
    case 'cleanup':
        echo json_encode(Maintenance::cleanup($db, $config, $s['vault']), JSON_PRETTY_PRINT), "\n";
        break;
    case 'disable':
        file_put_contents($config->storageDir() . '/DISABLED', Db::now() . "\n");
        echo "intake disabled (remove with: enable)\n";
        break;
    case 'enable':
        @unlink($config->storageDir() . '/DISABLED');
        echo "intake enabled (unless INTAKE_DISABLED=1 in env)\n";
        break;
    case 'contact': // contact show|purge REF --reason "..."
        $sub = $argv[2] ?? '';
        $ref = $argv[3] ?? '';
        $reason = opt($argv, 'reason') ?? '';
        try {
            if ($sub === 'show') {
                $email = $s['vault']->reveal($ref, actor(), $reason);
                echo $email ?? '(no contact stored, or already purged)', "\n";
            } elseif ($sub === 'purge') {
                if (!$s['vault']->isAuthorized(actor())) {
                    throw new RuntimeException('maintainer is not authorized');
                }
                echo $s['vault']->purge($ref, actor(), $reason ?: 'manual purge'), " contact record(s) purged\n";
            } else {
                echo "usage: contact show|purge REF --reason TEXT\n";
            }
        } catch (RuntimeException $e) {
            fwrite(STDERR, $e->getMessage() . "\n");
            exit(1);
        }
        break;
    default:
        echo "commands: queue | show REF | requeue REF [--kind K] | metrics | alert-check | cleanup | disable | enable | contact show|purge REF --reason TEXT\n";
}
