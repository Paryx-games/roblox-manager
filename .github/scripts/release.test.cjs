const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { validateTag, changelogNotes, confirmTag } = require('./release.cjs');
const { SUPPORT_BLOCK } = require('../../scripts/write-release-notes.cjs');

test('release tags match the application version and classify every hyphenated tag as prerelease', () => {
  assert.deepEqual(validateTag('v1.0.0', '1.0.0'), { prerelease: false, makeLatest: true });
  for (const suffix of ['rc.1', 'beta.1', 'alpha.1', 'preview.1']) {
    assert.deepEqual(validateTag(`v1.1.0-${suffix}`, `1.1.0-${suffix}`), { prerelease: true, makeLatest: false });
  }
  for (const tag of ['main', '--help', 'v1.0.0\nrun', 'v1.0.1']) assert.throws(() => validateTag(tag, '1.0.0'));
});

test('maintained notes require the exact RM tag heading and preserve the heading', () => {
  const text = '# Changes\n\n## Unreleased\n\nFuture\n\n## v1.0.0\n\n### Added\n\n- RM\n\n## v0.0.1\n\nOlder';
  assert.equal(changelogNotes(text, 'v1.0.0'), '## v1.0.0\n\n### Added\n\n- RM');
  assert.throws(() => changelogNotes(text, 'v0.2.0'));
  assert.throws(() => changelogNotes('## 1.0.0\n\nWrong heading', 'v1.0.0'));
});

test('publication resolves annotated and lightweight tags and rejects moved, missing, and non-commit refs', async () => {
  const original = { tag: process.env.RELEASE_TAG, sha: process.env.RELEASE_SHA };
  process.env.RELEASE_TAG = 'v1.0.0'; process.env.RELEASE_SHA = 'expected';
  const context = { repo: { owner: 'test', repo: 'release' } };
  let object = { type: 'tag', sha: 'annotation' };
  let missing = false;
  const github = { rest: { git: {
    getRef: async () => { if (missing) throw new Error('Not Found'); return { data: { object } }; },
    getTag: async () => ({ data: { object: { type: 'commit', sha: 'expected' } } }),
  } } };
  try {
    await confirmTag({ github, context });
    object = { type: 'commit', sha: 'expected' };
    await confirmTag({ github, context });
    object = { type: 'commit', sha: 'moved' };
    await assert.rejects(confirmTag({ github, context }), /tag moved/);
    object = { type: 'tree', sha: 'expected' };
    await assert.rejects(confirmTag({ github, context }), /tag moved/);
    missing = true;
    await assert.rejects(confirmTag({ github, context }), /Not Found/);
  } finally {
    for (const [key, value] of [['RELEASE_TAG', original.tag], ['RELEASE_SHA', original.sha]]) {
      if (value === undefined) delete process.env[key]; else process.env[key] = value;
    }
  }
});

test('all note variants end with the exact support block after security and notices', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'release-notes-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const folder of ['release', 'notes/changelog', 'notes/github', 'notes/security']) fs.mkdirSync(path.join(root, folder), { recursive: true });
  const expectedSupport = '## Support the Project\n\n<a href="https://ko-fi.com/paryx">\n  <img src="https://storage.ko-fi.com/cdn/brandasset/v2/support_me_on_kofi_dark.png" alt="Support me on Ko-fi" height="44">\n</a>';
  assert.equal(SUPPORT_BLOCK, expectedSupport);
  fs.writeFileSync(path.join(root, 'notes/changelog/release-notes.md'), '- Synthetic release change\n');
  fs.writeFileSync(path.join(root, 'release/SHA256SUMS.txt'), 'synthetic checksum\n');
  for (const name of ['roblox-manager-v1.0.0-windows-x64.exe', 'roblox-manager-v1.0.0-windows-x64-setup.exe']) fs.writeFileSync(path.join(root, 'release', name), 'Synthetic download');
  const run = tag => {
    execFileSync(process.execPath, [path.resolve('scripts/write-release-notes.cjs')], {
      cwd: root, env: { ...process.env, RELEASE_TAG: tag, RELEASE_SHA: 'synthetic-sha', GITHUB_REPOSITORY: 'test/release' },
    });
    return fs.readFileSync(path.join(root, 'final-release-notes.md'), 'utf8');
  };
  for (const suffix of ['', '-rc.1', '-beta.1', '-alpha.1', '-preview.1']) {
    for (const generated of ['', 'Synthetic generated notes']) {
      for (const security of ['', 'Synthetic scan results and provenance']) {
        fs.writeFileSync(path.join(root, 'notes/github/github-release-notes.md'), generated);
        fs.writeFileSync(path.join(root, 'notes/security/security.md'), security);
        const notes = run(`v1.0.0${suffix}`);
        assert.ok(notes.endsWith(expectedSupport));
        assert.equal(notes.split('## Support the Project').length, 2);
        assert.ok(notes.includes('\n\n' + expectedSupport));
        assert.ok(!notes.includes('\n\n\n' + expectedSupport));
        assert.doesNotMatch(notes, /Support development on Ko-fi|github\.com\/sponsors|github sponsors|donate|funding/i);
        assert.ok(notes.includes('Synthetic release change'));
        if (generated) assert.ok(notes.indexOf(generated) < notes.indexOf(expectedSupport));
        if (security) assert.ok(notes.indexOf(security) < notes.indexOf(expectedSupport));
        if (suffix === '-rc.1') assert.match(notes, /> \[!TIP\][\s\S]*release candidate version 1/);
        if (suffix === '-beta.1') assert.match(notes, /> \[!WARNING\]/);
        if (suffix === '-alpha.1') assert.match(notes, /> \[!CAUTION\]/);
        assert.match(notes, /windows-x64-setup\.exe/);
        assert.match(notes, /portable app \(requires WebView2 Runtime\)/);
      }
    }
  }
  fs.rmSync(path.join(root, 'notes/github/github-release-notes.md'));
  fs.rmSync(path.join(root, 'notes/security/security.md'));
  assert.ok(run('v1.0.0').endsWith(expectedSupport));
});
