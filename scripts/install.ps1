# Install igris on Windows: download the release archive, check its SHA-256 against the release's
# SHA256SUMS, and put igris.exe in a folder of your own (no administrator rights needed).
#
#   irm https://raw.githubusercontent.com/JlassiRafet/IGRIS/main/scripts/install.ps1 | iex
#
# Environment (all optional):
#   IGRIS_VERSION      a tag such as v0.1.0 (default: the latest release; a pre-release needs this)
#   IGRIS_INSTALL_DIR  where to put igris.exe (default: %LOCALAPPDATA%\Programs\igris)
#   IGRIS_NO_PATH=1    do not add the folder to your user PATH
#   IGRIS_DRY_RUN=1    print the download address and stop
#   IGRIS_REPO, IGRIS_BASE_URL  for tests
$ErrorActionPreference = 'Stop'

$Repo = if ($env:IGRIS_REPO) { $env:IGRIS_REPO } else { 'JlassiRafet/IGRIS' }
$Dir = if ($env:IGRIS_INSTALL_DIR) { $env:IGRIS_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\igris' }

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    throw "igris: there is no Windows build for $($env:PROCESSOR_ARCHITECTURE) (x86_64 only). Or build it: cargo install --git https://github.com/$Repo igris"
}
$Target = 'x86_64-pc-windows-msvc'

if ($env:IGRIS_VERSION) {
    $Tag = $env:IGRIS_VERSION
} else {
    $Tag = (Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing).tag_name
    if (-not $Tag) { throw 'igris: no stable release found. For a pre-release: $env:IGRIS_VERSION = "v0.1.0-rc1"' }
}

$Asset = "igris-$Tag-$Target.zip"
$Base = if ($env:IGRIS_BASE_URL) { $env:IGRIS_BASE_URL } else { "https://github.com/$Repo/releases/download/$Tag" }

if ($env:IGRIS_DRY_RUN -eq '1') { Write-Output "$Base/$Asset"; return }

function Get-File($Url, $Out) {
    if ($Url.StartsWith('file:')) { Copy-Item -LiteralPath ([Uri]$Url).LocalPath -Destination $Out }
    else { Invoke-WebRequest -Uri $Url -OutFile $Out -UseBasicParsing }
}

$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("igris-install-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    Write-Output "igris: downloading $Asset"
    $Zip = Join-Path $Tmp $Asset
    Get-File "$Base/$Asset" $Zip
    Get-File "$Base/SHA256SUMS" (Join-Path $Tmp 'SHA256SUMS')

    $Expected = $null
    foreach ($Line in Get-Content (Join-Path $Tmp 'SHA256SUMS')) {
        if ($Line -match '^([0-9a-fA-F]{64})\s+\*?(\S+)$' -and $Matches[2] -eq $Asset) { $Expected = $Matches[1].ToLower() }
    }
    if (-not $Expected) { throw "igris: $Asset is not listed in SHA256SUMS; not installing." }
    $Actual = (Get-FileHash -LiteralPath $Zip -Algorithm SHA256).Hash.ToLower()
    if ($Expected -ne $Actual) { throw "igris: the checksum of $Asset does not match SHA256SUMS; not installing." }

    Expand-Archive -LiteralPath $Zip -DestinationPath $Tmp -Force
    New-Item -ItemType Directory -Path $Dir -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $Tmp 'igris.exe') -Destination (Join-Path $Dir 'igris.exe') -Force
    Write-Output "igris: installed $(& (Join-Path $Dir 'igris.exe') --version) to $Dir"

    if ($env:IGRIS_NO_PATH -ne '1') {
        $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if (-not (($UserPath -split ';') -contains $Dir)) {
            [Environment]::SetEnvironmentVariable('Path', ($UserPath.TrimEnd(';') + ';' + $Dir), 'User')
            Write-Output "igris: added $Dir to your user PATH. Open a new terminal, then run:  igris"
        } else {
            Write-Output 'igris: run it with:  igris'
        }
    }
} finally {
    Remove-Item -LiteralPath $Tmp -Recurse -Force -ErrorAction SilentlyContinue
}
