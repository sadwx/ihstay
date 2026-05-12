# Installing IHSTAY

End-user install guide.

> **Heads up**: the project is currently **alpha**. The first tagged release is `v0.1.0`, flagged as a pre-release on GitHub. Binaries are unsigned, so Windows SmartScreen and macOS Gatekeeper will warn on install — see the platform notes below.

## Prerequisites

1. **Claude Code** installed and in `PATH`. Verify with `claude --version`.
2. **A supported terminal**:
   - **Windows**: [WezTerm](https://wezfurlong.org/wezterm/). Verify with `wezterm --version`.
   - **macOS**: WezTerm or [iTerm2](https://iterm2.com/).
3. **Write access** to `~/.claude/` (both the tray app and Claude Code write under this directory; created automatically on first run).

Windows Terminal is not supported as a focus target (cannot programmatically activate a specific tab). You can still run Claude Code inside Windows Terminal; clicking a pending entry just won't focus the right tab.

## Two-step install

### Step 1 · Install the tray app

Download the artifact for your OS from the [releases page](https://github.com/sadwx/ihstay/releases):

| OS | File | Notes |
|---|---|---|
| Windows | `IHSTAY_<version>_x64_en-US.msi` | MSI installer. Double-click, approve UAC. |
| Windows | `IHSTAY_<version>_x64-setup.exe` | NSIS portable-style installer. |
| macOS | `IHSTAY_<version>_universal.dmg` | Drag to Applications. |

**Windows**: SmartScreen may warn "Windows protected your PC". Click *More info → Run anyway*. This is expected because the artifact is unsigned during alpha.

**macOS**: Gatekeeper may say "can't be opened". Right-click the app → *Open*, or run `xattr -dr com.apple.quarantine /Applications/IHSTAY.app` once to clear the quarantine attribute.

After install, launch the app. A Catppuccin-pink "C" icon appears in the tray.

### Step 2 · Install the Claude Code plugin (hooks)

The tray app won't see any sessions until the plugin is registered. The easiest path: **click the tray icon → [Install plugin]** in the HUD's first-run setup card. The app shells out to the `claude plugin` CLI under your user account.

If you prefer the CLI directly:

```bash
claude plugin marketplace add sadwx/ihstay
claude plugin install ihstay@ihstay
```

Or from any Claude Code session:

```
/plugin marketplace add github:sadwx/ihstay
/plugin install ihstay@ihstay
/reload-plugins
```

Any of the three paths produces the same result: three hooks (`Notification`, `UserPromptSubmit`, `Stop`) registered with Claude Code.

### Step 2.5 · WSL (Windows users running Claude Code inside WSL)

If you launch Claude Code inside WSL via WezTerm, install the plugin **from inside each WSL distro** you use, not from a native Windows shell. The Linux hook script is what fires there. Open a WSL tab and run:

```bash
claude plugin marketplace add sadwx/ihstay
claude plugin install ihstay@ihstay
```

Repeat for every distro that runs Claude Code (Ubuntu, Debian, custom distros — each one needs its own `claude plugin install`).

That's it. The tray app detects WSL on launch and idempotently appends `WEZTERM_PANE/u` and `USERPROFILE/up` to your user `WSLENV`, so:

- `WEZTERM_PANE` crosses the Windows→WSL boundary and click-to-focus can address the right tab.
- `USERPROFILE` is path-translated (so `C:\Users\<you>` becomes `/mnt/c/Users/<you>` inside WSL), letting the bash hook write entries directly to the Windows-side `~/.claude/pending/board.jsonl` that the tray app watches. **Multi-distro setups don't need any per-distro symlink** — every distro's hook lands in the same Windows file, distinguished by a `wsl_distro` tag on each entry.

After the first launch, **open a fresh WezTerm tab** (or restart WezTerm) so it picks up the new `WSLENV`. Verify with:

```bash
echo $WEZTERM_PANE     # should print a number
echo $USERPROFILE      # should print /mnt/c/Users/<your-windows-user>
```

If you'd rather wire it up by hand, run this once from a Windows PowerShell tab and skip the auto-setup:

```powershell
[Environment]::SetEnvironmentVariable('WSLENV', "$env:WSLENV;WEZTERM_PANE/u:USERPROFILE/up", 'User')
```

If `$USERPROFILE` is not visible inside WSL (e.g. the auto-setup hasn't run yet, or you opened a tab before WezTerm restarted), the hook falls back to writing to the Linux-side `$HOME/.claude/pending/board.jsonl` — which the Windows tray app won't see. To avoid the fall-through, finish the WSLENV setup or drop in a manual symlink: `ln -s /mnt/c/Users/<you>/.claude/pending ~/.claude/pending`.

If `$WEZTERM_PANE` is unset inside WSL, entries still appear in the HUD (assuming the path-translation above is working) — clicking just opens a fresh tab via `wsl.exe -d <distro> -- bash -lc 'claude --resume <id>'` instead of focusing the existing one.

The Windows tray app is what runs and renders the HUD; only the hook scripts live in WSL.

## Step 3 · Verify

1. **Start a Claude Code session in WezTerm / iTerm2** in any project.
2. **Trigger a permission prompt** — e.g. ask Claude to run `ls`.
3. **The HUD auto-appears** near your tray icon with one red entry.
4. **Click the entry**. The WezTerm pane or iTerm2 session that owns it jumps to the foreground.
5. **Answer the prompt** in the terminal. The HUD auto-hides after a ~2-second grace delay.

If any step silently fails, jump to [Troubleshooting](#troubleshooting).

## Step 4 · Configure

Open **Settings…** from the tray and adjust any of the following:

| Setting | Default | Description |
|---|---|---|
| Cooldown after manual dismiss | 15 min | How long the HUD stays silent when you manually dismiss it |
| Reminding enabled | on | When on, the HUD re-opens at cooldown expiry if new items arrived during the cooldown |
| Auto-hide grace delay | 2 s | Delay after the last item clears before the HUD hides |
| Dismiss confirmation countdown | 5 s | Duration of the "Going silent for N minutes" panel |
| Skip dismiss confirmation | off | Bypass the confirmation panel entirely |
| Default terminal adapter | WezTerm (Windows) / iTerm2 (macOS) | Which adapter to use for focus and resume |
| HUD position | near tray | Drag the window to move; "Reset HUD position" returns it |

Changes apply immediately — no restart needed.

## Troubleshooting

### `/hooks` shows pwsh entries on macOS/Linux (or bash entries on Windows)

Claude Code 2.1.x ignores the `platform` field on hook entries, so the bundled `plugin.json` ships with one entry per OS for each event and Claude Code lists them all. The tray app strips foreign-platform entries from the installed `plugin.json` automatically:

- once at app startup,
- right after a tray-driven `[Install plugin]` click, and
- on every filesystem change under `~/.claude/plugins/cache/` that touches the plugin (so `claude plugin update` and marketplace auto-updates clear up within ~2 s without an app restart).

If you don't keep the tray app running, fresh installs will leave the foreign entries in place until you launch the app. To do the cleanup without launching the app, run the binary with `--sanitize-manifest`:

```bash
ihstay-app --sanitize-manifest        # macOS / Linux
"C:\Program Files\IHSTAY\ihstay-app.exe" --sanitize-manifest   # Windows
```

It exits immediately after rewriting `~/.claude/plugins/cache/.../plugin.json`. Idempotent — running it on an already-clean manifest is a no-op. Wire it to a `launchd`/`systemd`/Task Scheduler unit if you want it to run automatically after each `claude plugin update`.

### The HUD never appears when I trigger a permission prompt

1. Reopen the HUD (tray left-click). If it shows the "Hooks not installed" setup card, the plugin step was skipped — run the install from the card or the CLI commands above.
2. Check the plugin list: `claude plugin list` should include `ihstay`.
3. Tail `~/.claude/pending/logs/hook-errors.log` — if a hook fired but errored, the error lives here.
4. Tail `~/.claude/pending/board.jsonl` — if the hook wrote a line but the app didn't pick it up, restart the app.

### The setup card says "Claude Code not found"

The tray app couldn't find the `claude` CLI in `PATH`. Install Claude Code first, then restart the tray app (or reopen the HUD for it to re-check).

### Clicking an entry doesn't focus the right pane

1. Verify the right adapter is selected in Settings.
2. Verify the binary is in PATH: `wezterm --version` or check that iTerm2 is running on macOS.
3. On Windows, check Windows focus-steal protection — the HUD may flash the taskbar icon instead of stealing focus. Click the terminal icon to bring it forward.
4. If you run Claude Code inside an unsupported terminal (VS Code integrated terminal, Alacritty, ghostty, etc.), the ancestor walk won't find a known adapter and the click will fall through to `spawn_resume`.

### Entries never clear after I answer the prompt

The `UserPromptSubmit` hook isn't firing. Check `claude plugin list` and the contents of `~/.claude/pending/board.jsonl` — the `clear` op should appear within a second of your reply.

### The HUD appears at the wrong position on multi-monitor setups

If you unplug a monitor while a saved position is off-screen, the app resets to the tray-anchor default on next launch. If it doesn't, open **Settings… → Reset HUD position**.

### Logs

- Hook errors: `~/.claude/pending/logs/hook-errors.log`
- App logs: `~/.claude/pending/logs/app.log`
- Panic dumps: `~/.claude/pending/logs/panic.log`

Log verbosity defaults to `info`. Flip the "Debug logging" toggle in Settings for `trace`-level output.

## Uninstall

1. **Plugin** (hooks):
   ```bash
   claude plugin uninstall ihstay
   ```
   or from inside a Claude session: `/plugin uninstall ihstay`.
2. **Tray app**:
   - Windows: Settings → Apps → uninstall "IHSTAY".
   - macOS: drag `/Applications/IHSTAY.app` to Trash.
3. **State files** (optional — if you want a fully clean slate): delete `~/.claude/pending/`.

## Appendix A · Build from source

Required:

- Rust 1.83 or later (`rustup update`)
- Tauri 2 [prerequisites for your OS](https://v2.tauri.app/start/prerequisites/)
- Node.js 20+ (for the front-end build step)

Steps:

```bash
git clone https://github.com/sadwx/ihstay
cd ihstay
cargo tauri build
```

The built binary lands under `target/release/bundle/`. Copy it to a stable location and launch.

To run in dev mode with hot reload:

```bash
cargo tauri dev
```

Hook scripts live in `plugin/hooks/` (single source of truth — also what the plugin marketplace ships) and can be invoked directly while iterating — pipe a sample JSON payload into the script and check `~/.claude/pending/board.jsonl`:

```powershell
# Windows
'{"hook_event_name":"Notification","session_id":"...","cwd":"...","transcript_path":"...","notification_type":"permission_prompt","message":"Test"}' | pwsh -File plugin/hooks/pending_hook.ps1
```

```bash
# macOS / WSL
echo '{"hook_event_name":"Notification","session_id":"...","cwd":"...","transcript_path":"...","notification_type":"permission_prompt","message":"Test"}' | bash plugin/hooks/pending_hook.sh
```
