# Release Checklist

Manual verification required before cutting a new release.

## Pre-release

- [ ] `cargo fmt --check --all` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo test --workspace` all green (expected: ~60+ tests)
- [ ] `cargo tauri build --release` succeeds on each platform (Win / macOS)

## Smoke tests — per platform

Install the release artifact. Then run through:

- [ ] Tray icon appears on app launch
- [ ] HUD is hidden at startup (no entries)
- [ ] `echo '{"op":"add",...}' >> ~/.claude/pending/board.jsonl` causes HUD to auto-appear within 1 second
- [ ] HUD shows the entry under the right section (PERMISSION / IDLE)
- [ ] Clicking an entry focuses the owning terminal (requires WezTerm running)
- [ ] Clicking a stale entry spawns `claude --resume <id>` in a new tab
- [ ] HUD auto-hides 2s after board empties
- [ ] Dismiss X opens confirmation panel with DEFAULT pill and countdown
- [ ] Esc / countdown expiry commits the default
- [ ] Click "Wake me" / "Stay silent" commits immediately with override
- [ ] Tray left-click re-opens HUD from cooldown
- [ ] Tray right-click → Settings opens the settings window
- [ ] Settings form loads current config, Save persists, window auto-hides
- [ ] Reset HUD Position moves the HUD to bottom-right of primary monitor
- [ ] Non-activating: opening HUD does NOT steal keyboard focus from current app

## Plugin tests

- [ ] `/plugin install ihstay` from marketplace succeeds
- [ ] `/ihstay doctor` passes all checks with tray app + WezTerm installed
- [ ] Real Claude Code session with permission prompt writes to board.jsonl within 100ms

## Multi-monitor

- [ ] Drag HUD to secondary monitor, close app, relaunch — HUD restores on secondary
- [ ] Unplug secondary monitor while HUD was there — next launch falls back to primary

## Edge cases

- [ ] Kill the tray app while HUD is open — no zombie process, clean exit
- [ ] 50 entries in board.jsonl — HUD scrolls, doesn't grow
- [ ] Malformed line in board.jsonl — skipped silently, warning in app.log
- [ ] Delete board.jsonl while app is running — in-memory state clears, no crash

## Release

- [ ] Bump major or minor `version` in workspace `Cargo.toml`, `plugin/.claude-plugin/plugin.json`, `crates/app/tauri.conf.json` to the same plain-semver string. The auto-bump workflow detects that the push touched these files and skips itself, so your `0.3.0` (or whatever) survives. Patch-level bumps happen automatically on every commit; you only do this for releases.
- [ ] Update `CHANGELOG.md` (or release notes inline in the tag)
- [ ] Tag the release: `git tag -a v0.1.0 -m "..."`
- [ ] Push tag: `git push origin v0.1.0` — triggers `.github/workflows/release.yml`
- [ ] Verify GitHub Release has artifacts for all three OSes
- [ ] Verify `/plugin install` pulls the new version
- [ ] Announce (README badge, Discord, Twitter, etc.)
