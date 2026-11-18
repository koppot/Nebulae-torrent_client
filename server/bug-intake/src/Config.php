<?php
declare(strict_types=1);

namespace NebulaeIntake;

/**
 * Immutable configuration. Values come from an env-style file (default
 * /etc/nebulae-reports/env, outside any web root) or from an array in tests.
 */
final class Config
{
    private const DEFAULTS = [
        'DB_DSN' => 'mysql:host=127.0.0.1;dbname=nebulae_reports;charset=utf8mb4',
        'DB_USER' => '',
        'DB_PASS' => '',
        'STORAGE_DIR' => '/var/lib/nebulae-reports',
        'LOG_FILE' => '/var/log/nebulae-reports/intake.log',
        'INTAKE_DISABLED' => '0',
        'MAX_SCREENSHOTS' => '5',
        'MAX_SCREENSHOT_BYTES' => '10485760',
        'MAX_TOTAL_BYTES' => '26214400',
        'MAX_PIXELS' => '40000000',
        'MAX_DIMENSION' => '10000',
        'STORAGE_QUOTA_BYTES' => '21474836480',
        'RATE_PER_IP_HOUR' => '6',
        'RATE_PER_IP_DAY' => '20',
        'RATE_GLOBAL_DAY' => '500',
        'MAX_OPEN_REPORTS' => '2000',
        'LINEAR_API_KEY' => '',
        'LINEAR_TEAM_ID' => '',
        'LINEAR_PROJECT_ID' => '',
        'LINEAR_INTAKE_STATE_ID' => '',
        'LINEAR_LABEL_IDS' => '',
        'DISCORD_BUGS_WEBHOOK_URL' => '',
        'DISCORD_BUGS_CHANNEL_ID' => '',
        'DISCORD_GUILD_ID' => '',
        'DISCORD_MAX_MESSAGE_BYTES' => '8000000',
        'RECEIPT_HMAC_KEY' => '',
        'IP_HASH_KEY' => '',
        'RETENTION_ATTACHMENT_DAYS' => '30',
        'RETENTION_REPORT_DAYS' => '365',
        'RETENTION_STUCK_DAYS' => '14',
        'RETENTION_CONTACT_DAYS' => '90',
        'CONTACT_KEY_FILE' => '/etc/nebulae-reports/contact.key',
        'MAINTAINERS_FILE' => '/etc/nebulae-reports/maintainers',
        'ALERT_STUCK_MINUTES' => '120',
        'TRUSTED_PROXY_CIDRS' => '',
        'ALLOWED_ORIGINS' => 'tauri://localhost,http://tauri.localhost,https://tauri.localhost',
    ];

    /** @var array<string,string> */
    private array $values;

    /** @param array<string,string> $values */
    public function __construct(array $values = [])
    {
        $this->values = array_merge(self::DEFAULTS, array_map('strval', $values));
    }

    public static function fromEnvFile(?string $path = null): self
    {
        $path ??= getenv('NEBULAE_REPORTS_ENV') ?: '/etc/nebulae-reports/env';
        $values = [];
        if (is_readable($path)) {
            foreach (file($path, FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) ?: [] as $line) {
                $line = trim($line);
                if ($line === '' || $line[0] === '#' || !str_contains($line, '=')) {
                    continue;
                }
                [$k, $v] = explode('=', $line, 2);
                $v = trim($v);
                if (strlen($v) >= 2 && ($v[0] === '"' || $v[0] === "'") && $v[-1] === $v[0]) {
                    $v = substr($v, 1, -1);
                }
                $values[trim($k)] = $v;
            }
        }
        return new self($values);
    }

    public function str(string $key): string
    {
        return $this->values[$key] ?? '';
    }

    public function int(string $key): int
    {
        return (int) ($this->values[$key] ?? 0);
    }

    public function bool(string $key): bool
    {
        return in_array(strtolower($this->str($key)), ['1', 'true', 'yes', 'on'], true);
    }

    /** @return list<string> */
    public function list(string $key): array
    {
        return array_values(array_filter(array_map('trim', explode(',', $this->str($key))), fn ($s) => $s !== ''));
    }

    public function with(string $key, string $value): self
    {
        $copy = clone $this;
        $copy->values[$key] = $value;
        return $copy;
    }

    public function storageDir(): string
    {
        return rtrim($this->str('STORAGE_DIR'), '/');
    }
}
