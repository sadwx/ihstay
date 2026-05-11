use ihstay_adapters::AdapterRegistry;
use ihstay_core::board::store::StateStore;
use ihstay_core::config::Config;
use ihstay_core::types::Entry;
use ihstay_core::visibility::{VisibilityController, WallClock};
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub store: StateStore,
    pub visibility: VisibilityController,
    pub config: Config,
    pub adapter_registry: AdapterRegistry,
    /// Set by the WSLENV setup pass when it rewrites HKCU and detects a
    /// running wezterm-gui — meaning that WezTerm is still using its old
    /// WSLENV and click-to-focus into WSL won't work until the user
    /// restarts it. The HUD reads this on init (without clearing) and
    /// the periodic check loop clears it once every PID in
    /// `stale_wezterm_pids` has exited.
    pub wezterm_stale_warning: bool,
    /// PIDs of `wezterm-gui` processes captured at the moment we set
    /// `wezterm_stale_warning`. The check loop polls these to detect when
    /// the user has actually restarted WezTerm so the warning can
    /// auto-dismiss.
    pub stale_wezterm_pids: Vec<u32>,
}

pub type SharedState = Arc<Mutex<AppState>>;

impl AppState {
    pub fn new() -> Self {
        let config = Config::load(&Config::default_path());
        let clock = Arc::new(WallClock);
        let visibility = VisibilityController::new(clock, config.clone());
        let adapter_registry = AdapterRegistry::new();

        Self {
            store: StateStore::new(),
            visibility,
            config,
            adapter_registry,
            wezterm_stale_warning: false,
            stale_wezterm_pids: Vec::new(),
        }
    }

    pub fn entries(&self) -> Vec<Entry> {
        self.store.snapshot()
    }

    #[allow(dead_code)]
    pub fn entry_count(&self) -> usize {
        self.store.len()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
