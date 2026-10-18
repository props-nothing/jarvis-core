# Explicit Windows installation; synthesis itself never downloads or installs anything.
[CmdletBinding()]
param(
    # Public runtime files live outside managed profile folders whose ACLs are rechecked at startup.
    [string]$Runtime = (Join-Path $env:LOCALAPPDATA 'JARVIS-VoiceDesigner/voxcpm2-2.0.3'),
    [string]$Uv = (Join-Path $env:LOCALAPPDATA 'JARVIS/speech/uv-0.13.1/uv.exe'),
    [string]$Python = (Join-Path $env:LOCALAPPDATA 'JARVIS/speech/python/cpython-3.12.15-windows-x86_64-none/python.exe'),
    [string]$ModelSource,
    [ValidateSet('Auto', 'AMD', 'NVIDIA', 'CPU')]
    [string]$Backend = 'Auto'
)
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitProcess -or -not $env:LOCALAPPDATA) { throw 'Requires Windows x64 PowerShell.' }
if (-not [IO.Path]::IsPathRooted($Runtime)) { throw 'Runtime must be an absolute directory.' }
$Runtime = [IO.Path]::GetFullPath($Runtime)
if (-not (Test-Path -LiteralPath $Uv -PathType Leaf)) { throw 'Install uv 0.13.1 and pass its executable using -Uv.' }
if ((& $Uv --version) -notmatch '^uv 0\.13\.1\b') { throw 'This installer is verified with uv 0.13.1.' }
if (-not (Test-Path -LiteralPath $Python -PathType Leaf)) { throw 'Install CPython 3.12.15 and pass its executable using -Python.' }
if ((& $Python --version) -ne 'Python 3.12.15') { throw 'This installation requires CPython 3.12.15.' }
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../crates/jarvis-voice/src/description_assets.json') -Raw | ConvertFrom-Json
$model = Join-Path $Runtime 'model'
New-Item -ItemType Directory -Force -Path $model | Out-Null
$environmentPython = Join-Path $Runtime 'venv/Scripts/python.exe'
if (-not (Test-Path -LiteralPath $environmentPython)) {
    & $Uv venv --python $Python --no-python-downloads (Join-Path $Runtime 'venv')
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the private voice environment.' }
}
# Repair prior same-version installs which selected the universal SoundFile wheel without its DLL.
& $Uv pip install --python $environmentPython --no-deps --require-hashes --only-binary :all: --reinstall-package soundfile --index-url https://pypi.org/simple -r (Join-Path $PSScriptRoot 'voice-designer-windows.lock')
if ($LASTEXITCODE -ne 0) { throw 'Could not install the verified inference packages.' }
function Test-Asset($Path, $Asset) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    $item = Get-Item -LiteralPath $Path
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Model files must not be links.' }
    return $item.Length -eq $Asset.size -and (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -eq $Asset.sha256
}
foreach ($asset in $manifest.files) {
    $destination = Join-Path $model $asset.name
    if (Test-Asset $destination $asset) { continue }
    $partial = "$destination.partial"
    if ($ModelSource) {
        $source = Join-Path $ModelSource $asset.name
        if (-not (Test-Asset $source $asset)) { throw "Invalid source model asset: $($asset.name)" }
        Copy-Item -LiteralPath $source -Destination $partial -Force
    } else {
        $url = "https://huggingface.co/openbmb/VoxCPM2/resolve/$($manifest.revision)/$($asset.name)"
        Write-Host "Downloading $($asset.name) ($($asset.size) bytes)"
        Invoke-WebRequest -Uri $url -OutFile $partial -UseBasicParsing
    }
    if (-not (Test-Asset $partial $asset)) { throw "Model verification failed: $($asset.name)" }
    # Both paths are fixed children of the validated installation directory.
    Move-Item -LiteralPath $partial -Destination $destination -Force
}
$probe = @'
import io, wave
import voxcpm, torch, librosa, soundfile
buffer = io.BytesIO()
with wave.open(buffer, 'wb') as audio:
    audio.setnchannels(1)
    audio.setsampwidth(2)
    audio.setframerate(48000)
    audio.writeframes(b'\x01\x00' * 4800)
buffer.seek(0)
samples, rate = librosa.load(buffer, sr=16000)
assert rate == 16000 and len(samples) == 1600
print('Local voice imports and reference audio decoding passed.')
'@
& $environmentPython -I -c $probe
if ($LASTEXITCODE -ne 0) { throw 'The installed runtime could not decode reference audio. Repair the Windows inference packages before generating voices.' }
$gpuMarker = Join-Path $Runtime 'gpu/enabled'
# Disable an earlier GPU environment until this installation passes its native probe.
if (Test-Path -LiteralPath $gpuMarker) { Remove-Item -LiteralPath $gpuMarker }
if ($Backend -eq 'Auto') {
    $adapters = @(Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name)
    $Backend = if ($adapters -match 'NVIDIA') { 'NVIDIA' } elseif ($adapters -match 'AMD|Radeon') { 'AMD' } else { 'CPU' }
}
if ($Backend -ne 'CPU') {
    try {
        $gpuPython = Join-Path $Runtime 'gpu/venv/Scripts/python.exe'
        if (-not (Test-Path -LiteralPath $gpuPython)) {
            & $Uv venv --python $Python --no-python-downloads (Join-Path $Runtime 'gpu/venv')
            if ($LASTEXITCODE -ne 0) { throw 'GPU environment creation failed.' }
        }
        $lock = Join-Path $PSScriptRoot "voice-designer-$($Backend.ToLowerInvariant()).lock"
        & $Uv pip install --python $gpuPython --no-deps --require-hashes --only-binary :all: --index-url https://pypi.org/simple -r $lock
        if ($LASTEXITCODE -ne 0) { throw 'GPU package installation failed.' }
        if ($Backend -eq 'AMD') {
            # AMD publishes this pure-Python loader only as source. Use the hash-pinned
            # archive and already pinned setuptools, without network build dependencies.
            & $Uv pip install --python $gpuPython --no-deps --require-hashes --no-build-isolation -r (Join-Path $PSScriptRoot 'voice-designer-rocm.lock')
            if ($LASTEXITCODE -ne 0) { throw 'ROCm loader installation failed.' }
        }
        & $gpuPython -I -c $probe
        if ($LASTEXITCODE -ne 0) { throw 'GPU reference decoder probe failed.' }
        $worker = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../crates/jarvis-voice/src/description_worker.py') -Raw
        # Import the same selector used by inference without executing its main function.
        $gpuProbe = @'
import sys, torch
scope = {'__name__': 'probe'}
exec(sys.argv[1], scope)
device, backend, name = scope['select_device'](torch, 'gpu')
print('Verified GPU kernels: ' + name + ' (' + backend + ', ' + device + ')')
'@
        & $gpuPython -I -c $gpuProbe $worker
        if ($LASTEXITCODE -ne 0) { throw 'No compatible GPU kernels. Check the supported card and driver requirements.' }
        Set-Content -LiteralPath $gpuMarker -Value '1' -Encoding ascii
        Write-Host "$Backend acceleration enabled. Runtime failures retry automatically on CPU."
    } catch {
        Write-Warning "GPU setup did not pass: $($_.Exception.Message) CPU voice generation remains installed and selected."
    }
} else { Write-Host 'CPU voice generation selected.' }
Write-Host "Local voice designer installed at $Runtime"
Write-Host 'Add a local-voxcpm2 provider with this runtime_dir, then restart JARVIS. No startup task is needed.'
