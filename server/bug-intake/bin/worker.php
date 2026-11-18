<?php
declare(strict_types=1);

/**
 * Run from cron every minute under flock. Processes due jobs, then exits.
 * Manual, scoped run: worker.php --ref=NBR-AAAA,NBR-BBBB processes only those reports' jobs.
 */
$s = require __DIR__ . '/bootstrap.php';
NebulaeIntake\Schema::migrate($s['db']);
$deadline = time() + 50;
$total = ['ran' => 0, 'done' => 0, 'failed' => 0];
$refs = null;
foreach (array_slice($argv, 1) as $a) {
    if (str_starts_with($a, '--ref=')) {
        $refs = array_values(array_filter(array_map('trim', explode(',', substr($a, 6))), static fn ($r) => preg_match('/^NBR-[A-Z0-9]{4,12}$/', $r) === 1));
    }
}
while (time() < $deadline) {
    $r = $s['outbox']->runDue(10, $refs);
    foreach ($r as $k => $v) {
        $total[$k] += $v;
    }
    if ($r['ran'] === 0) {
        break;
    }
}
echo json_encode($total), "\n";
