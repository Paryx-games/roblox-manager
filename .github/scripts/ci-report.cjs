const marker = 'rm-ci-report-v1';
const bad = new Set(['failure', 'timed_out', 'cancelled', 'action_required', 'startup_failure', 'stale']);

function escape(text) {
  return String(text).slice(0, 180).replace(/[\r\n]/g, ' ')
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/([\\`*_[\]()])/g, '\\$1').replace(/@/g, '@\u200b');
}

function readState(body) {
  const match = body?.match(/<!-- rm-ci-report-v1:([A-Za-z0-9+/=]+) -->/);
  if (!match || match[1].length > 50000) return null;
  try {
    const state = JSON.parse(Buffer.from(match[1], 'base64').toString('utf8'));
    if (state.version !== 1 || typeof state.resolved !== 'boolean' ||
        !Number.isSafeInteger(state.runId) || !Number.isSafeInteger(state.attempt) ||
        !Array.isArray(state.entries) || state.entries.length > 100 ||
        state.entries.some(entry => typeof entry.name !== 'string' ||
          typeof entry.reason !== 'string' || !['failed', 'pending', 'fixed'].includes(entry.status))) return null;
    return state;
  } catch { return null; }
}

function latestJobs(jobs) {
  const latest = new Map();
  for (const job of jobs) {
    if (!latest.has(job.name) || job.id > latest.get(job.name).id) latest.set(job.name, job);
  }
  return latest;
}

function transition(previous, run, jobs) {
  if (previous && (previous.runId > run.id ||
      (previous.runId === run.id && previous.attempt > run.run_attempt))) return null;
  const state = {
    version: 1, runId: run.id, attempt: run.run_attempt, resolved: false,
    entries: previous && !previous.resolved ? previous.entries.map(entry => ({ ...entry })) : [],
  };
  const current = latestJobs(jobs);
  const failures = [...current.values()].filter(job => bad.has(job.conclusion));
  if (!failures.length && run.status === 'completed' && bad.has(run.conclusion)) {
    const workflowFailure = { name: 'CI workflow', conclusion: run.conclusion, steps: [] };
    current.set(workflowFailure.name, workflowFailure);
    failures.push(workflowFailure);
  }
  // the aggregate gate repeats other failures; show it only if it is the sole failure
  const relevant = failures.some(job => job.name !== 'CI passed')
    ? failures.filter(job => job.name !== 'CI passed') : failures;
  for (const job of relevant) {
    const steps = (job.steps ?? []).filter(step => bad.has(step.conclusion));
    const reason = steps.length
      ? `${job.conclusion}: ${steps.map(step => step.name).join(', ')}`
      : `Job ${job.conclusion}; open the job log for details.`;
    const entry = state.entries.find(item => item.name === job.name);
    const update = { name: job.name, reason: reason.slice(0, 800), jobId: job.id, runId: run.id, status: 'failed' };
    if (entry) Object.assign(entry, update);
    else state.entries.push(update);
  }
  for (const entry of state.entries) {
    const job = current.get(entry.name);
    if (job?.conclusion === 'success' ||
        (run.conclusion === 'success' && (!job || job.conclusion === 'skipped'))) {
      entry.status = 'fixed';
    } else if (!job || !job.conclusion || job.conclusion === 'skipped') {
      // running, blocked and skipped jobs are not evidence of a fix
      if (entry.status !== 'fixed') entry.status = 'pending';
    }
  }
  state.resolved = run.status === 'completed' && run.conclusion === 'success' &&
    state.entries.length > 0 && state.entries.every(entry => entry.status === 'fixed');
  return state;
}

function render(state, server, owner, repo) {
  const base = `${server}/${owner}/${repo}/actions/runs/${state.runId}`;
  const line = entry => {
    const link = Number.isSafeInteger(entry.jobId) && Number.isSafeInteger(entry.runId)
      ? `${server}/${owner}/${repo}/actions/runs/${entry.runId}/job/${entry.jobId}` : base;
    return `- ${entry.status === 'fixed' ? '[x]' : '[ ]'} **${escape(entry.name)}** - ${
      entry.status === 'fixed' ? 'Passed or no longer required by the successful run.' :
      entry.status === 'pending' ? 'Waiting for a successful check.' : escape(entry.reason)
    } [Job log](${link})`;
  };
  const open = state.entries.filter(entry => entry.status !== 'fixed');
  const fixed = state.entries.filter(entry => entry.status === 'fixed');
  return [
    `<!-- ${marker}:${Buffer.from(JSON.stringify(state)).toString('base64')} -->`,
    `### CI ${state.resolved ? 'resolved' : 'needs attention'}`, '',
    `[Latest CI run](${base}) - attempt ${state.attempt}`, '',
    ...open.map(line), '',
    ...(fixed.length ? ['<details>', `<summary>Fixed (${fixed.length})</summary>`, '',
      ...fixed.map(line), '', '</details>', ''] : []),
    'Failed jobs and steps are listed above. Full diagnostics stay in the job logs.',
  ].join('\n');
}

async function findPull(github, context, run) {
  const { owner, repo } = context.repo;
  const listed = run.pull_requests ?? [];
  const candidates = listed.length ? listed : await github.paginate(github.rest.pulls.list, {
    owner, repo, state: 'open', head: `${run.head_repository.owner.login}:${run.head_branch}`, per_page: 100,
  });
  for (const candidate of candidates) {
    const { data: pull } = await github.rest.pulls.get({ owner, repo, pull_number: candidate.number });
    if (pull.state === 'open' && pull.head.sha === run.head_sha &&
        pull.head.repo?.full_name === run.head_repository.full_name && pull.head.ref === run.head_branch &&
        pull.base.repo.full_name === `${owner}/${repo}`) return pull;
  }
  return null;
}

async function minimize(github, comment) {
  await github.graphql(`mutation($id: ID!) {
    minimizeComment(input: { subjectId: $id, classifier: RESOLVED }) {
      minimizedComment { isMinimized }
    }
  }`, { id: comment.node_id });
}

async function snapshot({ github, context, core }) {
  const { owner, repo } = context.repo;
  const eventRun = context.payload.workflow_run;
  const { data: run } = await github.rest.actions.getWorkflowRun({ owner, repo, run_id: eventRun.id });
  if (run.event !== 'pull_request' || run.path !== '.github/workflows/ci.yml' ||
      run.run_attempt !== eventRun.run_attempt || !run.head_repository) return true;
  const pull = await findPull(github, context, run);
  if (!pull) return true;
  const recent = await github.paginate(github.rest.actions.listWorkflowRuns, {
    owner, repo, workflow_id: 'ci.yml', event: 'pull_request', head_sha: run.head_sha, per_page: 100,
  });
  if (recent.some(other => other.id > run.id && other.head_branch === run.head_branch &&
      other.head_repository?.full_name === run.head_repository.full_name)) return true;
  const jobs = await github.paginate(github.rest.actions.listJobsForWorkflowRun, {
    owner, repo, run_id: run.id, filter: 'all', per_page: 100,
  });
  const comments = await github.paginate(github.rest.issues.listComments, {
    owner, repo, issue_number: pull.number, per_page: 100,
  });
  const owned = comments.filter(comment => comment.user?.login === 'github-actions[bot]' &&
    comment.user?.type === 'Bot' && readState(comment.body));
  // retry a failed minimize operation without reopening or duplicating the episode
  for (const comment of owned) {
    const state = readState(comment.body);
    if (state.resolved && state.runId === run.id && state.attempt === run.run_attempt) {
      await minimize(github, comment);
    }
  }
  const comment = owned.filter(item => !readState(item.body).resolved).at(-1);
  const previous = comment ? readState(comment.body) : null;
  const state = transition(previous, run, jobs);
  if (!state || !state.entries.length) return run.status === 'completed';
  const body = render(state, context.serverUrl ?? 'https://github.com', owner, repo);
  // recheck after API reads: never attach a stale failure or recovery to a new commit
  const { data: freshPull } = await github.rest.pulls.get({ owner, repo, pull_number: pull.number });
  const { data: freshRun } = await github.rest.actions.getWorkflowRun({ owner, repo, run_id: run.id });
  if (freshPull.state !== 'open' || freshPull.head.sha !== run.head_sha || freshRun.run_attempt !== run.run_attempt) return true;
  let saved = comment;
  if (!comment) {
    ({ data: saved } = await github.rest.issues.createComment({ owner, repo, issue_number: pull.number, body }));
  } else if (comment.body !== body) {
    ({ data: saved } = await github.rest.issues.updateComment({ owner, repo, comment_id: comment.id, body }));
  }
  if (state.resolved) await minimize(github, saved);
  core.info(state.resolved ? 'CI comment resolved.' : 'CI failure comment is up to date.');
  return run.status === 'completed';
}

async function report(args, { sleep = ms => new Promise(resolve => setTimeout(resolve, ms)), maxPolls = 180 } = {}) {
  // a completed-event reporter provides the final update even if this watcher times out
  for (let poll = 0; poll < maxPolls; poll++) {
    if (await snapshot(args)) return;
    await sleep(20000);
  }
  args.core.warning('Live CI watcher reached its time limit; completion will trigger a final update.');
}

module.exports = { readState, transition, render, findPull, snapshot, report };
