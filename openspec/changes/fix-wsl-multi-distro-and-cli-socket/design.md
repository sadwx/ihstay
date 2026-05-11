## Context

Four interlocking defects in the click-to-focus path surfaced together while debugging a multi-WSL-distro setup (a default `Ubuntu-24.04` plus a non-default `Lobaemon`). The most damaging one — the `WEZTERM_UNIX_SOCKET` regression — affects every Windows user, not just multi-distro WSL users; it has been masked during development because `cargo tauri dev` inherits the env from a WezTerm-parented shell, but goes loud once the binary is launched from the Windows startup folder. The other three were the original report scope (multi-distro WSL).

Live evidence captured before this change:

- `app.log`: every click in the last session emits `WARN focus_pane failed, falling back to spawn_resume error=… failed to connect to Socket("gui-sock-51348"): connecting to gui-sock-51348; terminating`. PID 51348 matches the running `wezterm-gui.exe`. The socket file at `<USERPROFILE>\.local\share\wezterm\gui-sock-51348` exists. Connection still fails when the env var is unset; succeeds when set to the absolute path.
- `Get-ChildItem` shows `Lobaemon`'s `~/.claude/pending` is a real directory (no symlink), so its hook output stays inside the WSL fs and never reaches the Windows `board.jsonl` the tray watches.
- `wsl -d Lobaemon -e claude --version` → `execvpe(claude) failed: No such file or directory`. `wsl -d Lobaemon -- bash -lc 'claude --version'` → `2.1.126 (Claude Code)`.

Reverse-engineering note: WezTerm's CLI ([cli.rs in wezterm 20240203](https://wezfurlong.org/wezterm/) — exact build under test) computes the per-mux Unix-domain socket name as `gui-sock-<pid>` but treats the path as relative when `WEZTERM_UNIX_SOCKET` is unset. Inside a WezTerm-spawned shell the var is set to an absolute path, so the bug never surfaces interactively.

## Goals / Non-Goals

**Goals:**

- Click-to-focus works end-to-end for entries from any source (Windows native shell, WSL default distro, WSL non-default distro), without manual symlinks or per-distro environment tweaks.
- Resume falls back gracefully when the captured pane has been closed, including for WSL users whose `claude` lives outside the default non-login PATH.
- The cli-socket fix is robust against multiple wezterm-gui instances and against macOS, where the same socket-path scheme applies.
- WSLENV auto-setup remains idempotent and preserves existing tokens, exactly the way it does today for `WEZTERM_PANE/u`.

**Non-Goals:**

- **Native Linux desktop support** — stays out of scope, same boundary as `add-wsl-support`.
- **Cross-process WSL liveness checks** for the reaper — the WSL change skips them; this change does not revisit that.
- **Removing the legacy `~/.claude/pending` symlink workaround** — keeps working, just becomes unnecessary. Existing PoC users don't have to migrate.
- **A "wezterm not running" handler** — if the user has no wezterm-gui at all, click-to-focus has nothing to address; current spawn behaviour (which can launch a new wezterm-gui via `wezterm cli spawn` if the binary itself is on PATH) is unchanged.

## Decisions

### Decision 1 — Resolve `WEZTERM_UNIX_SOCKET` from the running `wezterm-gui` PID, not from the entry's `terminal_pid`

**What:** Add a helper `fn wezterm_socket_path() -> Option<PathBuf>` that picks a live `wezterm-gui` PID by enumerating processes (sysinfo, already a dep) and constructs `<USERPROFILE>\.local\share\wezterm\gui-sock-<pid>` on Windows / `$HOME/.local/share/wezterm/gui-sock-<pid>` on macOS. The adapter sets the env var on every `Command::new(wezterm) cli ...` invocation.

**Why over alternatives:**

- *Use the entry's `terminal_pid`.* Sometimes null (legacy entries, WSL-origin entries). Even when present, it points at the wezterm-gui that owned the pane *at hook time* — not necessarily the same gui process running now. PID-based discovery against the live process table is more reliable.
- *Capture `WEZTERM_UNIX_SOCKET` once at app startup.* Doesn't help — if the app started before wezterm, or wezterm restarts, the captured value is stale.
- *Wait for an upstream wezterm fix.* The bug has shipped for at least a year across multiple versions; no upstream fix is visible in the WezTerm changelog as of this design's writing. We're going to need a defensive setter regardless.

