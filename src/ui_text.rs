// User-facing launcher copy. Keep identifiers, queries, and asset paths out of this module.

pub mod actions {
    pub const UPDATING: &str = "Updating…";
    pub const CHECKING: &str = "Checking…";
    pub const BACK_TO_SEARCH: &str = "Back to search";
    pub const UPDATE_SCRY: &str = "Update Scry Search";
    pub const CHECK_AGAIN: &str = "Check again";
    pub const RESTARTING: &str = "Restarting…";
    pub const RESTART_TO_UPDATE: &str = "Restart to update";
    pub const BACK: &str = "Back";
    pub const SEARCH_FILES: &str = "Search files";
    pub const DISMISS: &str = "Dismiss";
    pub const RETRY_UPDATE: &str = "Retry update";
    pub const VIEW_UPDATE: &str = "View update";
    pub const STARTING: &str = "Starting…";
    pub const UNAVAILABLE: &str = "Unavailable";
    pub const CHECK_FOR_UPDATES: &str = "Check for updates";
    pub const UPDATE_HAYAI: &str = "Update Hayai";
    pub const UPDATE_REQUIRED: &str = "Update required";
    pub const COPY: &str = "Copy";
    pub const OPEN_FOLDER: &str = "Open Folder";
    pub const OPEN_FILE: &str = "Open File";
    pub const SHOW: &str = "Show";
    pub const INSTALL: &str = "Install";
    pub const START: &str = "Start";
    pub const ABOUT_HAYAI: &str = "About Hayai";
    pub const OPEN_APPLICATION: &str = "Open Application";
    pub const CHOOSE_APP: &str = "Choose another app…";
    pub const MENU_OPEN_APPLICATION: &str = "Open application";
    pub const MENU_OPEN_FOLDER: &str = "Open folder";
    pub const OPEN: &str = "Open";
    pub const COPY_DIAGNOSTICS: &str = "Copy diagnostic log";
    pub const OPEN_WITH: &str = "Open with…";
    pub const OPEN_PARENT_FOLDER: &str = "Open parent folder";
    pub const SHOW_IN_EXPLORER: &str = "Show in Explorer";
    pub const COPY_PATH: &str = "Copy path";
    pub const RUN_AS_ADMIN: &str = "Run as administrator";
}

pub mod search {
    pub const FILES_PLACEHOLDER: &str = "Search files...";
    pub const APPS_PLACEHOLDER: &str = "Search apps or calculate...";
    pub const FOLDER_PLACEHOLDER: &str = "Search this folder...";
    pub const COMMAND_PLACEHOLDER: &str = "Type a command, e.g. f for files";
    pub const UP_ONE_LEVEL: &str = "Up one level";
    pub const EXIT_FILE_SEARCH: &str = "Exit file search";
    pub const CANCEL_COMMAND: &str = "Cancel command";
    pub const COMMANDS: &str = "Commands";
    pub const ENTER_FOLDER: &str = "Enter folder";
    pub const ACTIONS: &str = "Actions";
    pub const ALL_FILES: &str = "All files";
    pub const NO_FILES: &str = "No files found. Try another search.";
    pub const NO_APPS: &str = "No apps found. Try another search.";
    pub const LEARN_MORE: &str = "Learn more";
    pub const START_FAILED: &str = "Couldn't start file search";
    pub const START_RETRY: &str = "Press Enter to retry, or use Learn more for setup help.";
}

pub mod updates {
    pub const HAYAI_PROMPT: &str = "Check for a new Hayai release.";
    pub const SCRY_CHECKING: &str = "Checking Scry Search…";
    pub const SCRY_AVAILABLE: &str = "Scry Search update available";
    pub const SCRY_CURRENT: &str = "Scry Search is up to date";
    pub const SCRY_NO_RELEASE: &str = "No compatible Scry Search release found";
    pub const SCRY_CHECK_FAILED: &str = "Couldn't check Scry Search for updates";
    pub const SCRY_UPDATING: &str = "Updating Scry Search…";
    pub const SCRY_READY: &str = "File search is ready";
    pub const SCRY_REQUIRED: &str = "Scry Search update required";
    pub const SCRY_SETUP: &str = "Set up Scry Search";
    pub const CHECKING: &str = "Checking for updates…";
    pub const SCRY_TITLE: &str = "Scry Search updates";
    pub const RESTARTING: &str = "Restarting to update…";
    pub const RESTART_FAILED: &str = "Couldn't restart to update";
    pub const READY: &str = "Update ready";
    pub const HAYAI_TITLE: &str = "Hayai updates";
    pub const APPLYING: &str = "Applying the update and restarting Hayai.";
    pub const DOWNLOADING: &str = "Checking and downloading the latest release.";
    pub const RETRY_RESTART: &str = "Try restarting again.";
    pub const SCRY_UPDATING_SHORT: &str = "Updating Scry Search";
    pub const SCRY_STARTING: &str = "Starting file search";
    pub const SCRY_WAITING: &str = "Waiting for Scry Search to become ready…";
    pub const SCRY_UPDATED: &str = "Scry Search updated";
    pub const SCRY_UPDATE_FAILED: &str = "Couldn't update Scry Search";
    pub const SCRY_CHECKING_UPDATES: &str = "Checking for Scry Search updates…";
    pub const SCRY_NO_UPDATE: &str = "No compatible Scry Search update";
    pub const HAYAI_CHECKING: &str = "Checking for Hayai updates…";
    pub const HAYAI_AVAILABLE: &str = "Hayai update available";
    pub const HAYAI_CURRENT: &str = "Hayai is up to date";
    pub const HAYAI_CHECK_FAILED: &str = "Couldn't check Hayai for updates";
    pub const INSTALLING: &str = "Installation in progress.";
    pub const DISMISS_HINT: &str = "Enter to dismiss.";
    pub const RETRY_DETAILS: &str = "Open details to retry.";
    pub const SEARCH_HINT: &str = "You can keep searching.";
    pub const START_UPDATE_FAILED: &str = "Couldn't start the update. Select to retry.";
    pub const TRY_LATER: &str = "Try again later.";
    pub const MANUAL_CHECK: &str = "Manually check for a new Hayai release";
    pub const SCRY_PREPARING: &str = "Preparing the Scry Search update…";

