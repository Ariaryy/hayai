use super::*;

// A footer action is an identity, not its editable display label.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PrimaryAction {
    Updating,
    Checking,
    BackToSearch,
    UpdateScry,
    CheckAgain,
    Restarting,
    RestartToUpdate,
    Back,
    SearchFiles,
    Dismiss,
    RetryUpdate,
    ViewUpdate,
    Starting,
    Unavailable,
    CheckForUpdates,
    UpdateHayai,
    UpdateRequired,
    Copy,
    OpenFolder,
    OpenFile,
    Show,
    Install,
    Start,
    AboutHayai,
    OpenApplication,
    None,
}

impl PrimaryAction {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Updating => text::actions::UPDATING,
            Self::Checking => text::actions::CHECKING,
            Self::BackToSearch => text::actions::BACK_TO_SEARCH,
            Self::UpdateScry => text::actions::UPDATE_SCRY,
            Self::CheckAgain => text::actions::CHECK_AGAIN,
            Self::Restarting => text::actions::RESTARTING,
            Self::RestartToUpdate => text::actions::RESTART_TO_UPDATE,
            Self::Back => text::actions::BACK,
            Self::SearchFiles => text::actions::SEARCH_FILES,
            Self::Dismiss => text::actions::DISMISS,
            Self::RetryUpdate => text::actions::RETRY_UPDATE,
            Self::ViewUpdate => text::actions::VIEW_UPDATE,
            Self::Starting => text::actions::STARTING,
            Self::Unavailable => text::actions::UNAVAILABLE,
            Self::CheckForUpdates => text::actions::CHECK_FOR_UPDATES,
            Self::UpdateHayai => text::actions::UPDATE_HAYAI,
            Self::UpdateRequired => text::actions::UPDATE_REQUIRED,
            Self::Copy => text::actions::COPY,
            Self::OpenFolder => text::actions::OPEN_FOLDER,
            Self::OpenFile => text::actions::OPEN_FILE,
            Self::Show => text::actions::SHOW,
            Self::Install => text::actions::INSTALL,
            Self::Start => text::actions::START,
            Self::AboutHayai => text::actions::ABOUT_HAYAI,
            Self::OpenApplication => text::actions::OPEN_APPLICATION,
            Self::None => "",
        }
    }
}

#[derive(Clone)]
pub(super) struct ActionMenu {
    pub(super) diagnostics_only: bool,
    pub(super) notification: bool,
    pub(super) notification_action: &'static str,
    pub(super) submenu: Option<OpenWithMenu>,
    pub(super) path: PathBuf,
    pub(super) is_dir: bool,
    pub(super) is_app: bool,
    pub(super) can_run_as_admin: bool,
    pub(super) selected: usize,
}

#[derive(Clone)]
pub(super) struct OpenWithApp {
    pub(super) id: String,
    pub(super) name: SharedString,
    pub(super) icon: Option<Arc<RenderImage>>,
}

#[derive(Clone)]
pub(super) struct OpenWithMenu {
    pub(super) request: u64,
    pub(super) loading: bool,
    pub(super) apps: Arc<[OpenWithApp]>,
}

#[derive(Clone, Copy)]
pub(super) enum MenuAction {
    CopyDiagnosticLog,
    DismissNotification,
    Open,
    OpenWith,
    AssociatedApp(usize),
    ChooseApp,
    Reveal,
    CopyPath,
    RunAsAdmin,
}

impl ActionMenu {
    pub(super) fn return_to_parent(&mut self) -> bool {
        if self.submenu.take().is_none() {
            return false;
        }
        self.selected = 0;
        true
    }
    pub(super) fn count(&self) -> usize {
        match &self.submenu {
            Some(submenu) if submenu.loading => self.actions().count(),
            Some(submenu) => submenu.apps.len() + 1,
            None => self.actions().count(),
        }
    }

