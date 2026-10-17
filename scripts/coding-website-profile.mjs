import assert from 'node:assert/strict';
import { realpath } from 'node:fs/promises';
import { basename, dirname } from 'node:path';

// Resolve aliases before accepting a profile that the harness may reconfigure.
export async function disposableProfile(evidence, requested) {
  const root = await realpath(requested);
  assert.equal(dirname(root), await realpath(evidence), 'only this harness\'s disposable profiles may be continued');
  assert(basename(root).startsWith('run-'), 'not a disposable website profile');
  return root;
}
