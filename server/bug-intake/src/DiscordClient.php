<?php
declare(strict_types=1);

namespace NebulaeIntake;

final class DiscordClient
{
    public function __construct(private Config $config, private Transport $http)
    {
    }

    /**
     * Posts one message. `files` entries: ['name'=>string,'path'=>string,'mime'=>string].
     * Mentions are disabled for every message.
     *
     * @param array<string,mixed>|null $embed
     * @param list<array{name:string,path:string,mime:string}> $files
     * @return string created message id
     */
    public function post(?string $content, ?array $embed, array $files): string
    {
        $payload = ['allowed_mentions' => ['parse' => []]];
        if ($content !== null) {
            $payload['content'] = mb_substr($content, 0, 1900);
        }
        if ($embed !== null) {
            $payload['embeds'] = [$embed];
        }
        $fields = [];
        $attachments = [];
        foreach ($files as $i => $f) {
            $attachments[] = ['id' => $i, 'filename' => $f['name']];
            $fields["files[$i]"] = new \CURLFile($f['path'], $f['mime'], $f['name']);
        }
        if ($attachments !== []) {
            $payload['attachments'] = $attachments;
        }
        $fields['payload_json'] = json_encode($payload, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
        $resp = $this->http->send('POST', $this->url('?wait=true'), [], $fields, 60);
        $this->check($resp, true);
        $json = json_decode($resp->body, true);
        $id = is_array($json) ? (string) ($json['id'] ?? '') : '';
        if ($id === '') {
            throw new DeliveryError('Discord did not return a message id', true, true);
        }
        return $id;
    }

    /** @param array<string,mixed> $embed */
    public function editEmbed(string $messageId, array $embed): void
    {
        $body = json_encode(['embeds' => [$embed], 'allowed_mentions' => ['parse' => []]], JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
        $resp = $this->http->send('PATCH', $this->url('/messages/' . rawurlencode($messageId)), ['Content-Type' => 'application/json'], $body, 30);
        $this->check($resp, false);
    }

    private function url(string $suffix): string
    {
        $base = $this->config->str('DISCORD_BUGS_WEBHOOK_URL');
        if (!preg_match('~^https://(?:[a-z]+\.)?discord(?:app)?\.com/api/(?:v\d+/)?webhooks/\d+/[A-Za-z0-9_\-]+$~i', $base)) {
            throw new DeliveryError('Discord webhook is not configured', true, false, 1800);
        }
        return $base . $suffix;
    }

    private function check(HttpResponse $r, bool $isCreate): void
    {
        if ($r->error !== null) {
            throw new DeliveryError('Discord request failed: ' . $r->error, true, $r->requestSent && $isCreate);
        }
        if ($r->status === 429) {
            $j = json_decode($r->body, true);
            $ra = (int) ceil((float) ($j['retry_after'] ?? $r->retryAfter() ?? 5));
            throw new DeliveryError('Discord rate limit', true, false, max(1, $ra));
        }
        if ($r->status === 401 || $r->status === 403 || $r->status === 404) {
            throw new DeliveryError('Discord rejected the webhook (HTTP ' . $r->status . ')', true, false, 1800);
        }
        if ($r->status >= 500) {
            throw new DeliveryError('Discord server error HTTP ' . $r->status, true, $isCreate);
        }
        if (!$r->ok()) {
            throw new DeliveryError('Discord HTTP ' . $r->status, true, false, 300);
        }
    }
}
