const { test } = require('node:test');
const assert = require('node:assert/strict');
const { plan, changedPaths, label } = require('./ci-plan.cjs');
const { failures } = require('./ci-gate.cjs');
const { readState, transition, render, findPull, snapshot, report } = require('./ci-report.cjs');

const full = { frontend: true, frontendBuild: true, rust: true, build: true };
const docs = { frontend: false, frontendBuild: false, rust: false, build: false };
test('select only needed work, and fail safe for unknown inputs', () => {
  assert.deepEqual(plan(['README.md', 'docs/guides/accounts.md']), docs);
  assert.deepEqual(plan(['.github/workflows/ci-report.yml', '.github/dependabot.yml']), docs);
  assert.deepEqual(plan(['ram_ui/frontend/AccountsPage.tsx']), { ...full, rust: false });
  assert.deepEqual(plan(['ram_core/src/auth.rs']), { ...full, frontend: false });
  for (const path of ['Cargo.lock', 'ram_ui/src-tauri/tauri.conf.json', 'assets/icons/a.svg',
    'package.json', 'bump-version.bat', '.cargo/config.toml', '.github/workflows/ci.yml', '.github/scripts/ci-plan.cjs']) {
    const selected = plan([path]);
    assert.equal(selected.build, true, path);
    assert.equal(selected.rust, true, path);
  }
  assert.deepEqual(plan(['new-build-input'], false), full);
  assert.deepEqual(plan([], true), full);
});

test('renames include the previous filename and incomplete lists force full work', async () => {
  const context = { repo: { owner: 'test', repo: 'rm' }, payload: { pull_request: { number: 1, changed_files: 2 } } };
  const github = { rest: { pulls: { listFiles() {} } }, paginate: async () => [
    { filename: 'docs/example.md', previous_filename: 'ram_core/src/example.rs' },
  ] };
  const changed = await changedPaths(github, context);
  assert.deepEqual(changed.paths, ['docs/example.md', 'ram_core/src/example.rs']);
  assert.equal(changed.incomplete, true);
});

test('labels preserve manual labels, remove obsolete labels and are idempotent', async () => {
  const context = { repo: { owner: 'test', repo: 'rm' }, payload: { pull_request: { number: 1, changed_files: 1 } } };
  const listFiles = () => {};
  const listLabelsOnIssue = () => {};
  const names = new Set(['manual', 'rust']);
  const github = {
    rest: { pulls: { listFiles }, issues: {
      listLabelsOnIssue,
      addLabels: async ({ labels }) => labels.forEach(name => names.add(name)),
      removeLabel: async ({ name }) => names.delete(name),
    } },
    paginate: async fn => fn === listFiles ? [{ filename: 'docs/guide.md' }] : [...names].map(name => ({ name })),
  };
  await label({ github, context });
  await label({ github, context });
  assert.deepEqual([...names], ['manual', 'documentation']);
  context.payload.pull_request.changed_files = 2;
  await assert.rejects(label({ github, context }), /incomplete/);
  assert.deepEqual([...names], ['manual', 'documentation']);
});

function gate(selected) {
  return {
    changes: { result: 'success', outputs: Object.fromEntries(Object.entries(selected).map(([k, v]) => [k, String(v)])) },
    'workflow-check': { result: 'success' }, zizmor: { result: 'success' },
    frontend: { result: selected.frontend ? 'success' : 'skipped' },
    'frontend-build': { result: selected.frontendBuild ? 'success' : 'skipped' },
    ...Object.fromEntries(['fmt', 'clippy', 'test'].map(k => [k, { result: selected.rust ? 'success' : 'skipped' }])),
    build: { result: selected.build ? 'success' : 'skipped' },
  };
}
test('aggregate gate distinguishes deliberate skips from broken or cancelled checks', () => {
  assert.deepEqual(failures(gate(docs)), []);
  assert.deepEqual(failures(gate(full)), []);
  for (const result of ['skipped', 'failure', 'cancelled']) {
    const needs = gate(full);
    needs.test.result = result;
    assert.ok(failures(needs).length);
  }
  const needs = gate(docs);
  needs.changes.outputs = {};
  assert.ok(failures(needs).length);
  needs.changes.result = 'failure';
  assert.ok(failures(needs).some(error => error.startsWith('changes:')));
});

