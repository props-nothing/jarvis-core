#!/usr/bin/env bash
# Signed prebuilt install. An explicit third SHA256 selects historical unsigned mode.
# Usage: bash install-prebuilt.sh TAG VERSION [SHA256] [--dir ABSOLUTE_DIR] [--no-path]
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
    --tlsv1.2 --max-redirs 5 --connect-timeout 15 --max-time 120 --max-filesize "${3:-268435456}" \
    --output "$2" "$1" || die 'Download failed; installation was not run.'
}

verify_release_signature() (
  local stage=$1 label=$2 tag=$3 verifier pin actual ref
  case "$label" in
    linux-x86_64) verifier=cosign-linux-amd64; pin=4629c757b7618056f8ddd7e2625ae9fdd94c0372a65049520bc7d9df9efc7f71 ;;
    linux-aarch64) verifier=cosign-linux-arm64; pin=c5d324e091826b0d7a78eb16fef316450b4eb9aaec045611c08ba06f5e73220a ;;
    macos-x86_64) verifier=cosign-darwin-amd64; pin=2347488e5d5b25336644024dfeca5601b190e91197a71a917bda44744aff106c ;;
    macos-aarch64) verifier=cosign-darwin-arm64; pin=5cf948c2f4dfe59687bdd0b8523709067383e03982cc543475c8a7dc70e92a76 ;;
    *) die 'No verifier for this platform.' ;;
  esac
  download "https://github.com/sigstore/cosign/releases/download/v3.1.3/$verifier" "$stage/cosign"
  actual=$("${hash[@]}" "$stage/cosign"); actual=${actual%% *}
  [ "$actual" = "$pin" ] || die 'Signature verifier checksum mismatch.'
  chmod 700 "$stage/cosign"
  ref=refs/heads/main; [[ "$tag" != v* ]] || ref="refs/tags/$tag"
  mkdir "$stage/trust-home"
  # Isolate trust configuration/cache from ambient Cosign/Sigstore overrides.
  env -i PATH="$PATH" HOME="$stage/trust-home" XDG_CACHE_HOME="$stage/trust-home" TMPDIR="$stage" \
    "$stage/cosign" verify-blob --timeout 2m --bundle "$stage/release-manifest.sigstore.json" \
    --certificate-identity "https://github.com/props-nothing/jarvis-core/.github/workflows/release.yml@$ref" \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com "$stage/release-manifest.txt" \
    || die 'Release signature verification failed.'
)

manifest_digest() {
  local file=$1 tag=$2 version=$3 label=$4 line n=0 commit platform name size digest extra extension found=''
  local expected=(linux-x86_64 linux-aarch64 macos-aarch64 macos-x86_64 windows-x86_64)
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    case "$n" in
      1) [ "$line" = JARVIS-RELEASE-1 ] || die 'Unsupported release manifest.' ;;
      2) [ "$line" = repository=props-nothing/jarvis-core ] || die 'Manifest repository mismatch.' ;;
      3) [ "$line" = "tag=$tag" ] || die 'Manifest release tag mismatch.' ;;
      4) commit=${line#commit=}; [[ "$line" = commit=* && "$commit" =~ ^[0-9a-f]{40}$ ]] || die 'Invalid manifest commit.'
         [[ "$tag" != ci-* || "$tag" = "ci-$commit" ]] || die 'Manifest commit mismatch.' ;;
      5) [ "$line" = "version=$version" ] || die 'Manifest version mismatch.'
         [[ "$tag" != v* || "$tag" = "v$version" ]] || die 'Version tag mismatch.' ;;
      6|7|8|9|10)
        IFS=$'\t' read -r platform name size digest extra <<< "$line"
        [ "$platform" = "${expected[$((n - 6))]}" ] || die 'Manifest platform order/membership mismatch.'
        extension=tar.gz; [ "$platform" != windows-x86_64 ] || extension=zip
        [ "$name" = "jarvis-$version-$platform.$extension" ] || die 'Manifest archive name mismatch.'
        [[ "$size" =~ ^[1-9][0-9]{0,8}$ && "$digest" =~ ^[0-9a-f]{64}$ ]] || die 'Invalid manifest size/digest.'
        [ "$size" -le 268435456 ] || die 'Manifest archive exceeds download limit.'
        [ "$line" = "$(printf '%s\t%s\t%s\t%s' "$platform" "$name" "$size" "$digest")" ] || die 'Malformed manifest row.'
        [ "$platform" != "$label" ] || found=$digest ;;
      *) die 'Unexpected manifest content.' ;;
    esac
  done < "$file"
  [ "$n" -eq 10 ] && [ -n "$found" ] || die 'Incomplete release manifest.'
  printf '%s' "$found"
}

main() (
  [ "$#" -ge 2 ] || die 'Usage: install-prebuilt.sh TAG VERSION [SHA256] [--dir ABSOLUTE_DIR] [--no-path]'
  local tag=$1 version=$2 digest='' dir=${HOME:-}/.local/share/jarvis no_path=0 label name stage actual listing bytes reported
  shift 2
  if [[ "${1:-}" =~ ^[0-9a-fA-F]{64}$ ]]; then digest=$1; shift; fi
  unset TAR_OPTIONS GZIP
  [[ "$tag" =~ ^ci-[0-9a-f]{40}$ || "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || die 'Choose an exact ci-<commit> or vMAJOR.MINOR.PATCH release tag.'
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die 'Version must be MAJOR.MINOR.PATCH.'
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
  if [ -z "$digest" ]; then
    printf 'Verifying signed release: %s, %s.\n' "$tag" "$label"
    download "https://github.com/props-nothing/jarvis-core/releases/download/$tag/release-manifest.txt" "$stage/release-manifest.txt" 8192
    download "https://github.com/props-nothing/jarvis-core/releases/download/$tag/release-manifest.sigstore.json" "$stage/release-manifest.sigstore.json" 1048576
    verify_release_signature "$stage" "$label" "$tag"
    digest=$(manifest_digest "$stage/release-manifest.txt" "$tag" "$version" "$label")
  else
    printf 'Unsigned prebuilt install: %s, %s. Verifying the supplied SHA-256.\n' "$tag" "$label"
  fi
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
