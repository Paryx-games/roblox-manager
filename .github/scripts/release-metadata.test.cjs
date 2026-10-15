const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync, spawnSync } = require('node:child_process');

test('metadata fetches main, excludes prereleases only for stable notes, and rejects off-main tags', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'release-metadata-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const git = args => execFileSync('git', ['-c', 'tag.gpgSign=false', '-c', 'commit.gpgSign=false', ...args], { cwd: root, timeout: 15000, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  git(['init', '--quiet', '--initial-branch=main']);
  const commit = message => {
    fs.writeFileSync(path.join(root, 'fixture.txt'), message);
    git(['add', '.']);
    git(['-c', 'user.name=Release Test', '-c', 'user.email=release@example.invalid', 'commit', '--quiet', '-m', message]);
    return git(['rev-parse', 'HEAD']);
  };
  commit('stable'); git(['tag', 'v1.0.0']);
  commit('preview'); git(['tag', 'v1.1.0-beta.1']);
  const sha = commit('release'); git(['tag', 'v1.1.0']); git(['tag', 'v1.1.0-rc.1']);
  git(['remote', 'add', 'origin', root]);
  const output = path.join(os.tmpdir(), path.basename(root) + '-output');
  t.after(() => fs.rmSync(output, { force: true }));
  const run = tag => {
    fs.writeFileSync(output, '');
    const result = spawnSync('pwsh', ['-NoProfile', '-File', path.resolve('scripts/resolve-release-metadata.ps1')], {
      cwd: root, env: { ...process.env, RELEASE_TAG: tag, GITHUB_OUTPUT: output }, encoding: 'utf8',
    });
    return { ...result, outputs: Object.fromEntries(fs.readFileSync(output, 'utf8').trim().split(/\r?\n/).map(line => line.split('='))) };
  };
  let result = run('v1.1.0');
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(result.outputs, { sha, previous: 'v1.0.0', prerelease: 'false', make_latest: 'true' });
  git(['tag', '-d', 'v1.1.0']);
  result = run('v1.1.0-rc.1');
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(result.outputs, { sha, previous: 'v1.1.0-beta.1', prerelease: 'true', make_latest: 'false' });
  git(['switch', '--quiet', '-c', 'side']);
  commit('off main'); git(['tag', 'v2.0.0']);
  result = run('v2.0.0');
  assert.notEqual(result.status, 0);
  assert.match(result.stdout + result.stderr, /::error::v2\.0\.0 is not on main/);
  result = run('v9.9.9');
  assert.notEqual(result.status, 0);
});

test('CI wait trusts only exact-SHA push/dispatch runs and preserves failed-versus-active decisions', async () => {
  const workflow = fs.readFileSync('.github/workflows/release.yml', 'utf8');
  const block = workflow.split('  wait-for-ci:\n')[1].split('\n  build:')[0];
  const script = block.split('          script: |\n')[1].split('\n').map(line => line.startsWith('            ') ? line.slice(12) : line).join('\n');
  const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
  const evaluate = new AsyncFunction('github', 'context', 'core', 'process', 'setTimeout', script);
  const run = (batches) => {
    let calls = 0;
    let failure;
    const messages = [];
    const github = { rest: { actions: { listWorkflowRuns: async request => {
      assert.equal(request.head_sha, 'validated');
      return { data: { workflow_runs: batches[Math.min(calls++, batches.length - 1)] } };
    } } } };
    const promise = evaluate(github, { repo: { owner: 'test', repo: 'release' } }, {
      info: message => messages.push(message), setFailed: message => { failure = message; },
    }, { env: { SHA: 'validated' } }, resolve => resolve());
    return promise.then(() => ({ calls, failure, messages }));
  };
  const ci = (event, status, conclusion, head_sha = 'validated') => ({ event, status, conclusion, head_sha, html_url: 'https://example.invalid/run' });
  const failed = ci('push', 'completed', 'failure');
  let result = await run([[failed, ci('push', 'in_progress', null)], [ci('push', 'completed', 'success')]]);
  assert.equal(result.calls, 2); assert.equal(result.failure, undefined);
  result = await run([[failed]]);
  assert.equal(result.calls, 1); assert.match(result.failure, /does not have green CI/);
  result = await run([[ci('workflow_dispatch', 'completed', 'success')]]);
  assert.equal(result.failure, undefined);
  result = await run([[ci('pull_request', 'completed', 'success'), ci('push', 'completed', 'success', 'other')]]);
  assert.equal(result.calls, 120); assert.match(result.failure, /No successful ci.yml run/);
});


test('publish integrity gate rejects checksum tampering and a different source commit', t => {
  const bash = process.platform === 'win32' ? path.join(process.env.ProgramFiles, 'Git/bin/bash.exe') : 'bash';
  if (process.platform === 'win32' && !fs.existsSync(bash)) { t.skip('Git Bash is unavailable'); return; }
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'release-integrity-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'release'));
  const crypto = require('node:crypto');
  const checksum = text => crypto.createHash('sha256').update(text).digest('hex');
  fs.writeFileSync(path.join(root, 'release/app.exe'), 'Synthetic app');
  fs.writeFileSync(path.join(root, 'release/COMMIT.txt'), '\ufeffvalidated\r\n');
  const manifest = () => fs.writeFileSync(path.join(root, 'release/SHA256SUMS.txt'), `${checksum('Synthetic app')}  app.exe\n${checksum(fs.readFileSync(path.join(root, 'release/COMMIT.txt')))}  COMMIT.txt\n`);
  manifest();
  const workflow = fs.readFileSync('.github/workflows/release.yml', 'utf8');
  const step = workflow.split('      - name: Verify downloaded release packages\n')[1].split('      - name: Assemble release notes')[0];
  const script = step.split('        run: |\n')[1].split('\n').map(line => line.startsWith('          ') ? line.slice(10) : line).join('\n');
  const run = sha => spawnSync(bash, ['-c', script], { cwd: root, env: { ...process.env, RELEASE_SHA: sha }, encoding: 'utf8' });
  let result = run('validated');
  assert.equal(result.status, 0, result.stderr);
  result = run('other');
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /Artifact commit mismatch/);
  fs.writeFileSync(path.join(root, 'release/app.exe'), 'Tampered app');
  assert.notEqual(run('validated').status, 0);
  fs.writeFileSync(path.join(root, 'release/app.exe'), 'Synthetic app');
  fs.rmSync(path.join(root, 'release/COMMIT.txt'));
  assert.notEqual(run('validated').status, 0);
});
