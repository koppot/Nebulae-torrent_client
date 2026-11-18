<?php
declare(strict_types=1);

$s = require __DIR__ . '/bootstrap.php';
NebulaeIntake\Schema::migrate($s['db']);
echo "schema ok\n";
