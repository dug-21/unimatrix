@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "NODE_REC=@@NODE@@"
set "NODE="
if exist "%NODE_REC%" set "NODE=%NODE_REC%"
if defined NODE goto :run
for %%N in (node.exe) do set "NODE=%%~$PATH:N"
if defined NODE goto :run
>&2 echo unimatrix: node not found; run 'npx unimatrix init'
exit /b 1
:run
"%NODE%" "%~dp0shim-entry.js" %*
exit /b %ERRORLEVEL%
