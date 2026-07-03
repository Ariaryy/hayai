#![allow(dead_code)]

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub enum CommandAction {
    LaunchApplication(PathBuf),
    OpenFile(PathBuf),
    CopyToClipboard(String),
    ShowText(String),
}

#[derive(Clone, Debug)]
pub struct CommandItem {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub action: CommandAction,
}

pub trait CommandProvider {
    fn namespace(&self) -> &'static str;
    fn search(&self, query: &str) -> Vec<CommandItem>;
}
