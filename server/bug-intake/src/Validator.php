<?php
declare(strict_types=1);

namespace NebulaeIntake;

/**
 * Turns the untrusted client report into an allowlisted payload.
 * Only the fields built here can ever reach Linear or Discord; the contact e-mail is returned
 * separately and is never part of the payload.
 */
final class Validator
{
    public const CATEGORIES = [
        'Playback & Media Center',
        'Torrent Engine & Transfers',
        'Tracker Integration & Swarms',
        'Windows UI & Window Controls',
        'Performance & Responsiveness',
        'Other',
    ];

    private const TEXT_LIMITS = [
        'summary' => [1, 120],
        'description' => [1, 8000],
        'steps' => [0, 4000],
        'expected' => [0, 2000],
        'actual' => [0, 2000],
        'diagnostics' => [0, 20000],
    ];

    /**
     * @param array<string,mixed> $in
     * @return array{payload:array<string,string>, contact:?string, submission_id:string}
     */
    public static function report(array $in): array
    {
        $out = [];
        foreach (self::TEXT_LIMITS as $field => [$min, $max]) {
            $out[$field] = self::text($in[$field] ?? '', $field, $min, $max);
        }
        $category = (string) ($in['category'] ?? 'Other');
        $out['category'] = in_array($category, self::CATEGORIES, true) ? $category : 'Other';

        foreach (['app_version' => 32, 'build_id' => 64, 'arch' => 32] as $field => $max) {
            $v = (string) ($in[$field] ?? '');
            if ($v === '' || strlen($v) > $max || !preg_match('/^[0-9A-Za-z._+\-]+$/', $v)) {
                throw new ValidationError('invalid_' . $field, "Invalid $field.");
            }
            $out[$field] = $v;
        }
        $out['os'] = self::text($in['os'] ?? '', 'os', 1, 64);
        $out['os_version'] = self::text($in['os_version'] ?? '', 'os_version', 0, 128);
        $fp = (string) ($in['error_fingerprint'] ?? '');
        $out['error_fingerprint'] = preg_match('/^[0-9A-Za-z\-]{0,64}$/', $fp) ? $fp : '';

        $sid = (string) ($in['submission_id'] ?? '');
        if (!preg_match('/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/', $sid)) {
            throw new ValidationError('invalid_submission_id', 'A valid submission identifier is required.');
        }

        $contact = null;
        $rawEmail = trim((string) ($in['contact_email'] ?? ''));
        if ($rawEmail !== '') {
            $contact = ContactVault::normalize($rawEmail);
            if ($contact === null) {
                throw new ValidationError('invalid_contact_email', 'The contact e-mail address is not valid.');
            }
        }
        return ['payload' => $out, 'contact' => $contact, 'submission_id' => $sid];
    }

    private static function text(mixed $value, string $field, int $min, int $max): string
    {
        if (!is_string($value)) {
            throw new ValidationError('invalid_' . $field, "Invalid $field.");
        }
        if (!mb_check_encoding($value, 'UTF-8')) {
            throw new ValidationError('invalid_' . $field, "$field is not valid UTF-8.");
        }
        $value = str_replace("\r\n", "\n", $value);
        $value = preg_replace('/[\x00-\x08\x0B\x0C\x0E-\x1F\x7F]/u', '', $value) ?? '';
        $value = trim($value);
        if (mb_strlen($value) > $max) {
            throw new ValidationError('too_long_' . $field, "$field is longer than $max characters.", 413);
        }
        if (mb_strlen($value) < $min) {
            throw new ValidationError('missing_' . $field, "$field is required.");
        }
        return Redactor::redact($value);
    }
}
