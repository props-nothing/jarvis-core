#!/usr/bin/env bash
# Run ON a disposable clean Linux/macOS machine. No development runtime required.
set -euo pipefail
die() { printf '%s\n' "$*" >&2; exit 1; }
[ "$#" = 6 ] || [ "$#" = 10 ] || die 'Usage: test-clean-lifecycle.sh --disposable BASE_TAG CANDIDATE_TAG BASE_VERSION CANDIDATE_VERSION REPORT_DIR [MODEL BASE_URL MODEL_KEY_FILE VOICE_KEY_FILE]'
[ "$1" = --disposable ] || die 'A disposable machine is required.'
base=$2 candidate=$3 base_version=$4 candidate_version=$5 report=$6
for tool in rustc cargo rustup node nodejs npm python python2 python3 pip pip3; do
  if command -v "$tool" >/dev/null 2>&1; then die "Clean-machine prerequisite failed: $tool is available. Do not hide it with a restricted PATH."; fi
done
# A clean image ID/inventory is recorded independently; this is a prerequisite,
# not proof that no toolchain is installed elsewhere on disk.
case "$(uname -s)" in
  Linux) config=${XDG_CONFIG_HOME:-$HOME/.config}/jarvis; data=${XDG_DATA_HOME:-$HOME/.local/share}/jarvis; service=$HOME/.config/systemd/user/jarvisd.service ;;
  Darwin) config="$HOME/Library/Application Support/JARVIS"; data=$config; service=$HOME/Library/LaunchAgents/dev.jarvis.jarvisd.plist ;;
  *) die 'Use test-clean-lifecycle.ps1 on Windows.' ;;
esac
install=$HOME/.local/share/jarvis-clean-acceptance
binary=$install/jarvis
for p in "$install" "$config" "$data" "$service" "$HOME/.local/bin/jarvis" "$report"; do
  [ ! -e "$p" ] && [ ! -L "$p" ] || die "Refusing existing acceptance target: $p"
done
[[ "$report" = /* ]] || die 'Report path must be absolute.'
init_args=(init)
if [ "$#" = 10 ]; then
  model=$7 base_url=$8 model_key=$9 voice_key=${10}
  [[ "$model_key" = /* && "$voice_key" = /* ]] && [ -f "$model_key" ] && [ -f "$voice_key" ] || die 'Absolute test-account key files are required.'
  init_args+=(--base-url "$base_url" --model "$model" --api-key-file "$model_key" --voice-key-file "$voice_key")
else
  [ -t 0 ] || die 'Interactive setup requires a terminal; batch callers must provide model and key-file arguments.'
fi
[ "$base" != "$candidate" ] || die 'Two distinct signed releases are required.'
if env | cut -d= -f1 | grep -q '^JARVIS_'; then die 'Remove ambient JARVIS overrides before acceptance.'; fi
umask 077
mkdir "$report"
printf 'started\n' > "$report/state.txt"
uname -srm > "$report/os.txt"
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
bash "$script_dir/install-prebuilt.sh" "$base" "$base_version" --dir "$install"
"$binary" "${init_args[@]}"
config_hash() { digest "$config/config.toml"; }
digest() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1;
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
ready() {
  local health
  for ((i=0; i<60; i++)); do
    if health=$("$binary" health --json 2>/dev/null) &&
      printf '%s' "$health" | grep -Eq '"ready"[[:space:]]*:[[:space:]]*true' &&
      printf '%s' "$health" | grep -Eq '"live"[[:space:]]*:[[:space:]]*true'; then return; fi
    sleep 1
  done
  die 'Authenticated readiness failed.'
}
start() {
  if [ "$(uname -s)" = Darwin ]; then launchctl start dev.jarvis.jarvisd;
  else systemctl --user start jarvisd.service; fi
  ready
}
stop() { "$binary" stop; }
"$binary" service install
ready
"$binary" ask 'Reply with the single word READY. Do not use tools or make changes.' > /dev/null
stop
initial=$(digest "$binary")
config_before=$(config_hash)
"$binary" update stage "$candidate" "$candidate_version" --dir "$install" > "$report/stage.json"
# Read only the UUID from the production response, never evaluate JSON as shell code.
id=$(sed -n 's/.*"id":"\([0-9a-f-]*\)".*/\1/p' "$report/stage.json")
[[ "$id" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]] || die 'Invalid stage ID.'
helper=$install/.jarvis-update/$id/updater
[ -f "$helper" ] || die 'Retained helper missing.'
start
if "$helper" update activate "$id" --dir "$install" > /dev/null 2>&1; then die 'Active-owner guard failed.'; fi
stop
"$helper" update activate "$id" --dir "$install" > "$report/activation.json"
[ "$(digest "$binary")" != "$initial" ] || die 'Candidate bytes are not distinct.'
# The baseline helper may predate repair. Retain a separate copy of the verified
# activated candidate, so Windows can repair/remove the installed executable.
helper=$report/lifecycle-helper
cp "$binary" "$helper"
chmod 700 "$helper"
[ "$(digest "$helper")" = "$(digest "$binary")" ] || die 'Maintenance helper copy mismatch.'
start
"$binary" ask 'Reply with the single word UPDATED. Do not use tools or make changes.' > /dev/null
stop
database=$data/jarvis.sqlite3
after_update=$(digest "$database")
"$helper" update rollback --dir "$install" > "$report/rollback.json"
[ "$(digest "$binary")" = "$initial" ] && [ "$(digest "$database")" = "$after_update" ] || die 'Rollback changed current data or failed to restore the executable.'
start; stop
before_repair=$(digest "$database")
# Preserve the exact missing-image fixture; never remove arbitrary directories.
mv "$binary" "$report/removed-program"
"$helper" update repair --dir "$install" > "$report/repair.json"
[ "$(digest "$binary")" = "$initial" ] && [ "$(digest "$database")" = "$before_repair" ] || die 'Missing-image repair failed.'
printf 'damaged acceptance executable\n' > "$binary"
"$helper" update repair --dir "$install" > /dev/null
[ "$(digest "$binary")" = "$initial" ] && [ "$(digest "$database")" = "$before_repair" ] || die 'Damaged-image repair failed.'
start; stop
before_remove=$(digest "$database")
"$helper" uninstall --dir "$install"
"$helper" uninstall --dir "$install"
[ ! -e "$binary" ] && [ ! -e "$service" ] && [ ! -L "$HOME/.local/bin/jarvis" ] || die 'Uninstall left an active installation.'
[ "$(digest "$database")" = "$before_remove" ] && [ "$(config_hash)" = "$config_before" ] || die 'Uninstall changed profile data.'
printf 'passed: signed-install live-onboarding first-response readiness update rollback repair uninstall data-preservation\n' > "$report/state.txt"
printf 'Lifecycle passed. Keep the private report at %s with the independent OS image/tool inventory. Reboot/logon and interactive hidden-key entry remain separate observations.\n' "$report"
