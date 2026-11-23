# Tests for install.ps1 with a local fixture release: no network, nothing outside a temporary folder.
# Usage (from the repository root, after cargo build): ./scripts/test-install.ps1 -Binary target/debug/igris.exe
param([Parameter(Mandatory = $true)][string]$Binary)
$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Install = Join-Path $Here 'install.ps1'
$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("igris-test-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Tmp | Out-Null
function Fail($Message) { Write-Error "FAIL: $Message"; exit 1 }

# 1. The download address.
$env:IGRIS_VERSION = 'v9.9.9'; $env:IGRIS_DRY_RUN = '1'
$Got = & $Install
if (-not ($Got -like '*igris-v9.9.9-x86_64-pc-windows-msvc.zip')) { Fail "wrong asset: $Got" }
Write-Output 'ok  asset name'
$env:IGRIS_DRY_RUN = ''

# 2. A fixture release built from the real binary: install works, igris.exe runs.
$Rel = Join-Path $Tmp 'rel'; $Pkg = Join-Path $Tmp 'pkg'
New-Item -ItemType Directory -Path $Rel, $Pkg | Out-Null
Copy-Item -LiteralPath $Binary -Destination (Join-Path $Pkg 'igris.exe')
$Asset = 'igris-vtest-x86_64-pc-windows-msvc.zip'
Compress-Archive -LiteralPath (Join-Path $Pkg 'igris.exe') -DestinationPath (Join-Path $Rel $Asset)
$Sum = (Get-FileHash -LiteralPath (Join-Path $Rel $Asset) -Algorithm SHA256).Hash.ToLower()
"$Sum  $Asset" | Set-Content -Path (Join-Path $Rel 'SHA256SUMS') -Encoding ascii
$env:IGRIS_VERSION = 'vtest'
$env:IGRIS_BASE_URL = ([Uri](Resolve-Path $Rel).Path).AbsoluteUri
$env:IGRIS_INSTALL_DIR = Join-Path $Tmp 'bin'
$env:IGRIS_NO_PATH = '1'
& $Install | Out-Null
$Out = & (Join-Path $env:IGRIS_INSTALL_DIR 'igris.exe') --version
if ($Out -notlike 'igris *') { Fail "the installed binary does not run: $Out" }
Write-Output 'ok  install from a fixture release, binary runs'

# 3. A wrong checksum is refused and nothing is installed.
"0000000000000000000000000000000000000000000000000000000000000000  $Asset" | Set-Content -Path (Join-Path $Rel 'SHA256SUMS') -Encoding ascii
$env:IGRIS_INSTALL_DIR = Join-Path $Tmp 'bin2'
$Refused = $false
try { & $Install | Out-Null } catch { $Refused = $_.Exception.Message -like '*does not match*' }
if (-not $Refused) { Fail 'a wrong checksum was not refused' }
if (Test-Path (Join-Path $env:IGRIS_INSTALL_DIR 'igris.exe')) { Fail 'a binary was installed despite the mismatch' }
Write-Output 'ok  a wrong checksum installs nothing'

Remove-Item -LiteralPath $Tmp -Recurse -Force -ErrorAction SilentlyContinue
Write-Output 'test-install: all checks passed'