    pub fn scry_ready_to_install(version: &str) -> String {
        format!("Scry Search {version} is ready to install.")
    }

    pub fn scry_requirement(message: &str) -> String {
        format!("File search requires a newer Scry Search version.\n{message}")
    }

    pub fn hayai_ready_to_install(version: &str) -> String {
        format!("Hayai {version} is ready to install.")
    }

    pub fn hayai_version(version: &str) -> String {
        format!("Hayai {version}")
    }
}

pub mod headings {
    pub const NOTIFICATIONS: &str = "Notifications";
    pub const HAYAI: &str = "Hayai";
    pub const APPLICATIONS: &str = "Applications";
    pub const FILES: &str = "Files";
    pub const CALCULATOR: &str = "Calculator";
}

pub mod about {
    pub const DESCRIPTION: &str = "A fast, lightweight launcher for Windows.";
    pub const PROJECT_LINK: &str = "View project on GitHub";
    pub fn version(version: &str) -> String {
        format!("Version {version}")
    }
}

pub mod keys {
    pub const SPACE: &str = "Space";
    pub const TAB: &str = "Tab";
    pub const BACKSPACE: &str = "Backspace";
}
pub mod settings {
    pub const WINDOW_TITLE: &str = "Hayai Settings";
    pub const GENERAL: &str = "General";
    pub const APPEARANCE: &str = "Appearance";
    pub const KEYBINDINGS: &str = "Key bindings";
    pub const FILE_SEARCH: &str = "File search";
    pub const ABOUT: &str = "About";
    pub const LAUNCHER_SHORTCUT: &str = "Open Hayai";
    pub const LAUNCHER_SHORTCUT_HELP: &str = "Show or hide Hayai from any app.";
    pub const SETTINGS_LOCATION: &str = "Preferences";
    pub const SETTINGS_LOCATION_HELP: &str =
        "Saved in your local Hayai app data folder and restored on startup.";
    pub const APPEARANCE_DESCRIPTION: &str =
        "Choose how Hayai looks. Applies to the launcher and settings.";
    pub const PREFERENCES_FILE: &str = "Preferences file";
    pub const COLOR_SCHEME: &str = "Color scheme";
    pub const THEMES: &str = "Themes";
    pub const SEARCH_ENGINE: &str = "Search engine";
    pub const SCRY_NAME: &str = "Scry Search";
    pub const WINDOWS_INDEX: &str = "Windows index";
    pub const SYSTEM_SCOPE: &str = "Available system-wide";
    pub const LAUNCHER_SCOPE: &str = "When the launcher is focused";
    pub const DARK: &str = "Dark";
    pub const LIGHT: &str = "Light";
    pub const KEYBINDINGS_DESCRIPTION: &str =
        "Click a shortcut and press its replacement. Changes save automatically.";
    pub const SYSTEM_GROUP: &str = "System";
    pub const LAUNCHER_GROUP: &str = "Launcher";
    pub const PRESS_SHORTCUT: &str = "Press a shortcut…";
    pub const RESET: &str = "Reset";
    pub const RECORDING_HELP: &str = "Press a key combination. Esc cancels.";
    pub const FILE_DESCRIPTION: &str =
        "File search is powered by Scry Search, using its Windows index.";
    pub const FILE_SEARCH_HELP: &str =
        "Open file search directly, or type Space then f in the launcher.";
    pub const FILE_GUIDE: &str = "File search setup and search syntax";
    pub const SAVING: &str = "Saving…";
    pub const MODIFIER_REQUIRED: &str = "Include Ctrl, Alt or Win so typing keeps working.";
    pub const RESERVED_KEY: &str =
        "This key is reserved for editing, navigation or closing windows.";
    pub const CONFLICT: &str = "Another action already uses this shortcut.";
    pub const GLOBAL_SUPPORTED: &str =
        "Use a letter, digit, function key or Space for the system shortcut.";
    pub const LOAD_FAILED: &str = "Could not load preferences. Using defaults.";
    pub const SAVE_FAILED: &str = "Could not save preferences.";
    pub const HOTKEY_FAILED: &str =
        "Windows could not register that shortcut. It may belong to another app.";
    pub const OPEN_ACTIONS: &str = "Toggle action panel";
    pub const OPEN_SETTINGS: &str = "Open settings";
    pub const SETTINGS_ITEM: &str = "Settings";
    pub const THIRD_PARTY_NOTICES: &str = "Third-party notices";
    pub fn theme_label(family: crate::theme::ThemeFamily) -> &'static str {
        match family {
            crate::theme::ThemeFamily::Hayai => "Hayai",
        }
    }
    pub fn shortcut_label(action: crate::settings::Shortcut) -> &'static str {
        match action {
            crate::settings::Shortcut::ToggleLauncher => LAUNCHER_SHORTCUT,
            crate::settings::Shortcut::Actions => OPEN_ACTIONS,
            crate::settings::Shortcut::FileSearch => FILE_SEARCH,
            crate::settings::Shortcut::Settings => OPEN_SETTINGS,
        }
    }
}
