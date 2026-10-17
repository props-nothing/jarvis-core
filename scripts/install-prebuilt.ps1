# Opt-in UNSIGNED prebuilt first install. Windows PowerShell 5.1+ and curl 8.4+.
# .\install-prebuilt.ps1 -Tag ci-<commit> -Version 0.1.0 -Sha256 <archive digest>
[CmdletBinding()]
param([string]$Tag, [string]$Version, [string]$Sha256, [string]$Dir, [switch]$NoPath)
$ErrorActionPreference = 'Stop'

function Get-JarvisPlatform([string]$Architecture) {
    if ($Architecture -cne 'AMD64') { throw "Unsupported Windows architecture: $Architecture. Only native x86_64 packages are available." }
    return 'windows-x86_64'
}

function Invoke-JarvisDownload([string]$Url, [string]$Destination) {
    $curl = (Get-Command curl.exe -CommandType Application -ErrorAction Stop).Source
    $info = (& $curl -q --version) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $info -notmatch '\Acurl ([0-9]+)\.([0-9]+)\.') { throw 'Could not identify curl.exe.' }
    if ([int]$Matches[1] -lt 8 -or ([int]$Matches[1] -eq 8 -and [int]$Matches[2] -lt 4)) { throw 'curl 8.4 or newer is required for bounded downloads.' }
    & $curl -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 --max-redirs 5 --connect-timeout 15 --max-time 120 --max-filesize 268435456 --output $Destination $Url
    if ($LASTEXITCODE -ne 0) { throw 'Download failed; installation was not run.' }
}

function Expand-JarvisProgram([string]$Archive, [string]$Member, [string]$Destination) {
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        $entries = @($zip.Entries | Where-Object { $_.FullName -ceq $Member })
        if ($entries.Count -ne 1) { throw 'Archive must contain exactly one executable.' }
        $entry = $entries[0]
        $kind = ($entry.ExternalAttributes -shr 16) -band 0xF000
        if ($kind -notin @(0, 0x8000) -or ($entry.ExternalAttributes -band 0x10) -ne 0) { throw 'Archive executable must be a regular file.' }
        if ($entry.Length -le 0 -or $entry.Length -gt 268435456) { throw 'Executable is empty or exceeds 256 MiB.' }
        $inputStream = $entry.Open()
        try {
            $outputStream = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew)
            try {
                $buffer = New-Object byte[] 65536
                [long]$total = 0
                while (($count = $inputStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    $total += $count
                    if ($total -gt $entry.Length -or $total -gt 268435456) { throw 'Executable exceeds its declared size.' }
                    $outputStream.Write($buffer, 0, $count)
                }
                if ($total -ne $entry.Length) { throw 'Executable is truncated.' }
            } finally { $outputStream.Dispose() }
        } finally { $inputStream.Dispose() }
    } finally { $zip.Dispose() }
}

function Invoke-JarvisProgram([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "JARVIS $($Arguments[0]) failed with exit code $LASTEXITCODE." }
}

function Install-JarvisPrebuilt([string]$ReleaseTag, [string]$PackageVersion, [string]$Digest, [string]$Directory, [bool]$SkipPath) {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'Use install-prebuilt.sh on macOS/Linux.' }
    if ($ReleaseTag -cnotmatch '\A(ci-[0-9a-f]{40}|v[0-9]+\.[0-9]+\.[0-9]+)\z') { throw 'Choose an exact ci-<commit> or vMAJOR.MINOR.PATCH release tag.' }
    if ($PackageVersion -cnotmatch '\A[0-9]+\.[0-9]+\.[0-9]+\z') { throw 'Version must be MAJOR.MINOR.PATCH.' }
    if ($Digest -notmatch '\A[0-9a-fA-F]{64}\z') { throw 'Supply the selected platform archive SHA-256 (64 hexadecimal characters).' }
    $architecture = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    $label = Get-JarvisPlatform $architecture
    if (-not $Directory) {
        if (-not $env:LOCALAPPDATA) { throw 'LOCALAPPDATA is required.' }
        $Directory = Join-Path $env:LOCALAPPDATA 'Programs\JARVIS'
    }
    # Reject relative, drive-relative and network locations before any download.
    if ($Directory -notmatch '\A[A-Za-z]:[\\/]' -or $Directory -match '[\r\n]') { throw '--dir must be an absolute local directory.' }
    $Directory = [IO.Path]::GetFullPath($Directory)
    $target = Join-Path $Directory 'jarvis.exe'
    if (Test-Path -LiteralPath $target) { throw 'An installation already exists there. This first-install bootstrap does not update it.' }
    $name = "jarvis-$PackageVersion-$label"
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
    $stage = Join-Path $tempRoot ('jarvis-prebuilt-' + [Guid]::NewGuid().ToString('N'))
    $null = New-Item -ItemType Directory -Path $stage
    try {
        Write-Host "Unsigned prebuilt install: $ReleaseTag, $label. Verifying the supplied SHA-256."
        $archive = Join-Path $stage 'archive.zip'
        Invoke-JarvisDownload "https://github.com/props-nothing/jarvis-core/releases/download/$ReleaseTag/$name.zip" $archive
        if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ine $Digest) { throw 'SHA-256 mismatch; installation was not run.' }
        $program = Join-Path $stage 'jarvis.exe'
        Expand-JarvisProgram $archive "$name/jarvis.exe" $program
        $reported = @(Invoke-JarvisProgram $program @('version'))
        if ($reported.Count -ne 1 -or $reported[0] -cne "jarvis-cli $PackageVersion") { throw 'Downloaded program version does not match the requested package.' }
        if (Test-Path -LiteralPath $target) { throw 'An installation appeared at the destination; refusing to replace it.' }
        $installArgs = @('install', '--dir', $Directory)
        if ($SkipPath) { $installArgs += '--no-path' }
        Invoke-JarvisProgram $program $installArgs
        $quotedProgram = "'" + (Join-Path $Directory 'jarvis.exe').Replace("'", "''") + "'"
        Write-Host "JARVIS is installed. Setup has not run. Run: & $quotedProgram"
        Write-Host 'Setup requires your model and ElevenLabs keys and explicit folder grants.'
        Write-Host "After setup succeeds: & $quotedProgram start; & $quotedProgram doctor --live"
    } finally {
        # Only remove our generated direct child of the resolved temp directory.
        $resolvedStage = [IO.Path]::GetFullPath($stage)
        if ([IO.Path]::GetDirectoryName($resolvedStage) -cne $tempRoot -or [IO.Path]::GetFileName($resolvedStage) -notmatch '\Ajarvis-prebuilt-[0-9a-f]{32}\z') { throw 'Refusing unexpected staging cleanup path.' }
        Remove-Item -LiteralPath $resolvedStage -Recurse -Force
    }
}

if ($MyInvocation.InvocationName -ne '.') { Install-JarvisPrebuilt $Tag $Version $Sha256 $Dir $NoPath.IsPresent }
