// Small GitHub Actions monitor for this repository. Credentials never leave memory.
const { execFileSync } = require('node:child_process');
const repo = 'Skale-Club/' + require('../package.json').name;
const args = process.argv.slice(2);
if (!args.length || args[0] === '--help') {
  console.log('Usage: node scripts/ci_monitor.cjs runs | run <id> | jobs <id> | secrets');
  process.exit(0);
}
async function main() {
  const headers = { Accept: 'application/vnd.github+json', 'User-Agent': 'xpace-ci-monitor' };
  if (args[0] === 'secrets') {
    const value = execFileSync('git', ['-c', 'credential.interactive=never', 'credential', 'fill'], {
      input: 'protocol=https\nhost=github.com\n\n', encoding: 'utf8',
      env: { ...process.env, GCM_INTERACTIVE: 'Never', GIT_TERMINAL_PROMPT: '0' }, stdio: ['pipe', 'pipe', 'pipe'],
    });
    const token = value.split(/\r?\n/).find(line => line.startsWith('password='))?.slice(9);
    if (!token) throw new Error('No non-interactive GitHub credential is available.');
    headers.Authorization = `Bearer ${token}`;
  }
  const routes = { runs: 'actions/runs?per_page=5', run: `actions/runs/${Number(args[1])}`, jobs: `actions/runs/${Number(args[1])}/jobs`, secrets: 'actions/secrets' };
  const route = routes[args[0]];
  if (!route) throw new Error('Unknown command; use --help.');
  const response = await fetch(`https://api.github.com/repos/${repo}/${route}`, { headers });
  if (!response.ok) throw new Error(`GitHub returned HTTP ${response.status}`);
  const data = await response.json();
  if (args[0] === 'secrets') {
    const orgResponse = await fetch(`https://api.github.com/repos/${repo}/actions/organization-secrets`, { headers });
    const org = orgResponse.ok ? await orgResponse.json() : { secrets: [] };
    console.log(JSON.stringify({ repository: data.secrets.map(secret => secret.name), organization: org.secrets.map(secret => secret.name) }));
  }
  else if (args[0] === 'runs') console.log(JSON.stringify(data.workflow_runs.map(run => ({id:run.id,name:run.name,status:run.status,conclusion:run.conclusion,head:run.head_sha,url:run.html_url})), null, 2));
  else if (args[0] === 'jobs') console.log(JSON.stringify(data.jobs.map(job => ({id:job.id,name:job.name,status:job.status,conclusion:job.conclusion,steps:job.steps})), null, 2));
  else console.log(JSON.stringify({id:data.id,status:data.status,conclusion:data.conclusion,url:data.html_url,head:data.head_sha}, null, 2));
}
main().catch(error => { console.error(error instanceof Error && error.message.startsWith('Command failed') ? 'GitHub credential lookup failed.' : error.message); process.exitCode = 1; });
