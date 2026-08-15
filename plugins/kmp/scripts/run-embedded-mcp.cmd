@echo off
rem KMP plugin launcher for Windows hosts. Mirrors run-embedded-mcp.sh:
rem selects the embedded backend and leaves data-directory resolution to
rem the kernel (KMP_MCP_DATA_DIR, project root, then per-user data home).
setlocal

set "PLUGIN_ROOT=%~dp0.."
set "BINARY=%PLUGIN_ROOT%\bin\kmp-mcp.exe"

if not exist "%BINARY%" (
  echo KMP plugin: missing executable %BINARY% 1>&2
  echo KMP plugin: build the local plugin bundle before installing it 1>&2
  exit /b 127
)

set "KMP_MCP_BACKEND=embedded"

"%BINARY%" %*
