import fs from 'node:fs';
import { pathToFileURL } from 'node:url';

export function releaseSource(eventName, event, { repository, sha, ref }) {
  if (eventName === 'workflow_run') {
    const run = event.workflow_run;
    if (run?.name !== 'CI' || run.status !== 'completed' || run.conclusion !== 'success' || run.event !== 'push'
        || run.head_branch !== 'main' || run.head_repository?.full_name !== repository) {
      return null;
    }
    sha = run.head_sha;
  } else if (eventName !== 'workflow_dispatch'
      && !(eventName === 'push' && /^refs\/tags\/v.+/.test(ref))) {
    return null;
  }
  return typeof sha === 'string' && /^[0-9a-f]{40}$/.test(sha) ? sha : null;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const event = JSON.parse(fs.readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
  const sha = releaseSource(process.env.GITHUB_EVENT_NAME, event, {
    repository: process.env.GITHUB_REPOSITORY,
    sha: process.env.GITHUB_SHA,
    ref: process.env.GITHUB_REF,
  });
  fs.appendFileSync(process.env.GITHUB_OUTPUT, `ready=${sha !== null}\nsha=${sha ?? ''}\n`);
  console.log(sha ? `Release source: ${sha}` : 'No eligible successful CI push; packaging skipped.');
}
