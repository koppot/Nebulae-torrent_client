<?php
declare(strict_types=1);

namespace NebulaeIntake;

/**
 * Builds the outgoing Linear and Discord content from explicit allowlists.
 * The intake record is never serialized wholesale, and the contact e-mail is not an input here.
 */
final class Formatter
{
    /** Markdown code fence longer than any backtick run inside $text, so user text cannot break out. */
    public static function fence(string $text): string
    {
        $max = 2;
        if (preg_match_all('/`+/', $text, $m)) {
            foreach ($m[0] as $run) {
                $max = max($max, strlen($run));
            }
        }
        $f = str_repeat('`', $max + 1);
        return "$f\n$text\n$f";
    }

    public static function linearTitle(string $ref, array $p): string
    {
        $s = preg_replace('/\s+/', ' ', $p['summary']) ?? $p['summary'];
        return mb_substr("[$ref] $s", 0, 250);
    }

    /**
     * @param array<string,string> $p validated payload
     * @param list<array<string,mixed>> $attachments
     */
    public static function linearBody(string $ref, array $p, array $attachments, ?string $duplicateOf, ?string $discordLink): string
    {
        $o = [];
        $o[] = "**Report reference:** `$ref`";
        $o[] = '**Category:** ' . self::inline($p['category']);
        $o[] = '**App:** ' . self::inline($p['app_version'] . ' (build ' . $p['build_id'] . ')')
            . ' · **OS:** ' . self::inline(trim($p['os'] . ' ' . $p['os_version'])) . ' · **Arch:** ' . self::inline($p['arch']);
        if ($p['error_fingerprint'] !== '') {
            $o[] = '**Error fingerprint:** `' . $p['error_fingerprint'] . '`';
        }
        if ($duplicateOf !== null) {
            $o[] = "**Possible duplicate:** same error fingerprint as `$duplicateOf`. Not merged automatically; please triage.";
        }
        $o[] = '_Submitted through the in-app bug reporter. Reporter text is untrusted and is shown verbatim in code blocks._';
        foreach (['description' => 'Description', 'steps' => 'Steps to reproduce', 'expected' => 'Expected behavior', 'actual' => 'Actual behavior'] as $k => $label) {
            if ($p[$k] !== '') {
                $o[] = "## $label\n" . self::fence($p[$k]);
            }
        }
        if ($p['diagnostics'] !== '') {
            $o[] = "## Diagnostics (reporter-reviewed excerpt)\n" . self::fence($p['diagnostics']);
        }
        if ($attachments !== []) {
            $lines = [];
            foreach ($attachments as $a) {
                $n = (int) $a['idx'] + 1;
                $lines[] = !empty($a['linear_asset_url'])
                    ? '![Screenshot ' . $n . '](' . $a['linear_asset_url'] . ')'
                    : "Screenshot $n: upload pending (will be added to this issue automatically).";
            }
            $o[] = "## Screenshots\n_Reporter-selected images. Text redaction does not apply to image content._\n\n" . implode("\n\n", $lines);
        }
        if ($discordLink !== null) {
            $o[] = "**Discord notification:** $discordLink";
        }
        $o[] = "## Triage checklist\n- [ ] Reproduced\n- [ ] Root cause identified\n- [ ] Fix verified by the reporter or owner\n- [ ] Reporter follow-up done (contact data is held only in the intake backend)";
        return implode("\n\n", $o);
    }

    private static function inline(string $s): string
    {
        return '`' . str_replace('`', "'", $s) . '`';
    }

    /** Neutralize Discord markdown, mentions and links in untrusted text. */
    public static function discordEscape(string $s): string
    {
        $s = preg_replace('/([\\\\*_~`|>#\[\]()<])/u', '\\\\$1', $s) ?? $s;
        $s = str_replace('@', "@\u{200B}", $s);
        $s = preg_replace('~https?://~i', "$0\u{200B}", $s) ?? $s;
        return $s;
    }

    /**
     * @param array<string,string> $p
     * @return array<string,mixed> embed object
     */
    public static function discordEmbed(string $ref, array $p, int $screenshots, ?string $linearUrl, ?string $linearId): array
    {
        $excerpt = $p['description'];
        $truncated = false;
        if (mb_strlen($excerpt) > 1400) {
            $excerpt = mb_substr($excerpt, 0, 1400);
            $truncated = true;
        }
        $desc = self::discordEscape($excerpt);
        if ($truncated) {
            $desc .= "\n\n_Excerpt only; the full report is in Linear" . ($linearId ? " ($linearId)" : '') . '._';
        }
        $fields = [
            ['name' => 'Reference', 'value' => $ref, 'inline' => true],
            ['name' => 'App', 'value' => self::discordEscape(mb_substr($p['app_version'] . ' (build ' . $p['build_id'] . ')', 0, 200)), 'inline' => true],
            ['name' => 'OS', 'value' => self::discordEscape(mb_substr(trim($p['os'] . ' ' . $p['os_version']) . ' / ' . $p['arch'], 0, 200)), 'inline' => true],
            ['name' => 'Category', 'value' => self::discordEscape($p['category']), 'inline' => true],
            ['name' => 'Screenshots', 'value' => (string) $screenshots, 'inline' => true],
            ['name' => 'Linear', 'value' => $linearUrl !== null ? "[$linearId]($linearUrl)" : 'Pending (will be linked here)', 'inline' => true],
        ];
        return [
            'title' => mb_substr("$ref · " . self::discordEscape($p['summary']), 0, 256),
            'description' => $desc,
            'color' => 0xE50914,
            'fields' => $fields,
            'footer' => ['text' => 'Untrusted user report. Contact data is not included.'],
        ];
    }
}
