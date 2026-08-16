@echo off
rem KMP plugin launcher for Windows hosts. Mirrors run-embedded-mcp.sh:
rem selects the embedded backend and leaves data-directory resolution to
rem the kernel (KMP_MCP_DATA_DIR, project root, then per-user data home).
setlocal

set "PLUGIN_ROOT=%~dp0.."
set "BINARY=%PLUGIN_ROOT%\bin\kmp-mcp.exe"

rem The release bundle ships bin\kmp-mcp.exe and keeps priority. A marketplace
rem install has no bin\ — that path is gitignored — so fall back to an
rem installed kmp-mcp on PATH rather than failing to start.
if not exist "%BINARY%" (
  for %%I in (kmp-mcp.exe) do set "BINARY=%%~$PATH:I"
)

if not defined BINARY goto :nobinary
if not exist "%BINARY%" goto :nobinary
goto :run

:nobinary
echo KMP plugin: no kmp-mcp executable found. 1>&2
echo KMP plugin: looked for %PLUGIN_ROOT%\bin\kmp-mcp.exe and kmp-mcp on PATH. 1>&2
echo KMP plugin: install one with "cargo install kmp-mcp", or install the plugin from a release package. 1>&2
exit /b 127

:run

set "KMP_MCP_BACKEND=embedded"

"%BINARY%" %*
