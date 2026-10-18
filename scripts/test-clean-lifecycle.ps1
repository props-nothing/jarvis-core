# Run ON a disposable clean Windows machine. Uses Windows PowerShell 5.1 only.
[CmdletBinding()]
param([switch]$Disposable, [string]$BaseTag, [string]$CandidateTag,
    [string]$BaseVersion = '0.1.0', [string]$CandidateVersion = '0.1.0',
    [string]$Model, [string]$BaseUrl, [string]$ModelKeyFile, [string]$VoiceKeyFile, [string]$ReportDir)
$ErrorActionPreference = 'Stop'
if (-not $Disposable) { throw 'A disposable machine is required.' }
foreach ($name in @('rustc','cargo','rustup','node','nodejs','npm','python','python2','python3','py','pip','pip3')) {
    if (Get-Command $name -CommandType Application -ErrorAction SilentlyContinue) {
        throw "Clean-machine prerequisite failed: $name is available. Do not hide it with a restricted PATH."
    }
}
if (-not $BaseTag -or -not $CandidateTag -or $BaseTag -eq $CandidateTag) {
    throw 'Two distinct exact signed release tags are required.'
}
foreach ($path in @($ModelKeyFile, $VoiceKeyFile) | Where-Object { $_ }) {
    if (-not [IO.Path]::IsPathRooted($path) -or -not (Test-Path -LiteralPath $path -PathType Leaf)) { throw 'Absolute test-account key-file paths are required.' }
}
if (-not [IO.Path]::IsPathRooted($ReportDir)) { throw 'An absolute report directory is required.' }
if (Get-ChildItem Env:JARVIS_*) { throw 'Remove ambient JARVIS overrides before acceptance.' }
$install = Join-Path $env:LOCALAPPDATA 'Programs\JARVIS-clean-acceptance'
$binary = Join-Path $install 'jarvis.exe'
$config = Join-Path $env:APPDATA 'JARVIS'
$data = Join-Path $env:LOCALAPPDATA 'JARVIS'
$service = Join-Path $env:USERPROFILE '.jarvis\service\jarvisd-launcher.json'
$registry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
foreach ($path in @($install,$config,$data,$service,$ReportDir)) {
    if (Test-Path -LiteralPath $path) { throw "Refusing existing acceptance target: $path" }
}
$registryKey = Get-Item -LiteralPath $registry -ErrorAction SilentlyContinue
if ($registryKey -and $registryKey.GetValueNames() -contains 'JARVIS') { throw 'Existing login launcher; refusing.' }
$null = New-Item -ItemType Directory -Path $ReportDir
Set-Content -LiteralPath (Join-Path $ReportDir 'state.txt') -Value 'started'
[Environment]::OSVersion.VersionString | Set-Content -LiteralPath (Join-Path $ReportDir 'os.txt')
function Invoke-Program([string]$Program, [string[]]$Arguments) {
    # Windows PowerShell treats native stderr as ErrorRecords. Exit status is the
    # success contract; informational stderr from init must not become a failure.
    $ErrorActionPreference = 'Continue'
    $result = & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Lifecycle command failed: $($Arguments[0])" }
    return $result
}
function Invoke-Jarvis([string[]]$Arguments) { Invoke-Program $binary $Arguments }
function Digest([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash }
function Wait-Ready {
    $ErrorActionPreference = 'Continue'
    for ($i=0; $i -lt 60; $i++) {
        $value = & $binary health --json 2>$null
        if ($LASTEXITCODE -eq 0) {
            $health = $value | ConvertFrom-Json
            if ($health.ready -and $health.live) { return }
        }
        Start-Sleep -Seconds 1
    }
    throw 'Authenticated readiness failed.'
}
function Start-Jarvis {
    # Execute the persisted login command; a real logon is an additional observation.
    $entry = (Get-ItemProperty -LiteralPath $registry -Name JARVIS).JARVIS
    & $env:ComSpec /d /c $entry
    if ($LASTEXITCODE -ne 0) { throw 'Login launcher failed.' }
    Wait-Ready
}
& (Join-Path $PSScriptRoot 'install-prebuilt.ps1') -Tag $BaseTag -Version $BaseVersion -Dir $install
$initArguments = @('init')
if ($BaseUrl) { $initArguments += @('--base-url',$BaseUrl) }
if ($Model) { $initArguments += @('--model',$Model) }
if ($ModelKeyFile) { $initArguments += @('--api-key-file',$ModelKeyFile) }
if ($VoiceKeyFile) { $initArguments += @('--voice-key-file',$VoiceKeyFile) }
Invoke-Jarvis $initArguments
$configFile = Join-Path $config 'config.toml'
$configBefore = Digest $configFile
Invoke-Jarvis @('service','install')
Start-Jarvis
$null = Invoke-Jarvis @('ask','Reply with the single word READY. Do not use tools or make changes.')
Invoke-Jarvis @('stop')
$initial = Digest $binary
$stage = (Invoke-Jarvis @('update','stage',$CandidateTag,$CandidateVersion,'--dir',$install)) | ConvertFrom-Json
$helper = $stage.updater
$id = $stage.stage.id
$stage | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $ReportDir 'stage.json')
Start-Jarvis
try {
    $ErrorActionPreference = 'Continue'
    $null = & $helper update activate $id --dir $install 2>$null
    $activationExit = $LASTEXITCODE
} finally { $ErrorActionPreference = 'Stop' }
if ($activationExit -eq 0) { throw 'Active-owner guard failed.' }
Invoke-Jarvis @('stop')
Invoke-Program $helper @('update','activate',$id,'--dir',$install) | Set-Content -LiteralPath (Join-Path $ReportDir 'activation.json')
if ((Digest $binary) -eq $initial) { throw 'Candidate bytes are not distinct.' }
$helper = Join-Path $ReportDir 'lifecycle-helper.exe'
Copy-Item -LiteralPath $binary -Destination $helper
if ((Digest $helper) -ne (Digest $binary)) { throw 'Maintenance helper copy mismatch.' }
Start-Jarvis
$null = Invoke-Jarvis @('ask','Reply with the single word UPDATED. Do not use tools or make changes.')
Invoke-Jarvis @('stop')
$database = Join-Path $data 'jarvis.sqlite3'
$afterUpdate = Digest $database
Invoke-Program $helper @('update','rollback','--dir',$install) | Set-Content -LiteralPath (Join-Path $ReportDir 'rollback.json')
if ((Digest $binary) -ne $initial -or (Digest $database) -ne $afterUpdate) { throw 'Rollback changed current data or failed to restore the executable.' }
Start-Jarvis
Invoke-Jarvis @('stop')
$beforeRepair = Digest $database
Move-Item -LiteralPath $binary -Destination (Join-Path $ReportDir 'removed-program.exe')
Invoke-Program $helper @('update','repair','--dir',$install) | Set-Content -LiteralPath (Join-Path $ReportDir 'repair.json')
if ((Digest $binary) -ne $initial -or (Digest $database) -ne $beforeRepair) { throw 'Missing-image repair failed.' }
Set-Content -LiteralPath $binary -Value 'damaged acceptance executable'
$null = Invoke-Program $helper @('update','repair','--dir',$install)
if ((Digest $binary) -ne $initial -or (Digest $database) -ne $beforeRepair) { throw 'Damaged-image repair failed.' }
Start-Jarvis
Invoke-Jarvis @('stop')
$beforeRemove = Digest $database
Invoke-Program $helper @('uninstall','--dir',$install)
Invoke-Program $helper @('uninstall','--dir',$install)
if ((Test-Path -LiteralPath $binary) -or (Test-Path -LiteralPath $service)) { throw 'Uninstall left an active installation.' }
if ((Get-Item -LiteralPath $registry).GetValueNames() -contains 'JARVIS') { throw 'Uninstall left a login launcher.' }
if (([Environment]::GetEnvironmentVariable('Path','User') -split ';') -contains $install) { throw 'Uninstall left its PATH entry.' }
if ((Digest $database) -ne $beforeRemove -or (Digest $configFile) -ne $configBefore) { throw 'Uninstall changed profile data.' }
Set-Content -LiteralPath (Join-Path $ReportDir 'state.txt') -Value 'passed: signed-install live-onboarding first-response readiness update rollback repair uninstall data-preservation'
Write-Output "Lifecycle passed. Keep the private report at $ReportDir with the independent OS image/tool inventory. Reboot/logon and interactive hidden-key entry remain separate observations."
