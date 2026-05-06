# CLAUDE.md

Project-specific context for Claude Code sessions working on this repo.

## What this is

`claude-pending-board` is a cross-platform tray app that surfaces every waiting Claude Code CLI session in one floating HUD. Claude Code hooks write to `~/.claude/pending/board.jsonl`; the tray app watches that file and renders a HUD, and clicking an entry focuses the owning WezTerm (Windows) or iTerm2 (macOS) pane.

Design + spec live under `openspec/changes/add-claude-pending-board/`.

## Repo layout

```
claude-pending-board/
├── crates/
│   ├── core/        # pure Rust, no Tauri — parser, store, watcher,
│   │                # compaction, visibility FSM, reaper, config,
│   │                # terminal trait + ancestor walk
│   ├── adapters/    # WezTerm (all OSes) + iTerm2 (macOS cfg-gated)
│   │                # implementations of the terminal trait
│   └── app/         # Tauri 2 app — commands, tray, services,
│                    # HUD window, Settings window
├── scripts/         # source of truth for hook scripts
│                    # (PowerShell + Bash), plus smoke tests
├── plugin/          # Claude Code plugin — copies hook scripts into
│                    # its own hooks/ and registers them via
│                    # .claude-plugin/plugin.json
├── openspec/        # change proposals, design, spec, tasks
├── docs/
│   ├── superpowers/plans/   # phase implementation plans
│   ├── release-checklist.md # manual verification before tagging
│   └── screenshots/         # comparison with v5 design mock
└── .github/workflows/       # ci.yml (fmt+clippy+test) + release.yml
```

## Commands

```bash
# Build
cargo build -p claude-pending-board-app                # debug, Windows
cargo tauri build                                       # release, current OS

# Run
./target/debug/claude-pending-board-app.exe             # directly
cargo tauri dev                                         # with dev tools / hot reload

# Test
cargo test --workspace                                  # all 66 tests
cargo test -p claude-pending-board-core -- parser       # filter by module
cargo test -- --ignored                                 # contract tests (requires WezTerm)

# Lint / format — MUST pass before committing
cargo fmt --check --all
cargo clippy --workspace -- -D warnings

# Smoke tests
pwsh scripts/smoke-test-auto.ps1                        # ~40s non-interactive
pwsh scripts/smoke-test.ps1                             # interactive, for humans
bash scripts/smoke-test.sh                              # same, for macOS
```

## Conventions

- **TDD with unit tests inline at the bottom of each module** in a `#[cfg(test)] mod tests` block. Integration tests go in `crates/core/tests/`.
- **Catppuccin Mocha** palette for all UI (colors defined as CSS custom properties in `crates/app/ui/hud/style.css`).
- **Vanilla HTML/CSS/JS** on the frontend — no framework, no bundler. Script tags use `defer` or load at bottom; avoid `type="module"` unless actually importing.
- **No `innerHTML` with dynamic content** — the security hook blocks it. Use `textContent` or build with `createElement`, or use separate hidden elements and swap visibility.
- **Windows line endings** (CRLF) are auto-applied on checkout — don't fight the warnings.
- **Commits are conventional-ish**: `feat(scope):`, `fix(scope):`, `docs:`, `test:`, `ci:`, `chore:`. Scope is usually the crate name (`core`, `adapters`, `app`, `plugin`, `hooks`).

## Known gotchas (things that bit us)

