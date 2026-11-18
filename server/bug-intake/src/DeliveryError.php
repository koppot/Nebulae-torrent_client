<?php
declare(strict_types=1);

namespace NebulaeIntake;

/** A failed delivery attempt. `ambiguous` means the destination may have processed the request. */
final class DeliveryError extends \RuntimeException
{
    public function __construct(
        string $message,
        public readonly bool $retryable = true,
        public readonly bool $ambiguous = false,
        public readonly ?int $retryAfter = null,
    ) {
        parent::__construct(Log::safeError($message));
    }
}
