#!/usr/bin/env bash
# Real tar/gzip and shell execution; network/signature seams test installer ordering.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
installer=${1:-$root/scripts/install-prebuilt.sh}
fixture=$(mktemp -d)
trap 'rm -rf -- "$fixture"' EXIT
export FIXTURE=$fixture
name=jarvis-0.1.0-linux-x86_64
mkdir -p "$fixture/contents/$name" "$fixture/tmp"
export TMPDIR="$fixture/tmp"
cat > "$fixture/contents/$name/jarvis" <<'PROGRAM'
#!/usr/bin/env bash
set -eu
printf '%s\n' "$*" >> "$FIXTURE/calls"
case "$1" in
  version) printf 'jarvis-cli %s\n' "${FIXTURE_VERSION:-0.1.0}" ;;
  install)
    [ "${FIXTURE_INSTALL_FAIL:-0}" = 0 ] || exit 17
    [ "$2" = --dir ]; mkdir -p "$3"; cp "$0" "$3/jarvis"
    ;;
  *) exit 18 ;;
esac
PROGRAM
tar -cf "$fixture/good.tar" -C "$fixture/contents" "$name"
gzip -c "$fixture/good.tar" > "$fixture/good.tar.gz"
cat > "$fixture/run.sh" <<'RUN'
source "$INSTALLER"
original_verify=$(declare -f verify_release_signature)
eval "${original_verify/verify_release_signature/real_verify_release_signature}"
verify_release_signature() {
  if [ "${FIXTURE_VERIFY_PIN:-0}" = 1 ]; then real_verify_release_signature "$@"; return; fi
  printf 'verify\n' >> "$FIXTURE/signatures"
  [ "${FIXTURE_SIGNATURE_FAIL:-0}" = 0 ] || die 'fixture signature refused'
}
uname() { if [ "$1" = -s ]; then printf '%s' "${FIXTURE_OS:-Linux}"; else printf '%s' "${FIXTURE_ARCH:-x86_64}"; fi; }
download() {
  printf '%s\n' "$1" >> "$FIXTURE/downloads"
  [ "${FIXTURE_DOWNLOAD_FAIL:-0}" = 0 ] || die 'fixture download failed'
  case "$1" in
    */release-manifest.txt) cp "$FIXTURE/manifest" "$2" ;;
    */release-manifest.sigstore.json) printf 'fixture bundle' > "$2" ;;
    */cosign-*) printf 'untrusted verifier' > "$2" ;;
    *) cp "$FIXTURE_ARCHIVE" "$2" ;;
  esac
}
main "$@"
RUN
export INSTALLER=$installer FIXTURE_ARCHIVE="$fixture/good.tar.gz"
if command -v sha256sum >/dev/null; then hash=(sha256sum); else hash=(shasum -a 256); fi
digest() { local value; value=$("${hash[@]}" "$1"); printf '%s' "${value%% *}"; }
sha=$(digest "$FIXTURE_ARCHIVE")
tag=ci-0123456789012345678901234567890123456789
run() {
  local args=("$tag" 0.1.0)
  [ -z "$sha" ] || args+=("$sha")
  bash "$fixture/run.sh" "${args[@]}" --dir "$fixture/install here" --no-path > "$fixture/output" 2>&1
}
reset_case() { rm -rf "$fixture/install here" "$fixture/calls" "$fixture/downloads" "$fixture/signatures"; }
refused() {
  local expected=$1
  if run; then cat "$fixture/output"; echo "expected refusal: $expected" >&2; exit 1; fi
  grep -q "$expected" "$fixture/output" || { cat "$fixture/output"; exit 1; }
  [ ! -e "$fixture/install here/jarvis" ]
  ! grep -q 'JARVIS is installed' "$fixture/output"
  [ -z "$(ls -A "$fixture/tmp")" ]
}

run
[ -f "$fixture/install here/jarvis" ]
grep -q '^version$' "$fixture/calls"
grep -q 'install --dir .*install here --no-path' "$fixture/calls"
grep -q 'Setup has not run' "$fixture/output"
grep -q "^https://github.com/props-nothing/jarvis-core/releases/download/$tag/$name.tar.gz$" "$fixture/downloads"
[ -z "$(ls -A "$fixture/tmp")" ]
if run; then echo 'replaced an existing install' >&2; exit 1; fi
[ "$(wc -l < "$fixture/downloads")" -eq 1 ]
reset_case

# Signed mode never falls back to a checksum supplied by the same release server.
{
  printf 'JARVIS-RELEASE-1\nrepository=props-nothing/jarvis-core\ntag=%s\ncommit=%s\nversion=0.1.0\n' "$tag" "${tag#ci-}"
  for label in linux-x86_64 linux-aarch64 macos-aarch64 macos-x86_64 windows-x86_64; do
    extension=tar.gz; [ "$label" != windows-x86_64 ] || extension=zip
    printf '%s\tjarvis-0.1.0-%s.%s\t%s\t%s\n' "$label" "$label" "$extension" "$(wc -c < "$FIXTURE_ARCHIVE" | tr -d ' ')" "$sha"
  done
} > "$fixture/manifest.good"
cp "$fixture/manifest.good" "$fixture/manifest"
sha=''
run
[ -f "$fixture/install here/jarvis" ]
[ "$(wc -l < "$fixture/downloads")" -eq 3 ]
reset_case
export FIXTURE_SIGNATURE_FAIL=1
refused 'fixture signature refused'
[ ! -e "$fixture/calls" ]; [ "$(wc -l < "$fixture/downloads")" -eq 2 ]
unset FIXTURE_SIGNATURE_FAIL
reset_case
export FIXTURE_VERIFY_PIN=1
refused 'Signature verifier checksum mismatch'
[ ! -e "$fixture/calls" ]; [ "$(wc -l < "$fixture/downloads")" -eq 3 ]
unset FIXTURE_VERIFY_PIN
reset_case
for replacement in 's/repository=props-nothing/repository=foreign/' 's/tag=ci-/tag=ci-f/' 's/commit=0/commit=f/' 's/version=0.1.0/version=0.2.0/' 's/linux-x86_64/linux-unknown/' 's/\.tar.gz/\.zip/'; do
  sed "$replacement" "$fixture/manifest.good" > "$fixture/manifest"
  refused 'manifest\|Manifest'
  [ ! -e "$fixture/calls" ]; [ "$(wc -l < "$fixture/downloads")" -eq 2 ]
  reset_case