- **`sysinfo 0.33` API**: use `ProcessRefreshKind::everything()`, not `::new()` (which no longer exists). Same for `ProcessesToUpdate::All`.
- **`wezterm cli` needs `WEZTERM_UNIX_SOCKET` set to an absolute path** when invoked from a process that wasn't itself spawned by WezTerm (i.e. anything launched from the Windows startup folder, the tray icon, or a non-WezTerm parent shell). Without it, the CLI computes the socket name as relative `gui-sock-<pid>` and the connection fails with `failed to connect to Socket("gui-sock-<pid>"): connecting to gui-sock-<pid>; terminating`. Every `Command::new(wezterm)` in `crates/adapters/src/wezterm.rs` MUST go through `wezterm_command(&binary)`, which finds a running `wezterm-gui` PID via sysinfo, builds `<USERPROFILE|HOME>\.local\share\wezterm\gui-sock-<pid>`, and sets the env. The bug is invisible during `cargo tauri dev` (env propagates from the parent WezTerm shell) and only surfaces in the installed tray app — keep this guard rail in mind on adapter changes.
- **WSLENV auto-setup carries multiple tokens.** `crates/app/src/wsl_env_setup.rs::TOKENS` is `["WEZTERM_PANE/u", "USERPROFILE/up"]`. The `/up` flag means "Unix path translation" — `C:\Users\<you>` becomes `/mnt/c/Users/<you>` inside WSL, which is what makes the bash hook's `$USERPROFILE/.claude/pending` resolve to a Windows-side path. If you add a new WSLENV token, append it to `TOKENS` and update the merge tests; users with WezTerm already running will see the existing one-shot "restart WezTerm" warning.
- **WSL resume runs through `bash -lc`, not `wsl.exe -e claude`.** The `-e` flag skips the login shell, so any claude install that adds itself to PATH only via rcfiles (`~/.local/bin`, `~/.npm-global/bin`, asdf/mise/nvm shims, /mnt/c-mounted Windows npm shims) fails with `execvpe(claude) failed: No such file or directory`. The login form sources `/etc/profile`, `~/.profile`, and friends before exec'ing claude. Session id is UUID-shaped per `Op::Add`; we `debug_assert!` `is_uuid_like(s)` before interpolating into the command line.
- **Tauri window creation from a command handler hangs WebView2 on Windows.** Pre-create all windows during `tauri::Builder::setup()` on the main thread with `visible(false)`, then show/hide them from commands. This is why `hud` and `settings` are both created in `crates/app/src/main.rs` at boot.
- **Tauri 2 capabilities must exist for `invoke`/`listen` to work.** Without `crates/app/capabilities/default.json` listing the window labels and granting `core:default`, the JS bridge silently fails and the window renders blank.
- **`frontendDist` is relative to `tauri.conf.json`** and paths can't use `..`. Current config points `frontendDist: "ui"` and windows load `hud/index.html` or `settings/index.html`.
- **Settings window: intercept close-to-hide.** Default behavior destroys the window, which breaks the pre-create strategy. The close request handler in `main.rs` calls `api.prevent_close()` + `window.hide()`.
- **Hook scripts must always `exit 0`.** Any non-zero exit blocks Claude Code. Both `pending_hook.ps1` and `pending_hook.sh` wrap their bodies in `try`/`catch` and log to `~/.claude/pending/logs/hook-errors.log` on failure.
- **Pill and countdown positioning**: the DEFAULT pill is `position: absolute; top: -10px; left: 50%; transform: translateX(-50%)` — floats above the button border, not inline after the label. The countdown is an inline `<span>` sibling to `.btn-label` so "Wake me · 5s" renders on one line.
- **HUD width must be `100%` not `380px`**. DPI scaling can make explicit pixel widths clip the dismiss X button. The window size (`inner_size(380.0, 240.0)`) comes from Tauri; CSS fills it.
- **Tray left-click needs `show_menu_on_left_click(false)`.** Tauri 2's default is to show the attached menu on any click, so the custom `on_tray_icon_event` for `MouseButton::Left` never fires. Without this, left-click just opens the menu — breaking the "left-click re-opens HUD, right-click shows menu" UX. Set it explicitly on the `TrayIconBuilder` in `crates/app/src/tray.rs`.
- **HUD drag on macOS needs both the explicit capability and a JS fallback.** `core:default` does NOT include `core:window:allow-start-dragging` — the capability must be listed explicitly in `crates/app/capabilities/default.json`. On top of that, `data-tauri-drag-region` alone was unreliable on macOS with `decorations(false) + always_on_top`; a manual `mousedown → getCurrentWindow().startDragging()` handler on `.header` in `crates/app/ui/hud/main.js` works consistently. Keep both.
- **Icon must be RGBA on macOS.** `crates/app/icons/icon.png` must have an alpha channel; plain 8-bit RGB fails at compile time with `icon ... is not RGBA` from `tauri::generate_context!`. If regenerating the icon, run `python3 -c "from PIL import Image; Image.open('icon.png').convert('RGBA').save('icon.png')"` (or equivalent) before committing.
- **`tracing` to default stderr SIGABRTs `.app` bundles on macOS.** Finder / login-item launches inherit a closed-or-broken stderr. `tracing_subscriber::fmt()` defaults to writing events to stderr; Rust's `__eprint` panics with `failed printing to stderr` on any write failure; the panic handler `abort()`s. Reproducible by clicking the tray menu → Settings (the `tracing::info!("settings window shown")` event is the first to flush). `crates/app/src/main.rs::init_tracing()` routes events to `~/.claude/pending/logs/app.log` with an `io::sink` fallback — never reintroduce the default stderr writer for any new tracing init in this app.
- **Hook commands must end with `|| exit 0` in `plugin.json`.** Claude Code 2.1.x ignores the `platform` field on hook entries, so the Windows pwsh entry runs through `/bin/sh` on macOS/Linux (and the bash entries run through `cmd.exe` on Windows). Without the `|| exit 0` suffix, a missing launcher exits 127, and CC surfaces it as `Failed with non-blocking status code: <shell>: <launcher>: command not found`. Both `||` and `exit 0` are valid in `/bin/sh` and `cmd.exe`, so the suffix is safe on every platform. Keep the suffix on every command line in `plugin/.claude-plugin/plugin.json`.
- **Manifest sanitization runs in four places.** `crates/app/src/plugin_install.rs::sanitize_installed_plugin_json()` strips foreign-platform hook entries from the cached `plugin.json` so `/hooks` only shows entries that can run on the current OS. Triggered after `install()` (one-click setup card), as a best-effort startup task in `main.rs setup()`, on every plugin-cache filesystem change via `crates/app/src/plugin_watch.rs` (debounced 1.5 s — covers `claude plugin update` and marketplace auto-update while the tray app is running), and via the `--sanitize-manifest` CLI flag for users who don't keep the tray app running. All four paths must keep working — if you add a new install path, wire sanitize into it too.

