<?php
declare(strict_types=1);

require dirname(__DIR__) . '/src/autoload.php';

use NebulaeIntake\{Config, ContactVault, Db, DiscordClient, HttpResponse, ImageProcessor, Intake, LinearClient, Log, Maintenance, Outbox, Schema, Transport};

final class T
{
    public static int $pass = 0;
    public static int $fail = 0;

    public static function ok(bool $cond, string $name): void
    {
        if ($cond) {
            self::$pass++;
        } else {
            self::$fail++;
            echo "FAIL: $name\n";
        }
    }

    public static function eq(mixed $a, mixed $b, string $name): void
    {
        self::ok($a === $b, $name . ' (got ' . var_export($a, true) . ', want ' . var_export($b, true) . ')');
    }
}

/** Scriptable Linear + Discord fake. Records every request body for leak assertions. */
final class FakeNet implements Transport
{
    /** @var list<array{method:string,url:string,body:string,files:int}> */
    public array $requests = [];
    public int $issues = 0;
    public int $discordPosts = 0;
    public array $issueRows = [];
    public int $linearFail = 0;       // next N Linear calls fail with 5xx before processing
    public int $linearAmbiguous = 0;  // next N createIssue calls are created but the response is lost
    public int $linearRateLimit = 0;
    public int $uploadFail = 0;
    public int $discordFail = 0;
    public int $discord429 = 0;
    public array $descriptions = [];
    public array $discordPayloads = [];
    public array $discordFileSizes = [];

    public function send(string $method, string $url, array $headers, string|array|null $body = null, int $timeout = 30): HttpResponse
    {
        $flat = is_array($body) ? json_encode(array_map(fn ($v) => $v instanceof CURLFile ? 'FILE:' . $v->getPostFilename() : $v, $body)) : (string) $body;
        $files = is_array($body) ? count(array_filter($body, fn ($v) => $v instanceof CURLFile)) : 0;
        $this->requests[] = ['method' => $method, 'url' => $url, 'body' => $flat, 'files' => $files];

        if (str_contains($url, 'api.linear.app')) {
            if ($this->linearRateLimit > 0) {
                $this->linearRateLimit--;
                return new HttpResponse(400, [], json_encode(['errors' => [['message' => 'rate', 'extensions' => ['code' => 'RATELIMITED']]]]));
            }
            if ($this->linearFail > 0) {
                $this->linearFail--;
                return new HttpResponse(503, [], 'down');
            }
            $q = json_decode((string) $body, true);
            $query = $q['query'];
            if (str_contains($query, 'issueCreate')) {
                $id = 'iss-' . (++$this->issues);
                $this->issueRows[$q['variables']['input']['title']] = ['id' => $id, 'identifier' => 'ASI-' . (100 + $this->issues), 'url' => "https://linear.app/x/issue/$id"];
                $this->descriptions[$id] = $q['variables']['input']['description'];
                if ($this->linearAmbiguous > 0) {
                    $this->linearAmbiguous--;
                    return new HttpResponse(0, [], '', 'transport error 28', true);
                }
                $row = $this->issueRows[$q['variables']['input']['title']];
                return new HttpResponse(200, [], json_encode(['data' => ['issueCreate' => ['success' => true, 'issue' => $row]]]));
            }
            if (str_contains($query, 'issues(filter')) {
                $ref = $q['variables']['ref'];
                foreach ($this->issueRows as $title => $row) {
                    if (str_contains($title, $ref)) {
                        return new HttpResponse(200, [], json_encode(['data' => ['issues' => ['nodes' => [$row]]]]));
                    }
                }
                return new HttpResponse(200, [], json_encode(['data' => ['issues' => ['nodes' => []]]]));
            }
            if (str_contains($query, 'fileUpload')) {
                return new HttpResponse(200, [], json_encode(['data' => ['fileUpload' => ['success' => true, 'uploadFile' => [
                    'uploadUrl' => 'https://storage.example/upload/' . bin2hex(random_bytes(4)),
                    'assetUrl' => 'https://uploads.linear.app/asset/' . bin2hex(random_bytes(4)),
                    'headers' => [['key' => 'x-goog-meta', 'value' => 'v']],
                ]]]]));
            }
            if (str_contains($query, 'issueUpdate')) {
                $this->descriptions[$q['variables']['id']] = $q['variables']['input']['description'];
                return new HttpResponse(200, [], json_encode(['data' => ['issueUpdate' => ['success' => true]]]));
            }
            return new HttpResponse(400, [], '{}');
        }
        if (str_starts_with($url, 'https://storage.example/upload/')) {
            if ($this->uploadFail > 0) {
                $this->uploadFail--;
                return new HttpResponse(500, [], 'x');
            }
            return new HttpResponse(200, [], '');
        }
        if (str_contains($url, 'discord.com/api/webhooks')) {
            if ($this->discord429 > 0) {
                $this->discord429--;
                return new HttpResponse(429, ['retry-after' => '2'], json_encode(['retry_after' => 1.5]));
            }
            if ($this->discordFail > 0) {
                $this->discordFail--;
                return new HttpResponse(502, [], 'bad gateway');
            }
            if ($method === 'POST') {
                $this->discordPosts++;
                $payload = json_decode(is_array($body) ? (string) $body['payload_json'] : '{}', true);
                $this->discordPayloads[] = $payload;
                if (is_array($body)) {
                    foreach ($body as $k => $v) {
                        if ($v instanceof CURLFile) {
                            $this->discordFileSizes[] = strlen((string) file_get_contents($v->getFilename()));
                        }
                    }
                }
                return new HttpResponse(200, [], json_encode(['id' => (string) (5000 + $this->discordPosts)]));
            }
            if ($method === 'PATCH') {
                $this->discordPayloads[] = json_decode((string) $body, true);
                return new HttpResponse(200, [], '{}');
            }
        }
        return new HttpResponse(404, [], '');
    }

