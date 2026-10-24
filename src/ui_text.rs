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
    pub const CTRL: &str = "Ctrl";
    pub const K: &str = "K";
}
