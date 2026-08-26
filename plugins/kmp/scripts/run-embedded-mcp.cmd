@echo off
rem KMP plugin launcher for Windows hosts. Mirrors run-embedded-mcp.sh:
rem selects the embedded backend and leaves data-directory resolution to
rem the kernel (KMP_MCP_DATA_DIR, project root, then per-user data home).
setlocal

set "PLUGIN_ROOT=%~dp0.."

rem An explicit binary is an operator override and wins without a version
rem gate. Every automatically selected binary must match the plugin manifest.
if not "%KMP_MCP_BIN%"=="" goto :explicitBinary
set "BUNDLED_BINARY=%PLUGIN_ROOT%\bin\kmp-mcp.exe"
set "PATH_BINARY="
for %%I in (kmp-mcp.exe) do set "PATH_BINARY=%%~$PATH:I"
set "PLUGIN_VERSION="
for /f "usebackq delims=" %%V in (`powershell.exe -NoProfile -NonInteractive -Command "$m=Get-Content -Raw -LiteralPath '%PLUGIN_ROOT%\.codex-plugin\plugin.json'|ConvertFrom-Json; $m.version"`) do set "PLUGIN_VERSION=%%V"
if not defined PLUGIN_VERSION (
  echo KMP plugin: cannot read its version from the plugin manifest. 1>&2
  exit /b 127
)
for /f "tokens=1 delims=+" %%V in ("%PLUGIN_VERSION%") do set "EXPECTED_ENGINE_VERSION=%%V"
set "BUNDLED_VERSION="
goto :checkBundled

:explicitBinary
set "BINARY=%KMP_MCP_BIN%"
if not exist "%BINARY%" (
  echo KMP plugin: KMP_MCP_BIN is set to "%BINARY%", which does not exist. 1>&2
  exit /b 127
)
goto :run

:checkBundled
if not exist "%BUNDLED_BINARY%" goto :checkPath
set "BINARY=%BUNDLED_BINARY%"
call :readBinaryVersion "%BINARY%"
if "%ACTUAL_VERSION%"=="%EXPECTED_ENGINE_VERSION%" goto :run
set "BUNDLED_VERSION=%ACTUAL_VERSION%"
if not defined BUNDLED_VERSION set "BUNDLED_VERSION=unknown"

:checkPath
if not defined PATH_BINARY goto :noMatchingBinary
if not exist "%PATH_BINARY%" goto :noMatchingBinary
set "BINARY=%PATH_BINARY%"
call :readBinaryVersion "%BINARY%"
if not "%ACTUAL_VERSION%"=="%EXPECTED_ENGINE_VERSION%" goto :noMatchingBinary
if defined BUNDLED_VERSION (
  echo KMP plugin: cache engine %BUNDLED_VERSION% does not match plugin %PLUGIN_VERSION%; using matching PATH engine. 1>&2
  echo KMP plugin: run kmp setup to repair the plugin-owned engine. 1>&2
)
goto :run

:noMatchingBinary
if defined BUNDLED_VERSION (
  echo KMP plugin: cache engine %BUNDLED_VERSION% does not match plugin %PLUGIN_VERSION%. 1>&2
  echo KMP plugin: no %EXPECTED_ENGINE_VERSION% engine was found on PATH; run kmp setup. 1>&2
  exit /b 127
)
if defined PATH_BINARY (
  echo KMP plugin: the PATH engine does not match plugin %PLUGIN_VERSION%. 1>&2
  echo KMP plugin: run kmp setup to install engine %EXPECTED_ENGINE_VERSION%. 1>&2
  exit /b 127
)
goto :nobinary


:readBinaryVersion
set "ACTUAL_VERSION="
for /f "tokens=2" %%V in ('"%~1" --version 2^>nul') do if not defined ACTUAL_VERSION set "ACTUAL_VERSION=%%V"
exit /b 0

:nobinary
echo KMP plugin: no kmp-mcp executable found. 1>&2
echo KMP plugin: looked for %PLUGIN_ROOT%\bin\kmp-mcp.exe and kmp-mcp on PATH. 1>&2
echo KMP plugin: or name one directly with KMP_MCP_BIN. 1>&2
echo KMP plugin: install one with "cargo install kmp-mcp", or install the plugin from a release package. 1>&2
exit /b 127

:run

set "KMP_MCP_BACKEND=embedded"

rem No %* — the launcher starts the MCP server and nothing else. The binary
rem reads a leading argument as a maintenance command (`migrate`, `--version`),
rem so forwarding whatever a host happened to pass would exit 2 instead of
rem serving, and only on Windows. The POSIX launcher already drops them.
"%BINARY%"
exit /b %ERRORLEVEL%
