# Clean-machine lifecycle acceptance

The clean-machine gate is open until actual Windows, macOS and Linux target results
are attached. Hosted developer images, hidden PATH entries and provider-free profiles
are useful tests but do not satisfy this gate.

## Target preparation

Use disposable, ordinary user accounts on identified clean OS images. Record image
source/version, CPU architecture and installed software inventory before installation.
Keep the ordinary system PATH. The scripts refuse Rust, Cargo, rustup, Node/npm and
Python/pip on PATH; independently confirm they are not installed elsewhere. Windows
Store Python aliases and macOS developer-tool stubs need explicit inventory treatment;
do not mask them with a narrowed PATH to obtain a pass.

The production bootstrap requires curl 8.4+ and native archive/hash utilities (Bash
on Unix or Windows PowerShell 5.1). Record any prerequisite installation, including
curl upgrades. The repository is not needed on the target: transfer the appropriate
`test-clean-lifecycle` and `install-prebuilt` script pair from the reviewed commit.
No Rust, Node, Python, Git or GitHub CLI is required by these drivers.

Choose two distinct compatible signed releases, with a candidate containing
ADR-0243's repair and guarded uninstall. After activation the driver retains a separate
copy of that verified candidate for repair/removal, including on Windows.
Maintenance protocol 2, exact migration history and all numeric schemas must match.
Two commits at package version 0.1.0 prove distinct signed builds, not a package-version
upgrade. Interactive runs prompt for model and ElevenLabs keys with hidden entry;
you do not need to send keys to anyone or configure GitHub secrets. Batch runs can
use private absolute key-file paths. Do not paste keys into commands, reports or CI logs.
ADR-0214 onboarding makes a real completion and short voice request and may use credits.
The default voice must be available to that account.

Native certificate signing is still unavailable. Sigstore manifest verification does
not prove Authenticode, notarization, SmartScreen or Gatekeeper acceptance. Record any
OS warning and its exact handling; do not turn it into a signed-OS-distribution claim.

## Run on Windows

In Windows PowerShell, replace the named values with the selected public release and
test-host details. The report directory must be new. Setup prompts for your keys.

```powershell
.\test-clean-lifecycle.ps1 -Disposable `
  -BaseTag $baseTag -CandidateTag $candidateTag `
  -BaseVersion '0.1.0' -CandidateVersion '0.1.0' `
  -ReportDir $newReportDirectory
```

## Run on macOS or Linux

```bash
bash test-clean-lifecycle.sh --disposable \
  "$BASE_TAG" "$CANDIDATE_TAG" 0.1.0 0.1.0 "$NEW_REPORT_DIRECTORY"
```

The native user service manager must be available in that user's login session.
Headless SSH sessions without the user's launchd/systemd context are not equivalent.
For batch runs, append the model, base URL, model-key path and voice-key path on Unix;
on Windows use `-Model`, `-BaseUrl`, `-ModelKeyFile` and `-VoiceKeyFile`.

## What the driver checks

The driver refuses existing installations/profiles/services. It uses production
signed installation and real `init`, starts the native login service, waits for
authenticated health, asks a fixed synthetic question and stops gracefully. It stages
a second signed executable, proves activation refuses an active owner, activates it,
checks readiness and writes a second conversation through the real provider. Rollback
must preserve the exact current database digest. Missing and damaged executable repair
must restore the previous digest without changing the database. Repeated uninstall
must remove the executable/service/PATH entry while preserving configuration and data.
No key values or conversation text are written to the report. The profile and recovery
evidence remain private on the test host, and failed runs remain available for diagnosis.

Also observe interactive first run with hidden terminal key entry, restart/logon and
post-logon readiness on each platform. The script executes Windows's persisted Run
command; this is not a real logon. Review `doctor --live` and record degradations instead
of treating optional adapters as installed. Linux ARM64/Intel macOS need their own
runtime records if included in a release support claim.

## Evidence ledger

| Target | Image/no-toolchain inventory | Live onboarding | Lifecycle driver | Reboot/logon |
| --- | --- | --- | --- | --- |
| Windows x86_64 | Owner self-test pending | Interactive key entry | Not run | Not run |
| macOS ARM64 | Owner self-test pending | Interactive key entry | Not run | Not run |
| Linux x86_64 | Owner self-test pending | Interactive key entry | Not run | Not run |

Repository discovery found no self-hosted runners, Actions secrets or deployment
environments. The owner chose to test manually and enter keys when prompted.
[Research](../research/integrations/clean-machine-lifecycle.md) and
[ADR-0243](../adr/0243-repair-and-removal-preserve-installation-evidence.md) distinguish
these prerequisites from implementation and hosted-native regression evidence.
