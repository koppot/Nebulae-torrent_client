<?php
declare(strict_types=1);

require dirname(__DIR__) . '/src/autoload.php';

use NebulaeIntake\{Config, ContactVault, CurlTransport, Db, DiscordClient, ImageProcessor, LinearClient, Log, Outbox};

if (PHP_SAPI !== 'cli') {
    exit(1);
}
$config = Config::fromEnvFile();
$db = Db::connect($config);
$log = new Log($config->str('LOG_FILE'));
$http = new CurlTransport();
$images = new ImageProcessor($config);
$vault = new ContactVault($db, $config);
$outbox = new Outbox($db, $config, new LinearClient($config, $http), new DiscordClient($config, $http), $images, $log);

return compact('config', 'db', 'log', 'http', 'images', 'vault', 'outbox');
