const groups = {
  frontend: ['frontend'], frontendBuild: ['frontend-build'],
  rust: ['fmt', 'clippy', 'test'], build: ['build'],
};

function failures(needs) {
  const errors = [];
  for (const key of ['changes', 'workflow-check', 'zizmor']) {
    if (needs[key]?.result !== 'success') errors.push(`${key}: ${needs[key]?.result ?? 'missing'}`);
  }
  for (const [flag, jobs] of Object.entries(groups)) {
    const value = needs.changes?.outputs?.[flag];
    if (value !== 'true' && value !== 'false') {
      errors.push(`Missing or invalid selection: ${flag}`);
      continue;
    }
    for (const key of jobs) {
      const expected = value === 'true' ? 'success' : 'skipped';
      if (needs[key]?.result !== expected) errors.push(`${key}: expected ${expected}, got ${needs[key]?.result ?? 'missing'}`);
    }
  }
  return errors;
}

if (require.main === module) {
  const errors = failures(JSON.parse(process.env.CI_NEEDS));
  if (errors.length) {
    console.error(errors.join('\n'));
    process.exitCode = 1;
  } else {
    console.log('All required checks passed; only unneeded checks were skipped.');
  }
}

module.exports = { failures };
