//! Watch `~/.claude/plugins/cache/` for marketplace install / update events
//! that touch the `ihstay` plugin and re-run the
//! `plugin.json` sanitizer when one fires.
//!
//! Background: Claude Code 2.1.x ignores the `platform` field on hook
//! entries, so the bundled `plugin.json` ships one entry per OS for each
//! event. Without sanitizing, `/hooks` lists every command (pwsh on
//! macOS/Linux, bash on Windows) and Claude Code attempts to spawn each
//! one — every fire ENOENTs on the wrong-OS commands. The
//! `plugin_install::sanitize_installed_plugin_json` routine strips
//! foreign-platform entries from the on-disk `plugin.json`, but it only
//! ran at app boot and after a tray-driven `Install plugin` click.
//! `claude plugin update` (or marketplace auto-update) triggered while the
//! tray app is already running was unhandled; this watcher closes that gap.
//!
//! Mechanism: we watch the cache dir recursively with `notify` and
//! coalesce events through `notify-debouncer-full` (1.5 s window), so a
//! single install — which fires dozens of file events as Claude Code
//! unpacks the version dir tree — produces exactly one sanitize call.

use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub const PLUGIN_NAME: &str = "ihstay";

/// Debounce window. Must be long enough that a full `claude plugin install`
/// settles within it (creating a version dir tree fires many events) and
/// short enough that the user doesn't perceive lingering stale entries in
/// `/hooks` after an update.
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(1500);

pub type OnChange = Arc<dyn Fn() + Send + Sync + 'static>;

/// Hold the returned watcher in scope (or `mem::forget` it) for as long as
/// you want the auto-sanitize behavior to run. Dropping this stops both
/// the underlying file watcher and the debounce thread.
pub struct PluginCacheWatcher {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

impl PluginCacheWatcher {
    /// Watch the user's default plugin cache (`~/.claude/plugins/cache/`).
    pub fn start_default(on_change: OnChange) -> Result<Self, String> {
        let home = dirs_next::home_dir().ok_or_else(|| "no home dir".to_string())?;
        let cache_root = home.join(".claude").join("plugins").join("cache");
        Self::start(&cache_root, on_change)
    }

    /// Watch a specific cache root. Creates the directory if it doesn't
    /// exist (Claude Code creates it on first plugin install, but starting
    /// before that should still work — the watcher will see the eventual
    /// install events when they land).
    pub fn start(cache_root: &Path, on_change: OnChange) -> Result<Self, String> {
        std::fs::create_dir_all(cache_root).map_err(|e| format!("create {cache_root:?}: {e}"))?;

        let mut debouncer =
            new_debouncer(DEBOUNCE_WINDOW, None, move |result: DebounceEventResult| {
                let events = match result {
                    Ok(evs) => evs,
                    Err(errs) => {
                        for e in errs {
                            tracing::warn!(error = %e, "plugin cache watcher error");
                        }
                        return;
                    }
                };

                // Only react to events whose paths mention our plugin.
                // Filtering here (instead of at watch-scope) means the same
                // watcher could in principle be reused for multiple
                // plugins; for now there's just one.
                let qualifies = events.iter().any(|de| {
                    matches!(
                        de.event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) && de
                        .event
                        .paths
                        .iter()
                        .any(|p| p.components().any(|c| c.as_os_str() == PLUGIN_NAME))
                });
                if !qualifies {
                    return;
                }
                on_change();
            })
            .map_err(|e| format!("create debouncer: {e}"))?;

        debouncer
            .watch(cache_root, RecursiveMode::Recursive)
            .map_err(|e| format!("watch {cache_root:?}: {e}"))?;

        tracing::info!(?cache_root, "plugin cache watcher started");
        Ok(Self {
            _debouncer: debouncer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tempfile::TempDir;

    fn fire_count_callback() -> (Arc<AtomicU32>, OnChange) {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_for_cb = counter.clone();
        let cb: OnChange = Arc::new(move || {
            counter_for_cb.fetch_add(1, Ordering::SeqCst);
        });
        (counter, cb)
    }

    #[test]
    fn fires_on_plugin_subdir_creation() {
        let dir = TempDir::new().unwrap();
        let cache_root = dir.path().to_path_buf();
        let (counter, cb) = fire_count_callback();
        let _w = PluginCacheWatcher::start(&cache_root, cb).unwrap();

        // Give the watcher a beat to attach to the dir before we mutate.
        std::thread::sleep(Duration::from_millis(200));

        // Simulate `claude plugin install` creating a version dir tree.
        let plugin_dir = cache_root
            .join(PLUGIN_NAME)
            .join(PLUGIN_NAME)
            .join("0.2.5")
            .join(".claude-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("plugin.json"), "{}").unwrap();

        // Wait long enough for debounce to fire (window + slack).
        std::thread::sleep(DEBOUNCE_WINDOW + Duration::from_millis(1000));
        assert!(
            counter.load(Ordering::SeqCst) >= 1,
            "callback should have fired at least once"
        );
    }

    #[test]
    fn ignores_changes_outside_our_plugin() {
        let dir = TempDir::new().unwrap();
        let cache_root = dir.path().to_path_buf();
        let (counter, cb) = fire_count_callback();
        let _w = PluginCacheWatcher::start(&cache_root, cb).unwrap();
        std::thread::sleep(Duration::from_millis(200));

        let other = cache_root.join("some-other-plugin").join("subdir");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("plugin.json"), "{}").unwrap();

        std::thread::sleep(DEBOUNCE_WINDOW + Duration::from_millis(1000));
        assert_eq!(
            counter.load(Ordering::SeqCst),
            0,
            "callback must not fire for unrelated plugin activity"
        );
    }

    #[test]
    fn coalesces_burst_into_single_call() {
        let dir = TempDir::new().unwrap();
        let cache_root = dir.path().to_path_buf();
        let (counter, cb) = fire_count_callback();
        let _w = PluginCacheWatcher::start(&cache_root, cb).unwrap();
        std::thread::sleep(Duration::from_millis(200));

        // Many writes within the debounce window.
        let plugin_dir = cache_root.join(PLUGIN_NAME).join(PLUGIN_NAME).join("1.0.0");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        for i in 0..20 {
            let p = plugin_dir.join(format!("file-{i}.txt"));
            std::fs::write(&p, "x").unwrap();
        }

        std::thread::sleep(DEBOUNCE_WINDOW + Duration::from_millis(1000));
        let n = counter.load(Ordering::SeqCst);
        // Tolerate 1–2 fires: notify on Windows occasionally splits a
        // single logical event sequence across two debounce windows.
        // What we strictly want to avoid is "one fire per file" (~20).
        assert!(
            (1..=2).contains(&n),
            "expected coalesced fire count, got {n}"
        );
    }
}
