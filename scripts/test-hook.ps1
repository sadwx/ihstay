#!/usr/bin/env pwsh
# test-hook.ps1 — Regression test for plugin/hooks/pending_hook.ps1.
#
# Drives the hook the way Claude Code does: payload JSON on stdin, one event
# per invocation. Runs against an isolated $HOME so it never touches the real
# ~/.claude/pending board.
#
# Guards the stdin-reading path that motivated [Console]::In.ReadToEnd():
# long `last_assistant_message` / `prompt` payloads must parse and append a
# clear op, and hook-errors.log must stay empty.
#
# NOTE: the original `$input | Out-String` corruption (truncated / trailing
# JSON, logged under Path 'prompt'/'last_assistant_message') only reproduced
# inside Claude Code's actual hook spawn, not in a plain `payload | pwsh -File`
# pipe — so this test does NOT distinguish the old read from the new one in
# isolation. It is a forward guard: the hook must keep correctly handling
# oversized payloads and never log a parse error.
#
# Exit code = number of failed checks (0 = all pass), so CI can gate on it.

$ErrorActionPreference = "Stop"

$hook = Join-Path $PSScriptRoot ".." "plugin" "hooks" "pending_hook.ps1"
if (-not (Test-Path $hook)) {
    Write-Host "FAIL: hook not found at $hook" -ForegroundColor Red
    exit 1
}

# Isolated home — pwsh derives $HOME from USERPROFILE on Windows. The hook
# writes to $HOME/.claude/pending, so redirecting USERPROFILE sandboxes it.
$sandbox = Join-Path ([System.IO.Path]::GetTempPath()) ("ihstay-hooktest-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $sandbox -Force | Out-Null
$env:USERPROFILE = $sandbox

$boardFile = Join-Path $sandbox ".claude" "pending" "board.jsonl"
$errorLog  = Join-Path $sandbox ".claude" "pending" "logs" "hook-errors.log"

$pass = 0
$fail = 0

function Invoke-Hook($payload) {
    # Pipe the payload to the hook over stdin, exactly as Claude Code does.
    $payload | & pwsh -NoProfile -ExecutionPolicy Bypass -File $hook
}

function Last-Line($path) {
    if (-not (Test-Path $path)) { return "" }
    # Force an array — Get-Content returns a scalar string for a single-line
    # file, and [-1] on a string yields the last character, not the last line.
    $lines = @(Get-Content $path)
    if ($lines.Count -eq 0) { return "" }
    return $lines[-1]
}

# The hook emits ops from a plain @{} hashtable, whose JSON key order is not
# stable — so assert on field presence, not field ordering.
function Is-ClearOp($line, $reason) {
    return ($line -match '"op":"clear"') -and ($line -match ('"reason":"' + [regex]::Escape($reason) + '"'))
}

function Check($name, [scriptblock]$test) {
    Write-Host -NoNewline "  [$name] "
    try {
        if (& $test) {
            Write-Host "PASS" -ForegroundColor Green
            $script:pass++
        } else {
            Write-Host "FAIL" -ForegroundColor Red
            $script:fail++
        }
    } catch {
        Write-Host "ERROR: $_" -ForegroundColor Red
        $script:fail++
    }
}

Write-Host ""
Write-Host "pending_hook.ps1 — regression test" -ForegroundColor Magenta
Write-Host "Sandbox HOME: $sandbox" -ForegroundColor DarkGray
Write-Host ""

# 1. Stop with a very long last_assistant_message (the field that broke
#    $input | Out-String). Must append a clear op and log no error.
$longMsg = "a" * 5000
$stopPayload = @{
    hook_event_name       = "Stop"
    session_id            = "11111111-1111-1111-1111-111111111111"
    cwd                   = "D:/lab/proj"
    last_assistant_message = $longMsg
} | ConvertTo-Json -Compress
Invoke-Hook $stopPayload

Check "long Stop payload appends a clear op" {
    Is-ClearOp (Last-Line $boardFile) "stop"
}

# 2. UserPromptSubmit with a long prompt (the other field seen in the logs).
$longPrompt = "p" * 5000
$upsPayload = @{
    hook_event_name = "UserPromptSubmit"
    session_id      = "22222222-2222-2222-2222-222222222222"
    cwd             = "D:/lab/proj"
    prompt          = $longPrompt
} | ConvertTo-Json -Compress
Invoke-Hook $upsPayload

Check "long UserPromptSubmit payload appends a clear op" {
    Is-ClearOp (Last-Line $boardFile) "user_replied"
}

# 3. SessionEnd — the event in the user's report.
$sePayload = @{
    hook_event_name = "SessionEnd"
    session_id      = "33333333-3333-3333-3333-333333333333"
    cwd             = "D:/lab/proj"
} | ConvertTo-Json -Compress
Invoke-Hook $sePayload

Check "SessionEnd payload appends a clear op" {
    Is-ClearOp (Last-Line $boardFile) "session_ended"
}

# 4. Empty stdin must be a no-op, not an error.
Check "empty stdin is a clean no-op" {
    "" | & pwsh -NoProfile -ExecutionPolicy Bypass -File $hook
    $LASTEXITCODE -eq 0
}

# 5. The whole run must not have logged a single parse error.
Check "no errors logged to hook-errors.log" {
    -not (Test-Path $errorLog)
}

# Cleanup
Remove-Item $sandbox -Recurse -Force -ErrorAction SilentlyContinue

Write-Host ""
Write-Host ("=" * 50)
Write-Host "Result: $pass passed, $fail failed" -ForegroundColor $(if ($fail -eq 0) { "Green" } else { "Red" })
Write-Host ""

exit $fail
