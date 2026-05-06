## Why

Click-to-focus is broken on Windows for any user who runs the tray app outside a WezTerm-launched shell — `wezterm cli activate-pane` (and the rest of the CLI) needs `WEZTERM_UNIX_SOCKET` set to an absolute path; with the var unset, `wezterm cli` resolves the socket as a relative path and the connection fails. The tray app launches from the OS startup folder / tray with an empty `WEZTERM_UNIX_SOCKET`, so every focus and spawn call has been silently failing in production. Reproduced live on a current WezTerm `20240203-110809-5046fc22` build: `wezterm cli list` exits 0 with the env var set, exits 1 with the same error from `app.log` (`failed to connect to Socket("gui-sock-<pid>")`) when unset.

Three smaller WSL-specific defects are folded into the same release because they were the original report and they share the same `wezterm.rs` / hook-script surface area:

1. **Multi-distro WSL users never see entries from non-default distros**. The bash hook writes to `$HOME/.claude/pending/board.jsonl`, but the tray app reads only the Windows-side `~/.claude/pending/board.jsonl`. Today's only bridge is a manual `~/.claude/pending → /mnt/c/.../.claude/pending` symlink that's not documented anywhere user-facing — every WSL distro needs its own symlink and most users have only set one up for the default distro (or none).
2. **`spawn_resume` fails for any WSL user whose `claude` binary isn't on the non-login PATH**. The current `wsl.exe -d <distro> -e claude --resume <id>` skips the login shell, so users with `claude` in `~/.local/bin`, `~/.npm-global/bin`, asdf/mise, or a `/mnt/c/...` cross-mount get `execvpe(claude) failed: No such file or directory` and the new tab dies on launch — visually a "Terminal popped up and disappeared" experience.
3. **`WSLENV` auto-setup only carries `WEZTERM_PANE/u`** — adding `USERPROFILE/up` lets the bash hook resolve the Windows-side board path without shelling out to `cmd.exe` per fire, which is the cleanest way to solve (1).

## What Changes

- **Click to focus live terminal pane** — adapter sets `WEZTERM_UNIX_SOCKET` to an absolute path (`<USERPROFILE>\.local\share\wezterm\gui-sock-<pid>` on Windows, equivalent on macOS) on every `wezterm cli` invocation. Adds `CREATE_NO_WINDOW` on Windows so failed CLI calls no longer flash a console window.
- **Click to resume stale entry** — for WSL-origin entries, the resume command becomes `wsl.exe -d <distro> -- bash -lc 'claude --resume <id>'` instead of `wsl.exe -d <distro> -e claude --resume <id>`. Login shell brings rcfile-managed PATH adjustments back into scope.
- **Bash hook routes to Windows board when WSL is detected** — when `$WSL_DISTRO_NAME` and `$USERPROFILE` are both set and `$USERPROFILE` resolves to a writable directory, the hook writes to `$USERPROFILE/.claude/pending/board.jsonl` instead of `$HOME/.claude/pending/board.jsonl`. Eliminates the manual-symlink requirement for multi-distro use.
- **Automatic WSLENV configuration on Windows** — token list extended from `[WEZTERM_PANE/u]` to `[WEZTERM_PANE/u, USERPROFILE/up]`. `/up` is the WSLENV translation flag that converts `C:\Users\X` to `/mnt/c/Users/X` when crossing into WSL, so `$USERPROFILE` in the hook resolves to a Linux path the script can `cd`/`mkdir`/write into directly.

## Capabilities

### New Capabilities

(none — every change modifies existing pending-board behavior)

### Modified Capabilities

- `pending-board`: focus/resume routing for WezTerm now scopes the mux socket explicitly; resume across the WSL boundary uses a login shell; bash hook board path is WSL-aware; WSLENV auto-setup carries an additional token.

## Impact

- **Plugin** — `plugin/hooks/pending_hook.sh` gains the WSL-aware path resolution. Plugin manifest version bumps so `claude plugin update` delivers the new hook.
- **Adapter** — `crates/adapters/src/wezterm.rs` learns to compute the wezterm-gui mux socket path and propagates it via env on every `Command::new(wezterm)` call. `spawn_resume` switches to `bash -lc` for WSL.
- **App** — `crates/app/src/wsl_env_setup.rs` becomes multi-token. Existing tests for `merge_wslenv` extend to cover the `USERPROFILE/up` case; an integration test exercises the `wezterm cli list` happy path with the env-set adapter helper.
- **Cargo / version bump** — `Cargo.toml`, `crates/app/tauri.conf.json`, and `plugin/.claude-plugin/plugin.json` all jump to `0.3.0`. `crates/core/tests/plugin_version_sync.rs` enforces the cross-file match.
- **Docs** — `INSTALL.md` step 2.5 drops any (never-published) symlink instructions and tells multi-distro users to install the plugin in each distro. `CLAUDE.md` gotchas section gains an entry on the `WEZTERM_UNIX_SOCKET` requirement so the regression doesn't reappear.
- **Backward compat** — existing PoC symlinks remain harmless (`$USERPROFILE/.claude/pending` and `$HOME/.claude/pending` resolve to the same Windows file via the symlink). Plugin-only updates still fall back to `$HOME` if the tray app hasn't shipped the WSLENV change yet.
