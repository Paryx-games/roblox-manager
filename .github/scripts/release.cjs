function validateTag(tag, version) {
  if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/.test(tag)) {
    throw new Error('Expected an existing vX.Y.Z or vX.Y.Z-prerelease tag');
  }
  if (tag !== `v${version}`) throw new Error('Release tag does not match application version');
  const prerelease = tag.includes('-');
  return { prerelease, makeLatest: !prerelease };
}

function changelogNotes(changelog, tag) {
  const lines = changelog.split(/\r?\n/);
  const start = lines.findIndex(line => line.trimEnd() === `## ${tag}`);
  if (start < 0) throw new Error(`Missing changelog section for ${tag}`);
  let end = lines.findIndex((line, index) => index > start && line.startsWith('## '));
  if (end < 0) end = lines.length;
  return lines.slice(start, end).join('\n').trim();
}

async function confirmTag({ github, context }) {
  const { data: ref } = await github.rest.git.getRef({ ...context.repo, ref: `tags/${process.env.RELEASE_TAG}` });
  let object = ref.object;
  for (let count = 0; object.type === 'tag' && count < 10; count++) {
    const { data: tag } = await github.rest.git.getTag({ ...context.repo, tag_sha: object.sha });
    object = tag.object;
  }
  if (object.type !== 'commit' || object.sha !== process.env.RELEASE_SHA) throw new Error('Release tag moved or no longer points to the validated commit');
}

if (require.main === module) {
  const fs = require('node:fs');
  const { execFileSync } = require('node:child_process');
  const tag = process.env.RELEASE_TAG;
  validateTag(tag, tag?.slice(1));
  execFileSync(process.execPath, ['ram_ui/scripts/tauri.mjs', 'version:check', tag], { stdio: 'inherit' });
  fs.writeFileSync('release-notes.md', changelogNotes(fs.readFileSync('CHANGELOG.md', 'utf8'), tag) + '\n');
}

module.exports = { validateTag, changelogNotes, confirmTag };
