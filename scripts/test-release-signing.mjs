// Native public Sigstore acceptance. Requires network; no signing credentials.
// Fixture signatures/certificates stay temporary and are never committed.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const pins = {
  'linux-x64': ['linux-amd64', '4629c757b7618056f8ddd7e2625ae9fdd94c0372a65049520bc7d9df9efc7f71', 'linux-x86_64'],
  'linux-arm64': ['linux-arm64', 'c5d324e091826b0d7a78eb16fef316450b4eb9aaec045611c08ba06f5e73220a', 'linux-aarch64'],
  'darwin-x64': ['darwin-amd64', '2347488e5d5b25336644024dfeca5601b190e91197a71a917bda44744aff106c', 'macos-x86_64'],
  'darwin-arm64': ['darwin-arm64', '5cf948c2f4dfe59687bdd0b8523709067383e03982cc543475c8a7dc70e92a76', 'macos-aarch64'],
  'win32-x64': ['windows-amd64.exe', '9fe59be0eca1271873ce019061335eb1ac419b7059202e797828467ddabe33be', 'windows-x86_64'],
};
const [asset, pin, label] = pins[`${process.platform}-${process.arch}`] ?? [];
assert.ok(asset, 'unsupported signature acceptance platform');
const root = fileURLToPath(new URL('../', import.meta.url));
const tempRoot = path.resolve(os.tmpdir());
const dir = fs.mkdtempSync(path.join(tempRoot, 'jarvis-signature-'));
const verifier = path.join(dir, process.platform === 'win32' ? 'cosign.exe' : 'cosign');
const base = 'https://github.com/sigstore/cosign/releases/download/v3.1.3/';
function run(program, args, options = {}) {
  const result = spawnSync(program, args, { encoding: 'utf8', windowsHide: true, timeout: 150000, maxBuffer: 1024 * 1024, ...options });
  assert.ifError(result.error);
  return result;
}
function download(assetName, destination, limit) {
  const result = run(process.platform === 'win32' ? 'curl.exe' : 'curl', ['-q', '--fail', '--silent', '--show-error',
    '--location', '--proto', '=https', '--proto-redir', '=https', '--tlsv1.2', '--max-redirs', '5',
    '--connect-timeout', '15', '--max-time', '120', '--max-filesize', String(limit), '-o', destination, base + assetName]);
  assert.equal(result.status, 0, result.stderr);
}
try {
  download('cosign-' + asset, verifier, 268435456);
  assert.equal(createHash('sha256').update(fs.readFileSync(verifier)).digest('hex'), pin, 'verifier digest');
  fs.chmodSync(verifier, 0o700);
  const manifest = path.join(dir, 'release-manifest.txt'), bundle = path.join(dir, 'release-manifest.sigstore.json');
  download('cosign_checksums.txt', manifest, 8192);
  download('cosign_checksums.txt.sigstore.json', bundle, 1048576);
  const home = path.join(dir, 'home'); fs.mkdirSync(home);
  const env = { HOME: home, USERPROFILE: home, LOCALAPPDATA: home, APPDATA: home, XDG_CACHE_HOME: home, TEMP: dir, TMPDIR: dir };
  if (process.platform === 'win32') env.SystemRoot = process.env.SystemRoot;
  const identity = 'keyless@projectsigstore.iam.gserviceaccount.com', issuer = 'https://accounts.google.com';
  function verify(file, subject = identity, authority = issuer) {
    return run(verifier, ['verify-blob', '--timeout', '2m', '--bundle', bundle, '--certificate-identity', subject,
      '--certificate-oidc-issuer', authority, file], { env });
  }
  const valid = verify(manifest);
  assert.equal(valid.status, 0, valid.stderr);
  for (const [name, result] of [
    ['wrong identity', verify(manifest, 'https://github.com/foreign/repository')],
    ['wrong issuer', verify(manifest, identity, 'https://token.actions.githubusercontent.com')],
  ]) assert.notEqual(result.status, 0, `${name} accepted`);
  const changed = path.join(dir, 'tampered.txt');
  fs.writeFileSync(changed, fs.readFileSync(manifest, 'utf8') + 'tampered\n');
  assert.notEqual(verify(changed).status, 0, 'tampered signed bytes accepted');

  // The actual installer must reject a valid signature from a different publisher.
  // Only replace download with the already pinned verifier to avoid downloading it twice.
  const tag = 'ci-' + 'a'.repeat(40);
  let production;
  if (process.platform === 'win32') {
    const quote = value => "'" + value.replaceAll("'", "''") + "'";
    const script = path.join(dir, 'production.ps1');
    fs.writeFileSync(script, `
$ErrorActionPreference = 'Stop'
. ${quote(path.join(root, 'scripts', 'install-prebuilt.ps1'))}
function Invoke-JarvisDownload([string]$Url, [string]$Destination) {
  if ($Url -cne '${base}cosign-${asset}') { throw 'Unexpected verifier URL' }
  if ($Destination -cne ${quote(verifier)}) { throw 'Unexpected verifier destination' }
}
Test-JarvisReleaseSignature ${quote(dir)} '${tag}'
`);
    // A Node child does not get pwsh's automatic PS5.1 module-path adjustment.
    const shellEnv = Object.fromEntries(Object.entries(process.env).filter(([name]) => name.toLowerCase() !== 'psmodulepath'));
    production = run('powershell.exe', ['-NoProfile', '-File', script], { env: shellEnv });
  } else {
    const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
    production = run('/bin/bash', ['-c', `
source ${quote(path.join(root, 'scripts', 'install-prebuilt.sh'))}
if command -v sha256sum >/dev/null; then hash=(sha256sum); else hash=(shasum -a 256); fi
download() { [ "$1" = '${base}cosign-${asset}' ] && [ "$2" = ${quote(verifier)} ]; }
verify_release_signature ${quote(dir)} '${label}' '${tag}'
`]);
  }
  assert.notEqual(production.status, 0, 'installer accepted another publisher');
  assert.match(production.stderr, /Release signature verification failed/);
  console.log(`Real Sigstore verification passed on ${process.platform}/${process.arch}; tamper, issuer, identity and installer publisher guards refused.`);
} finally {
  assert.equal(path.dirname(path.resolve(dir)), tempRoot);
  assert.ok(path.basename(dir).startsWith('jarvis-signature-'));
  fs.rmSync(dir, { recursive: true, force: true });
}