const run = (id, status = 'in_progress', conclusion = null, attempt = 1) => ({ id, status, conclusion, run_attempt: attempt });
const job = (name, conclusion, id = 1) => ({ name, conclusion, id, steps: [{ name: 'Test frontend', conclusion }] });
test('failure episode accumulates jobs, tracks fixes and creates a new episode after resolution', () => {
  let state = transition(null, run(1), [job('Frontend tests', 'failure')]);
  state = transition(state, run(1), [job('Frontend tests', 'failure'), job('Rust tests', 'failure', 2)]);
  assert.equal(state.entries.length, 2);
  state = transition(state, run(2), [job('Frontend tests', 'success', 3), job('Rust tests', 'failure', 4)]);
  assert.deepEqual(state.entries.map(entry => entry.status), ['fixed', 'failed']);
  state = transition(state, run(3, 'completed', 'success'), [job('Frontend tests', 'success', 5), job('Rust tests', 'success', 6)]);
  assert.equal(state.resolved, true);
  assert.match(render(state, 'https://github.com', 'test', 'rm'), /<summary>Fixed \(2\)<\/summary>/);
  assert.match(render(state, 'https://github.com', 'test', 'rm'), /actions\/runs\/2\/job\/4/);
  assert.deepEqual(readState(render(state, 'https://github.com', 'test', 'rm')), state);
  const next = transition(state, run(4), [job('Rust tests', 'failure', 7)]);
  assert.equal(next.entries.length, 1);
  assert.equal(next.resolved, false);
});

test('blocked checks are not fixed and superseded attempts cannot overwrite state', () => {
  const state = transition(null, run(5, 'completed', 'failure', 2), [job('Rust tests', 'failure')]);
  assert.equal(transition(state, run(4), []), null);
  assert.equal(transition(state, run(5, 'completed', 'success', 1), []), null);
  const waiting = transition(state, run(6), [job('Rust tests', 'skipped')]);
  assert.equal(waiting.entries[0].status, 'pending');
  assert.equal(waiting.resolved, false);
  assert.equal(transition(waiting, run(6, 'completed', 'success'), []).resolved, true);
  const cancelled = transition(state, run(6, 'completed', 'cancelled'), [job('Rust tests', 'cancelled')]);
  assert.equal(cancelled.resolved, false);
});

test('latest retry wins; aggregate failures do not duplicate real causes', () => {
  const state = transition(null, run(1), [job('Rust tests', 'failure', 1), job('Rust tests', 'success', 2), job('CI passed', 'failure', 3)]);
  assert.deepEqual(state.entries.map(entry => entry.name), ['CI passed']);
  const failed = transition(null, run(1), [job('Rust tests', 'failure', 1), job('CI passed', 'failure', 3)]);
  assert.deepEqual(failed.entries.map(entry => entry.name), ['Rust tests']);
  const startup = transition(null, run(1, 'completed', 'failure'), []);
  assert.equal(startup.entries[0].name, 'CI workflow');
  assert.equal(startup.entries[0].status, 'failed');
  assert.equal(transition(startup, run(2, 'completed', 'success'), []).resolved, true);
});

