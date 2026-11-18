<?php
declare(strict_types=1);

namespace NebulaeIntake;

/** Operational log. Never receives report text, screenshots, e-mail addresses or credentials. */
final class Log
{
    public function __construct(private ?string $file = null)
    {
    }

    /** @param array<string,scalar|null> $ctx */
    public function info(string $event, array $ctx = []): void
    {
        $this->write('info', $event, $ctx);
    }

    /** @param array<string,scalar|null> $ctx */
    public function error(string $event, array $ctx = []): void
    {
        $this->write('error', $event, $ctx);
    }

    /** @param array<string,scalar|null> $ctx */
    private function write(string $level, string $event, array $ctx): void
    {
        if ($this->file === null || $this->file === '') {
            return;
        }
        $line = json_encode(['t' => gmdate('c'), 'level' => $level, 'event' => $event] + $ctx, JSON_UNESCAPED_SLASHES);
        @file_put_contents($this->file, $line . "\n", FILE_APPEND | LOCK_EX);
    }

    public static function safeError(string $message): string
    {
        $message = Redactor::redact($message);
        $message = preg_replace('~https?://[^\s"\']+~i', '[url]', $message) ?? $message;
        return mb_substr($message, 0, 300);
    }
}
