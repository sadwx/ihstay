# pending-board (WSL deltas)

This change document expresses the deltas that `add-wsl-support` introduces on top of the working spec at `openspec/specs/pending-board/spec.md`.

## MODIFIED Requirements

### Requirement: Live / stale liveness tracking

The system SHALL continuously verify that every live entry on the board corresponds to a still-running Claude Code process, and promote dead entries to the `stale` state. Entries that originated inside WSL SHALL be treated as live without consulting the Windows process table, because their `claude_pid` belongs to a different OS namespace.

#### Scenario: Claude Code process still alive

- **WHEN** the Reaper runs its periodic check (every 30 seconds) on a live entry with `claude_pid = P` and `wsl_distro = None`
- **AND** process `P` exists in the OS process table
- **AND** `~/.claude/sessions/P.json` exists with a `sessionId` matching the entry's `session_id`
- **THEN** the entry SHALL remain in the `live` state

#### Scenario: Process died — entry promoted to stale

- **WHEN** the Reaper runs on a live entry with `wsl_distro = None`
- **AND** process `claude_pid` no longer exists in the process table
- **THEN** the Reaper SHALL append `{"op":"stale","ts":<iso>,"session_id":<id>,"reason":"pid_dead"}` to `board.jsonl`
- **AND** mutate the entry's state to `stale` in the `StateStore`

#### Scenario: PID recycled to an unrelated process

- **WHEN** the Reaper runs on a live entry with `wsl_distro = None`
- **AND** `claude_pid = P` is alive but `~/.claude/sessions/P.json` does not exist or its `sessionId` does not match the entry
- **THEN** the Reaper SHALL write a `stale` op with `reason = "session_file_missing"` or `"mismatch"` respectively and mutate the entry

#### Scenario: WSL-origin entry is trusted live

- **WHEN** the Reaper runs on a live entry with `wsl_distro = Some(<distro_name>)`
- **THEN** the Reaper SHALL skip the Windows-side process and session-file checks for that entry
- **AND** the entry SHALL remain `live` until the Claude Code session in WSL emits a clearing op (`UserPromptSubmit` or `Stop`), the user dismisses it manually, or the periodic stale cleanup loop expires it after the configured TTL

### Requirement: Click to focus live terminal pane

The system SHALL focus the exact terminal pane owning a live entry when the user clicks that entry. When the hook captured `$WEZTERM_PANE` at notification time the system SHALL prefer that pane id over the host-side ancestor walk — this is what makes click-to-focus work across the WSL/Windows boundary (where the ancestor walk cannot succeed) and resolves the right pane on a Windows host with multiple WezTerm tabs. If the captured pane is no longer addressable (e.g. the user closed it), the click SHALL fall through to the resume path described in *Click to resume stale entry* rather than surface an error.

#### Scenario: Captured WezTerm pane is focused

- **WHEN** the user clicks a live entry whose `wezterm_pane_id` is `Some(<id>)`
- **THEN** the `WezTermAdapter` SHALL call `wezterm cli activate-pane --pane-id <id>` directly, without consulting the process tree
- **AND** the WezTerm top-level window SHALL be brought to the foreground

#### Scenario: WezTerm pane is focused via ancestor walk (no captured pane id)

- **WHEN** the user clicks a live entry with `wezterm_pane_id = None`, `wsl_distro = None`, and an ancestor walk from `claude_pid` matches a `wezterm-gui` process
- **THEN** the `WezTermAdapter` SHALL call `wezterm cli list --format json`, find the pane whose `pid` matches an ancestor in the walk, and call `wezterm cli activate-pane --pane-id <matched>`
- **AND** the WezTerm top-level window SHALL be brought to the foreground

#### Scenario: iTerm2 session is focused

- **WHEN** the user clicks a live entry on macOS with `wezterm_pane_id = None`, `wsl_distro = None`, and an ancestor walk matching an `iTerm2` process
- **THEN** the `ITerm2Adapter` SHALL activate iTerm2 via `osascript` and select the session whose `tty` matches the ancestor walk's terminal tty

#### Scenario: WSL live entry click without captured pane id

- **WHEN** the user clicks a live entry with `wsl_distro = Some(<distro>)` and `wezterm_pane_id = None` (e.g. `WSLENV` not configured to forward `WEZTERM_PANE` across the boundary)
- **THEN** the click SHALL be routed to the resume path because the host-side ancestor walk cannot reach a process inside WSL

#### Scenario: Captured pane no longer exists

- **WHEN** `wezterm cli activate-pane --pane-id <id>` fails (the pane was closed since the hook fired)
- **THEN** the click SHALL fall through to `spawn_resume` so the user lands on a working session instead of an error

