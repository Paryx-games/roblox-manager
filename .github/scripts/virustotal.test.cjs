const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');

test('VirusTotal reuses nonempty hash reports and falls back safely for missing, empty, or failed lookups', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'release-vt-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'release'));
  for (const name of ['app-setup.exe', 'app-portable.zip']) fs.writeFileSync(path.join(root, 'release', name), `Synthetic ${name}`);
  const checksums = ['app-setup.exe', 'app-portable.zip'].map(name => `${crypto.createHash('sha256').update(fs.readFileSync(path.join(root, 'release', name))).digest('hex')}  ${name}`);
  fs.writeFileSync(path.join(root, 'release/SHA256SUMS.txt'), checksums.join('\n'));
  const harness = String.raw`
$ErrorActionPreference = 'Stop'
$global:requests = [Collections.Generic.List[object]]::new()
$global:sleeps = [Collections.Generic.List[object]]::new()
$global:lookupCount = 0
function Start-Sleep {
  param([int]$Seconds, [int]$Milliseconds)
  $global:sleeps.Add(@{ Seconds = $Seconds; Milliseconds = $Milliseconds })
}
function Invoke-RestMethod {
  param($Uri, $Method = 'Get', $Headers, [int]$TimeoutSec, $Form)
  if ($TimeoutSec -le 0 -or $TimeoutSec -gt 45) { throw 'Request has no bounded timeout.' }
  $global:requests.Add(@{ Uri = $Uri; Method = "$Method"; Timeout = $TimeoutSec })
  if ($env:VT_TEST_MODE -eq 'outage') { throw 'Synthetic network outage' }
  if ($Uri -match '/files/[a-f0-9]{64}$') {
    $global:lookupCount++
    if ($env:VT_TEST_MODE -eq 'known' -or ($env:VT_TEST_MODE -eq 'mixed' -and $global:lookupCount -eq 1)) {
      return [pscustomobject]@{ data = @{ attributes = @{ last_analysis_stats = [pscustomobject]@{ malicious = 1; suspicious = 0; harmless = 69 } } } }
    }
    if ($env:VT_TEST_MODE -eq 'missing') { throw '404 Not Found' }
    if ($env:VT_TEST_MODE -eq 'zero') { return [pscustomobject]@{ data = @{ attributes = @{ last_analysis_stats = [pscustomobject]@{ malicious = 0; harmless = 0 } } } } }
    if ($env:VT_TEST_MODE -eq 'empty') { return [pscustomobject]@{ data = @{ attributes = @{ last_analysis_stats = [pscustomobject]@{} } } } }
    return [pscustomobject]@{ data = @{ attributes = @{} } }
  }
  if ($Method -eq 'Post') { return @{ data = @{ id = 'synthetic-analysis' } } }
  if ($env:VT_TEST_MODE -eq 'poll-error') { throw 'Synthetic polling error' }
  if ($env:VT_TEST_MODE -eq 'pending') { return @{ data = @{ attributes = @{ status = 'queued' } } } }
  return [pscustomobject]@{ data = @{ attributes = @{ status = 'completed'; stats = [pscustomobject]@{ malicious = 0; harmless = 70 } } } }
}
try { & $env:VT_TEST_SCRIPT } finally {
  @{ Requests = @($global:requests.ToArray()); Sleeps = @($global:sleeps.ToArray()) } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath trace.json
}
`;
  fs.writeFileSync(path.join(root, 'harness.ps1'), harness);
  const run = (mode, key = 'synthetic') => {
    fs.writeFileSync(path.join(root, 'summary.md'), '');
    const result = spawnSync('pwsh', ['-NoProfile', '-File', 'harness.ps1'], {
      cwd: root, env: { ...process.env, VT_TEST_MODE: mode, VT_API_KEY: key, VT_TEST_SCRIPT: path.resolve('scripts/virustotal.ps1'), GITHUB_STEP_SUMMARY: path.join(root, 'summary.md'), TAG_SHA: 'validated', REPO: 'test/release' }, encoding: 'utf8',
    });
    return { ...result, trace: JSON.parse(fs.readFileSync(path.join(root, 'trace.json'), 'utf8')), notes: fs.readFileSync(path.join(root, 'security.md'), 'utf8'), summary: fs.readFileSync(path.join(root, 'summary.md'), 'utf8') };
  };
  let result = run('known');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.trace.Requests.length, 2);
  assert.ok(result.trace.Requests.every(request => request.Method === 'Get' && /\/files\/[a-f0-9]{64}$/.test(request.Uri)));
  assert.equal(result.trace.Sleeps.length, 0);
  assert.match(result.notes, /1 \/ 70 security vendors flagged this file/);
  assert.match(result.summary, /Open VirusTotal report/);
  for (const mode of ['missing', 'empty', 'zero', 'no-stats']) {
    result = run(mode);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.trace.Requests.filter(request => request.Method === 'Post').length, 2, mode);
    assert.match(result.notes, /0 \/ 70 security vendors flagged this file/);
    assert.ok(result.trace.Sleeps.some(sleep => sleep.Milliseconds > 0), 'space uploads');
  }
  result = run('mixed');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.trace.Requests.filter(request => request.Method === 'Post').length, 1);
  assert.ok(!result.trace.Sleeps.some(sleep => sleep.Milliseconds > 0), 'no upload spacing for the cached file');
  for (const mode of ['outage', 'poll-error', 'pending']) {
    result = run(mode);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.notes, /Scan unavailable/);
  }
  result = run('known', '');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.trace.Requests.length, 0);
  assert.equal(result.trace.Sleeps.length, 0);
  assert.match(result.notes, /Scan unavailable for this build\./);
  assert.doesNotMatch(result.notes, /API_KEY|not configured/);
  fs.writeFileSync(path.join(root, 'release/app-setup.exe'), 'Tampered artifact');
  result = run('known');
  assert.notEqual(result.status, 0);
  assert.match(result.stdout + result.stderr, /SHA256 mismatch/);
  assert.equal(result.trace.Requests.length, 0);
});
