#!/usr/bin/env bash
# smoke-test.sh — Interactive smoke test for IHSTAY
# Usage: bash scripts/smoke-test.sh

BOARD_DIR="$HOME/.claude/pending"
BOARD_FILE="$BOARD_DIR/board.jsonl"

write_board() { mkdir -p "$BOARD_DIR"; echo "$1" >> "$BOARD_FILE"; }
ts() { date -u '+%Y-%m-%dT%H:%M:%S.000Z'; }

pause_step() {
    echo ""
    echo "  >> $1"
    echo "     Press Enter to continue..."
    read -r
}

header() {
    echo ""
    echo "=== Step $1 : $2 ==="
}

# Clean slate
rm -f "$BOARD_FILE"
echo ""
echo "  IHSTAY — Smoke Test"
echo "  Make sure the app is running (cargo tauri dev)"
echo ""

# Step 1
header 1 "Add a permission_prompt entry"
write_board "{\"op\":\"add\",\"ts\":\"$(ts)\",\"session_id\":\"smoke-1\",\"cwd\":\"$PWD\",\"claude_pid\":99999,\"terminal_pid\":null,\"transcript_path\":\"/tmp/t.jsonl\",\"notification_type\":\"permission_prompt\",\"message\":\"May I run cargo test?\"}"
pause_step "HUD should appear with 1 red PERMISSION entry"

# Step 2
header 2 "Add an idle_prompt entry"
write_board "{\"op\":\"add\",\"ts\":\"$(ts)\",\"session_id\":\"smoke-2\",\"cwd\":\"/tmp/other-project\",\"claude_pid\":88888,\"terminal_pid\":null,\"transcript_path\":\"/tmp/t2.jsonl\",\"notification_type\":\"idle_prompt\",\"message\":\"What would you like to do next?\"}"
pause_step "HUD should show 2 groups: PERMISSION (red) + IDLE (blue)"

# Step 3
header 3 "Clear the permission entry"
write_board "{\"op\":\"clear\",\"ts\":\"$(ts)\",\"session_id\":\"smoke-1\",\"reason\":\"user_replied\"}"
pause_step "Permission entry disappears, only idle entry remains"

# Step 4
header 4 "Test dismiss"
echo "  Click the X button on the HUD header."
pause_step "Confirmation panel should appear with 5s countdown"

# Step 5
header 5 "Test tray re-open"
echo "  Left-click the tray icon to re-open the HUD."
pause_step "HUD should re-appear (cooldown cancelled)"

# Step 6
header 6 "Test settings"
echo "  Right-click tray > Settings... to open Settings window."
pause_step "Settings window should show sliders and toggles"

# Step 7
header 7 "Clear all entries (auto-hide test)"
write_board "{\"op\":\"clear\",\"ts\":\"$(ts)\",\"session_id\":\"smoke-2\",\"reason\":\"stop\"}"
pause_step "HUD should auto-hide after ~2 seconds"

# Step 8
header 8 "Reaper test (wait 35s)"
write_board "{\"op\":\"add\",\"ts\":\"$(ts)\",\"session_id\":\"smoke-reaper\",\"cwd\":\"/tmp\",\"claude_pid\":77777,\"terminal_pid\":null,\"transcript_path\":\"/tmp/t.jsonl\",\"notification_type\":\"permission_prompt\",\"message\":\"Reaper test - fake PID\"}"
echo "  Waiting 35 seconds for reaper..."
sleep 35
if grep -q '"stale".*smoke-reaper' "$BOARD_FILE"; then
    echo "  PASS: Reaper wrote stale op"
else
    echo "  FAIL: No stale op found"
fi
pause_step "Entry should show as STALE (grey)"

# Cleanup
header 9 "Cleanup"
rm -f "$BOARD_FILE"
rm -f "$BOARD_DIR/logs/hook-errors.log"
echo "  Board file cleaned."
echo ""
echo "  Smoke test complete!"
echo ""