#### Scenario: No adapter matched

- **WHEN** the ancestor walk on a non-WSL entry with no captured pane id returns no known terminal binary
- **THEN** the click SHALL fall through to the user's default adapter via `spawn_resume` rather than failing silently

### Requirement: Click to resume stale entry

The system SHALL resume a stale session in a new terminal tab by invoking `claude --resume <session_id>` via the user's default adapter. For entries that originated inside WSL, the resume path SHALL launch Claude inside the originating WSL distro and SHALL set the new tab's working directory to the corresponding `\\wsl$\<distro>\<linux-cwd>` UNC path so the tab opens at the right project.

#### Scenario: Stale WezTerm entry resumed (non-WSL)

- **WHEN** the user clicks a stale entry with `wsl_distro = None` and the default adapter is WezTerm
- **THEN** the adapter SHALL run `wezterm cli spawn --cwd <original_cwd> -- claude --resume <session_id>`

#### Scenario: Stale iTerm2 entry resumed (non-WSL)

- **WHEN** the user clicks a stale entry on macOS with `wsl_distro = None` and the default adapter is iTerm2
- **THEN** the adapter SHALL invoke `osascript` to run `tell application "iTerm2" to tell current window to create tab with default profile command "cd <cwd> && claude --resume <session_id>"`

#### Scenario: Stale entry click on a WSL-origin session

- **WHEN** the user clicks a stale entry with `wsl_distro = Some("Ubuntu-24.04")` and `cwd = "/home/user/project"`
- **THEN** the WezTerm adapter SHALL spawn a new tab with working directory `\\wsl$\Ubuntu-24.04\home\user\project`
- **AND** the tab SHALL run `wsl.exe -d Ubuntu-24.04 -e claude --resume <session_id>` so the resumed Claude session lives inside the originating distro

## ADDED Requirements

### Requirement: WSL distro identification on board entries

The system SHALL record the originating WSL distro on every entry produced by a Claude Code session running inside WSL, so downstream consumers (reaper, adapters) can route the entry correctly across the WSL/Windows boundary.

#### Scenario: Hook fires inside WSL

- **WHEN** the bash hook script (`pending_hook.sh`) handles a `Notification` event
- **AND** the environment variable `WSL_DISTRO_NAME` is non-empty
- **THEN** the appended `add` op SHALL include a string field `"wsl_distro": "<name>"` matching the value of `$WSL_DISTRO_NAME`

#### Scenario: Hook fires on macOS

- **WHEN** the bash hook script handles a `Notification` event
- **AND** `WSL_DISTRO_NAME` is unset or empty
- **THEN** the `add` op SHALL omit the `wsl_distro` field entirely (not write `null`, not write an empty string)

### Requirement: WezTerm pane identification on board entries

The system SHALL record the WezTerm pane id captured from the hook's environment on every entry produced by a Claude Code session running inside a WezTerm shell, so click-to-focus can address that exact pane across both native Windows and the WSL/Windows boundary.

#### Scenario: Hook fires inside a WezTerm shell

- **WHEN** the hook script handles a `Notification` event
- **AND** the environment variable `WEZTERM_PANE` is non-empty (set by WezTerm in every shell it spawns — pwsh, bash, zsh, including bash inside WSL when `WSLENV` propagates it)
- **THEN** the appended `add` op SHALL include a string field `"wezterm_pane_id": "<value>"` matching `$WEZTERM_PANE`

#### Scenario: Hook fires outside WezTerm

- **WHEN** the hook script handles a `Notification` event
- **AND** `WEZTERM_PANE` is unset or empty (e.g. the user runs Claude inside a non-WezTerm terminal, or inside WSL without `WSLENV=WEZTERM_PANE/u` configured)
- **THEN** the `add` op SHALL omit the `wezterm_pane_id` field entirely (not write `null`, not write an empty string)

### Requirement: Automatic WSLENV configuration on Windows

The tray app SHALL ensure `WEZTERM_PANE/u` is included in the user's persistent `WSLENV` environment variable when WSL is detected on the host, so that click-to-focus works for WSL-origin entries without any manual user setup. The check SHALL run in the background on every app launch, SHALL be idempotent on subsequent runs, and SHALL preserve any pre-existing `WSLENV` tokens at either the machine or user scope so unrelated software (e.g. a JDK installer setting `JRE_HOME/p`) keeps working.

#### Scenario: User WSLENV empty, machine WSLENV has tokens

