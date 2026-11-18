<?php
declare(strict_types=1);

namespace NebulaeIntake;

final class LinearClient
{
    private const ENDPOINT = 'https://api.linear.app/graphql';

    public function __construct(private Config $config, private Transport $http)
    {
    }

    /**
     * @param array<string,mixed> $vars
     * @return array<string,mixed> the `data` object
     */
    public function gql(string $query, array $vars): array
    {
        $key = $this->config->str('LINEAR_API_KEY');
        if ($key === '') {
            throw new DeliveryError('Linear API key is not configured', true, false);
        }
        $auth = str_starts_with($key, 'lin_oauth_') ? "Bearer $key" : $key;
        $resp = $this->http->send('POST', self::ENDPOINT, ['Authorization' => $auth, 'Content-Type' => 'application/json'],
            json_encode(['query' => $query, 'variables' => $vars], JSON_UNESCAPED_SLASHES), 30);
        if ($resp->error !== null) {
            throw new DeliveryError('Linear request failed: ' . $resp->error, true, $resp->requestSent);
        }
        $json = json_decode($resp->body, true);
        $errors = is_array($json) ? ($json['errors'] ?? []) : [];
        $codes = [];
        foreach ($errors as $e) {
            $codes[] = (string) ($e['extensions']['code'] ?? '');
        }
        if ($resp->status === 429 || in_array('RATELIMITED', $codes, true)) {
            throw new DeliveryError('Linear rate limit', true, false, $resp->retryAfter() ?? 60);
        }
        if ($resp->status === 401 || $resp->status === 403 || in_array('AUTHENTICATION_ERROR', $codes, true)) {
            throw new DeliveryError('Linear rejected the credential (HTTP ' . $resp->status . ')', true, false, 1800);
        }
        if ($resp->status >= 500) {
            throw new DeliveryError('Linear server error HTTP ' . $resp->status, true, true);
        }
        if ($errors !== [] || !is_array($json) || !isset($json['data'])) {
            $msg = $errors !== [] ? (string) ($errors[0]['message'] ?? 'GraphQL error') : 'unexpected response';
            throw new DeliveryError('Linear error: ' . $msg, true, false, 300);
        }
        return $json['data'];
    }

    /** @return array{id:string,identifier:string,url:string}|null */
    public function findIssueByRef(string $ref): ?array
    {
        $d = $this->gql(
            'query($ref:String!,$team:ID!){ issues(filter:{team:{id:{eq:$team}},title:{contains:$ref}}, first:2){ nodes{ id identifier url } } }',
            ['ref' => $ref, 'team' => $this->config->str('LINEAR_TEAM_ID')]
        );
        $n = $d['issues']['nodes'] ?? [];
        return $n === [] ? null : ['id' => $n[0]['id'], 'identifier' => $n[0]['identifier'], 'url' => $n[0]['url']];
    }

    /** @return array{id:string,identifier:string,url:string} */
    public function createIssue(string $title, string $description): array
    {
        $input = [
            'teamId' => $this->config->str('LINEAR_TEAM_ID'),
            'title' => $title,
            'description' => $description,
        ];
        if ($this->config->str('LINEAR_PROJECT_ID') !== '') {
            $input['projectId'] = $this->config->str('LINEAR_PROJECT_ID');
        }
        if ($this->config->str('LINEAR_INTAKE_STATE_ID') !== '') {
            $input['stateId'] = $this->config->str('LINEAR_INTAKE_STATE_ID');
        }
        if ($this->config->list('LINEAR_LABEL_IDS') !== []) {
            $input['labelIds'] = $this->config->list('LINEAR_LABEL_IDS');
        }
        $d = $this->gql('mutation($input:IssueCreateInput!){ issueCreate(input:$input){ success issue{ id identifier url } } }', ['input' => $input]);
        $i = $d['issueCreate']['issue'] ?? null;
        if (!($d['issueCreate']['success'] ?? false) || !is_array($i)) {
            throw new DeliveryError('Linear did not confirm issue creation', true, true);
        }
        return ['id' => $i['id'], 'identifier' => $i['identifier'], 'url' => $i['url']];
    }

    public function updateDescription(string $issueId, string $description): void
    {
        $d = $this->gql('mutation($id:String!,$input:IssueUpdateInput!){ issueUpdate(id:$id,input:$input){ success } }',
            ['id' => $issueId, 'input' => ['description' => $description]]);
        if (!($d['issueUpdate']['success'] ?? false)) {
            throw new DeliveryError('Linear did not confirm the issue update', true, false);
        }
    }

    /** Uploads bytes to Linear's private storage and returns the asset URL. */
    public function uploadFile(string $filename, string $mime, string $bytes): string
    {
        $d = $this->gql(
            'mutation($contentType:String!,$filename:String!,$size:Int!){ fileUpload(contentType:$contentType, filename:$filename, size:$size, makePublic:false){ success uploadFile{ uploadUrl assetUrl headers{ key value } } } }',
            ['contentType' => $mime, 'filename' => $filename, 'size' => strlen($bytes)]
        );
        $u = $d['fileUpload']['uploadFile'] ?? null;
        if (!($d['fileUpload']['success'] ?? false) || !is_array($u) || !str_starts_with((string) $u['uploadUrl'], 'https://')) {
            throw new DeliveryError('Linear did not provide an upload slot', true, false);
        }
        $headers = ['Content-Type' => $mime, 'Cache-Control' => 'public, max-age=31536000'];
        foreach ($u['headers'] ?? [] as $h) {
            $headers[(string) $h['key']] = (string) $h['value'];
        }
        $put = $this->http->send('PUT', (string) $u['uploadUrl'], $headers, $bytes, 120);
        if (!$put->ok()) {
            throw new DeliveryError('Screenshot upload to Linear storage failed (HTTP ' . $put->status . ')', true, false, $put->retryAfter());
        }
        return (string) $u['assetUrl'];
    }
}
