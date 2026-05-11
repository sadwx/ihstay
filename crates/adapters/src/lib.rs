#[cfg(target_os = "macos")]
pub mod iterm2;
pub mod wezterm;

use ihstay_core::terminal::TerminalAdapter;

/// Registry of available terminal adapters.
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn TerminalAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        #[allow(unused_mut)]
        let mut adapters: Vec<Box<dyn TerminalAdapter>> =
            vec![Box::new(wezterm::WezTermAdapter::new())];
        #[cfg(target_os = "macos")]
        adapters.push(Box::new(iterm2::ITerm2Adapter::new()));
        Self { adapters }
    }

    pub fn detect(
        &self,
        claude_pid: u32,
    ) -> Option<(&dyn TerminalAdapter, ihstay_core::types::TerminalMatch)> {
        for adapter in &self.adapters {
            if let Some(m) = adapter.detect(claude_pid) {
                return Some((adapter.as_ref(), m));
            }
        }
        None
    }

    pub fn get_by_name(&self, name: &str) -> Option<&dyn TerminalAdapter> {
        self.adapters
            .iter()
            .find(|a| a.name().eq_ignore_ascii_case(name))
            .map(|a| a.as_ref())
    }

    pub fn adapter_names(&self) -> Vec<&str> {
        self.adapters.iter().map(|a| a.name()).collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_wezterm() {
        let registry = AdapterRegistry::new();
        let names = registry.adapter_names();
        assert!(names.contains(&"WezTerm"));
    }

    #[test]
    fn test_registry_get_by_name() {
        let registry = AdapterRegistry::new();
        assert!(registry.get_by_name("wezterm").is_some());
        assert!(registry.get_by_name("WezTerm").is_some());
        assert!(registry.get_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_detect_returns_none_for_fake_pid() {
        let registry = AdapterRegistry::new();
        assert!(registry.detect(0xFFFFFF).is_none());
    }
}
