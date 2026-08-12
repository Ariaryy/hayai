# Use Hayai

Press `Alt+Space` to open Hayai. The window appears on the monitor containing
the pointer and focuses the search field. Press the same shortcut again, press
`Esc`, or focus another window to return Hayai to the tray.

## Applications

Type an application name. Hayai searches Start Menu shortcuts and packaged
Microsoft Store apps, with recent launches shown first when the query is empty.
Use `Up` and `Down` to select a result and `Enter` to launch it.

## Files

Hayai reserves a literal leading space for command modes. Press `Space`, then
type the provider keyword; `f` is the current file-search keyword. In other
words, `Space` → `f` enters Scry-powered file mode. The same router can support
future keywords such as `Space` → `cp` without treating ordinary application
queries as commands.

The `␠` symbol below represents the one leading space and is not typed:

```text
␠f invoice
␠f projects annual report ext:pdf
␠f type:dir modified:<7d
```

Hayai also recognizes a directory scope before the query. The exact matching
and ranking are provided by Scry. See [File search](file-search.md) and Scry's
[query-language reference](https://github.com/Ariaryy/scry-search/blob/main/docs/users/search-syntax.md).

## Calculator

Calculator-shaped queries are detected automatically. Its explicit provider
keyword uses the same leading-space command router when needed.

```text
2 * (7 + 3)
5k km to miles
72 f to c
100 usd to inr
0xff to decimal
1pm + 5
3pm est to ist
2 weeks ago
```

Press `Enter` to copy the displayed calculator value. Currency results use a
locally cached rate when available and refresh rates outside the UI thread.

## Search history

With an empty query, press `Up` to recall earlier searches and calculations.
Use `Up` and `Down` to move through history; typing a new query returns to
normal result navigation.

## Contextual actions

Press `Ctrl+K` on the selected result to open its available actions. See the
[action reference](actions.md).