- **WHEN** the tray app starts on Windows
- **AND** WSL is detected (i.e. `wsl.exe -l -q` exits 0 with non-empty output)
- **AND** `HKCU\Environment\WSLENV` is unset or empty
- **AND** `HKLM\…\Session Manager\Environment\WSLENV` contains existing tokens (e.g. `JRE_HOME/p`)
- **THEN** the app SHALL write to `HKCU\Environment\WSLENV` a value built by appending `WEZTERM_PANE/u` to a copy of the machine-scope tokens, separated by `:` (e.g. `JRE_HOME/p:WEZTERM_PANE/u`)
- **AND** broadcast `WM_SETTINGCHANGE` so subsequently-spawned shells inherit the new value

This avoids silently clobbering the machine-scope tokens at process launch — Windows resolves USER over MACHINE for non-PATH env vars, so a HKCU value of `WEZTERM_PANE/u` alone would suppress the machine value entirely.

#### Scenario: User WSLENV missing the token but already has other entries

- **WHEN** the tray app starts on Windows, WSL is detected
- **AND** `HKCU\Environment\WSLENV` is non-empty but does not include `WEZTERM_PANE/u`
- **THEN** the app SHALL append `WEZTERM_PANE/u` to the existing user value (preserving any existing tokens, separated by `:`)
- **AND** broadcast `WM_SETTINGCHANGE`

#### Scenario: User WSLENV both empty and machine WSLENV empty

- **WHEN** the tray app starts on Windows, WSL is detected
- **AND** both `HKCU\Environment\WSLENV` and `HKLM\…\Environment\WSLENV` are unset or empty
- **THEN** the app SHALL set `HKCU\Environment\WSLENV` to exactly `WEZTERM_PANE/u`

#### Scenario: User WSLENV already configured

- **WHEN** the tray app starts on Windows, WSL is detected
- **AND** `HKCU\Environment\WSLENV` already includes the token `WEZTERM_PANE/u`
- **THEN** the app SHALL make no changes and log at DEBUG level

#### Scenario: WSLENV updated while WezTerm is already running

- **WHEN** the tray app starts on Windows, WSL is detected
- **AND** `HKCU\Environment\WSLENV` did not include `WEZTERM_PANE/u` and the app rewrote it
- **AND** at least one `wezterm-gui.exe` process is running at the time of the write
- **THEN** the app SHALL surface a one-shot warning to the HUD telling the user to restart WezTerm so its child shells pick up the new `WSLENV`
- **AND** the HUD SHALL be shown if hidden, so the warning is visible without the user clicking the tray
- **AND** the app SHALL record the PIDs of every running `wezterm-gui.exe` at the moment of the write
- **AND** the warning SHALL auto-dismiss when every recorded PID has exited (the user has restarted WezTerm), without requiring user interaction
- **AND** the warning SHALL also be dismissible by the user via a banner button, for the corner case where WezTerm was relaunched from a parent process with stale env (e.g. PowerToys Command Palette) so the recorded PIDs exit but new wezterm-gui inherits the same stale env
- **AND** the warning SHALL NOT persist across app restarts (it represents the boot-time stale-env condition only)

This addresses the practical reality that WezTerm reads `WSLENV` once at process launch and never refreshes — even though Windows broadcasts `WM_SETTINGCHANGE`, a long-running WezTerm keeps using the env it captured at startup. Without this warning, click-to-focus into WSL silently falls through to spawn-a-new-tab and the user has no signal that the fix is one restart away.

#### Scenario: WSL not detected

- **WHEN** the tray app starts and `wsl.exe` is missing from `PATH`, or `wsl.exe -l -q` exits non-zero, or returns no distros
- **THEN** the app SHALL skip the WSLENV check entirely (no env-var write, no log noise above DEBUG)

#### Scenario: Non-Windows platforms

- **WHEN** the tray app starts on macOS
- **THEN** the WSLENV configuration logic SHALL be compiled out entirely (no runtime cost)

### Requirement: Plugin manifest covers Linux platforms

The Claude Code plugin SHALL register its bash hook script for the `linux` platform in addition to `windows` and `darwin`, so that running `claude plugin install ihstay@ihstay` inside WSL registers all three hooks without manual `settings.json` editing.

#### Scenario: Plugin install from inside WSL

- **WHEN** a user runs `claude plugin marketplace add sadwx/ihstay` followed by `claude plugin install ihstay@ihstay` inside a WSL distro
- **THEN** Claude Code SHALL register the bash variant of `pending_hook.sh` for the `Notification`, `UserPromptSubmit`, and `Stop` events under the user's `~/.claude/settings.json` (or its plugin equivalent)
- **AND** subsequent Claude sessions inside that WSL distro SHALL fire the hook on every event without further configuration
