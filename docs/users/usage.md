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

Press `Enter` to copy a calculator result. Currency cards round for display;
the clipboard keeps the engine value before currency display rounding. Rates
are cached locally and refreshed outside the UI thread. The card shows the
rate date and marks expired cached rates when offline.

Spaces between an amount and its unit are optional: `1C to F` and `1 C to F`,
`5.5kg to lb` and `5.5 kg to lb`, `$100 to INR` and `$ 100 to INR` agree.
Clock and duration spacing works the same way: `1pm+30min`, `1 pm + 30 min`,
and `1 pm+30min` agree. Conversion keywords (`to`, `in`, `as`) remain separate
words. Multiword currencies and places keep their spaces: `100 US dollars to
Indian rupees`, `12pm New York to London`.

Physical symbols preserve case: `1 Mb to MB` is `0.125 MB`, while `1 MB to Mb`
is `8 Mb`. Legacy lowercase byte aliases (`kb`, `mb`, `gb`, `tb`) still mean
bytes; use `b`, `Mb`, `Gb`, `Tb` or full bit names for bits. `C` denotes
Celsius; a bare `c` denotes the speed of light. In an explicit temperature
pair such as `1c to f`, the target makes the temperature meaning clear.
`k`/`K` means thousands only after an amount in a conversion (`5k km to miles`),
not in ordinary arithmetic or time offsets.

Named places follow Windows daylight-saving rules, including supported IANA
names such as `America/New_York`; explicit abbreviations such as `EST` retain
their fixed offsets. Months and whole days use the local calendar, while
hours, minutes and seconds are elapsed durations: `in 1 day` and `in 24 hours`
can differ across DST. Fractional days contribute elapsed hours. Seconds are
retained, and clock arithmetic shows a day change when it crosses midnight.
Invalid dates, nonexistent local times, and offsets outside the supported
calendar range (years 1–9999) show an empty state with a calculator icon and a
short message. Invalid expressions have no selectable result or copy action. During a repeated local hour,
Windows chooses the offset.

Currency recognition includes the expression engine's ISO code set; actual
conversion availability depends on the rate feed. Lowercase `cup` remains a
kitchen unit; use uppercase `CUP` for an isolated Cuban peso amount. Grouped and scientific
amounts (`1,000 USD`, `1e3 USD`) and compound expressions (`100 USD + 50 EUR to
INR`) use the same rate cache. Currency displays follow common zero-, two-,
and three-decimal minor units and preserve nonzero amounts smaller than a
minor unit.

Calculations have input, nesting, output, and execution limits to keep the
launcher responsive. Use `Space` → `=` for an explicit calculator request
when automatic detection declines an expression.

## Search history

With an empty query, press `Up` to recall earlier searches and calculations.
Use `Up` and `Down` to move through history; typing a new query returns to
normal result navigation.

## Contextual actions

Press `Ctrl+K` on the selected result to open its available actions. See the
[action reference](actions.md).
