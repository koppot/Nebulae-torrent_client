<?php
declare(strict_types=1);

namespace NebulaeIntake;

final class Intake
{
    private const ALPHABET = 'ABCDEFGHJKMNPQRSTVWXYZ23456789';

    public function __construct(
        private Db $db,
        private Config $config,
        private ImageProcessor $images,
        private ContactVault $vault,
        private Log $log,
    ) {
    }

    public function enabled(): bool
    {
        return !$this->config->bool('INTAKE_DISABLED') && !is_file($this->config->storageDir() . '/DISABLED');
    }

    public function receiptFor(string $submissionId): string
    {
        $mac = hash_hmac('sha256', 'receipt:' . $submissionId, $this->config->str('RECEIPT_HMAC_KEY'), true);
        return rtrim(strtr(base64_encode($mac), '+/', '-_'), '=');
    }

    private function receiptHash(string $token): string
    {
        return hash_hmac('sha256', 'lookup:' . $token, $this->config->str('RECEIPT_HMAC_KEY'));
    }

    /**
     * @param array<string,mixed> $fields POST fields
     * @param list<array{tmp_name:string,error:int,size:int}> $files uploaded screenshot files
     * @return array{0:int,1:array<string,mixed>,2?:array<string,string>} status, body, extra headers
     */
    public function submit(array $fields, array $files, string $clientIp, ?string $idempotencyKey): array
    {
        if (!$this->enabled()) {
            return [503, ['error' => 'intake_disabled', 'message' => 'Bug reporting is temporarily unavailable. Your draft stays on your computer.'], ['Retry-After' => '3600']];
        }
        try {
            $raw = $fields['report'] ?? null;
            if (!is_string($raw) || strlen($raw) > 131072) {
                throw new ValidationError('invalid_report', 'The report body is missing or too large.');
            }
            $decoded = json_decode($raw, true, 8);
            if (!is_array($decoded)) {
                throw new ValidationError('invalid_report', 'The report body is not valid JSON.');
            }
            $v = Validator::report($decoded);
            if ($idempotencyKey !== null && $idempotencyKey !== $v['submission_id']) {
                throw new ValidationError('idempotency_mismatch', 'Idempotency key does not match the submission.');
            }
            $existing = $this->db->one('SELECT report_ref FROM reports WHERE submission_id = ?', [$v['submission_id']]);
            if ($existing !== null) {
                return [200, $this->receipt($existing['report_ref'], $v['submission_id'], true)];
            }
            $this->admission($clientIp);
            $prepared = $this->prepareFiles($files);
        } catch (ValidationError $e) {
            return [$e->httpStatus, ['error' => $e->errorCode, 'message' => $e->getMessage()], $e->httpStatus === 429 ? ['Retry-After' => '600'] : []];
        }

        $written = [];
        try {
            foreach ($prepared as $i => $img) {
                $written[$i] = $this->writeBlob($img['bytes']);
            }
            $ref = $this->db->transaction(function () use ($v, $prepared, $written, $clientIp): string {
                $ref = $this->newRef();
                $p = $v['payload'];
                $dup = $p['error_fingerprint'] !== ''
                    ? $this->db->one('SELECT report_ref FROM reports WHERE error_fingerprint = ? ORDER BY id DESC LIMIT 1', [$p['error_fingerprint']])
                    : null;
                $now = Db::now();
                $this->db->run(
                    'INSERT INTO reports (report_ref, submission_id, receipt_hash, category, error_fingerprint, payload_json, ip_hash, possible_duplicate_of, created_at) VALUES (?,?,?,?,?,?,?,?,?)',
                    [$ref, $v['submission_id'], $this->receiptHash($this->receiptFor($v['submission_id'])), $p['category'], $p['error_fingerprint'],
                        json_encode($p, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR), $this->ipHash($clientIp), $dup['report_ref'] ?? null, $now]
                );
                $id = (int) $this->db->pdo->lastInsertId();
                foreach ($prepared as $i => $img) {
                    $this->db->run(
                        'INSERT INTO attachments (report_id, idx, public_name, blob_path, mime, bytes, width, height, sha256) VALUES (?,?,?,?,?,?,?,?,?)',
                        [$id, $i, 'screenshot-' . ($i + 1) . '.' . ['image/png' => 'png', 'image/jpeg' => 'jpg', 'image/webp' => 'webp'][$img['mime']],
                            $written[$i], $img['mime'], strlen($img['bytes']), $img['width'], $img['height'], hash('sha256', $img['bytes'])]
                    );
                }
                if ($v['contact'] !== null) {
                    $this->vault->store($id, $v['contact']);
                }
                Outbox::enqueue($this->db, $id, 'linear');
                Outbox::enqueue($this->db, $id, 'discord');
                $this->db->run('INSERT INTO rate_events (bucket, at_ts) VALUES (?, ?)', ['ip:' . $this->ipHash($clientIp), time()]);
                $this->db->run('INSERT INTO rate_events (bucket, at_ts) VALUES (?, ?)', ['global', time()]);
                return $ref;
            });
        } catch (\PDOException $e) {
            $this->removeBlobs($written);
            $dupe = $this->db->one('SELECT report_ref FROM reports WHERE submission_id = ?', [$v['submission_id']]);
            if ($dupe !== null) {
                return [200, $this->receipt($dupe['report_ref'], $v['submission_id'], true)];
            }
            $this->log->error('store_failed', ['class' => 'PDOException']);
            return [503, ['error' => 'storage_error', 'message' => 'The report could not be stored. Nothing was received; please retry later.'], ['Retry-After' => '300']];
        } catch (\Throwable $e) {
            $this->removeBlobs($written);
            $this->log->error('store_failed', ['class' => get_class($e)]);
            return [503, ['error' => 'storage_error', 'message' => 'The report could not be stored. Nothing was received; please retry later.'], ['Retry-After' => '300']];
        }
        $this->log->info('received', ['ref' => $ref, 'screenshots' => count($prepared)]);
        return [201, $this->receipt($ref, $v['submission_id'], false)];
    }