**Multiple wezterm-gui instances.** Pick the one with the most recent start time. Justification: the typical user has one gui; on the rare multi-instance setup (different `--class`, different workspace) the most-recently-started one is closest to "current foreground intent" and matches the heuristic the existing `raise_window_windows` uses for `SetForegroundWindow`. Edge case where the entry was hooked from instance A but instance B started later: focus_pane fails, falls through to spawn_resume in B, user lands on a working session. Acceptable.

**Idempotency.** The helper recomputes the socket path on every CLI call. Cheap (one process-table sweep, one path concat). No caching needed; if wezterm restarts mid-session, the next click picks up the new PID without re-launching anything.

### Decision 2 — Suppress the console window flash on `wezterm cli` calls

**What:** Add `.creation_flags(CREATE_NO_WINDOW)` (Windows-only, behind `#[cfg(target_os = "windows")]`) on every `Command::new(wezterm) cli` invocation in the adapter.

**Why:** The tray app is a windowed Tauri process with no parent console. `Command::new` on a console-subsystem program (wezterm.exe is one for cli mode) auto-allocates a console; the brief flash was visible to the user and likely contributed to the "Terminal popped up and disappeared" observation. Same flag is already used in `wsl_env_setup.rs` for `wsl.exe` and `powershell.exe`. No reason for `wezterm cli` to be different.

### Decision 3 — Use `bash -lc` for the WSL resume command, not `--shell-type login`

**What:** `spawn_resume` for WSL becomes `wsl.exe -d <distro> -- bash -lc 'claude --resume <id>'` (was `wsl.exe -d <distro> -e claude --resume <id>`).

**Why over `--shell-type login`:** `--shell-type` was added in WSL 1.0.0 (the Microsoft Store distribution) but isn't present in the in-box `wsl.exe` shipped with older Windows builds. `bash -lc` works on every WSL build still in support, including the in-box one. The cost is one extra process spawn (login bash → claude), which is invisible on click-latency timescales.

**Why over `bash -c '. ~/.profile && claude --resume <id>'`:** Hand-sourcing `.profile` doesn't pull in `~/.bashrc`, `/etc/profile.d/*`, or asdf/mise/nvm shims that depend on a real login shell. `-l` is the canonical way to trigger the full login path.

**Session id escaping:** Session ids are UUIDs (`[0-9a-f-]{36}` per the existing `Op::Add` schema in `crates/core/src/types.rs`). No metacharacters, so straight `format!("claude --resume {}", session_id)` is safe. Implementation adds a `debug_assert!` that the id matches the UUID shape so a malformed entry can't silently exfiltrate.

### Decision 4 — Hook routes via `$USERPROFILE` (path-translated by WSLENV), not via `cmd.exe` shellout

**What:** Inside `pending_hook.sh`, when `$WSL_DISTRO_NAME` is set and `$USERPROFILE` is a directory, set `BOARD_DIR="$USERPROFILE/.claude/pending"`. Otherwise fall back to today's `$HOME/.claude/pending`. The `$USERPROFILE` value is supplied by the Windows side via `WSLENV=USERPROFILE/up` — `/up` is the documented Microsoft flag that means "translate the value through `wslpath -u` when crossing into WSL", so `C:\Users\X` arrives as `/mnt/c/Users/X`.

**Why over `cmd.exe /c "echo %USERPROFILE%"`:** Hooks fire frequently (every Notification / UserPromptSubmit / Stop / SessionEnd / PermissionDenied). `cmd.exe` cold-start through the WSL interop layer costs ~150-300ms per call; doing it inside every hook fire would be felt as latency. WSLENV translation is a constant-cost copy on shell startup, paid once per shell.

**Why over `wslpath -aw "$HOME"`:** That converts WSL → Windows; we need the inverse direction (Windows USERPROFILE → WSL mount path). No native command does that without already knowing the Windows username.

**Why over hard-coding `/mnt/c/Users/$USER`:** `$USER` inside WSL is the WSL user, not the Windows user. They commonly differ (this user has `simon` in Ubuntu, `lobaemon` in Lobaemon, `RDSimon` on Windows). And the Windows home isn't always on C: — `$USERPROFILE` is the only authoritative source.