    pub(super) fn row(
        &self,
        index: usize,
    ) -> Option<(MenuAction, SharedString, Option<Arc<RenderImage>>)> {
        if let Some(submenu) = &self.submenu
            && !submenu.loading
        {
            if let Some(app) = submenu.apps.get(index) {
                return Some((
                    MenuAction::AssociatedApp(index),
                    app.name.clone(),
                    app.icon.clone(),
                ));
            }
            return (index == submenu.apps.len()).then(|| {
                (
                    MenuAction::ChooseApp,
                    text::actions::CHOOSE_APP.into(),
                    None,
                )
            });
        }
        self.actions()
            .nth(index)
            .map(|(action, label)| (action, label.into(), None))
    }
    /// One source of truth for visible rows, number shortcuts, and navigation.
    /// Open-with and elevation are mutually exclusive, keeping this to four rows.
    pub(super) fn actions(&self) -> impl Iterator<Item = (MenuAction, &'static str)> + '_ {
        let local_path = self.path.is_absolute();
        let open_with = local_path
            && !self.is_app
            && !self.is_dir
            && !self.can_run_as_admin
            && !self.path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("lnk") || ext.eq_ignore_ascii_case("url")
            });
        [
            (!self.diagnostics_only).then_some((
                MenuAction::Open,
                if self.notification {
                    self.notification_action
                } else if self.is_app {
                    text::actions::MENU_OPEN_APPLICATION
                } else if self.is_dir {
                    text::actions::MENU_OPEN_FOLDER
                } else {
                    text::actions::OPEN
                },
            )),
            (self.notification && !self.diagnostics_only)
                .then_some((MenuAction::DismissNotification, text::actions::DISMISS)),
            (self.notification || self.diagnostics_only).then_some((
                MenuAction::CopyDiagnosticLog,
                text::actions::COPY_DIAGNOSTICS,
            )),
            open_with.then_some((MenuAction::OpenWith, text::actions::OPEN_WITH)),
            local_path.then_some((
                MenuAction::Reveal,
                if self.is_dir {
                    text::actions::OPEN_PARENT_FOLDER
                } else {
                    text::actions::SHOW_IN_EXPLORER
                },
            )),
            local_path.then_some((MenuAction::CopyPath, text::actions::COPY_PATH)),
            self.can_run_as_admin
                .then_some((MenuAction::RunAsAdmin, text::actions::RUN_AS_ADMIN)),
        ]
        .into_iter()
        .flatten()
    }
}

pub(super) fn can_run_as_admin(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some(extension)
            if extension.eq_ignore_ascii_case("exe")
                || extension.eq_ignore_ascii_case("com")
                || extension.eq_ignore_ascii_case("bat")
                || extension.eq_ignore_ascii_case("cmd")
                || extension.eq_ignore_ascii_case("msi")
    )
}

