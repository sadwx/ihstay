---
description: Manage the IHSTAY plugin — status, install, doctor, uninstall hooks
argument-hint: "[status | install | doctor | hooks-uninstall]"
---

# /ihstay

You are the ihstay operator. Run the requested subcommand and report the result to the user.

Subcommand: **$ARGUMENTS** (defaults to `status` if empty)

---

## `status`

Report the current state of the pending board:

1. Read `~/.claude/pending/board.jsonl` (if it exists). Count lines.
2. Parse it and count live entries by notification type (permission_prompt, idle_prompt) and state (live, stale).
3. Report the tray app binary location if it's known (check `~/.claude/pending/config.toml` or PATH for `ihstay-app`).
4. Report whether the app process is currently running (via `tasklist` on Windows or `pgrep` on Unix).

Output format:
```
IHSTAY — status
  board.jsonl: <N> lines, <M> live entries (<P> permission, <I> idle), <S> stale
  tray app: <running | not running> (<path if known>)
  last activity: <timestamp of most recent op in board.jsonl, or "never">
```

---

## `install`

Explain to the user how to install the tray app:

1. Visit https://github.com/sadwx/ihstay/releases
2. Download the artifact for your OS
3. Launch it — the tray icon should appear

Do NOT download or run the binary for them. Just print the instructions.

---

## `doctor`

Run the following diagnostic checks and report each as OK or FAIL with a remediation hint on failure:

1. **Hooks registered**: Read `~/.claude/settings.json` OR verify the plugin is enabled in `~/.claude/plugins/installed_plugins.json`. Look for `Notification`, `UserPromptSubmit`, and `Stop` hooks.
2. **Hook scripts exist**: Check `${CLAUDE_PLUGIN_ROOT}/hooks/pending_hook.ps1` (Windows) or `pending_hook.sh` (Unix).
3. **Board file writable**: Try `touch ~/.claude/pending/board.jsonl` (create if missing). Verify append works.
4. **Log directory writable**: Same for `~/.claude/pending/logs/`.
5. **Terminal adapter in PATH**: Check `wezterm --version` (Windows) or `osascript -e 'tell application "iTerm2" to version'` (macOS).
6. **Tray app installed**: Check for `ihstay-app` on PATH or standard install locations.

Output format:
```
IHSTAY — doctor
  OK Hooks registered: Notification, UserPromptSubmit, Stop
  OK Hook scripts: pending_hook.ps1 (at <path>)
  OK Board file writable: ~/.claude/pending/board.jsonl
  OK Log directory writable: ~/.claude/pending/logs/
  OK Terminal adapter: wezterm 20240203-...
  FAIL Tray app: not found — install from https://github.com/sadwx/ihstay/releases
```

---

## `hooks-uninstall`

Help the user uninstall the plugin:

1. Explain that `/plugin uninstall ihstay` removes the hooks.
2. Offer to clean up `~/.claude/pending/` (board.jsonl, logs, config) — but do NOT delete without confirmation.

---

**Default (no argument):** Run `status`.
