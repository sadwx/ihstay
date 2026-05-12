# Tasks — WSL support

Implementation is split across three PRs. PR-A and PR-C are the critical path; PR-B is a manifest tweak that can land any time after PR-A. PR-B grew at implementation time to also bundle a `WEZTERM_PANE`-based click-to-focus path so WSL clicks (and Windows multi-pane clicks) land on the existing tab instead of always spawning a new one — that's still the same v0.2 scope, just one more piece in the same PR.

## PR-A · Reaper short-circuits WSL entries (shipped — `9821d38`)

- [x] Add `pub wsl_distro: Option<String>` to `Entry` in `crates/core/src/types.rs`.
- [x] Add the field to the `Op::Add` variant with `#[serde(skip_serializing_if = "Option::is_none")]` so absent → omitted in JSON.
- [x] Thread the field through `StateStore::apply` in `crates/core/src/board/store.rs` and through the parser in `crates/core/src/board/parser.rs`.
- [x] In `crates/core/src/reaper.rs::check_liveness`, return `LivenessResult::Alive` immediately when `entry.wsl_distro.is_some()`.
- [x] Update `crates/core/src/board/compaction.rs` to round-trip the field on rewrite.
- [x] Tests: unit test in `reaper.rs` that an entry with `wsl_distro = Some("Ubuntu-24.04")` and a dead `claude_pid` resolves to `Alive`. Round-trip parser test with the new field present and absent.

## PR-C · WezTerm adapter — distro-aware spawn + resume (shipped — `94dcf83`)

- [x] Update `scripts/pending_hook.sh` and `plugin/hooks/pending_hook.sh` to emit `"wsl_distro": "<name>"` when `$WSL_DISTRO_NAME` is set; absent otherwise.
- [x] In `crates/adapters/src/wezterm.rs::spawn_resume`, branch on whether the entry's `cwd` came with a `wsl_distro`.
- [x] Add a helper `wsl_cwd_to_unc(distro: &str, linux_cwd: &Path) -> String` that produces `\\wsl$\<distro>\<rest>`. Pure string transform, no I/O.
- [x] Build the resume command as `wsl.exe -d <distro> -e claude --resume <session_id>` and pass `--cwd <unc>` to `wezterm cli spawn`.
- [x] Threading note: `Adapter::spawn_resume` currently takes `(cwd, session_id)`. Widen to `(cwd, session_id, wsl_distro: Option<&str>)` to keep the diff small. (Passing the whole `Entry` is a future option but premature for this PR.)
- [x] Tests: `wsl_cwd_to_unc("Ubuntu-24.04", Path::new("/home/user/project")) → "\\\\wsl$\\Ubuntu-24.04\\home\\user\\project"` etc.
- [x] Manual smoke on the maintainer's WSL: trigger a permission prompt in WSL Claude, verify HUD entry stays live, click → new WezTerm tab opens with WSL cwd and resumes the session.

## PR-B · Plugin manifest + WEZTERM_PANE click-to-focus

Manifest piece (the original scope):

- [x] In `plugin/.claude-plugin/plugin.json`, mirror each existing `platform: "darwin"` bash entry with a `platform: "linux"` entry pointing at the same script.
- [x] In `scripts/pending_hook.sh` (the source of truth), confirm no Linux-specific assumptions broke during the `af26a1e` revert window — the `ps -o ppid=` branch is fine for both macOS and Linux.
- [x] Update `INSTALL.md` to mention WSL works (with caveats — entry takes 2–5 s to appear; click-to-focus needs WezTerm on the Windows side).
- [x] Bump `Cargo.toml`, `tauri.conf.json`, `marketplace.json`, and `plugin.json` versions to `0.2.0`.

WEZTERM_PANE click-to-focus (bundled in):

- [x] Add `wezterm_pane_id: Option<String>` to `Entry` and `Op::Add` (same serde shape as `wsl_distro`).
- [x] In `scripts/pending_hook.sh` and `pending_hook.ps1` (and plugin copies), capture `$WEZTERM_PANE` / `$env:WEZTERM_PANE` and emit it on the Add op.
- [x] In `crates/app/src/commands.rs::focus_entry`, when `entry.wezterm_pane_id` is `Some` build a `TerminalMatch` directly and call WezTerm's `focus_pane` — skip the ancestor walk entirely. Falls back to `spawn_resume` if `focus_pane` errors (the captured pane may have been closed).
- [x] Tests: parser round-trip with and without `wezterm_pane_id`.
- [x] Document the `WSLENV=WEZTERM_PANE/u` one-time setup in `INSTALL.md` for WSL users (without it, WSL hooks fire fine but the env var doesn't cross the boundary, so click falls back to spawn_resume).

## PR-D · Automatic WSLENV configuration

- [ ] New module `crates/app/src/wsl_env_setup.rs`, gated `#[cfg(target_os = "windows")]`. Public entry point `ensure_wezterm_pane_in_wslenv()`.
- [ ] Detection: shell out `wsl.exe -l -q`. Treat exit-0-with-output as "WSL configured", anything else (missing binary, non-zero exit, empty output) as "not configured".
- [ ] Read current `WSLENV` from `HKCU\Environment` via the `windows` crate's registry APIs. Treat absent / empty as "no current value".
- [ ] Pure-string helper `merge_wslenv(current: &str, token: &str) -> Option<String>` — returns `Some(new)` if the token is missing and needs appending, `None` if it's already present. Tokens are colon-separated; comparison is case-sensitive on the var name.
- [ ] Write the merged value back to the same registry key and broadcast `WM_SETTINGCHANGE` with `lParam = "Environment"` so explorer.exe propagates the change to subsequently-spawned shells.
- [ ] Wire into `tauri::Builder::setup()` as a `spawn_blocking` task — must not block app boot. Errors are logged at WARN, not surfaced to the user.
- [ ] Unit tests for `merge_wslenv`: empty input, input already containing the token (idempotent), input with other tokens, edge cases (leading/trailing colons).
- [ ] Update `INSTALL.md` Step 2.5: replace the manual `setx WSLENV …` instruction with "the tray app handles this for you on first launch — open a fresh WezTerm tab afterward to pick up the change."

## Validation gate (before tagging v0.2)

- [ ] PR-A merged: WSL PoC entries on the test machine no longer go stale within seconds. Wait at least one reaper sweep (30 s) after firing a WSL hook and confirm the entry is still `Live`.
- [ ] PR-C merged: clicking a WSL entry opens a WezTerm tab inside the right distro and runs `claude --resume`; the prompt actually returns to the user.
- [ ] PR-B merged: in WSL, `claude plugin install ihstay@ihstay` registers all three hooks without manual settings.json editing. The maintainer reverts the PoC settings.json (backup is at `~/.claude/settings.json.pre-pending-board-poc`).
- [ ] Documentation: `INSTALL.md` has a "WSL" section with the caveats listed above.

## Deferred to a later change

- WSL liveness via `wsl.exe -d <distro> -e ps -p <pid>` instead of skipping the check entirely. Worth doing once we have a way to amortize the per-call cost (sweep batching, or a long-lived `wsl.exe -e bash` shell maintained by the adapter).
- Native Linux desktop support. Independent decision; not implied by this change.
