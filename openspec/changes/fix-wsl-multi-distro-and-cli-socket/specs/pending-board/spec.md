# pending-board (cli-socket + WSL multi-distro deltas)

This change document expresses the deltas that `fix-wsl-multi-distro-and-cli-socket` introduces on top of the spec state produced by `add-wsl-support`. The MODIFIED requirements below carry the full final post-change content (including the `add-wsl-support` deltas) so archive merging produces a coherent working spec.

## MODIFIED Requirements

### Requirement: Click to focus live terminal pane

The system SHALL focus the exact terminal pane owning a live entry when the user clicks that entry. When the hook captured `$WEZTERM_PANE` at notification time the system SHALL prefer that pane id over the host-side ancestor walk — this is what makes click-to-focus work across the WSL/Windows boundary (where the ancestor walk cannot succeed) and resolves the right pane on a Windows host with multiple WezTerm tabs. If the captured pane is no longer addressable (e.g. the user closed it), the click SHALL fall through to the resume path described in *Click to resume stale entry* rather than surface an error.

Every `wezterm cli` subprocess invoked by the adapter (`list`, `activate-pane`, `spawn`) SHALL be executed with `WEZTERM_UNIX_SOCKET` set to the absolute path of the mux socket of a currently-running `wezterm-gui` process. WezTerm's CLI does not auto-discover the absolute socket path when the env var is unset; without this explicit setter, every subprocess fails with `failed to connect to Socket("gui-sock-<pid>")`. The tray app inherits no `WEZTERM_UNIX_SOCKET` because it is launched from the OS startup folder / tray, not from inside a WezTerm shell.

#### Scenario: Captured WezTerm pane is focused

- **WHEN** the user clicks a live entry whose `wezterm_pane_id` is `Some(<id>)`
- **THEN** the `WezTermAdapter` SHALL call `wezterm cli activate-pane --pane-id <id>` directly, without consulting the process tree
- **AND** the subprocess SHALL inherit `WEZTERM_UNIX_SOCKET=<socket-path>` where `<socket-path>` resolves a currently-running `wezterm-gui` mux
- **AND** the WezTerm top-level window SHALL be brought to the foreground

#### Scenario: WezTerm pane is focused via ancestor walk (no captured pane id)

- **WHEN** the user clicks a live entry with `wezterm_pane_id = None`, `wsl_distro = None`, and an ancestor walk from `claude_pid` matches a `wezterm-gui` process
- **THEN** the `WezTermAdapter` SHALL call `wezterm cli list --format json`, find the pane whose `pid` matches an ancestor in the walk, and call `wezterm cli activate-pane --pane-id <matched>`
- **AND** both subprocesses SHALL inherit `WEZTERM_UNIX_SOCKET=<socket-path>` as above
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

#### Scenario: Mux socket discovery on Windows

