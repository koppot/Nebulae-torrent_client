<?php
declare(strict_types=1);

/** Run from cron every minute under flock. Processes due jobs, then exits. */
$s = require __DIR__ . '/bootstrap.php';
NebulaeIntake\Schema::migrate($s['db']);
$deadline = time() + 50;
$total = ['ran' => 0, 'done' => 0, 'failed' => 0];
while (time() < $deadline) {
    $r = $s['outbox']->runDue(10);
    foreach ($r as $k => $v) {
        $total[$k] += $v;
    }
    if ($r['ran'] === 0) {
        break;
    }
}
echo json_encode($total), "\n";