### Decision 5 — Generalize `wsl_env_setup.rs` to a token list, keep one warning per write

**What:** Replace `const TOKEN: &str = "WEZTERM_PANE/u";` with `const TOKENS: &[&str] = &["WEZTERM_PANE/u", "USERPROFILE/up"];`. Iterate the existing `merge_wslenv` logic per token, fold updates into a single registry write so users see at most one `Updated` outcome per launch. The "restart WezTerm" warning still fires once if any token was added and any wezterm-gui was running at write time.

**Why not split into two separate write passes:** A single registry write means one `WM_SETTINGCHANGE` broadcast and one warning event, which matches today's UX. Two passes would multi-fire the warning when neither token was present (the default state for first-launch users).

## Risks / Trade-offs

| Risk | Mitigation |
|---|---|
| Multiple `wezterm-gui` instances with different mux sockets — focus_pane lands in the wrong one. | Pick most-recent start time (heuristic). On miss, focus_pane fails; existing fall-through to spawn_resume opens a working tab. The user notices a new tab instead of focus, but doesn't lose the session. |
| `bash -lc` doesn't exist (truly minimal Linux distro). | All known WSL distros ship `bash` by default. Can't bootstrap a Claude Code install without bash anyway. Documented as a prerequisite. |
| `$USERPROFILE` translates to `/mnt/c/Users/<name>` but the user has multiple Windows accounts and the WSL session was launched from a *different* Windows account than the one that runs the tray app. | The same multi-account hazard already exists for the manual symlink. The hook gracefully falls back to `$HOME` when `$USERPROFILE` is empty (which it would be if WSLENV doesn't carry it). Documented as a single-Windows-user assumption matching the rest of the project. |
| Plugin update arrives before tray-app update — hook expects `$USERPROFILE`, WSLENV doesn't have `USERPROFILE/up` yet. | Hook checks `[ -n "${USERPROFILE:-}" ]` and falls back to `$HOME`. Only worse than today's behaviour if the user lacks the manual symlink — still no regression for the symlinked case. |
| Tray-app update arrives before plugin update — WSLENV ships `USERPROFILE/up` but old hook still writes to `$HOME`. | No-op: extra WSLENV token doesn't harm the old hook; the user still needs the new plugin to pick up the WSL-aware path. |
| `Command::new(wezterm).env(...)` on macOS unexpectedly differs from current behaviour. | Same env propagation rules apply on both OSes; macOS already has `WEZTERM_UNIX_SOCKET` set in WezTerm-spawned shells via `~/.local/share/wezterm/gui-sock-<pid>`. Adding the explicit setter is strictly safer than relying on env inheritance. Manual smoke test on the maintainer's macOS machine before tagging. |

## Migration

Plugin and app version both bump to `0.3.0` together. Manual deployment on the user's other laptop:

1. Install the v0.3.0 MSI / install dev build (`cargo install --path crates/app`). On launch, the app appends `WEZTERM_PANE/u:USERPROFILE/up` to `WSLENV`. If wezterm was running, the existing one-shot "restart WezTerm" warning fires.
2. In each WSL distro: `claude plugin update ihstay` (or `claude plugin install` if not yet installed). The new `pending_hook.sh` ships with the plugin.
3. Open a fresh wezterm tab in each distro so the new `WSLENV` is captured.
4. Trigger a notification (e.g. `claude` + a permission prompt). Verify the entry tagged with the right `wsl_distro` shows up in the Windows-side HUD.
5. Click the entry. Should focus the right pane. No symlink configured anywhere.

Existing PoC symlinks at `~/.claude/pending → /mnt/c/.../pending` are harmless — both `$USERPROFILE/.claude/pending` and `$HOME/.claude/pending` resolve to the same Windows file via the symlink. Users may delete them at leisure; doing so is not required.

## Open Questions

- Should the helper cache the wezterm-gui PID across CLI calls within a single click? Probably not; the existing pattern recomputes per call (e.g. `raise_window_windows` does its own sweep). One sweep adds <2 ms to the click; cache invalidation is harder than just doing the work.
- Does WezTerm 2026 (when it ships) keep the same socket-path scheme? If they switch to TLS-by-default or a different socket layout, this helper needs an update. Worth pinning the wezterm version in `INSTALL.md` (already done by version probe in `find_binary`).