    /** @return array<string,mixed> */
    private function receipt(string $ref, string $submissionId, bool $replay): array
    {
        return ['status' => 'received', 'report_ref' => $ref, 'receipt_token' => $this->receiptFor($submissionId), 'replay' => $replay];
    }

    /** @return array{0:int,1:array<string,mixed>} */
    public function status(string $token, string $clientIp): array
    {
        if (!$this->rateOk('status:' . $this->ipHash($clientIp), 3600, 60)) {
            return [429, ['error' => 'rate_limited', 'message' => 'Too many requests.']];
        }
        $this->db->run('INSERT INTO rate_events (bucket, at_ts) VALUES (?, ?)', ['status:' . $this->ipHash($clientIp), time()]);
        $r = $this->db->one('SELECT id, report_ref, created_at FROM reports WHERE receipt_hash = ?', [$this->receiptHash($token)]);
        if ($r === null) {
            return [404, ['error' => 'not_found', 'message' => 'Unknown receipt.']];
        }
        return [200, [
            'report_ref' => $r['report_ref'],
            'received_at' => str_replace(' ', 'T', (string) $r['created_at']) . 'Z',
            'linear' => Outbox::stateFor($this->db, (int) $r['id'], 'linear'),
            'discord' => Outbox::stateFor($this->db, (int) $r['id'], 'discord'),
        ]];
    }

    private function admission(string $ip): void
    {
        $key = 'ip:' . $this->ipHash($ip);
        if (!$this->rateOk($key, 3600, $this->config->int('RATE_PER_IP_HOUR')) || !$this->rateOk($key, 86400, $this->config->int('RATE_PER_IP_DAY'))) {
            throw new ValidationError('rate_limited', 'Too many reports from this network. Please try again later.', 429);
        }
        if (!$this->rateOk('global', 86400, $this->config->int('RATE_GLOBAL_DAY'))) {
            throw new ValidationError('intake_busy', 'Bug reporting is busy. Please try again later.', 503);
        }
        $open = $this->db->one("SELECT COUNT(DISTINCT report_id) AS n FROM jobs WHERE state <> 'done'");
        if ((int) ($open['n'] ?? 0) >= $this->config->int('MAX_OPEN_REPORTS')) {
            throw new ValidationError('intake_busy', 'Bug reporting is busy. Please try again later.', 503);
        }
    }

