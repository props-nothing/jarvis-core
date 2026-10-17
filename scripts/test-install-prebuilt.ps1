# Native ZIP parsing and bounded extraction; replace transport and program invocation.
param([string]$Installer = (Join-Path $PSScriptRoot 'install-prebuilt.ps1'))
$ErrorActionPreference = 'Stop'
. $Installer
$script:realVerify = ${function:Test-JarvisReleaseSignature}
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$testRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
$fixture = Join-Path $testRoot ('jarvis-install-test-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $fixture
$script:calls = @()
$script:downloads = @()
$script:stages = @()
$script:fixtureVersion = '0.1.0'
$script:failInstall = $false
$script:failDownload = $false
$member = 'jarvis-0.1.0-windows-x86_64/jarvis.exe'
$script:archiveFixture = Join-Path $fixture 'good.zip'
$target = Join-Path $fixture 'install here'
$tag = 'ci-0123456789012345678901234567890123456789'

function Assert($Condition, [string]$Message) { if (-not $Condition) { throw "Assertion failed: $Message" } }
function Make-Zip([string]$Path, [string[]]$Names, [int]$Attributes = 0) {
    $zip = [IO.Compression.ZipFile]::Open($Path, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($name in $Names) {
            $entry = $zip.CreateEntry($name)
            $entry.ExternalAttributes = $Attributes
            $stream = $entry.Open()
            try { $bytes = [Text.Encoding]::UTF8.GetBytes('inert executable fixture'); $stream.Write($bytes, 0, $bytes.Length) }
            finally { $stream.Dispose() }
        }
    } finally { $zip.Dispose() }
}
function Invoke-JarvisDownload([string]$Url, [string]$Destination) {
    $script:downloads += $Url
    $script:stages += [IO.Path]::GetDirectoryName($Destination)
    if ($script:failDownload) { throw 'fixture download failed' }
    if ($Url.EndsWith('/release-manifest.txt')) { [IO.File]::WriteAllText($Destination, $script:manifest) }
    elseif ($Url.EndsWith('/release-manifest.sigstore.json')) { [IO.File]::WriteAllText($Destination, 'fixture bundle') }
    elseif ($Url.EndsWith('/cosign-windows-amd64.exe')) { [IO.File]::WriteAllText($Destination, 'untrusted verifier') }
    else { Copy-Item -LiteralPath $script:archiveFixture -Destination $Destination }
}
function Test-JarvisReleaseSignature([string]$Stage, [string]$ReleaseTag) {
    if ($script:verifyPin) { & $script:realVerify $Stage $ReleaseTag; return }
    $script:verified = $true
    if ($script:failSignature) { throw 'fixture signature refused' }
}
function Invoke-JarvisProgram([string]$Program, [string[]]$Arguments) {
    Assert (([IO.File]::ReadAllText($Program)) -ceq 'inert executable fixture') 'only the selected ZIP member is executed'
    $script:calls += ,$Arguments
    if ($Arguments[0] -eq 'version') { return "jarvis-cli $script:fixtureVersion" }
    Assert ($Arguments[0] -eq 'install') 'no setup or service is started'
    Assert ($Arguments[-1] -eq '--no-path') 'no-PATH handoff'
    if ($script:failInstall) { throw 'fixture install failed' }
    $null = New-Item -ItemType Directory -Path $Arguments[2] -Force
    Copy-Item -LiteralPath $Program -Destination (Join-Path $Arguments[2] 'jarvis.exe')
}
function Refused([string]$Expected, [string]$Digest = $script:sha, [string]$Release = $tag) {
    $caught = $false
    try { Install-JarvisPrebuilt $Release '0.1.0' $Digest $target $true }
    catch { $caught = $true; Assert ($_.Exception.Message -match $Expected) "expected $Expected, received $($_.Exception.Message)" }
    Assert $caught "expected refusal: $Expected"
    Assert (-not (Test-Path -LiteralPath (Join-Path $target 'jarvis.exe'))) 'failure did not install'
}
try {
    Make-Zip $script:archiveFixture @($member, '../../outside', 'other/jarvis.exe')
    $script:sha = (Get-FileHash -LiteralPath $script:archiveFixture).Hash
    Install-JarvisPrebuilt $tag '0.1.0' $script:sha $target $true
    Assert (Test-Path -LiteralPath (Join-Path $target 'jarvis.exe')) 'successful first install'
    Assert ($script:calls.Count -eq 2) 'only version and install ran'
    Assert ($script:downloads[0] -ceq "https://github.com/props-nothing/jarvis-core/releases/download/$tag/jarvis-0.1.0-windows-x86_64.zip") 'pinned official URL'
    Assert (-not (Test-Path -LiteralPath (Join-Path $fixture 'outside'))) 'archive paths were not extracted'
    $caught = $false
    try { Install-JarvisPrebuilt $tag '0.1.0' $script:sha $target $true } catch { $caught = $_.Exception.Message -match 'already exists' }
    Assert $caught 'existing install refused'
    Assert ($script:downloads.Count -eq 1) 'existing install refused before network'
    Remove-Item -LiteralPath (Join-Path $target 'jarvis.exe')
    $script:calls = @()
    $rows = @('JARVIS-RELEASE-1', 'repository=props-nothing/jarvis-core', "tag=$tag", "commit=$($tag.Substring(3))", 'version=0.1.0')
    foreach ($label in @('linux-x86_64', 'linux-aarch64', 'macos-aarch64', 'macos-x86_64', 'windows-x86_64')) {
        $extension = if ($label.StartsWith('windows-')) { 'zip' } else { 'tar.gz' }
        $rows += "$label`tjarvis-0.1.0-$label.$extension`t$((Get-Item -LiteralPath $script:archiveFixture).Length)`t$($script:sha.ToLowerInvariant())"
    }
    $goodManifest = ($rows -join "`n") + "`n"
    $script:manifest = $goodManifest
    $script:downloads = @()
    Install-JarvisPrebuilt $tag '0.1.0' '' $target $true
    Assert ($script:downloads.Count -eq 3) 'manifest and bundle precede archive'
    Remove-Item -LiteralPath (Join-Path $target 'jarvis.exe')
    $script:calls = @(); $script:downloads = @()
    $script:failSignature = $true; Refused 'fixture signature refused' ''; $script:failSignature = $false
    Assert ($script:calls.Count -eq 0 -and $script:downloads.Count -eq 2) 'signature failure precedes archive download/execution'
    $script:downloads = @()
    $script:verifyPin = $true; Refused 'Signature verifier checksum mismatch' ''; $script:verifyPin = $false
    Assert ($script:calls.Count -eq 0 -and $script:downloads.Count -eq 3) 'untrusted verifier never executed'
    foreach ($change in @(@('repository=props-nothing', 'repository=foreign'), @('tag=ci-', 'tag=ci-f'),
        @('commit=0', 'commit=f'), @('version=0.1.0', 'version=0.2.0'), @('linux-x86_64', 'linux-unknown'), @('.tar.gz', '.zip'))) {
        $script:downloads = @()
        $script:manifest = $goodManifest.Replace($change[0], $change[1])
        Refused 'manifest' ''
        Assert ($script:calls.Count -eq 0 -and $script:downloads.Count -eq 2) 'invalid manifest precedes archive download/execution'
    }
    $script:manifest = $goodManifest + "extra`n"
    Refused 'manifest' ''
    $script:manifest = $goodManifest.Replace($script:sha.ToLowerInvariant(), ('0' * 64))
    Refused 'SHA-256 mismatch' ''
    Assert ($script:calls.Count -eq 0) 'signed digest still checked before execution'
    Refused 'SHA-256 mismatch' ('0' * 64)
    Assert ($script:calls.Count -eq 0) 'wrong checksum never executed'
    Refused 'exact ci-' $script:sha 'latest'
    Refused 'SHA-256' ($script:sha + "`n")
    $script:failDownload = $true; Refused 'fixture download failed'; $script:failDownload = $false
    Assert ($script:calls.Count -eq 0) 'bad selector/download never executed'
    $script:fixtureVersion = '9.9.9'; Refused 'version does not match'; $script:fixtureVersion = '0.1.0'
    Assert ($script:calls.Count -eq 1) 'wrong version never installed'
    $script:failInstall = $true; Refused 'fixture install failed'; $script:failInstall = $false
    foreach ($kind in @('duplicate', 'missing', 'link', 'directory')) {
        $script:calls = @()
        $script:archiveFixture = Join-Path $fixture "$kind.zip"
        switch ($kind) {
            duplicate { Make-Zip $script:archiveFixture @($member, $member) }
            missing { Make-Zip $script:archiveFixture @('jarvis.exe') }
            link { Make-Zip $script:archiveFixture @($member) (-1610612736) }
            directory { Make-Zip $script:archiveFixture @($member) 16 }
        }
        $script:sha = (Get-FileHash -LiteralPath $script:archiveFixture).Hash
        Refused 'Archive'
        Assert ($script:calls.Count -eq 0) "$kind archive never executed"
    }
    $script:archiveFixture = Join-Path $fixture 'oversized.zip'
    $zip = [IO.Compression.ZipFile]::Open($script:archiveFixture, [IO.Compression.ZipArchiveMode]::Create)
    try {
        $stream = $zip.CreateEntry($member).Open()
        try {
            $zeros = New-Object byte[] 65536
            for ($i = 0; $i -lt 4097; $i++) { $stream.Write($zeros, 0, $zeros.Length) }
        } finally { $stream.Dispose() }
    } finally { $zip.Dispose() }
    $script:sha = (Get-FileHash -LiteralPath $script:archiveFixture).Hash
    Refused 'exceeds 256 MiB'
    Assert ($script:calls.Count -eq 0) 'oversized entry never executed'
    foreach ($stage in $script:stages) { Assert (-not (Test-Path -LiteralPath $stage)) 'staging removed after success and failure' }
    $caught = $false
    try { Get-JarvisPlatform ARM64 } catch { $caught = $_.Exception.Message -match 'Unsupported' }
    Assert $caught 'unsupported native architecture refused'
    Write-Host 'Prebuilt PowerShell install, hash/selector/archive refusal and failure propagation passed.'
} finally {
    $resolvedFixture = [IO.Path]::GetFullPath($fixture)
    if ([IO.Path]::GetDirectoryName($resolvedFixture) -cne $testRoot -or [IO.Path]::GetFileName($resolvedFixture) -notmatch '\Ajarvis-install-test-[0-9a-f]{32}\z') { throw 'Unexpected test cleanup path.' }
    Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
}
