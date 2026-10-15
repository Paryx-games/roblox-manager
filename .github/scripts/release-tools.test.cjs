const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

test('Rust setup installs the exact file pin, components and targets and propagates failures', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'release-toolchain-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.writeFileSync(path.join(root, 'rust-toolchain.toml'), '[toolchain]\nchannel = "1.99.0"\nprofile = "minimal"\ncomponents = ["rustfmt", "clippy"]\ntargets = ["x86_64-pc-windows-msvc"]\n');
  fs.writeFileSync(path.join(root, 'harness.ps1'), String.raw`
$global:calls = [Collections.Generic.List[object]]::new()
function rustup {
  $global:calls.Add(@($args))
  $global:LASTEXITCODE = [int]$env:MOCK_RUSTUP_STATUS
}
try { & $env:SETUP_SCRIPT } finally { ConvertTo-Json -InputObject @($global:calls.ToArray()) -Depth 5 | Set-Content -LiteralPath calls.json }
`);
  const run = status => spawnSync('pwsh', ['-NoProfile', '-File', 'harness.ps1'], {
    cwd: root, env: { ...process.env, MOCK_RUSTUP_STATUS: String(status), SETUP_SCRIPT: path.resolve('scripts/setup-rust.ps1') }, encoding: 'utf8',
  });
  let result = run(0);
  assert.equal(result.status, 0, result.stderr);
  const calls = JSON.parse(fs.readFileSync(path.join(root, 'calls.json'), 'utf8'));
  assert.deepEqual(calls, [
    ['toolchain', 'install', '1.99.0', '--profile', 'minimal', '--no-self-update', '--component', 'rustfmt', '--component', 'clippy', '--target', 'x86_64-pc-windows-msvc'],
    ['show', 'active-toolchain'],
  ]);
  result = run(1);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /installation failed/);
  fs.writeFileSync(path.join(root, 'rust-toolchain.toml'), '[toolchain]\nchannel = "stable"\n');
  result = run(0);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /exact Rust version/);
});

test('approval summary includes the tag, validated commit, filenames and hashes', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'release-summary-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'release'));
  const checksums = 'abc123  app-setup.exe\ndef456  app-portable.zip\nghi789  COMMIT.txt';
  fs.writeFileSync(path.join(root, 'release/SHA256SUMS.txt'), checksums);
  const result = spawnSync('pwsh', ['-NoProfile', '-File', path.resolve('scripts/release-summary.ps1')], {
    cwd: root, env: { ...process.env, RELEASE_TAG: 'v1.0.0', RELEASE_SHA: 'validated', GITHUB_STEP_SUMMARY: path.join(root, 'summary.md') }, encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr);
  const summary = fs.readFileSync(path.join(root, 'summary.md'), 'utf8');
  for (const part of ['Ready to publish', 'v1.0.0', 'validated', checksums]) assert.ok(summary.includes(part));
});
