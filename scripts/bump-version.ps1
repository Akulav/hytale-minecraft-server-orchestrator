# Bumps the app version everywhere it lives and turns CHANGELOG's [Unreleased] into the new version.
#   scripts\bump-version.ps1            -> patch (0.1.0 -> 0.1.1)
#   scripts\bump-version.ps1 minor      -> 0.1.0 -> 0.2.0
#   scripts\bump-version.ps1 major      -> 0.1.0 -> 1.0.0
#   scripts\bump-version.ps1 1.4.2      -> explicit version
# Cargo.toml is the source of truth; Cargo.lock, installer\hyve.iss and installer\Info.plist follow.
# ASCII only (Windows PowerShell 5.1 reads scripts as ANSI).
param([string]$Part = "patch")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$utf8 = New-Object System.Text.UTF8Encoding($false)

$cargoPath = Join-Path $root "Cargo.toml"
$cargo = [IO.File]::ReadAllText($cargoPath)
$current = [regex]::Match($cargo, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
$v = [version]($current -replace '-.*$', '')

switch -regex ($Part) {
    '^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$' { $new = $Part }
    '^major$' { $new = "$($v.Major + 1).0.0" }
    '^minor$' { $new = "$($v.Major).$($v.Minor + 1).0" }
    default   { $new = "$($v.Major).$($v.Minor).$($v.Build + 1)" }
}

# a Regex *instance* is needed to replace only the first match
$cargo = ([regex]'(?m)^(version\s*=\s*")[^"]+(")').Replace($cargo, "`${1}$new`${2}", 1)
[IO.File]::WriteAllText($cargoPath, $cargo, $utf8)

$lockPath = Join-Path $root "Cargo.lock"
if (Test-Path $lockPath) {
    $lock = [IO.File]::ReadAllText($lockPath)
    $lock = ([regex]'(name = "hyve"\r?\nversion = ")[^"]+(")').Replace($lock, "`${1}$new`${2}", 1)
    [IO.File]::WriteAllText($lockPath, $lock, $utf8)
}

$issPath = Join-Path $root "installer\hyve.iss"
$iss = [IO.File]::ReadAllText($issPath)
$iss = ([regex]'(#define AppVersion ")[^"]+(")').Replace($iss, "`${1}$new`${2}", 1)
[IO.File]::WriteAllText($issPath, $iss, $utf8)

$plistPath = Join-Path $root "installer\Info.plist"
$plist = [IO.File]::ReadAllText($plistPath)
$plist = ([regex]'(<key>CFBundleShortVersionString</key><string>)[^<]+(</string>)').Replace($plist, "`${1}$new`${2}", 1)
$plist = ([regex]'(<key>CFBundleVersion</key><string>)[^<]+(</string>)').Replace($plist, "`${1}$new`${2}", 1)
[IO.File]::WriteAllText($plistPath, $plist, $utf8)

$clPath = Join-Path $root "CHANGELOG.md"
$cl = [IO.File]::ReadAllText($clPath)
$date = Get-Date -Format "yyyy-MM-dd"
if ($cl -match '(?m)^## \[Unreleased\]') {
    $cl = ([regex]'(?m)^## \[Unreleased\][^\r\n]*').Replace($cl, "## [Unreleased]`n`n## [$new] - $date", 1)
} else {
    $cl = ([regex]'## \[').Replace($cl, "## [$new] - $date`n### Changed`n- `n`n## [", 1)
}
[IO.File]::WriteAllText($clPath, $cl, $utf8)

Write-Host "Version $current -> $new (Cargo.toml, Cargo.lock, installer\hyve.iss, installer\Info.plist, CHANGELOG.md)"
Write-Host "Next: check CHANGELOG.md, then  git commit -am `"Release v${new}: <summary>`"  and push main - the Build workflow tags and publishes."
