# IHSTAY

> *I Have Something To Ask You* — a tray HUD that surfaces every waiting Claude Code session.

[![CI](https://github.com/sadwx/ihstay/actions/workflows/ci.yml/badge.svg)](https://github.com/sadwx/ihstay/actions/workflows/ci.yml)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS-blue)](#requirements)

A cross-platform tray app that surfaces every Claude Code CLI session waiting for your input — across projects, across terminals, in one floating window.

![Dismiss confirmation panel](./docs/screenshots/current-dismiss-v2.png)

## What it does

Claude Code sessions stall regularly: a `permission_prompt` waits for your approval, an `idle_prompt` waits for your next instruction. If you run several Claude Code sessions in parallel terminal tabs you lose flow finding the one that's waiting.

IHSTAY watches every session and pushes a single floating window when one of them needs you. Click an entry and it brings the exact WezTerm or iTerm2 pane that owns the session to the foreground. Answer the prompt, the window hides itself, you get back to what you were doing.

## Status

**Alpha (v0.1.0 pre-release).** First tagged release is available on the [releases page](https://github.com/sadwx/ihstay/releases). Binaries are unsigned during alpha — SmartScreen / Gatekeeper warnings are expected. See [`INSTALL.md`](./INSTALL.md) for install notes.

## How it works (high level)

```
Claude Code (multiple sessions)
  └─ Notification / UserPromptSubmit / Stop hooks
     └─ pending_hook.ps1  (Windows)
        pending_hook.sh   (macOS)
           └─ appends JSONL op to  ~/.claude/pending/board.jsonl

   Tauri 2 app (tray icon)
     └─ BoardWatcher → StateStore → VisibilityController → HUD window
                                 └─ Reaper (liveness)
                                 └─ TerminalAdapter (WezTerm / iTerm2)
```

- **Floating HUD**: 380 × 240 pixel draggable, non-activating window that auto-shows on the first pending entry and auto-hides when the board goes empty. Shows up to 3 entries at a time; scrolls when more accumulate.
- **Sorting**: permission > idle > stale, newest first within each group.
- **Click to focus**: live entries jump to the owning terminal pane. Stale entries (e.g. after a reboot) spawn a fresh `claude --resume <session_id>` in a new tab.
- **Dismiss with cooldown**: manually dismiss the window with a 5-second confirmation panel; configurable 15-minute cooldown; optional reminder when new items accumulate during the cooldown.
- **Cross-platform**: one Rust codebase for Windows and macOS. WezTerm adapter on both; iTerm2 adapter on macOS.

See `openspec/changes/archive/add-claude-pending-board/design.md` for the full design rationale.

## Requirements

- **Claude Code** installed and registered (any version with the `Notification`, `UserPromptSubmit`, and `Stop` hook events).
- **Terminal**:
  - WezTerm (Windows / macOS) — `wezterm` in `PATH`.
  - iTerm2 (macOS only) — for the iTerm2 adapter.
- **For building from source**: Rust 1.83+, the Tauri 2 prerequisites for your OS (`cargo-tauri`), and Node.js 20+ for the front-end toolchain.

Windows Terminal is explicitly **not supported** as a focus target because its public API cannot activate a specific tab. You can still run Claude Code inside Windows Terminal; clicking an entry just won't focus the right tab.

## Installation

Two steps:

1. Download the MSI (Windows) or DMG (macOS) from the [releases page](https://github.com/sadwx/ihstay/releases) and install. Launch the app — a pink "C" icon appears in the tray.
2. Click the tray icon to open the HUD. On the first-run card, click **[Install plugin]** — the app shells out to `claude plugin` to register the hooks. Alternatively, run from any terminal:
   ```bash
   claude plugin marketplace add sadwx/ihstay
   claude plugin install ihstay@ihstay
   ```

See [`INSTALL.md`](./INSTALL.md) for SmartScreen / Gatekeeper notes, verification, troubleshooting, and build-from-source instructions.

## Documentation

- [`INSTALL.md`](./INSTALL.md) — step-by-step install for end users
- [`openspec/changes/archive/add-claude-pending-board/proposal.md`](./openspec/changes/archive/add-claude-pending-board/proposal.md) — what and why
- [`openspec/changes/archive/add-claude-pending-board/design.md`](./openspec/changes/archive/add-claude-pending-board/design.md) — technical design
- [`openspec/changes/archive/add-claude-pending-board/specs/pending-board/spec.md`](./openspec/changes/archive/add-claude-pending-board/specs/pending-board/spec.md) — requirements and scenarios
- [`openspec/changes/archive/add-claude-pending-board/tasks.md`](./openspec/changes/archive/add-claude-pending-board/tasks.md) — implementation checklist
- [`docs/release-checklist.md`](./docs/release-checklist.md) — manual UX checklist per release

## Contributing

This project follows [spec-driven development](https://github.com/Fission-AI/OpenSpec). Before opening a PR for a non-trivial change:

1. Run `openspec new change <name>` and draft the artifacts.
2. Update or add requirements in the relevant `specs/<capability>/spec.md`.
3. Update `tasks.md` with a concrete implementation checklist.
4. Open the PR with the change proposal linked.

Small bug fixes and documentation improvements can skip the spec step.

## License

TBD — will be added before the first public release.
