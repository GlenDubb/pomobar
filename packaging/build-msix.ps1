<#
.SYNOPSIS
    Builds WillPomo and packages it as MSIX (one per architecture) plus an .msixbundle.

.DESCRIPTION
    Uses the Windows SDK's makepri.exe and makeappx.exe. The packages are not
    signed: the Microsoft Store signs them when you submit. To try a package
    locally before submitting, see "Testing the packaged app" in README.md.

.EXAMPLE
    ./packaging/build-msix.ps1                 # x64 + ARM64 and a bundle
    ./packaging/build-msix.ps1 -Arch x64       # x64 only
    ./packaging/build-msix.ps1 -SkipBuild      # package existing release builds
#>
[CmdletBinding()]
param(
    [ValidateSet('x64', 'arm64')]
    [string[]] $Arch = @('x64', 'arm64'),
    [string] $Version,
    [string] $OutDir = 'dist',
    [switch] $SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Find-SdkTool([string] $Name) {
    foreach ($kits in @("${env:ProgramFiles(x86)}\Windows Kits\10\bin", "$env:ProgramFiles\Windows Kits\10\bin")) {
        if (-not (Test-Path $kits)) { continue }
        $hit = Get-ChildItem -Path $kits -Filter $Name -Recurse -ErrorAction SilentlyContinue |
            Where-Object { $_.Directory.Name -eq 'x64' } |
            Sort-Object -Descending {
                $v = $_.Directory.Parent.Name
                if ($v -match '^\d+(\.\d+)+$') { [version]$v } else { [version]'0.0' }
            } |
            Select-Object -First 1
        if ($hit) { return $hit.FullName }
    }
    throw "$Name not found. Install the Windows SDK, e.g.: winget install Microsoft.WindowsSDK.10.0.26100"
}

function Invoke-Native([string] $Exe, [string[]] $Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$(Split-Path -Leaf $Exe) failed (exit code $LASTEXITCODE)" }
}

# The Store needs a four-part version with the last part 0, e.g. 1.2.3.0.
if (-not $Version) {
    $m = Select-String -Path 'Cargo.toml' -Pattern '^version\s*=\s*"(\d+)\.(\d+)\.(\d+)' | Select-Object -First 1
    if (-not $m) { throw 'Could not read the version from Cargo.toml' }
    $g = $m.Matches[0].Groups
    $Version = '{0}.{1}.{2}.0' -f $g[1].Value, $g[2].Value, $g[3].Value
}
if ($Version -notmatch '^\d+\.\d+\.\d+\.0$') { throw "Version must look like 1.2.3.0 (got $Version)" }

$identity = Get-Content 'packaging/identity.json' -Raw | ConvertFrom-Json
$makeappx = Find-SdkTool 'makeappx.exe'
$makepri = Find-SdkTool 'makepri.exe'
$targets = @{ x64 = 'x86_64-pc-windows-msvc'; arm64 = 'aarch64-pc-windows-msvc' }

New-Item -ItemType Directory -Force $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
Get-ChildItem $OutDir -Filter 'WillPomo_*' -File -ErrorAction SilentlyContinue | Remove-Item -Force

Write-Host "Packaging WillPomo $Version for $($Arch -join ', ')"
$packages = @()
foreach ($a in $Arch) {
    $target = $targets[$a]
    if (-not $SkipBuild) {
        Invoke-Native 'cargo' @('build', '--release', '--locked', '--target', $target)
    }
    $exe = "target/$target/release/pomobar.exe"
    if (-not (Test-Path $exe)) { throw "Missing $exe. Build it first or drop -SkipBuild." }

    # Lay out the package contents.
    $stage = Join-Path $OutDir "stage-$a"
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Force "$stage/Assets" | Out-Null
    Copy-Item $exe "$stage/WillPomo.exe"
    Copy-Item 'packaging/Assets/*.png' "$stage/Assets/"
    Copy-Item 'LICENSE' "$stage/LICENSE.txt"
    $manifest = (Get-Content 'packaging/AppxManifest.xml' -Raw).
        Replace('{{NAME}}', $identity.Name).
        Replace('{{PUBLISHER}}', [Security.SecurityElement]::Escape($identity.Publisher)).
        Replace('{{PUBLISHER_DISPLAY_NAME}}', [Security.SecurityElement]::Escape($identity.PublisherDisplayName)).
        Replace('{{VERSION}}', $Version).
        Replace('{{ARCH}}', $a)
    [IO.File]::WriteAllText("$stage/AppxManifest.xml", $manifest, (New-Object Text.UTF8Encoding $false))

    # resources.pri lets Windows pick the right logo size for each scale and taskbar size.
    $priConfig = Join-Path $OutDir 'priconfig.xml'
    Invoke-Native $makepri @('createconfig', '/cf', $priConfig, '/dq', 'en-US', '/pv', '10.0.0', '/o')
    Invoke-Native $makepri @('new', '/pr', $stage, '/cf', $priConfig, '/mn', "$stage/AppxManifest.xml", '/of', "$stage/resources.pri", '/o')

    $msix = Join-Path $OutDir "WillPomo_${Version}_$a.msix"
    Invoke-Native $makeappx @('pack', '/o', '/h', 'SHA256', '/d', $stage, '/p', $msix)
    $packages += $msix
}

if ($packages.Count -gt 1) {
    $bundleIn = Join-Path $OutDir 'bundle-input'
    if (Test-Path $bundleIn) { Remove-Item -Recurse -Force $bundleIn }
    New-Item -ItemType Directory -Force $bundleIn | Out-Null
    $packages | ForEach-Object { Copy-Item $_ $bundleIn }
    $bundle = Join-Path $OutDir "WillPomo_${Version}.msixbundle"
    Invoke-Native $makeappx @('bundle', '/o', '/bv', $Version, '/d', $bundleIn, '/p', $bundle)
    Write-Host "`nUpload this to Partner Center: $bundle"
} else {
    Write-Host "`nPackage: $($packages[0])"
}
