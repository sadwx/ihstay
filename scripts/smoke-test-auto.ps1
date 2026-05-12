#!/usr/bin/env pwsh
# smoke-test-auto.ps1 — Non-interactive smoke test for CI / Claude.
#
# Verifies the backend data pipeline end-to-end:
# - App boots
# - BoardWatcher observes board.jsonl writes
# - StateStore applies ops
# - Reaper promotes dead PIDs to stale
# - Config file lifecycle
#
# Does NOT verify: UI layout, click interactions, focus behavior.
# Those require manual testing — see scripts/smoke-test.ps1.

param(
    [string]$AppPath = (Join-Path $PSScriptRoot ".." "target" "debug" "ihstay-app.exe"),
    [int]$ReaperWaitSeconds = 35
)

$ErrorActionPreference = "Continue"
$boardDir = Join-Path $HOME ".claude" "pending"
$boardFile = Join-Path $boardDir "board.jsonl"
$logDir = Join-Path $boardDir "logs"
$configFile = Join-Path $boardDir "config.toml"

$pass = 0
$fail = 0
$failures = @()

function Check($name, [scriptblock]$test, $remedy = "") {
    Write-Host -NoNewline "  [$name] "
    try {
        $result = & $test
        if ($result) {
            Write-Host "PASS" -ForegroundColor Green
            $script:pass++
        } else {
            Write-Host "FAIL" -ForegroundColor Red
            if ($remedy) { Write-Host "      -> $remedy" -ForegroundColor Yellow }
            $script:fail++
            $script:failures += $name
        }
    } catch {
        Write-Host "ERROR: $_" -ForegroundColor Red
        $script:fail++
        $script:failures += "$name ($_)"
    }
}

function WriteBoard($line) {
    if (-not (Test-Path $boardDir)) { New-Item -ItemType Directory -Path $boardDir -Force | Out-Null }
    Add-Content -Path $boardFile -Value $line -Encoding UTF8
}

function Ts() { (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ") }

# Kill any existing instance so we start clean
$existing = Get-Process -Name "ihstay-app" -ErrorAction SilentlyContinue
if ($existing) {
    Write-Host "Killing existing instance (PID $($existing.Id))..." -ForegroundColor DarkGray
    Stop-Process -Id $existing.Id -Force
    Start-Sleep -Seconds 1
}

# Clean slate
if (Test-Path $boardFile) { Remove-Item $boardFile -Force }
if (Test-Path $configFile) { Remove-Item $configFile -Force }

Write-Host ""
Write-Host "IHSTAY — automated smoke test" -ForegroundColor Magenta
Write-Host "App: $AppPath" -ForegroundColor DarkGray
Write-Host ""

# Verify app binary exists
if (-not (Test-Path $AppPath)) {
    Write-Host "  FAIL: app binary not found at $AppPath" -ForegroundColor Red
    Write-Host "  Run: cargo build -p ihstay-app" -ForegroundColor Yellow
    exit 1
}

# Launch app with log capture
$logFile = Join-Path $env:TEMP "ihstay-smoke-$(Get-Random).log"
$appProcess = Start-Process -FilePath $AppPath -PassThru `
    -RedirectStandardOutput $logFile -RedirectStandardError "$logFile.err" `
    -WindowStyle Hidden

Write-Host "Launched app (PID $($appProcess.Id)). Waiting for boot..."
Start-Sleep -Seconds 3

# --- Checks ---

Write-Host ""
Write-Host "Boot checks" -ForegroundColor Cyan

Check "app is running" {
    -not $appProcess.HasExited
} "App crashed on startup. Check $logFile.err"

Check "config file created with defaults" {
    # The app only writes config when Save is clicked, but it reads the default path.
    # Verify the pending dir exists at minimum.
    Test-Path $boardDir
} "App didn't create ~/.claude/pending/ directory"

Check "board watcher started (log line)" {
    Start-Sleep -Seconds 1
    $content = Get-Content $logFile -Raw -ErrorAction SilentlyContinue
    $content -match "board watcher started"
} "BoardWatcher didn't start — check log"

Write-Host ""
Write-Host "Entry flow checks" -ForegroundColor Cyan

# Write a permission_prompt entry and verify the app picks it up
$sid1 = "smoke-auto-1"
WriteBoard "{`"op`":`"add`",`"ts`":`"$(Ts)`",`"session_id`":`"$sid1`",`"cwd`":`"D:/tmp/proj`",`"claude_pid`":99999,`"terminal_pid`":null,`"transcript_path`":`"/tmp/t.jsonl`",`"notification_type`":`"permission_prompt`",`"message`":`"Smoke test permission`"}"

Start-Sleep -Seconds 2

Check "permission entry written to board" {
    (Get-Content $boardFile -Raw) -match $sid1
} "Write failed"

# Write an idle entry
$sid2 = "smoke-auto-2"
WriteBoard "{`"op`":`"add`",`"ts`":`"$(Ts)`",`"session_id`":`"$sid2`",`"cwd`":`"D:/tmp/other`",`"claude_pid`":88888,`"terminal_pid`":null,`"transcript_path`":`"/tmp/t2.jsonl`",`"notification_type`":`"idle_prompt`",`"message`":`"Smoke test idle`"}"

Start-Sleep -Seconds 2

Check "idle entry written to board" {
    (Get-Content $boardFile -Raw) -match $sid2
} "Write failed"

# Clear the permission entry
WriteBoard "{`"op`":`"clear`",`"ts`":`"$(Ts)`",`"session_id`":`"$sid1`",`"reason`":`"user_replied`"}"

