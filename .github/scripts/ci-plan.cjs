const labelRules = {
  rust: /\.rs$|(?:^|\/)Cargo\.(?:toml|lock)$/i,
  javascript: /\.(?:js|jsx|mjs|cjs|ts|tsx)$|(?:^|\/)(?:package\.json|pnpm-lock\.yaml)$/i,
  'github-actions': /^\.github\/(?:workflows\/.+\.ya?ml|scripts\/)/i,
  documentation: /(?:^|\/)(?:README|CHANGELOG|CONTRIBUTING|SECURITY|CODE_OF_CONDUCT)(?:\.[^/]*)?$|\.mdx?$|^docs\//i,
};

function plan(paths, force = false) {
  const result = { frontend: false, frontendBuild: false, rust: false, build: false };
  for (const path of paths) {
    if (/^\.github\/(?:workflows\/(?:ci|release)\.yml|scripts\/ci-plan\.cjs)$/.test(path)) {
      force = true;
      continue;
    }
    if (labelRules.documentation.test(path) || /^\.github\//.test(path)) continue;
    if (/^ram_ui\/(?:frontend\/|scripts\/|package\.json$|pnpm-lock\.yaml$|[^/]+\.(?:json|[cm]?js|ts))/.test(path)) {
      result.frontend = result.frontendBuild = result.build = true;
    } else if (/\.rs$|(?:^|\/)Cargo\.(?:toml|lock)$|^clippy\.toml$|^rust-toolchain(?:\.toml)?$/.test(path)) {
      result.rust = result.frontendBuild = result.build = true;
    } else {
      // unknown build inputs are checked conservatively, never silently ignored
      force = true;
    }
  }
  if (force) for (const key of Object.keys(result)) result[key] = true;
  return result;
}

async function changedPaths(github, context) {
  const { owner, repo } = context.repo;
  const pull = context.payload.pull_request;
  const files = await github.paginate(github.rest.pulls.listFiles, {
    owner, repo, pull_number: pull.number, per_page: 100,
  });
  // the API caps its response at 3,000 files; incomplete lists require full CI
  const incomplete = files.length >= 3000 || files.length !== pull.changed_files;
  return {
    paths: files.flatMap(file => [file.filename, file.previous_filename].filter(Boolean)),
    incomplete,
  };
}

async function select({ github, context, core }) {
  const changed = context.eventName === 'pull_request'
    ? await changedPaths(github, context) : { paths: [], incomplete: true };
  const selected = plan(changed.paths, changed.incomplete);
  for (const [key, value] of Object.entries(selected)) core.setOutput(key, String(value));
  await core.summary.addHeading('Selected CI checks').addTable([
    [{ data: 'Check group', header: true }, { data: 'Required', header: true }],
    ...Object.entries(selected).map(([key, value]) => [key, value ? 'Yes' : 'No']),
  ]).write();
}

async function label({ github, context }) {
  const { owner, repo } = context.repo;
  const issue_number = context.payload.pull_request.number;
  const { paths, incomplete } = await changedPaths(github, context);
  // avoid removing valid labels when GitHub cannot return the whole diff
  if (incomplete) throw new Error('Changed file list is incomplete; labels were left unchanged.');
  const wanted = Object.entries(labelRules)
    .filter(([, pattern]) => paths.some(path => pattern.test(path))).map(([name]) => name);
  const current = await github.paginate(github.rest.issues.listLabelsOnIssue, {
    owner, repo, issue_number, per_page: 100,
  });
  const currentNames = new Set(current.map(item => item.name));
  const additions = wanted.filter(name => !currentNames.has(name));
  if (additions.length) await github.rest.issues.addLabels({ owner, repo, issue_number, labels: additions });
  for (const name of Object.keys(labelRules)) {
    if (currentNames.has(name) && !wanted.includes(name)) {
      await github.rest.issues.removeLabel({ owner, repo, issue_number, name });
    }
  }
}

module.exports = { plan, changedPaths, select, label };
