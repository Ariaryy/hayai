use std::cell::RefCell;
use std::rc::Rc;

use crate::commands::{CommandItem, CommandProvider};

#[derive(Clone)]
pub struct RegistryGlobal {
    registry: Rc<RefCell<PluginRegistry>>,
}

impl RegistryGlobal {
    pub fn install(cx: &mut gpui::App, registry: PluginRegistry) {
        cx.set_global(RegistryGlobal {
            registry: Rc::new(RefCell::new(registry)),
        });
    }

    pub fn clone_handle(&self) -> Rc<RefCell<PluginRegistry>> {
        self.registry.clone()
    }
}

impl gpui::Global for RegistryGlobal {}

pub struct PluginRegistry {
    providers: Vec<Box<dyn CommandProvider>>,
    /// Index into `providers` of the default (no-keyword) provider.
    default_provider: Option<usize>,
    /// Monotonic query generation. Bumped by every dispatch; async providers'
    /// results are dropped unless their generation still matches, so a slow
    /// provider can never paint results for a query the user has typed past.
    generation: u64,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            default_provider: None,
            generation: 0,
        }
    }

    pub fn register(&mut self, provider: impl CommandProvider + 'static) {
        if provider.keyword().is_none() {
            self.default_provider = Some(self.providers.len());
        }
        self.providers.push(Box::new(provider));
    }

    // Unused until the first async/debounced provider needs to check
    // whether its dispatch is still current before applying results.
    #[allow(dead_code)]
    pub fn current_generation(&self) -> u64 {
        self.generation
    }

    /// Route `raw_query` (untrimmed — the leading space is the mode sentinel)
    /// to exactly one provider and return the effective query to run. Pure
    /// routing; the caller runs the search so it can choose sync vs.
    /// debounced-async execution.
    pub fn dispatch(&mut self, raw_query: &str) -> Dispatch {
        self.generation += 1;
        let generation = self.generation;

        if let Some(provider) = self.auto_detect(raw_query) {
            return Dispatch {
                generation,
                provider,
                query: raw_query.trim().to_string(),
                debounce: self.providers[provider].wants_debounce(),
            };
        }

        if let Some(rest) = raw_query.strip_prefix(' ') {
            let mut parts = rest.splitn(2, char::is_whitespace);
            let token = parts.next().unwrap_or("");
            let remainder = parts.next().unwrap_or("").trim();
            if let Some(provider) = self.providers.iter().position(|p| {
                p.keyword()
                    .is_some_and(|kw| kw.eq_ignore_ascii_case(token))
            }) {
                return Dispatch {
                    generation,
                    provider,
                    query: remainder.to_string(),
                    debounce: self.providers[provider].wants_debounce(),
                };
            }
        }

        // No provider claimed the query (auto-detect declined, no keyword
        // matched): fall through to the default provider with the whole
        // trimmed query — a leading space must never make results vanish.
        let provider = self.default_provider.unwrap_or(0);
        Dispatch {
            generation,
            provider,
            query: raw_query.trim().to_string(),
            debounce: self
                .providers
                .get(provider)
                .is_some_and(|p| p.wants_debounce()),
        }
    }

    /// NL auto-detection stub: math/conversion providers will claim
    /// shape-matched queries here (see `PLANS.md` §3). Tried before keyword
    /// dispatch per `PLANS.md` § "Pipeline shape".
    fn auto_detect(&self, _query: &str) -> Option<usize> {
        None
    }

    pub fn search(&self, dispatch: &Dispatch) -> Vec<CommandItem> {
        self.providers[dispatch.provider].search(&dispatch.query)
    }
}

pub struct Dispatch {
    pub generation: u64,
    pub provider: usize,
    pub query: String,
    // Unused until the first async/debounced provider branches on it in
    // `perform_search` (see `src/launcher.rs`'s comment there).
    #[allow(dead_code)]
    pub debounce: bool,
}

/// Whether a result set computed for `incoming` should be dropped in favor
/// of whatever `current` already reflects.
pub fn is_stale(current: u64, incoming: u64) -> bool {
    incoming < current
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CommandAction;
    use std::cell::RefCell;

    struct FakeProvider {
        keyword: Option<&'static str>,
        queries: RefCell<Vec<String>>,
    }

    impl CommandProvider for FakeProvider {
        fn namespace(&self) -> &'static str {
            "fake"
        }

        fn keyword(&self) -> Option<&'static str> {
            self.keyword
        }

        fn search(&self, query: &str) -> Vec<CommandItem> {
            self.queries.borrow_mut().push(query.to_string());
            vec![CommandItem {
                id: "fake".into(),
                title: query.to_string(),
                subtitle: None,
                icon: crate::commands::IconSource::None,
                action: CommandAction::ShowText(query.to_string()),
            }]
        }
    }

    fn provider(keyword: Option<&'static str>) -> FakeProvider {
        FakeProvider {
            keyword,
            queries: RefCell::new(Vec::new()),
        }
    }

    #[test]
    fn keyword_routes_with_remainder() {
        let mut registry = PluginRegistry::new();
        registry.register(provider(None));
        registry.register(provider(Some("f")));

        let dispatch = registry.dispatch(" f report");
        assert_eq!(dispatch.provider, 1);
        assert_eq!(dispatch.query, "report");
    }

    #[test]
    fn keyword_with_no_remainder_routes_empty_query() {
        let mut registry = PluginRegistry::new();
        registry.register(provider(None));
        registry.register(provider(Some("f")));

        let dispatch = registry.dispatch(" f");
        assert_eq!(dispatch.provider, 1);
        assert_eq!(dispatch.query, "");
    }

    #[test]
    fn unknown_keyword_falls_through_to_default() {
        let mut registry = PluginRegistry::new();
        registry.register(provider(None));
        registry.register(provider(Some("f")));

        let dispatch = registry.dispatch(" zz report");
        assert_eq!(dispatch.provider, 0);
        assert_eq!(dispatch.query, "zz report");
    }

    #[test]
    fn no_leading_space_never_keyword_routes() {
        let mut registry = PluginRegistry::new();
        registry.register(provider(None));
        registry.register(provider(Some("f")));

        let dispatch = registry.dispatch("f report");
        assert_eq!(dispatch.provider, 0);
        assert_eq!(dispatch.query, "f report");
    }

    #[test]
    fn generations_strictly_increase() {
        let mut registry = PluginRegistry::new();
        registry.register(provider(None));

        let first = registry.dispatch("a").generation;
        let second = registry.dispatch("b").generation;
        assert!(second > first);
    }

    #[test]
    fn stale_guard() {
        assert!(is_stale(5, 3));
        assert!(!is_stale(5, 5));
        assert!(!is_stale(5, 7));
    }
}