    private function rateOk(string $bucket, int $window, int $max): bool
    {
        $r = $this->db->one('SELECT COUNT(*) AS n FROM rate_events WHERE bucket = ? AND at_ts > ?', [$bucket, time() - $window]);
        return (int) ($r['n'] ?? 0) < $max;
    }

    private function ipHash(string $ip): string
    {
        return hash_hmac('sha256', $ip, $this->config->str('IP_HASH_KEY'));
    }

    /**
     * @param list<array{tmp_name:string,error:int,size:int}> $files
     * @return list<array{bytes:string,mime:string,width:int,height:int}>
     */
    private function prepareFiles(array $files): array
    {
        if (count($files) > $this->config->int('MAX_SCREENSHOTS')) {
            throw new ValidationError('too_many_screenshots', 'At most ' . $this->config->int('MAX_SCREENSHOTS') . ' screenshots are allowed.', 413);
        }
        $total = 0;
        foreach ($files as $f) {
            if ($f['error'] !== UPLOAD_ERR_OK) {
                throw new ValidationError('upload_failed', 'A screenshot upload failed or exceeded the size limit.', $f['error'] === UPLOAD_ERR_INI_SIZE || $f['error'] === UPLOAD_ERR_FORM_SIZE ? 413 : 400);
            }
            $total += $f['size'];
        }
        if ($total > $this->config->int('MAX_TOTAL_BYTES')) {
            throw new ValidationError('screenshots_too_large', 'Screenshots exceed the combined size limit of ' . intdiv($this->config->int('MAX_TOTAL_BYTES'), 1048576) . ' MiB.', 413);
        }
        if ($this->storageUsed() + $total > $this->config->int('STORAGE_QUOTA_BYTES')
            || (@disk_free_space($this->config->storageDir()) !== false && disk_free_space($this->config->storageDir()) < $total * 3 + 104857600)) {
            throw new ValidationError('storage_full', 'The report service cannot accept attachments right now. Try again without screenshots or later.', 503);
        }
        $out = [];
        set_time_limit(60);
        foreach ($files as $f) {
            $raw = file_get_contents($f['tmp_name']);
            if ($raw === false) {
                throw new ValidationError('upload_failed', 'A screenshot could not be read.', 400);
            }
            $out[] = $this->images->sanitize($raw);
        }
        return $out;
    }

    public function storageUsed(): int
    {
        $r = $this->db->one('SELECT COALESCE(SUM(bytes), 0) AS n FROM attachments WHERE purged_at IS NULL');
        return (int) ($r['n'] ?? 0);
    }

    private function writeBlob(string $bytes): string
    {
        $dir = $this->config->storageDir() . '/blobs';
        $name = bin2hex(random_bytes(16));
        $sub = substr($name, 0, 2);
        if (!is_dir("$dir/$sub") && !@mkdir("$dir/$sub", 0750, true) && !is_dir("$dir/$sub")) {
            throw new \RuntimeException('cannot create blob directory');
        }
        $final = "$dir/$sub/$name.bin";
        $tmp = $final . '.tmp';
        $fh = fopen($tmp, 'xb');
        if ($fh === false) {
            throw new \RuntimeException('cannot create blob');
        }
        $ok = fwrite($fh, $bytes) === strlen($bytes) && fflush($fh);
        if (function_exists('fsync')) {
            $ok = $ok && fsync($fh);
        }
        fclose($fh);
        if (!$ok || !rename($tmp, $final)) {
            @unlink($tmp);
            throw new \RuntimeException('cannot persist blob');
        }
        @chmod($final, 0640);
        return "blobs/$sub/$name.bin";
    }

    /** @param array<int,string> $paths */
    private function removeBlobs(array $paths): void
    {
        foreach ($paths as $p) {
            @unlink($this->config->storageDir() . '/' . $p);
        }
    }

    private function newRef(): string
    {
        for ($i = 0; $i < 10; $i++) {
            $s = '';
            for ($j = 0; $j < 8; $j++) {
                $s .= self::ALPHABET[random_int(0, strlen(self::ALPHABET) - 1)];
            }
            $ref = "NBR-$s";
            if ($this->db->one('SELECT id FROM reports WHERE report_ref = ?', [$ref]) === null) {
                return $ref;
            }
        }
        throw new \RuntimeException('could not allocate a report reference');
    }
}
