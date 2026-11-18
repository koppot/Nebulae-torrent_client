<?php
declare(strict_types=1);

require dirname(__DIR__) . '/src/autoload.php';

use NebulaeIntake\{ClientIp, Config, ContactVault, Db, ImageProcessor, Intake, Log, Schema};

function respond(int $status, array $body, array $headers = []): never
{
    http_response_code($status);
    header('Content-Type: application/json; charset=utf-8');
    header('Cache-Control: no-store');
    header('X-Content-Type-Options: nosniff');
    foreach ($headers as $k => $v) {
        header("$k: $v");
    }
    echo json_encode($body, JSON_UNESCAPED_SLASHES);
    exit;
}

$config = Config::fromEnvFile();
$origin = $_SERVER['HTTP_ORIGIN'] ?? '';
if ($origin !== '' && in_array($origin, $config->list('ALLOWED_ORIGINS'), true)) {
    header('Access-Control-Allow-Origin: ' . $origin);
    header('Vary: Origin');
    header('Access-Control-Allow-Headers: Content-Type, Idempotency-Key, Authorization');
    header('Access-Control-Allow-Methods: GET, POST, OPTIONS');
    header('Access-Control-Max-Age: 600');
}
if (($_SERVER['REQUEST_METHOD'] ?? '') === 'OPTIONS') {
    respond(204, []);
}

$path = parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH) ?: '/';
$method = $_SERVER['REQUEST_METHOD'] ?? 'GET';
$log = new Log($config->str('LOG_FILE'));

try {
    if ($path === '/healthz' && $method === 'GET') {
        respond(200, ['ok' => true]);
    }
    $db = Db::connect($config);
    $intake = new Intake($db, $config, new ImageProcessor($config), new ContactVault($db, $config), $log);
    // Forwarded headers are client-controlled; honoured only when the TCP peer is a configured proxy (Cloudflare).
    $ip = ClientIp::resolve($_SERVER['REMOTE_ADDR'] ?? '0.0.0.0', $_SERVER['HTTP_CF_CONNECTING_IP'] ?? null, $config->list('TRUSTED_PROXY_CIDRS'));

    if ($path === '/v1/reports' && $method === 'POST') {
        $files = [];
        $up = $_FILES['screenshots'] ?? null;
        if (is_array($up) && is_array($up['name'] ?? null)) {
            foreach (array_keys($up['name']) as $i) {
                if (($up['error'][$i] ?? UPLOAD_ERR_NO_FILE) === UPLOAD_ERR_NO_FILE) {
                    continue;
                }
                if (($up['error'][$i] ?? 0) === UPLOAD_ERR_OK && !is_uploaded_file($up['tmp_name'][$i])) {
                    respond(400, ['error' => 'upload_failed', 'message' => 'Invalid upload.']);
                }
                $files[] = ['tmp_name' => (string) $up['tmp_name'][$i], 'error' => (int) $up['error'][$i], 'size' => (int) $up['size'][$i]];
            }
        }
        $key = $_SERVER['HTTP_IDEMPOTENCY_KEY'] ?? null;
        [$status, $body, $headers] = array_pad($intake->submit($_POST, $files, $ip, $key), 3, []);
        respond($status, $body, $headers);
    }
    if ($path === '/v1/reports/status' && $method === 'GET') {
        $auth = $_SERVER['HTTP_AUTHORIZATION'] ?? '';
        if (!preg_match('/^Bearer ([A-Za-z0-9_\-]{20,64})$/', $auth, $m)) {
            respond(401, ['error' => 'unauthorized', 'message' => 'A receipt token is required.']);
        }
        [$status, $body] = $intake->status($m[1], $ip);
        respond($status, $body);
    }
    respond(404, ['error' => 'not_found', 'message' => 'Not found.']);
} catch (\Throwable $e) {
    $log->error('unhandled', ['class' => get_class($e)]);
    respond(500, ['error' => 'internal_error', 'message' => 'The report service had an internal error. Nothing was received.']);
}
