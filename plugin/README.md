# IHSTAY — Claude Code Plugin

A thin wrapper that registers Claude Code hooks so pending prompts land on the IHSTAY HUD.

## Requirements

You must also have the [IHSTAY tray app](https://github.com/sadwx/ihstay/releases) installed and running. The plugin writes entries to `~/.claude/pending/board.jsonl`; the tray app reads them and surfaces the HUD.

## Install

```bash
/plugin marketplace add github:sadwx/ihstay
/plugin install ihstay@ihstay
/reload-plugins
```

## Verify

Run:
```bash
/ihstay doctor
```

This checks:
- The three hooks are registered
- The hook scripts exist and are executable
- `~/.claude/pending/board.jsonl` is writable
- The configured terminal adapter binary is in `PATH`

## How it works

When Claude Code fires a `Notification`, `UserPromptSubmit`, or `Stop` event, the plugin's hook scripts append a JSONL line to `~/.claude/pending/board.jsonl`. The IHSTAY tray app watches this file, renders entries in its HUD, and lets you click through to the owning terminal pane.

## Uninstall

```bash
/plugin uninstall ihstay
```

The hook scripts stop being invoked. Your existing `~/.claude/pending/` state (config, logs) is preserved — delete that directory manually if you want a clean slate.
