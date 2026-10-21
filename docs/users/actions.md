# Contextual actions

Select a result and press `Ctrl+K` to open its action panel. While the panel is
open, `Up` and `Down` move through actions, `Enter` runs the selected action,
and `Esc` closes the panel without dismissing Hayai.

**Open with…** opens a submenu of apps registered for that file type, with
icons and the same numbered shortcuts. `Esc` returns to the parent menu;
**Choose another app…** opens Windows' full application picker. Closing the
panel or hiding Hayai resets the submenu, so `Ctrl+K` starts at the parent menu.

Each action also has a numbered shortcut. Press the number shown beside an
action to run it directly, or click its row. Numbers work as action shortcuts
only while the panel is open.

Available actions depend on the selected result. Hayai currently exposes the
applicable subset of:

- open or launch the result;
- choose another application with **Open with…** for documents and other
  non-executable files;
- copy a result value or path;
- reveal a file in Explorer;
- run a supported result with administrator privileges.

The footer describes the primary `Enter` behavior for the current result.
Actions that do not apply to that result are not shown.
Folders offer **Open parent folder**; files offer **Show in Explorer**.
Packaged apps omit filesystem actions. The parent menu stays at four items or
fewer; Open with shows up to six suggested apps and the application picker.
