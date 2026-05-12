#!/usr/bin/env pwsh
# smoke-test.ps1 — Interactive smoke test for IHSTAY
# Usage: pwsh scripts/smoke-test.ps1

$boardDir = Join-Path $HOME ".claude" "pending"
$boardFile = Join-Path $boardDir "board.jsonl"

function Write-Board($line) {
    if (-not (Test-Path $boardDir)) {
        New-Item -ItemType Directory -Path $boardDir -Force | Out-Null
    }
    Add-Content -Path $boardFile -Value $line -Encoding UTF8
}

function Pause-Step($msg) {
    Write-Host ""
    Write-Host "  >> $msg" -ForegroundColor Yellow
    Write-Host "     Press Enter to continue..." -ForegroundColor DarkGray
    Read-Host | Out-Null
}

function Show-Header($step, $title) {
    Write-Host ""
    Write-Host "=== Step $step : $title ===" -ForegroundColor Cyan
}

# --- Clean slate ---
if (Test-Path $boardFile) { Remove-Item $boardFile -Force }
Write-Host ""
Write-Host "  IHSTAY — Smoke Test" -ForegroundColor Magenta
Write-Host "  Make sure the app is running (cargo tauri dev)" -ForegroundColor DarkGray
Write-Host ""

# --- Step 1: Permission prompt ---
Show-Header 1 "Add a permission_prompt entry"
$ts = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
Write-Board "{`"op`":`"add`",`"ts`":`"$ts`",`"session_id`":`"smoke-1`",`"cwd`":`"C:/example/project-a`",`"claude_pid`":99999,`"terminal_pid`":null,`"transcript_path`":`"/tmp/t.jsonl`",`"notification_type`":`"permission_prompt`",`"message`":`"May I run cargo test?`"}"
Pause-Step "HUD should appear with 1 red PERMISSION entry"

# --- Step 2: Idle prompt ---
Show-Header 2 "Add an idle_prompt entry"
$ts = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
Write-Board "{`"op`":`"add`",`"ts`":`"$ts`",`"session_id`":`"smoke-2`",`"cwd`":`"C:/example/project-b`",`"claude_pid`":88888,`"terminal_pid`":null,`"transcript_path`":`"/tmp/t2.jsonl`",`"notification_type`":`"idle_prompt`",`"message`":`"What would you like to do next?`"}"
Pause-Step "HUD should show 2 groups: PERMISSION (red) + IDLE (blue)"

# --- Step 3: Clear one entry ---
Show-Header 3 "Clear the permission entry"
$ts = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
Write-Board "{`"op`":`"clear`",`"ts`":`"$ts`",`"session_id`":`"smoke-1`",`"reason`":`"user_replied`"}"
Pause-Step "Permission entry should disappear, only idle entry remains"

# --- Step 4: Dismiss ---
Show-Header 4 "Test dismiss"
Write-Host "  Click the X button on the HUD header." -ForegroundColor White
Pause-Step "Confirmation panel should appear with Wake me / Stay silent buttons and 5s countdown"

# --- Step 5: Tray icon ---
Show-Header 5 "Test tray re-open"
Write-Host "  Left-click the tray icon to re-open the HUD." -ForegroundColor White
Pause-Step "HUD should re-appear (cooldown cancelled)"

# --- Step 6: Settings ---
Show-Header 6 "Test settings"
Write-Host "  Right-click tray > Settings... to open Settings window." -ForegroundColor White
Pause-Step "Settings window should show sliders and toggles"

# --- Step 7: Auto-hide ---
Show-Header 7 "Clear all entries (auto-hide test)"
$ts = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
Write-Board "{`"op`":`"clear`",`"ts`":`"$ts`",`"session_id`":`"smoke-2`",`"reason`":`"stop`"}"
Pause-Step "HUD should auto-hide after ~2 seconds (grace delay)"

# --- Step 8: Reaper ---
Show-Header 8 "Reaper test (wait 35s)"
$ts = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
Write-Board "{`"op`":`"add`",`"ts`":`"$ts`",`"session_id`":`"smoke-reaper`",`"cwd`":`"/tmp`",`"claude_pid`":77777,`"terminal_pid`":null,`"transcript_path`":`"/tmp/t.jsonl`",`"notification_type`":`"permission_prompt`",`"message`":`"Reaper test - fake PID`"}"
Write-Host "  Waiting 35 seconds for reaper to detect dead PID..." -ForegroundColor DarkGray
Start-Sleep -Seconds 35
$content = Get-Content $boardFile -Raw
if ($content -match '"op":"stale".*"smoke-reaper"') {
    Write-Host "  PASS: Reaper wrote stale op for smoke-reaper" -ForegroundColor Green
} else {
    Write-Host "  FAIL: No stale op found for smoke-reaper" -ForegroundColor Red
}
Pause-Step "Entry should show as STALE (grey) in the HUD"

# --- Cleanup ---
Show-Header 9 "Cleanup"
Remove-Item $boardFile -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $boardDir "logs" "hook-errors.log") -Force -ErrorAction SilentlyContinue
Write-Host "  Board file cleaned." -ForegroundColor Green
Write-Host ""
Write-Host "  Smoke test complete!" -ForegroundColor Magenta
Write-Host ""
