@echo off
rem Opens the desktop storybook (rur native). Extra arguments pass through, e.g.
rem   start.cmd --story "path\to\example.story.luau"
rem The first launch (or one after native\ changed) builds the app, which takes a few minutes.
setlocal
cd /d "%~dp0"

where node >nul 2>nul
if errorlevel 1 (
  echo Node 18+ is required but was not found on PATH.
  pause
  exit /b 1
)

if not exist node_modules (
  echo Installing dependencies...
  call npm install
  if errorlevel 1 (
    pause
    exit /b 1
  )
)

node cli\native.mjs %*
if errorlevel 1 (
  echo.
  echo The storybook exited with an error ^(code %errorlevel%^).
  pause
  exit /b %errorlevel%
)