done
cp "$fixture/manifest.good" "$fixture/manifest"
printf 'extra\n' >> "$fixture/manifest"
refused 'Unexpected manifest content'
[ ! -e "$fixture/calls" ]
reset_case
sed "s/$(digest "$FIXTURE_ARCHIVE")/$(printf '%064d' 0)/g" "$fixture/manifest.good" > "$fixture/manifest"
refused 'SHA-256 mismatch'
[ ! -e "$fixture/calls" ]
reset_case

sha=$(printf '%064d' 0)
refused 'SHA-256 mismatch'
[ ! -e "$fixture/calls" ]
sha=$(digest "$FIXTURE_ARCHIVE")
export FIXTURE_DOWNLOAD_FAIL=1
refused 'fixture download failed'
[ ! -e "$fixture/calls" ]; unset FIXTURE_DOWNLOAD_FAIL
export FIXTURE_VERSION=9.9.9
refused 'version does not match'
! grep -q '^install ' "$fixture/calls"; unset FIXTURE_VERSION
reset_case
export FIXTURE_INSTALL_FAIL=1
refused 'Installation failed'; unset FIXTURE_INSTALL_FAIL
reset_case
export FIXTURE_ARCH=riscv64
refused 'Unsupported platform'
[ ! -e "$fixture/downloads" ]; unset FIXTURE_ARCH
tag=latest
refused 'exact ci-'
[ ! -e "$fixture/downloads" ]
tag=v0.1.0

# Duplicate selected members, directories, links and a missing member must not run.
cp "$fixture/good.tar" "$fixture/duplicate.tar"
tar -rf "$fixture/duplicate.tar" -C "$fixture/contents" "$name/jarvis"
mkdir -p "$fixture/directory/$name/jarvis" "$fixture/missing"
tar -cf "$fixture/directory.tar" -C "$fixture/directory" "$name"
printf 'unrelated' > "$fixture/missing/other"
tar -cf "$fixture/missing.tar" -C "$fixture/missing" other
for kind in duplicate directory missing; do
  gzip -c "$fixture/$kind.tar" > "$fixture/$kind.tar.gz"
  export FIXTURE_ARCHIVE="$fixture/$kind.tar.gz"
  sha=$(digest "$FIXTURE_ARCHIVE")
  refused 'Archive'
  [ ! -e "$fixture/calls" ]
done
reset_case
# A small compressed download cannot expand without bound before tar is read.
dd if=/dev/zero bs=1048576 count=513 2>/dev/null | gzip -c > "$fixture/bomb.tar.gz"
export FIXTURE_ARCHIVE="$fixture/bomb.tar.gz"
sha=$(digest "$FIXTURE_ARCHIVE")
refused 'limit\|exceeds'
[ ! -e "$fixture/calls" ]
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) ;; # Windows Git Bash may emulate symlinks by copying.
  *)
    mkdir -p "$fixture/link/$name"
    ln -s /bin/sh "$fixture/link/$name/jarvis"
    tar -czf "$fixture/link.tar.gz" -C "$fixture/link" "$name"
    export FIXTURE_ARCHIVE="$fixture/link.tar.gz"; sha=$(digest "$FIXTURE_ARCHIVE")
    refused 'regular executable'; [ ! -e "$fixture/calls" ] ;;
esac

# Unit-test all target mappings without claiming execution on those architectures.
source "$installer"
[ "$(platform Linux arm64)" = linux-aarch64 ]
[ "$(platform Darwin arm64)" = macos-aarch64 ]
[ "$(platform Darwin x86_64)" = macos-x86_64 ]
# Exercise the transport's version floor and exact safety options without network.
curl() {
  if [ "$2" = --version ]; then printf 'curl %s fixture\n' "${FIXTURE_CURL_VERSION:-8.4.0}"; return; fi
  [ "$*" = "-q --fail --silent --show-error --location --proto =https --proto-redir =https --tlsv1.2 --max-redirs 5 --connect-timeout 15 --max-time 120 --max-filesize 268435456 --output $fixture/download https://github.com/fixture" ] || return 23
  printf called > "$fixture/transport"
}
download https://github.com/fixture "$fixture/download"
[ -f "$fixture/transport" ]; rm "$fixture/transport"
export FIXTURE_CURL_VERSION=8.3.0
if (download https://github.com/fixture "$fixture/download") > "$fixture/output" 2>&1; then echo 'old curl accepted' >&2; exit 1; fi
[ ! -e "$fixture/transport" ]
echo 'Prebuilt Bash install, hash/selector/archive refusal, failure propagation and cleanup passed.'
