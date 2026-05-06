## 1. Adapter — `WEZTERM_UNIX_SOCKET` regression fix

- [ ] 1.1 Add a private helper `fn wezterm_socket_path() -> Option<PathBuf>` in `crates/adapters/src/wezterm.rs` that finds running `wezterm-gui` PIDs via `sysinfo`, picks the most-recently-started one, and constructs the OS-appropriate path (`<USERPROFILE>\.local\share\wezterm\gui-sock-<pid>` on Windows, `$HOME/.local/share/wezterm/gui-sock-<pid>` on macOS).
- [ ] 1.2 Add a private helper `fn wezterm_command(binary: &str) -> Command` that wraps `Command::new(binary)`, calls `.env("WEZTERM_UNIX_SOCKET", path)` when the helper returns `Some`, and on Windows adds `.creation_flags(CREATE_NO_WINDOW)`.
- [ ] 1.3 Replace every `Command::new(&binary)` in `list_panes`, `activate_pane`, and `spawn_resume` with `wezterm_command(&binary)`.
- [ ] 1.4 Unit test for socket path construction: feed a mock PID and a fake `USERPROFILE`/`HOME`, assert the produced string matches the platform-correct format.
- [ ] 1.5 Unit test for the "no wezterm-gui running" branch: when the helper returns `None`, the wrapping `Command` SHALL NOT have `WEZTERM_UNIX_SOCKET` in its env (verified via `Command::get_envs`).

## 2. Adapter — `bash -lc` for WSL resume

- [ ] 2.1 In `WezTermAdapter::spawn_resume` (`crates/adapters/src/wezterm.rs`), change the WSL branch's argv from `"-d", distro, "-e", "claude", "--resume", session_id` to `"-d", distro, "--", "bash", "-lc", &format!("claude --resume {}", session_id)`.
- [ ] 2.2 Add a `debug_assert!` in the WSL branch that `session_id` matches the UUID shape (`/^[0-9a-f-]{36}$/`); helper can be `is_uuid_like(s: &str) -> bool` next to the function.
- [ ] 2.3 Update existing unit tests in the same file that assert the resume command shape (search for `--resume`); add a new test exercising the WSL branch's argv specifically.

## 3. WSLENV setup — multi-token

- [ ] 3.1 In `crates/app/src/wsl_env_setup.rs`, replace `const TOKEN: &str = "WEZTERM_PANE/u";` with `const TOKENS: &[&str] = &["WEZTERM_PANE/u", "USERPROFILE/up"];`.
- [ ] 3.2 Refactor `merge_wslenv(user, machine, token)` → `merge_wslenv(user, machine, tokens: &[&str])` returning `Option<String>`. Iterate tokens, append any that are missing from the seed, dedupe trailing colons. Keep the function pure (no I/O).
- [ ] 3.3 Update `ensure_wezterm_pane_in_wslenv` (consider renaming to `ensure_wsl_env_tokens`) to call the multi-token version. One registry write per launch covers all missing tokens. The "Updated"/"Unchanged"/"NoOp" `Status` enum remains unchanged.
- [ ] 3.4 Update tests in the bottom of `wsl_env_setup.rs`:
    - `merge_into_empty_when_neither_set` — both tokens appended in order.
    - `merge_when_user_already_has_both_tokens_is_noop`.
    - `merge_appends_only_missing_tokens` — user has `WEZTERM_PANE/u`, machine empty, result appends only `USERPROFILE/up`.
    - `merge_seeds_from_machine_when_user_unset` — machine `JRE_HOME/p`, result `JRE_HOME/p:WEZTERM_PANE/u:USERPROFILE/up`.
    - All existing tests get updated signatures.
- [ ] 3.5 Search call sites: any code that imports `TOKEN` directly (none expected outside the module) compiles cleanly after the rename.

## 4. Plugin — bash hook routes to USERPROFILE under WSL