    public function allText(): string
    {
        return implode("\n", array_map(fn ($r) => $r['url'] . ' ' . $r['body'], $this->requests));
    }
}

final class Env
{
    public Config $config;
    public Db $db;
    public FakeNet $net;
    public Intake $intake;
    public Outbox $outbox;
    public ContactVault $vault;
    public string $dir;
    public string $logFile;

    public function __construct(array $over = [])
    {
        $this->dir = sys_get_temp_dir() . '/nbr-test-' . bin2hex(random_bytes(4));
        mkdir($this->dir . '/blobs', 0750, true);
        $this->logFile = $this->dir . '/intake.log';
        file_put_contents($this->dir . '/contact.key', base64_encode(random_bytes(SODIUM_CRYPTO_SECRETBOX_KEYBYTES)));
        file_put_contents($this->dir . '/maintainers', "# authorized\nalice\n");
        $this->config = new Config($over + [
            'STORAGE_DIR' => $this->dir, 'LOG_FILE' => $this->logFile, 'CONTACT_KEY_FILE' => $this->dir . '/contact.key',
            'MAINTAINERS_FILE' => $this->dir . '/maintainers', 'RECEIPT_HMAC_KEY' => 'receipt-test-key', 'IP_HASH_KEY' => 'ip-test-key',
            'LINEAR_API_KEY' => 'lin_api_FAKEFAKEFAKEFAKEFAKEFAKEFAKE', 'LINEAR_TEAM_ID' => 'team-1', 'LINEAR_PROJECT_ID' => 'proj-1',
            'LINEAR_INTAKE_STATE_ID' => 'state-1', 'LINEAR_LABEL_IDS' => 'lab-1,lab-2',
            'DISCORD_BUGS_WEBHOOK_URL' => 'https://discord.com/api/webhooks/123456789012345678/TEST_webhook-token_ABC',
            'DISCORD_BUGS_CHANNEL_ID' => '777', 'DISCORD_GUILD_ID' => '999',
        ]);
        $this->db = Db::memory();
        Schema::migrate($this->db);
        $this->net = new FakeNet();
        $log = new Log($this->logFile);
        $images = new ImageProcessor($this->config);
        $this->vault = new ContactVault($this->db, $this->config);
        $this->intake = new Intake($this->db, $this->config, $images, $this->vault, $log);
        $this->outbox = new Outbox($this->db, $this->config, new LinearClient($this->config, $this->net), new DiscordClient($this->config, $this->net), $images, $log);
    }

    public function newOutbox(): Outbox
    {
        $log = new Log($this->logFile);
        return new Outbox($this->db, $this->config, new LinearClient($this->config, $this->net), new DiscordClient($this->config, $this->net), new ImageProcessor($this->config), $log);
    }

    public function makeDue(): void
    {
        $this->db->run("UPDATE jobs SET next_attempt_at = '2000-01-01 00:00:00', locked_until = NULL WHERE state = 'pending'");
    }

    public function drain(int $rounds = 6): void
    {
        for ($i = 0; $i < $rounds; $i++) {
            $this->makeDue();
            $this->outbox->runDue(20);
        }
    }

    public static function uuid(): string
    {
        $b = random_bytes(16);
        $b[6] = chr((ord($b[6]) & 0x0f) | 0x40);
        $b[8] = chr((ord($b[8]) & 0x3f) | 0x80);
        $h = bin2hex($b);
        return sprintf('%s-%s-%s-%s-%s', substr($h, 0, 8), substr($h, 8, 4), substr($h, 12, 4), substr($h, 16, 4), substr($h, 20));
    }

    /** @return array<string,mixed> */
    public static function report(array $over = []): array
    {
        return $over + [
            'summary' => 'Playback stutters on UHD', 'description' => 'Video freezes after 10 minutes.', 'steps' => '1. Open file', 'expected' => 'Smooth', 'actual' => 'Stutter',
            'category' => 'Playback & Media Center', 'app_version' => '0.2.8', 'build_id' => 'abc1234', 'os' => 'macOS', 'os_version' => '15.1', 'arch' => 'aarch64',
            'diagnostics' => 'engine ok', 'error_fingerprint' => 'fp-1234', 'submission_id' => self::uuid(),
        ];
    }

    public function submit(array $report, array $images = [], string $ip = '203.0.113.9'): array
    {
        $files = [];
        foreach ($images as $bytes) {
            $p = tempnam($this->dir, 'up');
            file_put_contents($p, $bytes);
            $files[] = ['tmp_name' => $p, 'error' => UPLOAD_ERR_OK, 'size' => strlen($bytes)];
        }
        return $this->intake->submit(['report' => json_encode($report)], $files, $ip, $report['submission_id']);
    }

    public static function png(int $w = 40, int $h = 30, string $extra = ''): string
    {
        $im = imagecreatetruecolor($w, $h);
        imagefill($im, 0, 0, imagecolorallocate($im, 10, 120, 200));
        ob_start();
        imagepng($im);
        return (string) ob_get_clean() . $extra;
    }

    public static function jpeg(int $w = 40, int $h = 30): string
    {
        $im = imagecreatetruecolor($w, $h);
        ob_start();
        imagejpeg($im);
        return (string) ob_get_clean();
    }

    public static function noisePng(int $w, int $h): string
    {
        $im = imagecreatetruecolor($w, $h);
        for ($i = 0; $i < $w * $h / 4; $i++) {
            imagesetpixel($im, random_int(0, $w - 1), random_int(0, $h - 1), random_int(0, 0xFFFFFF));
        }
        ob_start();
        imagepng($im, null, 0);
        return (string) ob_get_clean();
    }
}
