const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');

test('RM packaging binds both x64 downloads and the commit manifest to matching checksums', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rm-packaging-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'release'));
  const run = () => spawnSync('pwsh', ['-NoProfile', '-File', path.resolve('scripts/package-release.ps1')], {
    cwd: root, env: { ...process.env, RELEASE_TAG: 'v2.0.0-rc.1', RELEASE_SHA: 'synthetic-validated-sha' }, encoding: 'utf8',
  });
  assert.notEqual(run().status, 0);
  fs.writeFileSync(path.join(root, 'release/rm_tauri.exe'), 'Synthetic app');
  assert.notEqual(run().status, 0);
  fs.writeFileSync(path.join(root, 'release/installer.exe'), 'Synthetic installer');
  const result = run();
  assert.equal(result.status, 0, result.stderr);
  assert.equal(fs.readFileSync(path.join(root, 'release/COMMIT.txt'), 'utf8').trim(), 'synthetic-validated-sha');
  const entries = fs.readFileSync(path.join(root, 'release/SHA256SUMS.txt'), 'utf8').trim().split(/\r?\n/);
  assert.equal(entries.length, 3);
  for (const entry of entries) {
    const [, expected, name] = entry.match(/^([a-f0-9]{64})  (.+)$/);
    assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(root, 'release', name))).digest('hex'), expected);
  }
  for (const name of ['roblox-manager-v2.0.0-rc.1-windows-x64.exe', 'roblox-manager-v2.0.0-rc.1-windows-x64-setup.exe']) assert.ok(fs.existsSync(path.join(root, 'release', name)));
});