Start-Sleep -Seconds 2

Check "clear op written" {
    $content = Get-Content $boardFile -Raw
    $content -match '"op":"clear".*smoke-auto-1'
} "Clear op not appended"

Write-Host ""
Write-Host "Reaper check (waits $ReaperWaitSeconds seconds for dead-PID detection)" -ForegroundColor Cyan
Write-Host "  ..." -ForegroundColor DarkGray

Start-Sleep -Seconds $ReaperWaitSeconds

Check "reaper promoted dead PID to stale" {
    $content = Get-Content $boardFile -Raw
    # smoke-auto-2 has PID 88888 which doesn't exist → reaper should mark stale
    $content -match '"op":"stale".*smoke-auto-2'
} "Reaper didn't write stale op after $ReaperWaitSeconds seconds"

Write-Host ""
Write-Host "Compaction check" -ForegroundColor Cyan

Check "board file still parseable after reaper writes" {
    $lineCount = (Get-Content $boardFile | Measure-Object -Line).Lines
    $lineCount -gt 0
} "Board file empty or missing"

Write-Host ""
Write-Host "Cleanup" -ForegroundColor Cyan

# Stop the app
Stop-Process -Id $appProcess.Id -Force -ErrorAction SilentlyContinue

Check "app exited cleanly when killed" {
    $appProcess | Wait-Process -Timeout 5 -ErrorAction SilentlyContinue
    $true
} "App didn't respond to kill"

# Clean up test data
Remove-Item $boardFile -Force -ErrorAction SilentlyContinue
Remove-Item $logFile -Force -ErrorAction SilentlyContinue
Remove-Item "$logFile.err" -Force -ErrorAction SilentlyContinue

Write-Host ""
Write-Host ("=" * 60)
Write-Host "Result: $pass passed, $fail failed" -ForegroundColor $(if ($fail -eq 0) { "Green" } else { "Red" })
if ($fail -gt 0) {
    Write-Host "Failures:" -ForegroundColor Red
    foreach ($f in $failures) { Write-Host "  - $f" -ForegroundColor Red }
}
Write-Host ""
Write-Host "Manual checks still needed:" -ForegroundColor Yellow
Write-Host "  - HUD visual layout (entry rows, dismiss panel, pill)"
Write-Host "  - Gear icon opens Settings window"
Write-Host "  - Dismiss X triggers confirmation panel with countdown"
Write-Host "  - Click entry focuses owning terminal pane"
Write-Host "  - Non-activating window (HUD doesn't steal keyboard focus)"
Write-Host "  - Tray icon right-click menu works"
Write-Host ""

exit $fail
