@echo off
rem KMP plugin launcher for Windows hosts. Mirrors run-embedded-mcp.sh:
rem selects the embedded backend and leaves data-directory resolution to
rem the kernel (KMP_MCP_DATA_DIR, project root, then per-user data home).
setlocal

set "PLUGIN_ROOT=%~dp0.."

rem An explicit binary wins. The release bundle is built without the sqlite
rem engine — that is what keeps a default install free of a C toolchain — and
rem it otherwise takes priority over PATH, so without this an operator who ran
rem `cargo install kmp-mcp --features sqlite` could not reach the engine
rem through the plugin. It selects the executable and nothing else.
if not "%KMP_MCP_BIN%"=="" goto :explicitBinary
set "BINARY=%PLUGIN_ROOT%\bin\kmp-mcp.exe"
goto :bundledBinary

:explicitBinary
set "BINARY=%KMP_MCP_BIN%"
if not exist "%BINARY%" (
  echo KMP plugin: KMP_MCP_BIN is set to "%BINARY%", which does not exist. 1>&2
  exit /b 127
)
goto :run

:bundledBinary

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
