#!/usr/bin/env bash
# Opt-in UNSIGNED prebuilt first install. Requires Bash 3.2+ and curl 8.4+.
# Usage: bash install-prebuilt.sh TAG VERSION SHA256 [--dir ABSOLUTE_DIR] [--no-path]
# Obtain the exact platform archive's digest from the selected release before running.
set -euo pipefail

die() { printf 'jarvis installer: %s\n' "$*" >&2; exit 1; }

platform() {
  case "$1/$2" in
    Linux/x86_64) printf linux-x86_64 ;;
    Linux/aarch64|Linux/arm64) printf linux-aarch64 ;;
    Darwin/x86_64) printf macos-x86_64 ;;
    Darwin/arm64|Darwin/aarch64) printf macos-aarch64 ;;
    *) die "Unsupported platform: $1/$2. Windows uses install-prebuilt.ps1." ;;
  esac
}

download() {
  # -q must be first: a curlrc must not change the verification/transport contract.
  local info major minor
  info=$(curl -q --version) || die 'curl is required.'
  [[ "$info" =~ ^curl\ ([0-9]+)\.([0-9]+)\. ]] || die 'Could not identify curl.'
  major=${BASH_REMATCH[1]}; minor=${BASH_REMATCH[2]}
  (( major > 8 || (major == 8 && minor >= 4) )) || die 'curl 8.4 or newer is required for bounded downloads.'
  curl -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
    --tlsv1.2 --max-redirs 5 --connect-timeout 15 --max-time 120 --max-filesize 268435456 \
    --output "$2" "$1" || die 'Download failed; installation was not run.'
}

main() (
  [ "$#" -ge 3 ] || die 'Usage: install-prebuilt.sh TAG VERSION SHA256 [--dir ABSOLUTE_DIR] [--no-path]'
  local tag=$1 version=$2 digest=$3 dir=${HOME:-}/.local/share/jarvis no_path=0 label name stage actual listing bytes reported
  shift 3
  unset TAR_OPTIONS GZIP
  [[ "$tag" =~ ^ci-[0-9a-f]{40}$ || "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || die 'Choose an exact ci-<commit> or vMAJOR.MINOR.PATCH release tag.'
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die 'Version must be MAJOR.MINOR.PATCH.'
  [[ "$digest" =~ ^[0-9a-fA-F]{64}$ ]] || die 'Supply the selected platform archive SHA-256 (64 hexadecimal characters).'
  [ -n "${HOME:-}" ] || die 'HOME is required.'
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --dir) [ "$#" -ge 2 ] || die '--dir requires an absolute directory.'; dir=$2; shift 2 ;;
      --no-path) no_path=1; shift ;;
      *) die "Unknown option: $1" ;;
    esac
  done
  [[ "$dir" = /* && "$dir" != *$'\n'* && "$dir" != *$'\r'* ]] || die '--dir must be an absolute directory.'
  [ ! -e "$dir/jarvis" ] && [ ! -L "$dir/jarvis" ] || die 'An installation already exists there. This first-install bootstrap does not update it.'
  label=$(platform "$(uname -s)" "$(uname -m)")
  for tool in curl tar gzip head wc chmod mktemp tr; do command -v "$tool" >/dev/null || die "$tool is required."; done
  if command -v sha256sum >/dev/null; then hash=(sha256sum); else command -v shasum >/dev/null || die 'shasum or sha256sum is required.'; hash=(shasum -a 256); fi
  name="jarvis-$version-$label"
  umask 077
  stage=$(mktemp -d "${TMPDIR:-/tmp}/jarvis-prebuilt.XXXXXXXX")
  trap 'rm -rf -- "$stage"' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  printf 'Unsigned prebuilt install: %s, %s. Verifying the supplied SHA-256.\n' "$tag" "$label"
  download "https://github.com/props-nothing/jarvis-core/releases/download/$tag/$name.tar.gz" "$stage/archive.tar.gz"
  actual=$("${hash[@]}" "$stage/archive.tar.gz")
  actual=${actual%% *}
  digest=$(printf '%s' "$digest" | tr 'A-F' 'a-f')
  [ "$actual" = "$digest" ] || die 'SHA-256 mismatch; installation was not run.'

  # Bound decompression as well as transfer. Never unpack archive-controlled paths.
  gzip -dc "$stage/archive.tar.gz" | head -c 536870913 > "$stage/payload.tar" || die 'Archive decompression failed or exceeded its limit.'
  bytes=$(wc -c < "$stage/payload.tar")
  [ "$bytes" -le 536870912 ] || die 'Uncompressed archive exceeds 512 MiB.'
  listing=$(LC_ALL=C tar -tvf "$stage/payload.tar" "$name/jarvis" | head -c 4097) || die 'Archive executable is missing or invalid.'
  [[ "$listing" = -* && "$listing" != *$'\n'* && ${#listing} -le 4096 ]] || die 'Archive must contain exactly one regular executable.'
  tar -xOf "$stage/payload.tar" "$name/jarvis" | head -c 268435457 > "$stage/jarvis" || die 'Executable extraction failed or exceeded its limit.'
  bytes=$(wc -c < "$stage/jarvis")
  [ "$bytes" -gt 0 ] && [ "$bytes" -le 268435456 ] || die 'Executable is empty or exceeds 256 MiB.'
  chmod 700 "$stage/jarvis"
  reported=$("$stage/jarvis" version) || die 'The downloaded program could not run on this OS.'
  [ "$reported" = "jarvis-cli $version" ] || die 'Downloaded program version does not match the requested package.'
  [ ! -e "$dir/jarvis" ] && [ ! -L "$dir/jarvis" ] || die 'An installation appeared at the destination; refusing to replace it.'
  if [ "$no_path" = 1 ]; then
    "$stage/jarvis" install --dir "$dir" --no-path || die 'Installation failed.'
  else
    "$stage/jarvis" install --dir "$dir" || die 'Installation failed.'
  fi
  printf 'JARVIS is installed. Setup has not run. Run: %q\n' "$dir/jarvis"
  printf 'Setup requires your model and ElevenLabs keys and explicit folder grants.\n'
  printf 'After setup succeeds: %q start; %q doctor --live\n' "$dir/jarvis" "$dir/jarvis"
)

if [ "${BASH_SOURCE[0]}" = "$0" ]; then main "$@"; fi