- [ ] 4.1 In `plugin/hooks/pending_hook.sh`, replace the fixed `BOARD_DIR="$HOME/.claude/pending"` initialisation with the WSL-aware resolution from the spec (`$USERPROFILE/.claude/pending` when `$WSL_DISTRO_NAME` is set, `$USERPROFILE` is non-empty, and `[ -d "$USERPROFILE" ]`; fallback to `$HOME/.claude/pending` otherwise).
- [ ] 4.2 Verify on this machine: from inside Lobaemon (where `USERPROFILE` is now `/mnt/c/Users/RDSimon` once WSLENV ships), the hook writes to the Windows-side `board.jsonl`. Pipe a sample `Notification` payload into the script and tail the Windows file.
- [ ] 4.3 Verify fallback: temporarily `unset USERPROFILE` and re-run the hook payload; confirm it writes to `$HOME/.claude/pending/board.jsonl` instead.

## 5. Hook regression test

- [ ] 5.1 Update `crates/core/tests/no_hook_duplication.rs` if its assertions touch the bash hook content (likely just file-existence; check).
- [ ] 5.2 Add a smoke step to `scripts/smoke-test.sh` exercising the WSL-aware path (set `WSL_DISTRO_NAME=test` and `USERPROFILE=/tmp/fake-userprofile`, assert the hook writes to `/tmp/fake-userprofile/.claude/pending/board.jsonl`).

## 6. Version bumps

- [ ] 6.1 Bump `[workspace.package].version` in `Cargo.toml` from `0.2.8` to `0.3.0`.
- [ ] 6.2 Bump `version` in `plugin/.claude-plugin/plugin.json` from `0.2.8` (or current auto-bumped value) to `0.3.0`.
- [ ] 6.3 Bump `version` in `crates/app/tauri.conf.json` to `0.3.0`.
- [ ] 6.4 Run `cargo test -p claude-pending-board-core --test plugin_version_sync` to confirm the cross-file equality check passes.

## 7. Docs

- [ ] 7.1 In `INSTALL.md` §2.5, drop any (never-published) symlink instructions and explicitly state that multi-distro users install the plugin in each WSL distro — no per-distro setup beyond `claude plugin install`.
- [ ] 7.2 In `CLAUDE.md` "Known gotchas" section, add an entry on the `WEZTERM_UNIX_SOCKET` requirement so the regression doesn't reappear (e.g. "WezTerm CLI needs `WEZTERM_UNIX_SOCKET` in env to find the mux socket — adapter sets it explicitly via the `wezterm_command` helper. Direct `Command::new("wezterm")` calls regress click-to-focus.").
- [ ] 7.3 Update `docs/release-checklist.md` with two new manual scenarios: "click-to-focus on PowerShell tab works after fresh install" and "multi-distro WSL: install plugin in two distros, verify both surface entries and focus correctly".

## 8. Verification

- [ ] 8.1 `cargo fmt --check --all`.
- [ ] 8.2 `cargo clippy --workspace -- -D warnings`.
- [ ] 8.3 `cargo test --workspace`.
- [ ] 8.4 `pwsh scripts/smoke-test-auto.ps1` (the non-interactive run).
- [ ] 8.5 `openspec validate fix-wsl-multi-distro-and-cli-socket --strict` exits 0.
- [ ] 8.6 Manual: stop wezterm, start tray app, start wezterm, start a Claude session in PowerShell, trigger a permission prompt, click the entry. Pane focus succeeds. No console flash.
- [ ] 8.7 Manual (multi-distro): install plugin in both `Ubuntu-24.04` and `Lobaemon`. Trigger a notification in each. Both entries appear in HUD with correct `wsl_distro` tag. Both clicks focus the right pane.
- [ ] 8.8 Manual (resume fallback): close the WezTerm tab that owns a Lobaemon entry, click the entry. Resume opens a new tab via `wsl.exe -d Lobaemon -- bash -lc 'claude --resume <id>'`, claude finds itself, session continues.
