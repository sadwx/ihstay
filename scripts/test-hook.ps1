#!/usr/bin/env pwsh
# test-hook.ps1 — Regression test for plugin/hooks/pending_hook.ps1.
#
# Drives the hook the way Claude Code does: payload JSON on stdin, one event
# per invocation. Runs against an isolated $HOME so it never touches the real
# ~/.claude/pending board.
#
# Guards the stdin-reading path in pending_hook.ps1:
#   - Cases 1-2: long `last_assistant_message` / `prompt` payloads must parse
#     and append a clear op (forward guard against the old `$input | Out-String`
#     truncation, which only reproduced inside Claude Code's real hook spawn).
#   - Case 5: a UTF-8 CJK/emoji payload fed as raw bytes under a Big5 console
#     code page — this DOES reproduce the OEM-decode regression that the plain
#     `[Console]::In.ReadToEnd()` introduced, and fails against the old read.
# hook-errors.log must stay empty across the whole run.
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

# 5. UTF-8 payload with CJK + emoji fed as RAW BYTES under a non-UTF-8 console
#    code page — the real Claude-Code-on-Windows spawn. git-bash -> pwsh
#    inherits the OEM code page (e.g. Big5/CP950 on zh-TW), and the old
#    [Console]::In.ReadToEnd() decoded the UTF-8 stdin with it, corrupting
#    `last_assistant_message` and breaking ConvertFrom-Json. Guards the
#    explicit UTF-8 StreamReader read. Unlike the string-pipe cases above, a
#    `$payload | pwsh` pipe CANNOT reproduce this — the bytes must reach stdin
#    unencoded, so we chcp to Big5 and redirect a UTF-8 file via cmd.exe (both
#    PowerShell lacks a `<` operator and its pipe re-encodes).
# Build the message from code points so this file's own encoding can't distort
# the test input, and via -join (not [char]+[char], which integer-promotes and
# would collapse the CJK to a number). This exact content — 修好了 ✅ 繁體中文與表情 🚀 測試。 —
# is a verified deterministic breaker: decoded as Big5 it truncates and the
# closing quote is swallowed, so the old [Console]::In read fails with
# "Unterminated string. Path 'last_assistant_message'". The UTF-8 StreamReader
# read parses it correctly.
$pre  = -join (0x4FEE, 0x597D, 0x4E86, 0x20, 0x2705, 0x20, 0x7E41, 0x9AD4, 0x4E2D, 0x6587, 0x8207, 0x8868, 0x60C5, 0x20 | ForEach-Object { [char]$_ })
$post = -join (0x20, 0x6E2C, 0x8A66, 0x3002 | ForEach-Object { [char]$_ })
$cjkMsg = $pre + [char]::ConvertFromUtf32(0x1F680) + $post
$utf8Payload = '{"hook_event_name":"Stop","session_id":"44444444-4444-4444-4444-444444444444","cwd":"D:/lab/proj","last_assistant_message":"' + $cjkMsg + '"}'
$payloadFile = Join-Path $sandbox "payload-utf8.json"
[System.IO.File]::WriteAllText($payloadFile, $utf8Payload, [System.Text.UTF8Encoding]::new($false))
cmd.exe /c "chcp 950 >nul & pwsh -NoProfile -ExecutionPolicy Bypass -File `"$hook`" < `"$payloadFile`"" | Out-Null

Check "UTF-8 CJK/emoji payload parses under Big5 code page (raw bytes)" {
    Is-ClearOp (Last-Line $boardFile) "stop"
}

# 6. The whole run must not have logged a single parse error.
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
