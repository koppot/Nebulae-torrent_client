<?php
declare(strict_types=1);

require_once __DIR__ . '/Http.php';

spl_autoload_register(static function (string $class): void {
    $prefix = 'NebulaeIntake\\';
    if (strncmp($class, $prefix, strlen($prefix)) !== 0) {
        return;
    }
    $file = __DIR__ . '/' . str_replace('\\', '/', substr($class, strlen($prefix))) . '.php';
    if (is_file($file)) {
        require $file;
    }
});
