#!/usr/bin/env bash
# Exercises packaging in a disposable tree, without building or running JARVIS.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/scripts" "$fixture/docs/user" "$fixture/build" "$fixture/unpacked"
cp "$root/scripts/package-release.sh" "$fixture/scripts/"
cp "$root/Cargo.toml" "$root/LICENSE" "$root/NOTICE" "$root/THIRD_PARTY.md" "$fixture/"
cp "$root/docs/user/quick-start.md" "$root/docs/user/settings.md" "$root/docs/user/platforms.md" "$fixture/docs/user/"

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) exe=".exe"; kind="zip" ;;
  *) exe=""; kind="tar.gz" ;;
esac
printf 'inert packaging fixture; not an executable\n' > "$fixture/build/jarvis$exe"
bash "$fixture/scripts/package-release.sh" license-test "$fixture/build" > "$fixture/package-output.txt"
version="$(awk -F'"' '/^version = /{print $2; exit}' "$fixture/Cargo.toml")"
name="jarvis-$version-license-test"
archive="$fixture/dist/$name.$kind"
[ -s "$archive" ]
[ -s "$archive.sha256" ]
case "$kind" in
  zip)
    if command -v unzip >/dev/null 2>&1; then
      unzip -q "$archive" -d "$fixture/unpacked"
    else
      "${SYSTEMROOT:-C:/Windows}/System32/tar.exe" -xf "$archive" -C "$fixture/unpacked"
    fi ;;
  *) tar -xzf "$archive" -C "$fixture/unpacked" ;;
esac
for file in LICENSE NOTICE THIRD_PARTY.md; do
  cmp "$fixture/$file" "$fixture/unpacked/$name/$file"
done
grep -q 'Grant of Patent License' "$fixture/unpacked/$name/LICENSE"
grep -q 'The MediaPipe Authors' "$fixture/unpacked/$name/NOTICE"
grep -q 'LICENSE.*Apache License 2.0' "$fixture/unpacked/$name/README.txt"
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$fixture/dist" && sha256sum -c "$name.$kind.sha256")
else
  (cd "$fixture/dist" && shasum -a 256 -c "$name.$kind.sha256")
fi

for file in LICENSE NOTICE; do
  mv "$fixture/$file" "$fixture/$file.saved"
  if bash "$fixture/scripts/package-release.sh" missing-notice-test "$fixture/build" > "$fixture/refusal.txt" 2>&1; then
    echo "packaging succeeded without $file" >&2
    exit 1
  fi
  [ ! -e "$fixture/dist/jarvis-$version-missing-notice-test.$kind" ]
  mv "$fixture/$file.saved" "$fixture/$file"
done
echo 'Archive license, attribution, checksum and missing-notice refusal passed.'
