<?php
declare(strict_types=1);

namespace NebulaeIntake;

/**
 * Server-side second pass over untrusted text. The client already redacts;
 * this is defence in depth and must never be the only line of protection.
 */
final class Redactor
{
    public static function redact(string $text): string
    {
        $rules = [
            // Discord webhook URLs
            '~https?://(?:[a-z]+\.)?discord(?:app)?\.com/api/(?:v\d+/)?webhooks/\d+/[A-Za-z0-9_\-]+~i' => '[REDACTED_DISCORD_WEBHOOK]',
            // Tracker announce/scrape URLs (contain passkeys)
            '~https?://[^\s"\'<>]+/(?:announce|scrape)(?:\.php)?[^\s"\'<>]*~i' => '[REDACTED_TRACKER_URL]',
            // Credentials in query strings or key=value pairs
            '~(?<![A-Za-z])((?:pass[_-]?key|torrent[_-]?pass|auth[_-]?key|api[_-]?key|apikey|access[_-]?token|refresh[_-]?token|delivery[_-]?token|csrf[_-]?token|session[_-]?id|secret|password|passwd|token|credential)s?)(["\']?\s*[:=]\s*)(?:"[^"\r\n]*"|\'[^\'\r\n]*\'|[^\s&,;"\'<>]+)~i' => '$1$2[REDACTED]',
            // HTTP headers
            '~^(\s*(?:authorization|proxy-authorization|cookie|set-cookie|x-api-key|x-transmission-session-id)\s*:).*$~im' => '$1 [REDACTED]',
            '~\b(Bearer|Basic)\s+[A-Za-z0-9._\~+/=\-]{8,}~i' => '$1 [REDACTED]',
            // Well-known token formats
            '~\blin_(?:api|oauth)_[A-Za-z0-9]{20,}~' => '[REDACTED_LINEAR_KEY]',
            '~\bgh[pousr]_[A-Za-z0-9]{20,}~' => '[REDACTED_GITHUB_TOKEN]',
            '~\bgithub_pat_[A-Za-z0-9_]{20,}~' => '[REDACTED_GITHUB_TOKEN]',
            '~\bxox[abprs]-[A-Za-z0-9\-]{10,}~' => '[REDACTED_SLACK_TOKEN]',
            '~\b(?:secret|ntn)_[A-Za-z0-9]{30,}~' => '[REDACTED_NOTION_TOKEN]',
            '~\bAKIA[0-9A-Z]{16}\b~' => '[REDACTED_AWS_KEY]',
            '~-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----~s' => '[REDACTED_PRIVATE_KEY]',
            '~\buntrusted comment:[^\r\n]*\r?\n[A-Za-z0-9+/=]{40,}~' => '[REDACTED_SIGNATURE]',
            // Personal paths
            '~(/Users/)[^/\s"\']+~' => '$1[USER]',
            '~(/home/)[^/\s"\']+~' => '$1[USER]',
            '~([A-Za-z]:\\\\Users\\\\)[^\\\\\s"\']+~i' => '$1[USER]',
            // E-mail addresses typed into free text
            '~[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}~' => '[REDACTED_EMAIL]',
        ];
        foreach ($rules as $pattern => $replacement) {
            $out = preg_replace($pattern, $replacement, $text);
            if ($out !== null) {
                $text = $out;
            }
        }
        return $text;
    }
}
