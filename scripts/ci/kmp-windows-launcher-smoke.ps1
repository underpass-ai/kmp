$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$Plugin = Join-Path $Root "plugins/kmp"
$Fixture = Join-Path $Root "tests/plugin/kmp-smoke.jsonl"
$Work = Join-Path ([System.IO.Path]::GetTempPath()) ("kmp-windows-launcher-" + [guid]::NewGuid())
$Copy = Join-Path $Work "kmp"
$PathBin = Join-Path $Work "path-bin"
$MismatchBin = Join-Path $Work "mismatch-bin"
$OriginalPath = $env:PATH
$OriginalOverride = $env:KMP_MCP_BIN

function Fail([string] $Message) {
    throw "KMP Windows launcher smoke: $Message"
}

try {
    $Directories = @(
        (Join-Path $Copy "scripts")
        (Join-Path $Copy "bin")
        $PathBin
        $MismatchBin
    )
    New-Item -ItemType Directory -Force -Path $Directories | Out-Null
    Copy-Item (Join-Path $Plugin "scripts/run-embedded-mcp.cmd") (Join-Path $Copy "scripts")
    Copy-Item -Recurse (Join-Path $Plugin ".codex-plugin") $Copy
    Copy-Item -Recurse (Join-Path $Plugin ".claude-plugin") $Copy

    $Built = Join-Path $Plugin "bin/kmp-mcp.exe"
    if (-not (Test-Path $Built)) {
        Fail "the package smoke did not build plugins/kmp/bin/kmp-mcp.exe"
    }
    Copy-Item $Built (Join-Path $PathBin "kmp-mcp.exe")

    $StaleSource = Join-Path $Work "stale.rs"
    @'
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("kmp-mcp 0.0.1 (store format 1)");
    } else {
        println!("stale-cache-ran");
    }
}
'@ | Set-Content -Encoding utf8 $StaleSource
    $Stale = Join-Path $Copy "bin/kmp-mcp.exe"
    & rustc $StaleSource -o $Stale
    if ($LASTEXITCODE -ne 0) {
        Fail "could not compile the stale engine fixture"
    }
    Copy-Item $Stale (Join-Path $MismatchBin "kmp-mcp.exe")

    $Launcher = Join-Path $Copy "scripts/run-embedded-mcp.cmd"
    $SelectionError = Join-Path $Work "selection.err"
    $env:PATH = "$PathBin;$OriginalPath"
    Remove-Item Env:KMP_MCP_BIN -ErrorAction SilentlyContinue
    $Responses = Get-Content $Fixture | & $Launcher 2> $SelectionError
    if ($LASTEXITCODE -ne 0) {
        $Diagnostic = Get-Content -Raw $SelectionError
        Fail "matching PATH fallback exited $LASTEXITCODE`n$Diagnostic"
    }
    $ResponseText = $Responses -join "`n"
    if ($ResponseText -notmatch '"name":"kmp_wake"') {
        Fail "matching PATH engine did not serve the MCP fixture"
    }
    if ((Get-Content -Raw $SelectionError) -notmatch 'cache engine 0\.0\.1 does not match plugin') {
        Fail "stale cache engine was not diagnosed"
    }

    $MismatchError = Join-Path $Work "mismatch.err"
    $env:PATH = "$MismatchBin;$OriginalPath"
    $null = & $Launcher 2> $MismatchError
    if ($LASTEXITCODE -eq 0) {
        Fail "launcher started without an engine matching its manifest"
    }
    if ((Get-Content -Raw $MismatchError) -notmatch 'run kmp setup') {
        Fail "version mismatch did not name the setup repair"
    }

    $env:KMP_MCP_BIN = $Stale
    $Explicit = & $Launcher
    if (($Explicit -join "`n") -ne "stale-cache-ran") {
        Fail "explicit KMP_MCP_BIN override stopped winning"
    }

    Write-Host "KMP Windows launcher smoke passed"
}
finally {
    $env:PATH = $OriginalPath
    if ($null -eq $OriginalOverride) {
        Remove-Item Env:KMP_MCP_BIN -ErrorAction SilentlyContinue
    } else {
        $env:KMP_MCP_BIN = $OriginalOverride
    }
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}
