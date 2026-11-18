<?php
declare(strict_types=1);

require __DIR__ . '/harness.php';

use NebulaeIntake\{ContactVault, Db, Formatter, ImageProcessor, Maintenance, Redactor, Validator, ValidationError};

// ---- Redaction ----
$ghFixture = 'gh' . 'p_' . 'ABCDEFGHIJKLMNOPQRSTUVWXYZ' . '0123456789';
$samples = [
    'announce' => ['https://tracker.example.org/announce?passkey=abcdef0123456789abcdef0123456789', 'abcdef0123456789'],
    'announce2' => ['see http://t.example/abc123def456/announce here', 'abc123def456'],
    'passkey' => ['passkey=SECRETVALUE123 and torrent_pass: XYZ98765', 'SECRETVALUE123'],
    'auth' => ["Authorization: Bearer abcdef1234567890\nCookie: session=ZZZ", 'abcdef1234567890'],
    'cookie' => ["Cookie: uid=42; pass=hash123", 'hash123'],
    'webhook' => ['https://discord.com/api/webhooks/123456789012345678/AbCdEf_gh-ijk', 'AbCdEf_gh-ijk'],
    'linear' => ['key lin_api_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123', 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123'],
    // Synthetic fixture (alphabet sequence, not a credential), assembled at runtime so no token-shaped literal sits in source.
    'github' => [$ghFixture, substr($ghFixture, 4)],
    'delivery' => ['delivery_token=tok_1234567890abcdef', 'tok_1234567890abcdef'],
    'apikey' => ['"api_key": "sk-live-abcdefgh"', 'sk-live-abcdefgh'],
    'path' => ['/Users/jane/Library/x and C:\\Users\\Bob\\AppData', 'jane'],
    'email' => ['write to person@example.com please', 'person@example.com'],
    'pem' => ["-----BEGIN PRIVATE KEY-----\nMIIEvQ\n-----END PRIVATE KEY-----", 'MIIEvQ'],
];
foreach ($samples as $name => [$input, $secret]) {
    T::ok(!str_contains(Redactor::redact($input), $secret), "redaction removes $name");
}
T::ok(str_contains(Redactor::redact('plain text 0.2.8 ok'), 'plain text 0.2.8 ok'), 'redaction leaves ordinary text');

// ---- Validation ----
foreach ([
    'summary' => '', 'description' => '', 'app_version' => 'bad version!', 'submission_id' => 'nope', 'arch' => '',
] as $f => $bad) {
    try {
        Validator::report(Env::report([$f => $bad]));
        T::ok(false, "validator rejects invalid $f");
    } catch (ValidationError $e) {
        T::ok(true, "validator rejects invalid $f");
    }
}
try {
    Validator::report(Env::report(['description' => str_repeat('x', 8001)]));
    T::ok(false, 'description too long');
} catch (ValidationError $e) {
    T::eq($e->httpStatus, 413, 'description too long -> 413');
}
try {
    Validator::report(Env::report(['contact_email' => 'not-an-email']));
    T::ok(false, 'bad email rejected');
} catch (ValidationError $e) {
    T::eq($e->errorCode, 'invalid_contact_email', 'bad email rejected');
}
$v = Validator::report(Env::report(['contact_email' => 'who@example.org', 'description' => "x\x00y"]));
T::ok(!array_key_exists('contact_email', $v['payload']), 'contact is not part of the forwarded payload');
T::eq($v['contact'], 'who@example.org', 'contact returned separately');
T::eq($v['payload']['description'], 'xy', 'control characters stripped');

// ---- Images ----
$cfg = (new Env())->config;
$img = new ImageProcessor($cfg);
$ok = $img->sanitize(Env::png());
T::eq($ok['mime'], 'image/png', 'png accepted');
T::eq($img->sanitize(Env::jpeg())['mime'], 'image/jpeg', 'jpeg accepted');
if (function_exists('imagewebp')) {
    ob_start();
    imagewebp(imagecreatetruecolor(20, 20));
    T::eq($img->sanitize((string) ob_get_clean())['mime'], 'image/webp', 'webp accepted');
}
$trail = $img->sanitize(Env::png(40, 30, 'SECRET-TRAILER-DATA<?php echo 1;'));
T::ok(!str_contains($trail['bytes'], 'SECRET-TRAILER'), 're-encode drops trailing data');
foreach ([
    'text file' => 'hello world, not an image',
    'svg' => '<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>',
    'gif' => "GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;",
    'corrupt png' => substr(Env::png(), 0, 60),
    'html' => '<html><body>x</body></html>',
    'empty' => '',
] as $name => $bytes) {
    try {
        $img->sanitize($bytes);
        T::ok(false, "rejects $name");
    } catch (ValidationError $e) {
        T::ok(true, "rejects $name");
    }
}
try {
    $small = new ImageProcessor($cfg->with('MAX_SCREENSHOT_BYTES', '1000'));
    $small->sanitize(Env::noisePng(100, 100));
    T::ok(false, 'oversize rejected');
} catch (ValidationError $e) {
    T::eq($e->httpStatus, 413, 'oversize rejected');
}
try {
    (new ImageProcessor($cfg->with('MAX_PIXELS', '100')))->sanitize(Env::png(40, 30));
    T::ok(false, 'pixel limit');
} catch (ValidationError $e) {
    T::eq($e->errorCode, 'screenshot_dimensions', 'pixel limit enforced');
}

// ---- Intake: durable receipt, idempotency, validation ----
$e = new Env();
$r = Env::report(['contact_email' => 'private.person@example.org']);
[$st, $body] = $e->submit($r, [Env::png(), Env::jpeg()]);
T::eq($st, 201, 'submission accepted with 201');
T::ok(preg_match('/^NBR-[A-Z2-9]{8}$/', $body['report_ref']) === 1, 'report reference format');
T::eq($e->db->one('SELECT COUNT(*) n FROM attachments')['n'], 2, 'two attachments persisted');
T::eq($e->db->one("SELECT COUNT(*) n FROM jobs WHERE state='pending'")['n'], 2, 'two independent jobs queued');
[$st2, $body2] = $e->submit($r, [Env::png(), Env::jpeg()]);
T::eq($st2, 200, 'replay with same submission id returns existing');
T::eq($body2['report_ref'], $body['report_ref'], 'replay returns same reference');
T::eq($body2['receipt_token'], $body['receipt_token'], 'replay returns same receipt');
T::eq($e->db->one('SELECT COUNT(*) n FROM reports')['n'], 1, 'replay creates no second report');
$files = glob($e->dir . '/blobs/*/*.bin');
T::eq(count($files), 2, 'blobs on disk');
T::ok(!str_contains(file_get_contents($files[0]), 'private.person'), 'blob has no contact data');

// invalid report must not create anything
[$st3] = $e->submit(Env::report(['summary' => '']), []);
T::eq($st3, 400, 'invalid report rejected');
[$st4] = $e->submit(Env::report(), ['not an image']);
T::eq($st4, 415, 'invalid screenshot rejected with 415');
T::eq($e->db->one('SELECT COUNT(*) n FROM reports')['n'], 1, 'rejected submissions leave no report');
[$st5] = $e->submit(Env::report(), array_fill(0, 6, Env::png()));
T::eq($st5, 413, 'more than 5 screenshots rejected');
$big = new Env(['MAX_TOTAL_BYTES' => '3000']);
[$st6] = $big->submit(Env::report(), [Env::noisePng(60, 60), Env::noisePng(60, 60)]);
T::eq($st6, 413, 'combined size limit enforced');
$off = new Env(['INTAKE_DISABLED' => '1']);
[$st7, $b7] = $off->submit(Env::report());
T::eq($st7, 503, 'emergency disable returns 503');
T::eq($b7['error'], 'intake_disabled', 'emergency disable error code');
$q = new Env(['STORAGE_QUOTA_BYTES' => '10']);
[$st8, $b8] = $q->submit(Env::report(), [Env::png()]);
T::eq([$st8, $b8['error']], [503, 'storage_full'], 'storage quota enforced, nothing acknowledged');
T::eq($q->db->one('SELECT COUNT(*) n FROM reports')['n'], 0, 'no report when storage full');
$rl = new Env(['RATE_PER_IP_HOUR' => '2']);
$rl->submit(Env::report());
$rl->submit(Env::report());
[$st9] = $rl->submit(Env::report());
T::eq($st9, 429, 'per-IP rate limit');
[$st10] = $rl->submit(Env::report(), [], '198.51.100.7');
T::eq($st10, 201, 'other IP not limited');
// unreadable storage dir => no false receipt
$ro = new Env();
rename($ro->dir . '/blobs', $ro->dir . '/blobs-gone');
file_put_contents($ro->dir . '/blobs', 'x');
[$st11, $b11] = $ro->submit(Env::report(), [Env::png()]);
T::eq([$st11, $b11['error']], [503, 'storage_error'], 'blob write failure is not acknowledged');
T::eq($ro->db->one('SELECT COUNT(*) n FROM reports')['n'], 0, 'no report row after storage failure');

// ---- Status endpoint ----
[$s1, $sb] = $e->intake->status($body['receipt_token'], '203.0.113.9');
T::eq($s1, 200, 'status with receipt token');
T::eq(array_keys($sb), ['report_ref', 'received_at', 'linear', 'discord'], 'status exposes only reference and delivery state');
[$s2] = $e->intake->status('x' . substr($body['receipt_token'], 1), '203.0.113.9');
T::eq($s2, 404, 'unknown token');
T::ok(!str_contains(json_encode($sb), 'private.person'), 'status has no contact');
T::ok(!str_contains(json_encode($sb), 'Playback'), 'status has no report content');
[$s3] = $e->intake->status($body['report_ref'], '203.0.113.9');
T::eq($s3, 404, 'short report reference does not grant status');

// ---- Delivery happy path, contact isolation ----
$e->drain();
T::eq($e->net->issues, 1, 'exactly one Linear issue');
T::eq($e->net->discordPosts, 1, 'exactly one Discord message');
$row = $e->db->one('SELECT * FROM reports');
T::ok($row['linear_identifier'] !== null && $row['linear_issue_id'] !== null, 'Linear mapping stored');
T::eq($e->db->all('SELECT message_id FROM discord_messages')[0]['message_id'], '5001', 'Discord message id stored');
T::eq($e->db->one('SELECT COUNT(*) n FROM attachments WHERE linear_asset_url IS NOT NULL')['n'], 2, 'both screenshots uploaded to Linear');
T::eq($e->db->one("SELECT COUNT(*) n FROM jobs WHERE state='done'")['n'], 4, 'delivery + cross-link jobs done');
$firstDiscord = $e->net->discordPayloads[0];
T::eq($firstDiscord['allowed_mentions'], ['parse' => []], 'Discord mentions disabled');
T::eq(count($firstDiscord['attachments']), 2, 'Discord message carries two screenshots as files');
$all = $e->net->allText();
T::ok(!str_contains($all, 'private.person'), 'contact email never sent to Linear or Discord');
T::ok(!str_contains($all, 'example.org'), 'no contact domain in any outgoing request');
T::ok(!str_contains(file_get_contents($e->logFile), 'private.person'), 'contact not in logs');
T::ok(!str_contains(file_get_contents($e->logFile), 'lin_api_'), 'credentials not in logs');
T::ok(!str_contains(file_get_contents($e->logFile), 'TEST_webhook-token'), 'webhook not in logs');
$desc = end($e->net->descriptions);
T::ok(str_contains($desc, 'uploads.linear.app'), 'Linear issue embeds uploaded screenshots');
T::ok(str_contains($desc, 'discord.com/channels/999/777/5001'), 'Linear issue links the Discord message');
T::ok(str_contains(json_encode(end($e->net->discordPayloads)), 'linear.app'), 'Discord message edited with Linear link');
T::ok(!str_contains($e->db->one('SELECT payload_json FROM reports')['payload_json'], 'private.person'), 'payload_json has no contact');
T::ok(!str_contains(json_encode($e->db->all('SELECT * FROM jobs')), 'private.person'), 'jobs have no contact');
// vault
T::ok($e->vault->has((int) $row['id']), 'contact retained in backend');
T::ok(!str_contains($e->db->one('SELECT email_enc FROM contacts')['email_enc'], 'private.person'), 'contact stored encrypted');
T::eq($e->vault->reveal($body['report_ref'], 'alice', 'follow-up on report'), 'private.person@example.org', 'authorized maintainer retrieves contact');
try {
    $e->vault->reveal($body['report_ref'], 'mallory', 'curious');
    T::ok(false, 'unauthorized maintainer denied');
} catch (RuntimeException $ex) {
    T::ok(true, 'unauthorized maintainer denied');
}
$audit = json_encode($e->db->all('SELECT * FROM contact_access_log'));
T::ok(str_contains($audit, 'alice') && str_contains($audit, 'denied') && !str_contains($audit, 'private.person'), 'access audited without the address');
// retention
$e->db->run("UPDATE contacts SET created_at = ?", [Db::now(-86400 * 91)]);
T::eq($e->vault->purge(), 1, 'contact retention expiry purges');
T::eq($e->vault->reveal($body['report_ref'], 'alice', 'after retention'), null, 'purged contact not retrievable');

// ---- Mention and markdown injection ----
$m = new Env();
$m->submit(Env::report(['summary' => '@everyone **bold** [x](http://evil)', 'description' => "<@123456> @here ```\n# heading\n![p](http://tracker.test/x.png)"]));
$m->drain();
$dp = json_encode($m->net->discordPayloads[0], JSON_UNESCAPED_UNICODE);
T::ok(!preg_match('/@everyone|@here/', json_encode($m->net->discordPayloads[0])), 'Discord text neutralizes @everyone/@here');
T::eq($m->net->discordPayloads[0]['allowed_mentions'], ['parse' => []], 'allowed_mentions empty');
$ld = end($m->net->descriptions);
T::ok(str_contains($ld, "````\n"), 'Linear user text fenced with a longer fence');

// ---- Partial failures and idempotence ----
$p = new Env();
$p->net->linearFail = 3;
$p->submit(Env::report(), [Env::png()]);
$p->drain(1);
T::eq($p->net->discordPosts, 1, 'Discord delivered while Linear is down');
T::eq($p->db->one("SELECT state FROM jobs WHERE kind='linear'")['state'], 'pending', 'Linear job retries');
T::ok($p->db->one("SELECT last_error FROM jobs WHERE kind='linear'")['last_error'] !== null, 'sanitized error recorded');
$p->net->linearFail = 0;
$p->db->run('UPDATE reports SET linear_create_started_at = ?', [Db::now(-600)]); // outcome of the 503 was unknown; allow time to pass
$p->drain();
T::eq($p->net->issues, 1, 'Linear issue created once after outage');
T::eq($p->net->discordPosts, 1, 'Linear retry does not repost Discord');

$d = new Env();
$d->net->discordFail = 2;
$d->submit(Env::report(), [Env::png()]);
$d->drain(1);
T::eq($d->net->issues, 1, 'Linear issue created despite Discord outage');
$d->net->discordFail = 0;
$d->drain();
T::eq($d->net->issues, 1, 'Discord retry does not recreate the Linear issue');
T::eq($d->net->discordPosts, 1, 'Discord delivered after retry');

$rl2 = new Env();
$rl2->net->linearRateLimit = 1;
$rl2->net->discord429 = 1;
$rl2->submit(Env::report());
$rl2->drain();
T::eq([$rl2->net->issues, $rl2->net->discordPosts], [1, 1], 'recovers from 429/RATELIMITED');

$amb = new Env();
$amb->net->linearAmbiguous = 1;
$amb->submit(Env::report());
$amb->drain(1);
T::ok($amb->db->one('SELECT linear_issue_id FROM reports')['linear_issue_id'] === null, 'ambiguous create recorded no issue');
$amb->db->run("UPDATE reports SET linear_create_started_at = ?", [Db::now(-600)]);
$amb->drain(2);
T::eq($amb->net->issues, 1, 'ambiguous timeout reconciled, no second issue');
T::ok($amb->db->one('SELECT linear_issue_id FROM reports')['linear_issue_id'] !== null, 'reconciled issue mapped');

$up = new Env();
$up->net->uploadFail = 1;
$up->submit(Env::report(), [Env::png()]);
$up->drain(1);
T::eq($up->net->issues, 1, 'issue created before upload failure');
$up->drain();
T::eq($up->net->issues, 1, 'attachment repair does not create another issue');
T::eq($up->db->one('SELECT COUNT(*) n FROM attachments WHERE linear_asset_url IS NOT NULL')['n'], 1, 'attachment repaired on the same issue');

// restart between receipt and delivery
$rs = new Env();
$rs->submit(Env::report(), [Env::png()]);
$rs->outbox = $rs->newOutbox();
$rs->drain();
T::eq([$rs->net->issues, $rs->net->discordPosts], [1, 1], 'new worker process delivers pending reports');

// large screenshots are resized for Discord but stay full-size in Linear
$bg = new Env(['DISCORD_MAX_MESSAGE_BYTES' => '60000']);
$bg->submit(Env::report(), [Env::noisePng(400, 400), Env::noisePng(400, 400)]);
$bg->drain();
T::ok(max($bg->net->discordFileSizes) <= 60000, 'Discord copies fit the message size limit');
T::ok($bg->net->discordPosts >= 2, 'overflow screenshots go in follow-up messages');
T::eq($bg->db->one('SELECT COUNT(*) n FROM attachments WHERE linear_asset_url IS NOT NULL')['n'], 2, 'Linear receives all screenshots');

// duplicate flag without merging
$du = new Env();
$du->submit(Env::report(['error_fingerprint' => 'same-fp']));
$du->submit(Env::report(['error_fingerprint' => 'same-fp']));
T::eq($du->db->one('SELECT COUNT(*) n FROM reports')['n'], 2, 'similar reports are not merged');
T::ok($du->db->one('SELECT possible_duplicate_of FROM reports WHERE id = 2')['possible_duplicate_of'] !== null, 'possible duplicate flagged');

// ---- Retention / cleanup ----
$c = new Env();
$c->submit(Env::report(), [Env::png()]);
$blob = $c->dir . '/' . $c->db->one('SELECT blob_path FROM attachments')['blob_path'];
$c->db->run('UPDATE reports SET created_at = ?', [Db::now(-86400 * 40)]);
Maintenance::cleanup($c->db, $c->config, $c->vault);
T::ok(is_file($blob), 'blob kept while delivery is pending');
$c->drain();
Maintenance::cleanup($c->db, $c->config, $c->vault);
T::ok(!is_file($blob), 'blob removed after delivery and retention');
file_put_contents($c->dir . '/blobs/ab-orphan.bin', 'x');
touch($c->dir . '/blobs/ab-orphan.bin', time() - 7200);
T::eq(Maintenance::cleanup($c->db, $c->config, $c->vault)['orphans_removed'], 1, 'orphan upload cleaned');
$c->db->run('UPDATE reports SET created_at = ?', [Db::now(-86400 * 400)]);
T::eq(Maintenance::cleanup($c->db, $c->config, $c->vault)['reports_deleted'], 1, 'report row deleted after retention');

// ---- Scoped worker ----
$c = new Env();
$a = $c->submit(Env::report());
$b = $c->submit(Env::report());
$c->makeDue();
$c->outbox->runDue(20, [$a[1]['report_ref']]);
T::eq((int) $c->db->one("SELECT COUNT(*) AS n FROM jobs j JOIN reports r ON r.id = j.report_id WHERE r.report_ref = ? AND j.state = 'done'", [$a[1]['report_ref']])['n'] > 0, true, 'scoped run delivers the selected report');
T::eq((int) $c->db->one("SELECT COUNT(*) AS n FROM jobs j JOIN reports r ON r.id = j.report_id WHERE r.report_ref = ? AND j.state <> 'pending'", [$b[1]['report_ref']])['n'], 0, 'scoped run leaves other reports untouched');
T::eq($c->outbox->runDue(20, [])['ran'], 0, 'empty scope runs nothing');

// ---- Dead-letter retention ----
$c = new Env();
$c->submit(Env::report(), [Env::png()]);
$blob = $c->dir . '/' . $c->db->one('SELECT blob_path FROM attachments')['blob_path'];
$c->db->run("UPDATE reports SET created_at = ?", [Db::now(-86400 * 60)]);
$c->db->run("UPDATE jobs SET state = 'stuck', updated_at = ?", [Db::now(-86400 * 3)]);
Maintenance::cleanup($c->db, $c->config, $c->vault);
T::ok(is_file($blob), 'blob kept while a stuck job is within the dead-letter window');
$c->db->run("UPDATE jobs SET updated_at = ?", [Db::now(-86400 * 20)]);
$res = Maintenance::cleanup($c->db, $c->config, $c->vault);
T::eq($res['jobs_abandoned'], 2, 'stuck jobs abandoned after the dead-letter window');
Maintenance::cleanup($c->db, $c->config, $c->vault);
T::ok(!is_file($blob), 'screenshot purged once the jobs are abandoned');
$c->db->run('UPDATE reports SET created_at = ?', [Db::now(-86400 * 400)]);
T::eq(Maintenance::cleanup($c->db, $c->config, $c->vault)['reports_deleted'], 1, 'abandoned report deleted at report retention');

// ---- Client address resolution ----
$cf = ['173.245.48.0/20', '2400:cb00::/32'];
T::eq(NebulaeIntake\ClientIp::resolve('173.245.50.1', '198.51.100.9', $cf), '198.51.100.9', 'CF header trusted from a Cloudflare peer');
T::eq(NebulaeIntake\ClientIp::resolve('203.0.113.5', '198.51.100.9', $cf), '203.0.113.5', 'CF header ignored from other peers');
T::eq(NebulaeIntake\ClientIp::resolve('2400:cb00::1', '2001:db8::7', $cf), '2001:db8::7', 'IPv6 proxy range');
T::eq(NebulaeIntake\ClientIp::resolve('173.245.50.1', 'garbage', $cf), '173.245.50.1', 'invalid forwarded value ignored');

// ---- Source leak check ----
$src = shell_exec('grep -rIlE "lin_api_[A-Za-z0-9]{20,}|discord(app)?\\.com/api/webhooks/[0-9]+/[A-Za-z0-9_-]{20,}" ' . escapeshellarg(dirname(__DIR__)) . ' --exclude-dir=tests --exclude-dir=.git') ?: '';
T::eq(trim($src), '', 'no credentials in service source');

echo T::$pass . " passed, " . T::$fail . " failed\n";
exit(T::$fail === 0 ? 0 : 1);
