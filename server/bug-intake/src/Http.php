<?php
declare(strict_types=1);

namespace NebulaeIntake;

final class HttpResponse
{
    /** @param array<string,string> $headers lower-cased names */
    public function __construct(
        public readonly int $status,
        public readonly array $headers = [],
        public readonly string $body = '',
        public readonly ?string $error = null,
        /** false when the request provably never left this host (DNS/connect failure) */
        public readonly bool $requestSent = true,
    ) {
    }

    public function ok(): bool
    {
        return $this->error === null && $this->status >= 200 && $this->status < 300;
    }

    public function retryAfter(): ?int
    {
        $h = $this->headers['retry-after'] ?? null;
        return $h !== null && ctype_digit($h) ? (int) $h : null;
    }
}

interface Transport
{
    /**
     * @param array<string,string> $headers
     * @param string|array<string,mixed>|null $body string, or array for multipart (CURLFile values allowed)
     */
    public function send(string $method, string $url, array $headers, string|array|null $body = null, int $timeout = 30): HttpResponse;
}

final class CurlTransport implements Transport
{
    public function send(string $method, string $url, array $headers, string|array|null $body = null, int $timeout = 30): HttpResponse
    {
        $ch = curl_init($url);
        $hdrs = [];
        foreach ($headers as $k => $v) {
            $hdrs[] = "$k: $v";
        }
        $respHeaders = [];
        curl_setopt_array($ch, [
            CURLOPT_CUSTOMREQUEST => $method,
            CURLOPT_RETURNTRANSFER => true,
            CURLOPT_HTTPHEADER => $hdrs,
            CURLOPT_TIMEOUT => $timeout,
            CURLOPT_CONNECTTIMEOUT => 10,
            CURLOPT_FOLLOWLOCATION => false,
            CURLOPT_PROTOCOLS => CURLPROTO_HTTPS,
            CURLOPT_HEADERFUNCTION => function ($c, $line) use (&$respHeaders) {
                if (str_contains($line, ':')) {
                    [$k, $v] = explode(':', $line, 2);
                    $respHeaders[strtolower(trim($k))] = trim($v);
                }
                return strlen($line);
            },
        ]);
        if ($body !== null) {
            curl_setopt($ch, CURLOPT_POSTFIELDS, $body);
        }
        $raw = curl_exec($ch);
        $errno = curl_errno($ch);
        $status = (int) curl_getinfo($ch, CURLINFO_RESPONSE_CODE);
        $err = $errno !== 0 ? 'transport error ' . $errno : null;
        $neverSent = in_array($errno, [CURLE_COULDNT_RESOLVE_HOST, CURLE_COULDNT_CONNECT, CURLE_SSL_CONNECT_ERROR], true);
        curl_close($ch);
        return new HttpResponse($status, $respHeaders, is_string($raw) ? $raw : '', $err, !$neverSent);
    }
}