test('comment content cannot inject HTML, mentions or job log URLs', () => {
  const state = transition(null, run(1), [job('<script>@everyone [bad](https://evil.test)</script>', 'failure')]);
  const body = render(state, 'https://github.com', 'test', 'rm');
  assert.doesNotMatch(body, /<script>|@everyone|\[bad\]\(https/);
  assert.match(body, /&lt;script&gt;/);
  assert.equal(readState('<!-- rm-ci-report-v1:e30= -->'), null);
});

function harness() {
  const currentRun = { ...run(10), event: 'pull_request', path: '.github/workflows/ci.yml',
    head_sha: 'synthetic-sha', head_branch: 'feature', head_repository: { full_name: 'fork/rm', owner: { login: 'fork' } }, pull_requests: [] };
  const pull = { number: 1, state: 'open', head: { sha: 'synthetic-sha', ref: 'feature', repo: { full_name: 'fork/rm' } }, base: { repo: { full_name: 'test/rm' } } };
  const comments = [];
  const calls = { created: 0, updated: 0, minimized: 0, listHead: null };
  let currentJobs = [];
  const actions = {
    getWorkflowRun: async () => ({ data: currentRun }), listWorkflowRuns() {}, listJobsForWorkflowRun() {},
  };
  const pulls = { list() {}, get: async () => ({ data: pull }) };
  const issues = {
    listComments() {},
    createComment: async ({ body }) => {
      calls.created++;
      const comment = { id: comments.length + 1, node_id: 'synthetic-node', body, user: { login: 'github-actions[bot]', type: 'Bot' } };
      comments.push(comment);
      return { data: comment };
    },
    updateComment: async ({ comment_id, body }) => {
      calls.updated++;
      const comment = comments.find(item => item.id === comment_id);
      comment.body = body;
      return { data: comment };
    },
  };
  const github = {
    rest: { actions, pulls, issues }, graphql: async () => { calls.minimized++; },
    paginate: async (fn, params) => {
      if (fn === pulls.list) { calls.listHead = params.head; return [pull]; }
      if (fn === actions.listWorkflowRuns) return [currentRun];
      if (fn === actions.listJobsForWorkflowRun) return currentJobs;
      if (fn === issues.listComments) return comments;
      throw new Error('Unexpected API call');
    },
  };
  const context = { repo: { owner: 'test', repo: 'rm' }, payload: { workflow_run: { ...currentRun } } };
  const core = { info() {}, warning() {} };
  return { args: { github, context, core }, currentRun, pull, comments, calls, jobs: value => { currentJobs = value; } };
}

test('API reporter creates, edits, resolves and starts a fresh comment for fork PRs', async () => {
  const h = harness();
  h.jobs([job('Rust tests', 'failure')]);
  await snapshot(h.args);
  assert.equal(h.calls.listHead, 'fork:feature');
  assert.equal(h.calls.created, 1);
  await snapshot(h.args);
  assert.equal(h.calls.updated, 0);
  h.jobs([job('Rust tests', 'failure'), job('Frontend tests', 'failure', 2)]);
  await snapshot(h.args);
  assert.equal(h.calls.updated, 1);
  h.currentRun.status = 'completed';
  h.currentRun.conclusion = 'success';
  h.jobs([job('Rust tests', 'success', 3), job('Frontend tests', 'success', 4)]);
  await snapshot(h.args);
  assert.equal(h.calls.minimized, 1);
  assert.equal(readState(h.comments[0].body).resolved, true);
  h.currentRun.id = 11;
  h.currentRun.status = 'in_progress';
  h.currentRun.conclusion = null;
  h.args.context.payload.workflow_run = { ...h.currentRun };
  h.jobs([job('Rust tests', 'failure', 5)]);
  await snapshot(h.args);
  assert.equal(h.calls.created, 2);
});

test('stale/closed PRs, label events and newer rerun attempts never create comments', async () => {
  for (const mutate of [h => { h.pull.head.sha = 'new-sha'; }, h => { h.pull.state = 'closed'; },
    h => { h.currentRun.event = 'pull_request_target'; }, h => { h.currentRun.run_attempt = 2; }]) {
    const h = harness();
    h.jobs([job('Rust tests', 'failure')]);
    mutate(h);
    assert.equal(await snapshot(h.args), true);
    assert.equal(h.calls.created, 0);
  }
});

test('resolve API failure leaves retryable state without duplicating the comment', async () => {
  const h = harness();
  h.jobs([job('Rust tests', 'failure')]);
  await snapshot(h.args);
  h.currentRun.status = 'completed';
  h.currentRun.conclusion = 'success';
  h.jobs([job('Rust tests', 'success', 2)]);
  h.args.github.graphql = async () => { throw new Error('Synthetic moderation failure'); };
  await assert.rejects(snapshot(h.args), /moderation/);
  assert.equal(readState(h.comments[0].body).resolved, true);
  h.args.github.graphql = async () => { h.calls.minimized++; };
  await snapshot(h.args);
  assert.equal(h.calls.created, 1);
  assert.equal(h.calls.minimized, 1);
});

test('human marker copies are left untouched and a newer run supersedes reporting', async () => {
  const h = harness();
  const body = render(transition(null, run(1), [job('Rust tests', 'failure')]), 'https://github.com', 'test', 'rm');
  h.comments.push({ id: 100, body, user: { login: 'contributor', type: 'User' } });
  h.jobs([job('Rust tests', 'failure')]);
  await snapshot(h.args);
  assert.equal(h.comments[0].body, body);
  assert.equal(h.calls.created, 1);
  assert.equal(h.calls.updated, 0);
  const paginate = h.args.github.paginate;
  h.args.github.paginate = async (fn, params) => fn === h.args.github.rest.actions.listWorkflowRuns
    ? [{ ...h.currentRun, id: 11 }] : paginate(fn, params);
  assert.equal(await snapshot(h.args), true);
  assert.equal(h.calls.created, 1);
});

test('commit changes during API reads prevent writing a stale comment', async () => {
  const h = harness();
  h.jobs([job('Rust tests', 'failure')]);
  let reads = 0;
  h.args.github.rest.pulls.get = async () => {
    if (++reads > 1) h.pull.head.sha = 'new-sha';
    return { data: h.pull };
  };
  assert.equal(await snapshot(h.args), true);
  assert.equal(h.calls.created, 0);
});

test('watcher waits only while the current run is active', async () => {
  const h = harness();
  let sleeps = 0;
  await report(h.args, { maxPolls: 2, sleep: async ms => { assert.equal(ms, 20000); sleeps++; h.currentRun.status = 'completed'; } });
  assert.equal(sleeps, 1);
  assert.equal(h.calls.created, 0);
});