impl LauncherRoot {
    pub(super) fn selected_action_menu(&self, cx: &App) -> Option<ActionMenu> {
        let delegate = self.list.read(cx).delegate();
        if delegate.show_updates || delegate.show_scry_updates || delegate.needs_pinned_scry() {
            return Some(ActionMenu {
                diagnostics_only: true,
                notification: false,
                notification_action: "",
                submenu: None,
                path: PathBuf::new(),
                is_dir: false,
                is_app: false,
                can_run_as_admin: false,
                selected: 0,
            });
        }
        if delegate.calculator_view() {
            return None;
        }
        if delegate.search_pending {
            return None;
        }
        if delegate.notification_selected && delegate.notification_visible() {
            return Some(ActionMenu {
                diagnostics_only: false,
                notification: true,
                notification_action: self.selected_primary_action(cx).label(),
                submenu: None,
                path: PathBuf::new(),
                is_dir: false,
                is_app: false,
                can_run_as_admin: false,
                selected: 0,
            });
        }
        delegate
            .selected_index
            .and_then(|ix| delegate.item(ix))
            .and_then(|item| match &item.action {
                CommandAction::OpenFile(path) => Some(ActionMenu {
                    diagnostics_only: false,
                    notification: false,
                    notification_action: "",
                    submenu: None,
                    path: path.clone(),
                    is_dir: path.is_dir(),
                    is_app: false,
                    can_run_as_admin: can_run_as_admin(path),
                    selected: 0,
                }),
                CommandAction::LaunchApplication(path) => Some(ActionMenu {
                    diagnostics_only: false,
                    notification: false,
                    notification_action: "",
                    submenu: None,
                    path: path.clone(),
                    is_dir: false,
                    is_app: true,
                    can_run_as_admin: path.is_absolute()
                        && (can_run_as_admin(path)
                            || path
                                .extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))),
                    selected: 0,
                }),
                CommandAction::CheckForUpdates => Some(ActionMenu {
                    diagnostics_only: true,
                    notification: false,
                    notification_action: "",
                    submenu: None,
                    path: PathBuf::new(),
                    is_dir: false,
                    is_app: false,
                    can_run_as_admin: false,
                    selected: 0,
                }),
                CommandAction::CopyToClipboard(_)
                | CommandAction::ShowText(_)
                | CommandAction::EmptyState
                | CommandAction::PendingState
                | CommandAction::InstallFileSearch
                | CommandAction::StartFileSearch
                | CommandAction::UpdateFileSearch
                | CommandAction::CheckFileSearchUpdates
                | CommandAction::AboutHayai
                | CommandAction::RestartToUpdate => None,
            })
    }

    pub(super) fn selected_menu_action(&self) -> Option<MenuAction> {
        let menu = self.action_menu.as_ref()?;
        menu.row(menu.selected).map(|(action, _, _)| action)
    }

    pub(super) fn move_menu_selection(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.action_menu.as_mut() else {
            return false;
        };
        let count = menu.count();
        if count == 0 {
            return true;
        }
        menu.selected = (menu.selected as isize + delta).rem_euclid(count as isize) as usize;
        cx.notify();
        true
    }

    pub(super) fn run_menu_action(
        &mut self,
        action: MenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(action, MenuAction::OpenWith) {
            self.open_with_submenu(window, cx);
            return;
        }
        let Some(menu) = self.action_menu.take() else {
            return;
        };
        match action {
            MenuAction::CopyDiagnosticLog => {
                let delegate = self.list.read(cx).delegate();
                let combined = !delegate.show_updates
                    && !delegate.show_scry_updates
                    && !delegate.notification_selected
                    && delegate
                        .selected_index
                        .and_then(|ix| delegate.item(ix))
                        .is_some_and(|item| matches!(item.action, CommandAction::CheckForUpdates));
                let scry = delegate.show_scry_updates
                    || delegate.selected_notification() == Some(UpdateNotification::Scry);
                let log = if combined {
                    format!(
                        "=== Hayai ===\n{}\n=== Scry Search ===\n{}",
                        crate::updates::DIAGNOSTICS.snapshot(),
                        crate::scry_updates::diagnostic_snapshot()
                    )
                } else if scry {
                    crate::scry_updates::diagnostic_snapshot()
                } else {
                    crate::updates::DIAGNOSTICS.snapshot()
                };
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(log));
            }
            MenuAction::DismissNotification => {
                self.list.update(cx, |list, cx| {
                    if list.delegate().selected_notification() == Some(UpdateNotification::Scry) {
                        list.delegate_mut().scry_update_dismissed = true;
                    } else {
                        list.delegate_mut().update_dismissed = true;
                    }
                    list.delegate_mut().notification_selected = false;
                    let ix = (!list.delegate().results.is_empty())
                        .then(|| list.delegate().index_path(0));
                    let measured = list.delegate().index_path(0);
                    list.set_item_to_measure_index(measured, window, cx);
                    list.set_selected_index(ix, window, cx);
                    cx.notify();
                });
            }
            MenuAction::Open => self.list.update(cx, |list, cx| {
                list.delegate_mut().confirm(false, window, cx);
            }),
            MenuAction::ChooseApp => {
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::open_with(&menu.path);
                })
                .detach();
            }
            MenuAction::AssociatedApp(index) => {
                if let Some(app) = menu
                    .submenu
                    .and_then(|submenu| submenu.apps.get(index).cloned())
                {
                    LauncherState::dismiss(window, cx);
                    cx.background_spawn(async move {
                        native::open_with_app(&menu.path, &app.id);
                    })
                    .detach();
                }
            }
            MenuAction::OpenWith => unreachable!(),
            MenuAction::CopyPath => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                    menu.path.to_string_lossy().into_owned(),
                ));
                LauncherState::dismiss(window, cx);
            }
            MenuAction::Reveal => {
                let path = menu.path.parent().unwrap_or(&menu.path).to_path_buf();
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::launch_path(&path);
                })
                .detach();
            }
            MenuAction::RunAsAdmin if menu.can_run_as_admin => {
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::launch_path_as_admin(&menu.path);
                })
                .detach();
            }
            MenuAction::RunAsAdmin => {}
        }
    }

    fn open_with_submenu(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.action_menu.as_mut() else {
            return;
        };
        if menu.submenu.as_ref().is_some_and(|submenu| submenu.loading) {
            return;
        }
        self.menu_request = self.menu_request.wrapping_add(1);
        let request = self.menu_request;
        let path = menu.path.clone();
        menu.submenu = Some(OpenWithMenu {
            request,
            loading: true,
            apps: Arc::from([]),
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let apps = cx
                .background_spawn(async move {
                    let apps = native::associated_apps(&path)
                        .into_iter()
                        .map(|app| OpenWithApp {
                            id: app.id,
                            name: app.name.into(),
                            icon: app.icon.and_then(|icon| {
                                let pixels =
                                    image::RgbaImage::from_raw(icon.width, icon.height, icon.bgra)?;
                                Some(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
                            }),
                        })
                        .collect::<Arc<[OpenWithApp]>>();
                    (path, apps)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(menu) = this.action_menu.as_mut()
                    && menu.path == apps.0
                    && let Some(submenu) = menu.submenu.as_mut()
                    && submenu.request == request
                {
                    submenu.apps = apps.1;
                    submenu.loading = false;
                    menu.selected = 0;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
