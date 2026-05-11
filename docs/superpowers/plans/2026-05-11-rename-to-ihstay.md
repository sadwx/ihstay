# Rename Project to IHSTAY Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the project from `claude-pending-board` to `ihstay` ("I Have Something To Ask You") across crate names, plugin metadata, Tauri bundle, GitHub repo, scripts, and user-facing docs — keeping the existing data path and hook script filenames stable to avoid migrating user data.

**Architecture:** The rename touches eight independent surfaces; each gets its own phase so the workspace continues to build and test after every commit. Internal identifier rename (Cargo + Rust `use` statements) comes first because everything else depends on the new crate names compiling. User-visible surfaces (plugin name, slash command, productName) come after, because changing those before the code compiles would leave the repo in a broken state across multiple commits.

**Tech Stack:** Rust workspace (3 crates), Tauri 2, Claude Code plugin (`plugin.json` + `marketplace.json`), GitHub Actions workflows, PowerShell + Bash smoke tests, OpenSpec specs.

---

## Scope and Decisions

These decisions are baked into the plan. If you disagree with any, flag before starting Phase 1.

| Decision | Choice | Rationale |
|---|---|---|
| Data path `~/.claude/pending/` | **Keep** | Path describes the data ("pending sessions"), not the app. Avoids migration burden for existing users. |
| Hook scripts `pending_hook.{ps1,sh}` | **Keep filenames** | Scripts describe what they do (write to pending board); they don't carry the project name. |
| Slash command `/pending-board` | **Rename → `/ihstay`** | User-facing CC command — should match the new project name. |
| Tauri `productName` | **`"IHSTAY"`** (uppercase acronym) | Short, fits in tray tooltip, hints at the joke. Friendly subtitle "I Have Something To Ask You" goes in plugin description + README. |
| Bundle identifier | **`com.ihstay.app`** | macOS app bundle changes. Existing macOS users get a new install, not an update. Accept; document in release notes. |
| GitHub repo | **Rename `sadwx/claude-pending-board` → `sadwx/ihstay`** | GitHub auto-redirects old URLs for now — but the marketplace catalog must point at the new slug. |
| Plugin name in `claude plugin list` | **`ihstay`** | Existing plugin users must `/plugin uninstall claude-pending-board && /plugin install ihstay@ihstay`. Document in release notes. |
| Crate names | **`ihstay-core`, `ihstay-adapters`, `ihstay-app`** | Consistent with new project name. Binary becomes `ihstay-app`. |
| Tracing target | **`ihstay=info`** | Matches new crate prefix (`ihstay_core`, `ihstay_adapters`, `ihstay_app`). |
| Historical docs (`openspec/changes/archive/`, `docs/superpowers/plans/2026-04-*.md`) | **Do not rename** | They're historical artifacts. References stay frozen. |
| Local working directory `D:\lab\suxi\claude-pending-board\` | **Optional final cleanup** | Listed as Phase 10. Rename later if desired. |

## Phase 0: Pre-flight — Rename the GitHub repo

**Why first:** The Cargo.toml `repository` URL, `plugin.json` `homepage`/`repository`, the in-app "Install plugin" setup card, INSTALL.md, README.md, the release workflow, and the OpenSpec working spec all hardcode `sadwx/claude-pending-board`. Renaming the GitHub repo first means subsequent commits land on the new remote with valid URLs.

GitHub auto-redirects the old URL, so old marketplace `add` commands still work for a grace period — but plan on those redirects expiring once a new repo with the old name is ever created.

**Files:** None in this repo. Action happens on github.com.

- [ ] **Step 1: Rename the repo on github.com**

Visit https://github.com/sadwx/claude-pending-board/settings → "Repository name" → change to `ihstay` → Rename.

- [ ] **Step 2: Update the local git remote**

```powershell
git remote set-url origin https://github.com/sadwx/ihstay.git
git remote -v
```

Expected output:
```
origin  https://github.com/sadwx/ihstay.git (fetch)
origin  https://github.com/sadwx/ihstay.git (push)
```

- [ ] **Step 3: Verify push works**

```powershell
git fetch origin
git status
```

Expected: clean tracking against `origin/main`, no auth errors.

---

## Phase 1: Cargo workspace + Rust identifiers

**Why second:** Pure refactor — no user-visible change. After this phase, the workspace builds and all 66 tests pass with the new crate names. Everything downstream (plugin metadata, scripts, docs) can then reference the new names with confidence.

**Files:**
- Modify: `Cargo.toml:9` (workspace repository URL)
- Modify: `crates/core/Cargo.toml:2` (package name)
- Modify: `crates/adapters/Cargo.toml:2,8` (package name + core dep)
- Modify: `crates/app/Cargo.toml:2,8,9` (package name + 2 deps)
- Modify: Every Rust file that says `use claude_pending_board_*` or references the snake-case crate root

- [ ] **Step 1: Update workspace repository URL**

Edit `Cargo.toml` line 9:

```toml
repository = "https://github.com/sadwx/ihstay"
```

- [ ] **Step 2: Rename core crate package**

Edit `crates/core/Cargo.toml` line 2:

```toml
name = "ihstay-core"
```

- [ ] **Step 3: Rename adapters crate package + update core dep**

Edit `crates/adapters/Cargo.toml`:

```toml
[package]
name = "ihstay-adapters"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
ihstay-core = { path = "../core" }
```

(Only lines 2 and 8 change; rest stays identical to the existing file.)

- [ ] **Step 4: Rename app crate package + update both deps**

Edit `crates/app/Cargo.toml`:

```toml
[package]
name = "ihstay-app"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
ihstay-core = { path = "../core" }
ihstay-adapters = { path = "../adapters" }
```

(Only lines 2, 8, 9 change.)

- [ ] **Step 5: Update Rust `use` statements and snake-case references**

For every `.rs` file under `crates/`, replace:
- `claude_pending_board_core` → `ihstay_core`
- `claude_pending_board_adapters` → `ihstay_adapters`
- `claude_pending_board_app` → `ihstay_app` (none currently exist, but the EnvFilter in Phase 2 may need it — see Phase 2 Step 4)

Exact files and lines (from a grep at plan time):

- `crates/adapters/src/wezterm.rs:1,2,190`
- `crates/adapters/src/iterm2.rs:3,4,68`
- `crates/adapters/src/lib.rs:5,27`
- `crates/app/src/commands.rs:2,3,4,15,292`
- `crates/app/src/services.rs:3,4,5,6,7,109`
- `crates/app/src/state.rs:1,2,3,4,5`
- `crates/app/src/tray.rs:26,32,61,67`
- `crates/core/tests/end_to_end.rs:7`

Use a workspace-wide grep+replace:

```powershell
# PowerShell equivalent of a sed -i across crates/**/*.rs
Get-ChildItem -Path crates -Recurse -Filter *.rs | ForEach-Object {
    $content = Get-Content $_.FullName -Raw
    $content = $content -replace 'claude_pending_board_core', 'ihstay_core'
    $content = $content -replace 'claude_pending_board_adapters', 'ihstay_adapters'
    $content = $content -replace 'claude_pending_board_app', 'ihstay_app'
    Set-Content -Path $_.FullName -Value $content -NoNewline
}
```

- [ ] **Step 6: Regenerate Cargo.lock**

```powershell
cargo build -p ihstay-app
```

Expected: builds successfully, `Cargo.lock` updates the package names automatically.

- [ ] **Step 7: Run the full test suite**

```powershell
cargo test --workspace
```

Expected: all 66 tests pass. `plugin_version_sync` and `no_hook_duplication` still pass (they reference paths, not crate names).

- [ ] **Step 8: Lint check**

```powershell
cargo fmt --check --all
cargo clippy --workspace -- -D warnings
```

Expected: both clean.

- [ ] **Step 9: Commit**

```powershell
git add Cargo.toml Cargo.lock crates
git commit -m "refactor: rename Rust crates to ihstay-{core,adapters,app}"
```

---

## Phase 2: Tauri productName, bundle ID, window titles, tracing target

**Why now:** The bundle ID change forces a clean macOS install on first launch — get it in before public plugin-name rename so existing users only re-install once.

**Files:**
- Modify: `crates/app/tauri.conf.json:2,4`
- Modify: `crates/app/src/main.rs:56,74,137,177`

- [ ] **Step 1: Update Tauri config**

Edit `crates/app/tauri.conf.json` — change two fields, keep version + everything else:

```json
{
  "productName": "IHSTAY",
  "version": "0.3.0",
  "identifier": "com.ihstay.app",
  "build": {
    "frontendDist": "ui"
  },
  "app": {
    "withGlobalTauri": true,
    "macOSPrivateApi": true,
    "windows": [],
    "security": {
      "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

- [ ] **Step 2: Update HUD window title in main.rs**

`crates/app/src/main.rs` line 56 — replace `.title("Claude Pending Board")` with `.title("IHSTAY")`.

- [ ] **Step 3: Update Settings window title in main.rs**

`crates/app/src/main.rs` line 74 — replace `.title("Settings - Claude Pending Board")` with `.title("Settings - IHSTAY")`.

- [ ] **Step 4: Update tracing target prefix**

`crates/app/src/main.rs` line 177 — replace:

```rust
.unwrap_or_else(|_| EnvFilter::new("claude_pending_board=info"));
```

with:

```rust
.unwrap_or_else(|_| EnvFilter::new("ihstay=info"));
```

This matches the new crate prefix from Phase 1 (the EnvFilter prefix-matches against snake_case crate names; all three crates now start with `ihstay_`).

- [ ] **Step 5: Update startup log message**

`crates/app/src/main.rs` line 137 — replace:

```rust
tracing::info!("Claude Pending Board started");
```

with:

```rust
tracing::info!("IHSTAY started");
```

- [ ] **Step 6: Build to verify**

```powershell
cargo build -p ihstay-app
```

Expected: clean build, no warnings about the JSON or Rust changes.

- [ ] **Step 7: Smoke-test the rename locally**

```powershell
./target/debug/ihstay-app.exe
```

Verify the tray icon appears and right-click menu still shows the expected items. The window title is invisible (decorations off on HUD), but the Windows process list should now show `ihstay-app.exe`.

Quit the tray app before continuing.

- [ ] **Step 8: Commit**

```powershell
git add crates/app/tauri.conf.json crates/app/src/main.rs
git commit -m "feat(app): rename productName to IHSTAY and bundle id to com.ihstay.app"
```

---

## Phase 3: Plugin metadata + marketplace catalog + install constants

**Why now:** This is the user-visible plugin rename. After this phase, new installs use `ihstay@ihstay`; existing installs still work because Claude Code keys plugins by name and the watcher / sanitizer code uses the new `PLUGIN_NAME` constant.

**Files:**
- Modify: `plugin/.claude-plugin/plugin.json:3,5,10,11`
- Modify: `.claude-plugin/marketplace.json:3,10,12`
- Modify: `crates/app/src/plugin_install.rs:17,18,19`
- Modify: `crates/app/src/plugin_watch.rs:2,27`

- [ ] **Step 1: Update plugin manifest**

Edit `plugin/.claude-plugin/plugin.json` — change name, description, homepage, repository (leave the entire `hooks` block untouched):

```json
{
  "$schema": "https://json.schemastore.org/claude-code-plugin.json",
  "name": "ihstay",
  "version": "0.3.0",
  "description": "IHSTAY (I Have Something To Ask You) — surface every pending Claude Code session across all projects in a single floating HUD window.",
  "author": {
    "name": "sadwx",
    "url": "https://github.com/sadwx"
  },
  "homepage": "https://github.com/sadwx/ihstay",
  "repository": "https://github.com/sadwx/ihstay",
  "hooks": {
    ...
  }
}
```

Preserve the entire existing `hooks` block verbatim — only the four top-level fields change.

- [ ] **Step 2: Update marketplace catalog**

Edit `.claude-plugin/marketplace.json`:

```json
{
  "$schema": "https://json.schemastore.org/claude-code-marketplace.json",
  "name": "ihstay",
  "owner": {
    "name": "sadwx",
    "url": "https://github.com/sadwx"
  },
  "plugins": [
    {
      "name": "ihstay",
      "source": "./plugin",
      "description": "IHSTAY (I Have Something To Ask You) — registers Claude Code hooks that feed the IHSTAY tray app, surfacing every waiting Claude session in one floating HUD.",
      "version": "0.3.0"
    }
  ]
}
```

(The marketplace `version` field stayed at `0.2.0` historically — this plan bumps it to match `plugin.json` so the catalog stays consistent.)

- [ ] **Step 3: Update install constants**

Edit `crates/app/src/plugin_install.rs` lines 17-19:

```rust
const MARKETPLACE: &str = "sadwx/ihstay";
const PLUGIN_REF: &str = "ihstay@ihstay";
const PLUGIN_NAME: &str = "ihstay";
```

- [ ] **Step 4: Update plugin watcher**

Edit `crates/app/src/plugin_watch.rs` line 2 (the doc comment) and line 27:

```rust
//! that touch the `ihstay` plugin and re-run the
```

```rust
pub const PLUGIN_NAME: &str = "ihstay";
```

- [ ] **Step 5: Run tests to confirm version-sync and no-duplication guards still pass**

```powershell
cargo test --workspace
```

Expected: all tests pass. `plugin_version_sync` reads the JSON `version` field (unchanged); `no_hook_duplication` reads filesystem paths (unchanged).

- [ ] **Step 6: Commit**

```powershell
git add plugin/.claude-plugin/plugin.json .claude-plugin/marketplace.json crates/app/src/plugin_install.rs crates/app/src/plugin_watch.rs
git commit -m "feat(plugin): rename plugin to ihstay@ihstay"
```

---

## Phase 4: Slash command rename `/pending-board` → `/ihstay`

**Files:**
- Rename: `plugin/commands/pending-board.md` → `plugin/commands/ihstay.md`
- Modify (content of the renamed file): lines 2, 3, 6, 8, 20, 25, 37, 54, 58, 64, 73

- [ ] **Step 1: Rename the command file**

```powershell
git mv plugin/commands/pending-board.md plugin/commands/ihstay.md
```

- [ ] **Step 2: Rewrite the file content**

Replace `plugin/commands/ihstay.md` entirely with:

```markdown
---
description: Manage the IHSTAY plugin — status, install, doctor, uninstall hooks
argument-hint: "[status | install | doctor | hooks-uninstall]"
---

# /ihstay

You are the ihstay operator. Run the requested subcommand and report the result to the user.

Subcommand: **$ARGUMENTS** (defaults to `status` if empty)

---

## `status`

Report the current state of the pending board:

1. Read `~/.claude/pending/board.jsonl` (if it exists). Count lines.
2. Parse it and count live entries by notification type (permission_prompt, idle_prompt) and state (live, stale).
3. Report the tray app binary location if it's known (check `~/.claude/pending/config.toml` or PATH for `ihstay-app`).
4. Report whether the app process is currently running (via `tasklist` on Windows or `pgrep` on Unix).

Output format:
```
IHSTAY — status
  board.jsonl: <N> lines, <M> live entries (<P> permission, <I> idle), <S> stale
  tray app: <running | not running> (<path if known>)
  last activity: <timestamp of most recent op in board.jsonl, or "never">
```

---

## `install`

Explain to the user how to install the tray app:

1. Visit https://github.com/sadwx/ihstay/releases
2. Download the artifact for your OS
3. Launch it — the tray icon should appear

Do NOT download or run the binary for them. Just print the instructions.

---

## `doctor`

Run the following diagnostic checks and report each as OK or FAIL with a remediation hint on failure:

1. **Hooks registered**: Read `~/.claude/settings.json` OR verify the plugin is enabled in `~/.claude/plugins/installed_plugins.json`. Look for `Notification`, `UserPromptSubmit`, and `Stop` hooks.
2. **Hook scripts exist**: Check `${CLAUDE_PLUGIN_ROOT}/hooks/pending_hook.ps1` (Windows) or `pending_hook.sh` (Unix).
3. **Board file writable**: Try `touch ~/.claude/pending/board.jsonl` (create if missing). Verify append works.
4. **Log directory writable**: Same for `~/.claude/pending/logs/`.
5. **Terminal adapter in PATH**: Check `wezterm --version` (Windows) or `osascript -e 'tell application "iTerm2" to version'` (macOS).
6. **Tray app installed**: Check for `ihstay-app` on PATH or standard install locations.

Output format:
```
IHSTAY — doctor
  OK Hooks registered: Notification, UserPromptSubmit, Stop
  OK Hook scripts: pending_hook.ps1 (at <path>)
  OK Board file writable: ~/.claude/pending/board.jsonl
  OK Log directory writable: ~/.claude/pending/logs/
  OK Terminal adapter: wezterm 20240203-...
  FAIL Tray app: not found — install from https://github.com/sadwx/ihstay/releases
```

---

## `hooks-uninstall`

Help the user uninstall the plugin:

1. Explain that `/plugin uninstall ihstay` removes the hooks.
2. Offer to clean up `~/.claude/pending/` (board.jsonl, logs, config) — but do NOT delete without confirmation.

---

**Default (no argument):** Run `status`.
```

(Note: the hook script filenames `pending_hook.ps1` / `pending_hook.sh` and the data path `~/.claude/pending/` are deliberately preserved — see Scope and Decisions table.)

- [ ] **Step 3: Commit**

```powershell
git add plugin/commands/
git commit -m "feat(plugin): rename slash command /pending-board to /ihstay"
```

---

## Phase 5: Workflows + smoke-test scripts

**Files:**
- Modify: `.github/workflows/release.yml:39,40`
- Modify: `scripts/smoke-test-auto.ps1:15,57,69,76`
- Modify: `scripts/smoke-test.ps1:2,30`
- Modify: `scripts/smoke-test.sh:2,26`

(`.github/workflows/auto-version-bump.yml` and `ci.yml` don't reference the project name — no edits.)

- [ ] **Step 1: Update release workflow**

Edit `.github/workflows/release.yml` lines 39-40:

```yaml
          releaseName: "IHSTAY ${{ github.ref_name }}"
          releaseBody: "See the release notes below and [docs/release-checklist.md](https://github.com/sadwx/ihstay/blob/main/docs/release-checklist.md) for the manual verification steps."
```

- [ ] **Step 2: Update auto smoke test**

Edit `scripts/smoke-test-auto.ps1`:

Line 15 — binary path:
```powershell
    [string]$AppPath = (Join-Path $PSScriptRoot ".." "target" "debug" "ihstay-app.exe"),
```

Line 57 — process check:
```powershell
$existing = Get-Process -Name "ihstay-app" -ErrorAction SilentlyContinue
```

Line 69 — banner:
```powershell
Write-Host "IHSTAY — automated smoke test" -ForegroundColor Magenta
```

Line 76 — build hint:
```powershell
    Write-Host "  Run: cargo build -p ihstay-app" -ForegroundColor Yellow
```

- [ ] **Step 3: Update interactive smoke tests**

Edit `scripts/smoke-test.ps1`:

Line 2:
```powershell
# smoke-test.ps1 — Interactive smoke test for IHSTAY
```

Line 30:
```powershell
Write-Host "  IHSTAY — Smoke Test" -ForegroundColor Magenta
```

Edit `scripts/smoke-test.sh`:

Line 2:
```bash
# smoke-test.sh — Interactive smoke test for IHSTAY
```

Line 26:
```bash
echo "  IHSTAY — Smoke Test"
```

- [ ] **Step 4: Run the auto smoke test to confirm**

```powershell
cargo build -p ihstay-app
pwsh scripts/smoke-test-auto.ps1
```

Expected: ~40s run, finishes with "All checks passed" or equivalent banner. No failures from missing binary path.

- [ ] **Step 5: Commit**

```powershell
git add .github/workflows/release.yml scripts/
git commit -m "chore: rename to IHSTAY in workflows and smoke-test scripts"
```

---

## Phase 6: User-facing docs

**Files:**
- Modify: `README.md` (title + all references)
- Modify: `INSTALL.md` (title + all references + binary path)
- Modify: `plugin/README.md` (title + all references)
- Modify: `crates/app/ui/hud/index.html:77,78` (setup card)
- Modify: `docs/release-checklist.md` (all references)

For each file below, perform a literal find-and-replace of these pairs (in this order; the longer phrases first to avoid partial substitution):

| Old | New |
|---|---|
| `Claude Pending Board` | `IHSTAY` |
| `claude-pending-board-app` | `ihstay-app` |
| `claude-pending-board@claude-pending-board` | `ihstay@ihstay` |
| `sadwx/claude-pending-board` | `sadwx/ihstay` |
| `github:sadwx/claude-pending-board` | `github:sadwx/ihstay` |
| `claude-pending-board` (any remaining) | `ihstay` |

- [ ] **Step 1: Update README.md**

Apply the find-and-replace pairs above to `README.md`. If the top-level heading was `# Claude Pending Board`, it becomes `# IHSTAY`. Add a one-line note under the heading: `> I Have Something To Ask You — a tray HUD that surfaces every waiting Claude Code session.`

- [ ] **Step 2: Update INSTALL.md**

Apply the same pairs. Pay attention to line 131 (Windows install path):

```markdown
"C:\Program Files\IHSTAY\ihstay-app.exe" --sanitize-manifest   # Windows
```

The folder name `IHSTAY` comes from the new Tauri `productName`.

- [ ] **Step 3: Update plugin/README.md**

Apply the same pairs. The heading `# Claude Pending Board — Claude Code Plugin` becomes `# IHSTAY — Claude Code Plugin`.

- [ ] **Step 4: Update in-app setup card**

Edit `crates/app/ui/hud/index.html` lines 77-78:

```html
        <code>/plugin marketplace add github:sadwx/ihstay</code><br>
        <code>/plugin install ihstay@ihstay</code>
```

- [ ] **Step 5: Update release checklist**

Apply the find-and-replace pairs to `docs/release-checklist.md`.

- [ ] **Step 6: Spot-check the user-facing UI**

```powershell
cargo build -p ihstay-app
./target/debug/ihstay-app.exe
```

Open the HUD. If the setup card is showing (no plugin installed), confirm the marketplace add / install commands display the new `ihstay@ihstay` strings. Quit the app.

- [ ] **Step 7: Commit**

```powershell
git add README.md INSTALL.md plugin/README.md crates/app/ui/hud/index.html docs/release-checklist.md
git commit -m "docs: rename to IHSTAY in user-facing documentation"
```

---

## Phase 7: Internal docs — CLAUDE.md and the OpenSpec working spec

**Files:**
- Modify: `CLAUDE.md` (project name in intro paragraph)
- Modify: `openspec/specs/pending-board/spec.md:317,318,332,360,367` (install command + binary name)
- Modify: `openspec/specs/pending-board/spec.md:338` (slash command name)

CLAUDE.md history note: historical sections (`Phase plans (for historical context)`) reference dates and old filenames; leave them. Only update the current-state intro paragraph that describes what the project is.

OpenSpec working spec note: this is the source-of-truth spec. The behaviors don't change, but the identifiers (plugin name, slash command, binary) do — so update only the strings in the WHEN/THEN clauses that name those identifiers. The functional requirements stay verbatim.

- [ ] **Step 1: Update CLAUDE.md intro**

In the `## What this is` section of `CLAUDE.md` (top of file), replace the current opening sentence:

```markdown
`claude-pending-board` is a cross-platform tray app that surfaces every waiting Claude Code CLI session in one floating HUD. Claude Code hooks write to `~/.claude/pending/board.jsonl`; the tray app watches that file and renders a HUD, and clicking an entry focuses the owning WezTerm (Windows) or iTerm2 (macOS) pane.
```

with:

```markdown
`ihstay` ("I Have Something To Ask You") is a cross-platform tray app that surfaces every waiting Claude Code CLI session in one floating HUD. Claude Code hooks write to `~/.claude/pending/board.jsonl`; the tray app watches that file and renders a HUD, and clicking an entry focuses the owning WezTerm (Windows) or iTerm2 (macOS) pane.
```

Then update the repo-layout caption and any other `claude-pending-board` references *describing current state* — but leave any historical text in the `Phase plans` and `Known gotchas` sections that recounts past incidents using the old name.

The "Plugin versioning" section's three-file list references `plugin/.claude-plugin/plugin.json` (path unchanged) and the auto-bump workflow (also unchanged). No edits needed there beyond the literal name swaps that the find-replace handles.

- [ ] **Step 2: Update OpenSpec working spec — install command strings**

`openspec/specs/pending-board/spec.md` lines 317-318 (current):

```markdown
- **WHEN** the user runs `claude plugin marketplace add sadwx/claude-pending-board` followed by `claude plugin install claude-pending-board@claude-pending-board` (or the equivalent slash commands inside Claude Code)
- **THEN** the marketplace catalog at `.claude-plugin/marketplace.json` SHALL list `claude-pending-board` with `source = "./plugin"`
```

Replace with:

```markdown
- **WHEN** the user runs `claude plugin marketplace add sadwx/ihstay` followed by `claude plugin install ihstay@ihstay` (or the equivalent slash commands inside Claude Code)
- **THEN** the marketplace catalog at `.claude-plugin/marketplace.json` SHALL list `ihstay` with `source = "./plugin"`
```

- [ ] **Step 3: Update OpenSpec line 332 (auto-install shell command)**

Replace the line:

```markdown
- **THEN** the app SHALL shell out to `claude plugin marketplace add sadwx/claude-pending-board` and then `claude plugin install claude-pending-board@claude-pending-board` running as the user
```

with:

```markdown
- **THEN** the app SHALL shell out to `claude plugin marketplace add sadwx/ihstay` and then `claude plugin install ihstay@ihstay` running as the user
```

- [ ] **Step 4: Update OpenSpec line 338 (slash command name)**

Replace:

```markdown
- **WHEN** the user runs `/pending-board doctor`
```

with:

```markdown
- **WHEN** the user runs `/ihstay doctor`
```

- [ ] **Step 5: Update OpenSpec line 360 (binary name)**

Replace:

```markdown
- **WHEN** the user invokes the binary as `claude-pending-board-app --sanitize-manifest`
```

with:

```markdown
- **WHEN** the user invokes the binary as `ihstay-app --sanitize-manifest`
```

- [ ] **Step 6: Update OpenSpec line 367 (plugin cache path component)**

Replace:

```markdown
- **WHEN** the tray app is running and any filesystem event under `~/.claude/plugins/cache/` mentions a path component named `claude-pending-board` (typical of `claude plugin install` / `claude plugin update` / marketplace auto-update creating or replacing a version directory)
```

with:

```markdown
- **WHEN** the tray app is running and any filesystem event under `~/.claude/plugins/cache/` mentions a path component named `ihstay` (typical of `claude plugin install` / `claude plugin update` / marketplace auto-update creating or replacing a version directory)
```

- [ ] **Step 7: Commit**

```powershell
git add CLAUDE.md openspec/specs/pending-board/spec.md
git commit -m "docs(spec): rename to ihstay in working spec and CLAUDE.md"
```

---

## Phase 8: Full verification

**Why now:** All commits land cleanly; the workspace and the plugin are in their final state. This phase verifies nothing slipped through.

- [ ] **Step 1: Grep for any remaining old-name strings**

```powershell
# Should return zero results — anything that fires here is a missed rename.
# (Adjust this list if the actual matches turn out to be in historical files
# that we deliberately left untouched.)
rg "claude-pending-board" `
  --glob "!openspec/changes/archive/**" `
  --glob "!docs/superpowers/plans/2026-04-*.md" `
  --glob "!Cargo.lock"
```

Expected: zero hits in non-historical files. If anything in historical files (archive, old phase plans) shows up — that's fine, leave them.

```powershell
rg "claude_pending_board" `
  --glob "!openspec/changes/archive/**" `
  --glob "!docs/superpowers/plans/2026-04-*.md"
```

Expected: zero hits.

```powershell
rg "Claude Pending Board" `
  --glob "!openspec/changes/archive/**" `
  --glob "!docs/superpowers/plans/2026-04-*.md"
```

Expected: zero hits.

Anything that comes back, fix it inline and amend the corresponding phase commit (or add a small follow-up commit).

- [ ] **Step 2: Run the full test suite**

```powershell
cargo fmt --check --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Expected: all clean; all 66 tests pass.

- [ ] **Step 3: Release build**

```powershell
cargo tauri build
```

Expected: produces an MSI named after `IHSTAY` in `target/release/bundle/msi/`. The bundle path and product name reflect the rename.

- [ ] **Step 4: Run the auto smoke test against the release build**

```powershell
pwsh scripts/smoke-test-auto.ps1
```

Expected: passes.

- [ ] **Step 5: Manual UI verification**

Launch the just-built binary from `target/release/ihstay-app.exe`:

1. Tray icon appears
2. Right-click → menu opens (Show, Settings, Quit)
3. Left-click → HUD shows
4. If no plugin installed: setup card shows `ihstay@ihstay` install command
5. Settings window opens and closes without destroying state
6. `tasklist | findstr ihstay` shows the process

Quit before continuing.

- [ ] **Step 6: Plugin install end-to-end test (optional, if you have a fresh CC environment)**

In a Claude Code session pointed at this local repo:

```
/plugin marketplace add /path/to/local/ihstay
/plugin install ihstay@ihstay
```

Then trigger a pending event (e.g. start a long-running task). Confirm a JSONL line appears in `~/.claude/pending/board.jsonl` and shows up in the HUD.

---

## Phase 9: Release notes for v0.4.0

**Why now:** The rename is a breaking change for plugin users (uninstall+reinstall) and for macOS users (fresh install, not update). The release notes are how users learn the migration steps.

**Files:**
- Create: `docs/release-notes/v0.4.0-rename-to-ihstay.md` (or wherever release notes live in this repo — check `docs/` for existing release-notes layout; if none, this is the first file)

If `docs/release-notes/` doesn't exist, create it. Otherwise follow the existing naming convention.

- [ ] **Step 1: Bump version to 0.4.0 in all three files**

The auto-bump workflow only bumps patch numbers. A minor bump is a human-controlled bump (see CLAUDE.md "Plugin versioning"). Edit:

`Cargo.toml` `[workspace.package].version`:

```toml
version = "0.4.0"
```

`plugin/.claude-plugin/plugin.json` `version`:

```json
"version": "0.4.0",
```

`crates/app/tauri.conf.json` `version`:

```json
"version": "0.4.0",
```

`.claude-plugin/marketplace.json` plugin `version` (this one isn't covered by the auto-bump workflow but should still track):

```json
"version": "0.4.0"
```

- [ ] **Step 2: Write the release notes**

Create `docs/release-notes/v0.4.0-rename-to-ihstay.md`:

```markdown
# v0.4.0 — Renamed to IHSTAY

The project is renamed from `claude-pending-board` to **IHSTAY** ("I Have Something To Ask You"). No functional changes; same HUD, same hooks, same data path. Only the names change.

## What changed for users

- **Plugin name**: `claude-pending-board` → `ihstay`
- **Slash command**: `/pending-board` → `/ihstay`
- **Tray app product name**: "Claude Pending Board" → "IHSTAY"
- **GitHub repo**: `sadwx/claude-pending-board` → `sadwx/ihstay` (old URL auto-redirects)
- **Data path**: `~/.claude/pending/` — **unchanged**. Your existing board.jsonl, logs, and config carry over.

## Migration

### Plugin users (in Claude Code)

```
/plugin uninstall claude-pending-board
/plugin marketplace add github:sadwx/ihstay
/plugin install ihstay@ihstay
```

The marketplace `add` step is needed because the marketplace name itself changed. After this, `claude plugin list` shows `ihstay` instead of `claude-pending-board`.

### Tray app users

- **Windows**: Uninstall "Claude Pending Board" from Settings → Apps. Install the new `IHSTAY.msi` from the v0.4.0 release. If you had a Startup folder shortcut, recreate it pointing at the new `IHSTAY\ihstay-app.exe`.
- **macOS**: Drag `/Applications/Claude Pending Board.app` to Trash, then install the new `IHSTAY.dmg`. The bundle identifier changed (`com.claude-pending-board.app` → `com.ihstay.app`), so this is treated as a fresh install — your data path `~/.claude/pending/` is untouched.
- **Linux**: replace the binary in the same way you originally installed it.

## What didn't change

- `~/.claude/pending/board.jsonl` — same file, same format
- `pending_hook.{ps1,sh}` — hook script filenames unchanged
- All Claude Code hook events (Notification, UserPromptSubmit, Stop, SessionEnd, PermissionDenied)
- HUD behavior, focus-pane behavior, dismiss-timer behavior
```

- [ ] **Step 3: Run the full test suite one more time after the version bump**

```powershell
cargo test --workspace
```

Expected: `plugin_version_sync` still passes (we bumped all three version fields together).

- [ ] **Step 4: Commit and tag**

```powershell
git add Cargo.toml Cargo.lock plugin/.claude-plugin/plugin.json crates/app/tauri.conf.json .claude-plugin/marketplace.json docs/release-notes/
git commit -m "release: v0.4.0 — rename to IHSTAY"
git tag v0.4.0
```

Do not push the tag yet — push only when the release-checklist manual verification (Phase 8 Step 5) is satisfactory.

- [ ] **Step 5: Push and let the release workflow run**

```powershell
git push origin main
git push origin v0.4.0
```

Expected: the `Release` workflow on github.com builds the macOS DMG and Windows MSI, drafts a v0.4.0 release. The auto-bump workflow detects the manual version edit and skips itself (per the `manual.outputs.skip == 'true'` branch in `auto-version-bump.yml`).

---

## Phase 10 (optional): Rename the local working directory

This is a developer-convenience cleanup; nothing in the repo depends on the directory name.

- [ ] **Step 1: Close all editors, terminals, and the tray app**

Anything holding a file handle in `D:\lab\suxi\claude-pending-board\` will block the rename.

- [ ] **Step 2: Rename**

```powershell
Move-Item D:\lab\suxi\claude-pending-board D:\lab\suxi\ihstay
```

- [ ] **Step 3: Re-open the project at the new path**

Update IDE shortcuts, terminal `cd` aliases, etc. Update CLAUDE.md (additional working directories) if you reference the absolute path elsewhere.

---

## Self-review checklist

Before handing off the plan for execution, the author verified:

- **Spec coverage**: every cell in the Scope and Decisions table maps to at least one phase. Hook script filenames + data path stay (explicit non-change). GitHub repo rename = Phase 0. Crate names = Phase 1. ProductName + bundle ID = Phase 2. Plugin name + marketplace = Phase 3. Slash command = Phase 4. Workflows/scripts = Phase 5. Docs = Phase 6. CLAUDE.md + OpenSpec = Phase 7. Verification = Phase 8. Release notes + version bump = Phase 9. Local dir rename = optional Phase 10.

- **Placeholder scan**: no "TODO", no "add error handling", no "similar to Task N", no references to undefined identifiers. The few "leave historical text" instructions in Phase 7 Step 1 are scoped explicitly — engineer can identify them by grep.

- **Type consistency**: the new crate names (`ihstay-core`, `ihstay-adapters`, `ihstay-app`) and their snake-case forms (`ihstay_core`, `ihstay_adapters`, `ihstay_app`) are used consistently across Phases 1, 2, 5, 7, 8. The tracing target `ihstay=info` (Phase 2 Step 4) correctly prefix-matches all three new snake-case crate names.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-11-rename-to-ihstay.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per phase, review between phases, fast iteration. Best for catching missed renames early.

**2. Inline Execution** — Execute phases in this session using executing-plans, batch execution with checkpoints. Best if you want to watch each diff land in real time.

Which approach?
