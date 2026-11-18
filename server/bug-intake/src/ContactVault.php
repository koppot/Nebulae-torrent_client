<?php
declare(strict_types=1);

namespace NebulaeIntake;

/**
 * Restricted storage for the optional reporter contact e-mail.
 *
 * - Stored encrypted (libsodium secretbox) in its own table, keyed by report id.
 * - The key lives in a separate root-controlled file, never in the database or env file.
 * - Retrieval is only possible through the CLI tool for maintainers listed in MAINTAINERS_FILE,
 *   and every access is logged (maintainer, report ref, action, reason) without the address.
 * - Nothing here is reachable from the HTTP entry point.
 */
final class ContactVault
{
    public function __construct(private Db $db, private Config $config)
    {
    }

    public static function normalize(string $email): ?string
    {
        $email = trim($email);
        if ($email === '' || strlen($email) > 254 || preg_match('/[\x00-\x1F\x7F\s]/', $email)) {
            return null;
        }
        return filter_var($email, FILTER_VALIDATE_EMAIL) === false ? null : $email;
    }

    public function store(int $reportId, string $email): void
    {
        $key = $this->key();
        $nonce = random_bytes(SODIUM_CRYPTO_SECRETBOX_NONCEBYTES);
        $cipher = sodium_crypto_secretbox($email, $nonce, $key);
        $this->db->run(
            'INSERT INTO contacts (report_id, email_enc, created_at) VALUES (?, ?, ?)',
            [$reportId, base64_encode($nonce . $cipher), Db::now()]
        );
        sodium_memzero($key);
    }

    public function has(int $reportId): bool
    {
        $r = $this->db->one('SELECT purged_at FROM contacts WHERE report_id = ?', [$reportId]);
        return $r !== null && $r['purged_at'] === null;
    }

    /** @return string|null decrypted address; caller must be an authorized maintainer */
    public function reveal(string $reportRef, string $maintainer, string $reason): ?string
    {
        if (PHP_SAPI !== 'cli') {
            throw new \RuntimeException('contact retrieval is CLI-only');
        }
        if (!$this->isAuthorized($maintainer)) {
            $this->audit($maintainer, $reportRef, 'denied', $reason);
            throw new \RuntimeException('maintainer is not authorized');
        }
        if (trim($reason) === '') {
            throw new \RuntimeException('a reason is required');
        }
        $row = $this->db->one(
            'SELECT c.email_enc, c.purged_at FROM contacts c JOIN reports r ON r.id = c.report_id WHERE r.report_ref = ?',
            [$reportRef]
        );
        $this->audit($maintainer, $reportRef, 'view', $reason);
        if ($row === null || $row['purged_at'] !== null) {
            return null;
        }
        $raw = base64_decode((string) $row['email_enc'], true);
        $key = $this->key();
        $nonceLen = SODIUM_CRYPTO_SECRETBOX_NONCEBYTES;
        $plain = $raw === false ? false : sodium_crypto_secretbox_open(substr($raw, $nonceLen), substr($raw, 0, $nonceLen), $key);
        sodium_memzero($key);
        return $plain === false ? null : $plain;
    }

    public function isAuthorized(string $maintainer): bool
    {
        $file = $this->config->str('MAINTAINERS_FILE');
        if ($maintainer === '' || !is_readable($file)) {
            return false;
        }
        $lines = array_map('trim', file($file, FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) ?: []);
        return in_array($maintainer, array_filter($lines, fn ($l) => $l !== '' && $l[0] !== '#'), true);
    }

    /** Deletes ciphertext older than RETENTION_CONTACT_DAYS (or for one report). Returns rows purged. */
    public function purge(?string $reportRef = null, ?string $maintainer = null, string $reason = 'retention'): int
    {
        if ($reportRef !== null) {
            $st = $this->db->run(
                "UPDATE contacts SET email_enc = '', purged_at = ? WHERE purged_at IS NULL AND report_id IN (SELECT id FROM reports WHERE report_ref = ?)",
                [Db::now(), $reportRef]
            );
            $this->audit($maintainer ?? 'system', $reportRef, 'purge', $reason);
            return $st->rowCount();
        }
        $cutoff = Db::now(-86400 * max(1, $this->config->int('RETENTION_CONTACT_DAYS')));
        $st = $this->db->run("UPDATE contacts SET email_enc = '', purged_at = ? WHERE purged_at IS NULL AND created_at < ?", [Db::now(), $cutoff]);
        if ($st->rowCount() > 0) {
            $this->audit('system', '-', 'purge', 'retention expiry');
        }
        return $st->rowCount();
    }

    private function audit(string $maintainer, string $ref, string $action, string $reason): void
    {
        $this->db->run(
            'INSERT INTO contact_access_log (at, maintainer, report_ref, action, reason) VALUES (?, ?, ?, ?, ?)',
            [Db::now(), mb_substr($maintainer, 0, 64), mb_substr($ref, 0, 16), $action, mb_substr(Redactor::redact($reason), 0, 200)]
        );
    }

    private function key(): string
    {
        $file = $this->config->str('CONTACT_KEY_FILE');
        $raw = is_readable($file) ? trim((string) file_get_contents($file)) : '';
        $key = base64_decode($raw, true);
        if ($key === false || strlen($key) !== SODIUM_CRYPTO_SECRETBOX_KEYBYTES) {
            throw new \RuntimeException('contact encryption key is missing or invalid');
        }
        return $key;
    }
}
