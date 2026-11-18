<?php
declare(strict_types=1);

namespace NebulaeIntake;

/** Resolves the client address, trusting CF-Connecting-IP only when the TCP peer is a configured proxy. */
final class ClientIp
{
    /** @param list<string> $trustedCidrs */
    public static function resolve(string $remoteAddr, ?string $cfHeader, array $trustedCidrs): string
    {
        if ($cfHeader !== null && filter_var($cfHeader, FILTER_VALIDATE_IP) !== false) {
            foreach ($trustedCidrs as $cidr) {
                if (self::inCidr($remoteAddr, $cidr)) {
                    return $cfHeader;
                }
            }
        }
        return $remoteAddr;
    }

    public static function inCidr(string $ip, string $cidr): bool
    {
        [$net, $bits] = array_pad(explode('/', $cidr, 2), 2, null);
        $a = @inet_pton($ip);
        $b = @inet_pton((string) $net);
        if ($a === false || $b === false || strlen($a) !== strlen($b)) {
            return false;
        }
        $bits = $bits === null ? strlen($a) * 8 : (int) $bits;
        $full = intdiv($bits, 8);
        if (substr($a, 0, $full) !== substr($b, 0, $full)) {
            return false;
        }
        $rem = $bits % 8;
        if ($rem === 0) {
            return true;
        }
        $mask = (0xFF << (8 - $rem)) & 0xFF;
        return (ord($a[$full]) & $mask) === (ord($b[$full]) & $mask);
    }
}