## Plugin versioning

Three version fields are kept in sync:

- `Cargo.toml` `[workspace.package].version` — the tray-app version.
- `plugin/.claude-plugin/plugin.json` `version` — the Claude Code plugin version users see in `claude plugin list`. Claude Code only delivers updates when this field changes.
- `crates/app/tauri.conf.json` `version` — embedded in the MSI / NSIS / DMG bundle filename and metadata.

How the bumps work:
- **Auto-bump (CI, every push to main).** `.github/workflows/auto-version-bump.yml` increments the patch number across all three files (`0.2.1 → 0.2.2 → 0.2.3 …`) and commits the change. This is what makes `claude plugin update` always pick up the latest hooks. Plain semver, no SHA suffix.
- **Manual bump (humans, on releases).** When you bump major or minor (`0.2.x → 0.3.0` or `0.x → 1.0.0`), edit all three files in the same commit. The auto-bump workflow detects that the push already touched a version file and skips itself, so your `0.3.0` doesn't get bounced to `0.3.1` immediately. The next normal push (no version-file edits) resumes auto-bumps from your new base.
- The `crates/core/tests/plugin_version_sync.rs` test fails the build if `Cargo.toml`'s and `plugin.json`'s versions disagree.

## Don't edit

- `crates/app/gen/` — regenerated by `tauri-build` on every build. Changes are lost.

## Hook scripts — single source of truth

Hook scripts (`pending_hook.sh` and `pending_hook.ps1`) live **only** in `plugin/hooks/`. There is no copy in `scripts/`. The plugin marketplace ships the `plugin/` subtree as-is; manual install instructions in `INSTALL.md` and dev smoke-test commands in `scripts/README.md` reference `plugin/hooks/` directly. A regression test (`crates/core/tests/no_hook_duplication.rs`) fails the workspace build if `scripts/pending_hook.*` ever reappear, so a contributor can't accidentally re-introduce drift.

## Phase plans (for historical context)

- Phase 1 — core library: `docs/superpowers/plans/2026-04-16-phase1-core-library.md`
- Phase 2 — adapters + hook scripts: `docs/superpowers/plans/2026-04-16-phase2-adapters-hooks.md`
- Phase 3 — Tauri app + UI: `docs/superpowers/plans/2026-04-16-phase3-tauri-app.md`
- Phase 4 — plugin + docs + CI + release: `docs/superpowers/plans/2026-04-17-phase4-plugin-docs-release.md`

## Before cutting a release

Run through `docs/release-checklist.md` fully, especially the manual UI scenarios that `smoke-test-auto.ps1` cannot cover.
