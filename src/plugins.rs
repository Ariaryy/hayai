#![allow(dead_code)]

use crate::commands::CommandProvider;

pub trait Plugin: CommandProvider {
    fn id(&self) -> &'static str;
    fn title(&self) -> &'static str;
}

#[derive(Default)]
pub struct PluginRegistry {
    providers: Vec<Box<dyn CommandProvider>>,
}

impl PluginRegistry {
    pub fn register(&mut self, provider: impl CommandProvider + 'static) {
        self.providers.push(Box::new(provider));
    }

    pub fn providers(&self) -> &[Box<dyn CommandProvider>] {
        &self.providers
    }
}
