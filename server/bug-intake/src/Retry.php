<?php
declare(strict_types=1);

namespace NebulaeIntake;

final class Retry
{
    /** Exponential backoff with jitter, capped at 6 hours. */
    public static function delaySeconds(int $attempts, ?int $retryAfter = null): int
    {
        $base = min(21600, 30 * (2 ** max(0, min($attempts, 12))));
        $jitter = random_int(0, (int) ($base * 0.2));
        $delay = $base + $jitter;
        if ($retryAfter !== null && $retryAfter > $delay) {
            $delay = min(21600, $retryAfter + random_int(1, 5));
        }
        return $delay;
    }
}