- **WHEN** the adapter computes `WEZTERM_UNIX_SOCKET` for a `wezterm cli` subprocess on Windows
- **THEN** it SHALL enumerate running `wezterm-gui.exe` processes, pick the one with the most recent start time, and construct `<USERPROFILE>\.local\share\wezterm\gui-sock-<pid>`
- **AND** when no `wezterm-gui.exe` is running it SHALL invoke the subprocess without a custom env (allowing wezterm's own auto-start to launch a fresh mux)

#### Scenario: Mux socket discovery on macOS

- **WHEN** the adapter computes `WEZTERM_UNIX_SOCKET` for a `wezterm cli` subprocess on macOS
- **THEN** it SHALL enumerate running `wezterm-gui` processes, pick the one with the most recent start time, and construct `$HOME/.local/share/wezterm/gui-sock-<pid>`

#### Scenario: Console window suppression on Windows

- **WHEN** the adapter spawns any `wezterm cli` subprocess on Windows
- **THEN** the subprocess SHALL inherit `CREATE_NO_WINDOW` (`0x0800_0000`) creation flags so no console window flashes during the call

### Requirement: Click to resume stale entry

The system SHALL resume a stale session in a new terminal tab by invoking `claude --resume <session_id>` via the user's default adapter. For entries that originated inside WSL, the resume path SHALL launch Claude inside the originating WSL distro through a login shell so that PATH adjustments from rcfiles, asdf/mise/nvm shims, and `~/.local/bin`-style installs are in scope; it SHALL set the new tab's working directory to the corresponding `\\wsl$\<distro>\<linux-cwd>` UNC path so the tab opens at the right project.

Every `wezterm cli spawn` subprocess SHALL inherit `WEZTERM_UNIX_SOCKET=<socket-path>` and (on Windows) `CREATE_NO_WINDOW`, on the same terms as the focus path.

#### Scenario: Stale WezTerm entry resumed (non-WSL)

- **WHEN** the user clicks a stale entry with `wsl_distro = None` and the default adapter is WezTerm
- **THEN** the adapter SHALL run `wezterm cli spawn --cwd <original_cwd> -- claude --resume <session_id>`
- **AND** the subprocess SHALL inherit `WEZTERM_UNIX_SOCKET=<socket-path>`

#### Scenario: Stale iTerm2 entry resumed (non-WSL)

- **WHEN** the user clicks a stale entry on macOS with `wsl_distro = None` and the default adapter is iTerm2
- **THEN** the adapter SHALL invoke `osascript` to run `tell application "iTerm2" to tell current window to create tab with default profile command "cd <cwd> && claude --resume <session_id>"`

#### Scenario: Stale entry click on a WSL-origin session

- **WHEN** the user clicks a stale entry with `wsl_distro = Some("Ubuntu-24.04")` and `cwd = "/home/user/project"`
- **THEN** the WezTerm adapter SHALL spawn a new tab with working directory `\\wsl$\Ubuntu-24.04\home\user\project`
- **AND** the tab SHALL run `wsl.exe -d Ubuntu-24.04 -- bash -lc 'claude --resume <session_id>'` so the resumed Claude session lives inside the originating distro under a login shell that resolves PATH from rcfiles
- **AND** the `wezterm cli spawn` subprocess SHALL inherit `WEZTERM_UNIX_SOCKET=<socket-path>`

#### Scenario: Resume failure when claude is missing on the WSL non-login PATH

- **WHEN** a WSL distro lacks `claude` on the non-login PATH but has it accessible from a login shell (typical for `~/.npm-global/bin`, `~/.local/bin`, asdf/mise/nvm, and `/mnt/c/...` cross-mounts)
- **THEN** the resume command `wsl.exe -d <distro> -- bash -lc 'claude --resume <id>'` SHALL succeed because `bash -lc` reads `~/.profile` / `~/.bashrc` / `/etc/profile.d/*` before launching `claude`

### Requirement: Hook-driven entry capture

The system SHALL capture every `permission_prompt` and `idle_prompt` notification fired by Claude Code as a new entry on the pending board, keyed by `session_id`. When running inside WSL, the hook SHALL write the entry to the Windows-side board file (so a single tray app on the Windows host sees entries from every WSL distro) provided the Windows USERPROFILE has been crossed into WSL via WSLENV; otherwise it falls back to the Linux-side `$HOME/.claude/pending` (which the user may have manually symlinked to the Windows side).

#### Scenario: Permission prompt becomes a pending entry

- **WHEN** Claude Code fires a `Notification` hook event with `notification_type = "permission_prompt"` and a non-empty `session_id`
- **THEN** the installed hook script SHALL append a JSON line of shape `{"op":"add","ts":<iso>,"session_id":<id>,"cwd":<path>,"claude_pid":<int>,"terminal_pid":<int|null>,"transcript_path":<path>,"notification_type":"permission_prompt","message":<string>}` to the resolved board file (see *Hook board path resolution* below)
- **AND** the Tauri app's `BoardWatcher` SHALL observe the file change and insert the entry into the in-memory `StateStore`

#### Scenario: Idle prompt becomes a pending entry

- **WHEN** Claude Code fires a `Notification` hook event with `notification_type = "idle_prompt"`
- **THEN** the hook SHALL write an equivalent `add` op with `notification_type = "idle_prompt"` to the resolved board file

#### Scenario: Hook write failure does not block Claude Code

- **WHEN** the hook script encounters any error while preparing or writing the board line (missing directory, disk full, permission denied, malformed stdin JSON, internal script bug)
- **THEN** the script SHALL log the failure to `~/.claude/pending/logs/hook-errors.log` and exit with status 0
- **AND** Claude Code SHALL NOT be blocked or interrupted in any way

### Requirement: Automatic WSLENV configuration on Windows

The tray app SHALL ensure both `WEZTERM_PANE/u` and `USERPROFILE/up` are included in the user's persistent `WSLENV` environment variable when WSL is detected on the host. `WEZTERM_PANE/u` is what makes click-to-focus address the right pane across the WSL boundary; `USERPROFILE/up` is what makes the bash hook resolve the Windows-side board file without a manual symlink. The check SHALL run in the background on every app launch, SHALL be idempotent on subsequent runs, and SHALL preserve any pre-existing `WSLENV` tokens at either the machine or user scope so unrelated software (e.g. a JDK installer setting `JRE_HOME/p`) keeps working.

When the registry value is rewritten on a launch where any `wezterm-gui.exe` is already running, the app SHALL surface the same one-shot "restart WezTerm" warning as today. The warning fires at most once per app launch regardless of how many tokens were appended in the same write.

#### Scenario: User WSLENV empty, machine WSLENV has tokens

- **WHEN** the tray app starts on Windows
- **AND** WSL is detected (i.e. `wsl.exe -l -q` exits 0 with non-empty output)
- **AND** `HKCU\Environment\WSLENV` is unset or empty
- **AND** `HKLM\…\Session Manager\Environment\WSLENV` contains existing tokens (e.g. `JRE_HOME/p`)
- **THEN** the app SHALL write to `HKCU\Environment\WSLENV` a value built by appending `WEZTERM_PANE/u` and `USERPROFILE/up` (both, in order) to a copy of the machine-scope tokens, separated by `:` (e.g. `JRE_HOME/p:WEZTERM_PANE/u:USERPROFILE/up`)
- **AND** broadcast `WM_SETTINGCHANGE` so subsequently-spawned shells inherit the new value

#### Scenario: User WSLENV missing one or both tokens but already has other entries

- **WHEN** the tray app starts on Windows, WSL is detected
- **AND** `HKCU\Environment\WSLENV` is non-empty but does not include both `WEZTERM_PANE/u` and `USERPROFILE/up`
- **THEN** the app SHALL append exactly the missing tokens to the existing user value (preserving any existing tokens, separated by `:`)
- **AND** broadcast `WM_SETTINGCHANGE`

#### Scenario: User WSLENV both empty and machine WSLENV empty

- **WHEN** the tray app starts on Windows, WSL is detected
- **AND** both `HKCU\Environment\WSLENV` and `HKLM\…\Environment\WSLENV` are unset or empty
- **THEN** the app SHALL set `HKCU\Environment\WSLENV` to exactly `WEZTERM_PANE/u:USERPROFILE/up`

#### Scenario: User WSLENV already configured

- **WHEN** the tray app starts on Windows, WSL is detected
- **AND** `HKCU\Environment\WSLENV` already includes both `WEZTERM_PANE/u` and `USERPROFILE/up`
- **THEN** the app SHALL make no changes and log at DEBUG level

#### Scenario: WSLENV updated while WezTerm is already running

- **WHEN** the tray app starts on Windows, WSL is detected
- **AND** `HKCU\Environment\WSLENV` did not include one or both required tokens and the app rewrote it
- **AND** at least one `wezterm-gui.exe` process is running at the time of the write
- **THEN** the app SHALL surface a one-shot warning to the HUD telling the user to restart WezTerm so its child shells pick up the new `WSLENV`
- **AND** the HUD SHALL be shown if hidden, so the warning is visible without the user clicking the tray
- **AND** the app SHALL record the PIDs of every running `wezterm-gui.exe` at the moment of the write
- **AND** the warning SHALL auto-dismiss when every recorded PID has exited (the user has restarted WezTerm), without requiring user interaction
- **AND** the warning SHALL also be dismissible by the user via a banner button, for the corner case where WezTerm was relaunched from a parent process with stale env (e.g. PowerToys Command Palette) so the recorded PIDs exit but new wezterm-gui inherits the same stale env
- **AND** the warning SHALL NOT persist across app restarts (it represents the boot-time stale-env condition only)

#### Scenario: WSL not detected

- **WHEN** the tray app starts and `wsl.exe` is missing from `PATH`, or `wsl.exe -l -q` exits non-zero, or returns no distros
- **THEN** the app SHALL skip the WSLENV check entirely (no env-var write, no log noise above DEBUG)

#### Scenario: Non-Windows platforms

- **WHEN** the tray app starts on macOS
- **THEN** the WSLENV configuration logic SHALL be compiled out entirely (no runtime cost)

## ADDED Requirements

### Requirement: Hook board path resolution

The bash hook SHALL resolve its board directory based on whether it is running inside WSL with the Windows USERPROFILE crossed in. This eliminates the need for a manual `~/.claude/pending → /mnt/c/Users/<winuser>/.claude/pending` symlink that was previously required to surface multi-distro WSL entries in the Windows-side HUD.

#### Scenario: WSL bash hook with USERPROFILE crossed in

- **WHEN** the bash hook script (`pending_hook.sh`) runs
- **AND** the environment variable `WSL_DISTRO_NAME` is non-empty
- **AND** the environment variable `USERPROFILE` is non-empty
- **AND** the path named by `$USERPROFILE` is an existing directory
- **THEN** the hook SHALL set `BOARD_DIR="$USERPROFILE/.claude/pending"` and write all ops to `$BOARD_DIR/board.jsonl`

#### Scenario: WSL bash hook without USERPROFILE crossed in

- **WHEN** the bash hook script runs
- **AND** `WSL_DISTRO_NAME` is non-empty but `USERPROFILE` is unset, empty, or does not resolve to an existing directory
- **THEN** the hook SHALL fall back to `BOARD_DIR="$HOME/.claude/pending"` and write all ops to `$BOARD_DIR/board.jsonl`
- **AND** SHALL log nothing about the fallback (it is the original behaviour and may be the user's deliberate symlink-based setup)

#### Scenario: macOS bash hook

- **WHEN** the bash hook script runs on macOS
- **AND** `WSL_DISTRO_NAME` is unset or empty
- **THEN** the hook SHALL set `BOARD_DIR="$HOME/.claude/pending"` regardless of any value of `USERPROFILE`
- **AND** SHALL write all ops to `$BOARD_DIR/board.jsonl`

#### Scenario: PowerShell hook on Windows

- **WHEN** the PowerShell hook script (`pending_hook.ps1`) runs on Windows
- **THEN** it SHALL continue to use `$env:USERPROFILE\.claude\pending` exactly as today (no change)
