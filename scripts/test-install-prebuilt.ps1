# Native ZIP parsing and bounded extraction; replace transport and program invocation.
param([string]$Installer = (Join-Path $PSScriptRoot 'install-prebuilt.ps1'))
$ErrorActionPreference = 'Stop'
. $Installer
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
    Copy-Item -LiteralPath $script:archiveFixture -Destination $Destination
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
