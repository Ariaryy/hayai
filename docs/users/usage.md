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
nov 19 11pm ET
2 weeks ago
days until 25 december
1 jan 2026 + 30 days
200 + 10%
```

Percentages added to or subtracted from a value change it by that share:
`200 + 10%` is `220`, `200 - 10%` is `180`, and `15% off 200` is `170`, while
`10% of 200` is `20`. `6 x 7` multiplies when spaced.

Dates may be written as `25 dec 2026`, `December 25, 2026`, `2026-12-25`, or
all-numeric as `25-12-26`, `25/12/2026`, or `25.12.26`. Four-digit years
work in every form (`25 dec 1900`, `15/08/1947`), and two-digit years mean
1950–2049 (`26` is 2026, `98` is 1998). Numeric dates are always read day
first; a date that only makes sense month first (`12/25/26`) is read that way.
A numeric date that is a real date is treated as a date, not as subtraction
or division.

A date alone shows its weekday. Durations move it (`1 jan 2026 + 30 days`,
`2 weeks before 25 dec 2026`), and two dates give the days between them
(`1 jan 2026 to 10 nov 2026`, `15 aug 2000 to now`). `days until 25 december`
and `days since 1 jan` count to the next or from the most recent occurrence
when the year is omitted. `noon` and `midnight` work as clock times
(`noon ist to cst`), and clock offsets accept `+ 1:30` and `+ 1h30m`.

Press `Enter` to copy a calculator result. Currency cards round for display;
the clipboard keeps the engine value before currency display rounding. Rates
are cached locally and refreshed outside the UI thread. The card shows the
rate date and marks expired cached rates when offline.

Spaces between an amount and its unit are optional: `1C to F` and `1 C to F`,
`5.5kg to lb` and `5.5 kg to lb`, `$100 to INR` and `$ 100 to INR` agree.
Clock and duration spacing works the same way: `1pm+30min`, `1 pm + 30 min`,
and `1 pm+30min` agree, as do `12:30PM IST to CST` and `12:30 PM IST to CST`.
Conversion keywords (`to`, `in`, `as`) remain separate words, except in
heights such as `5 ft 10 in to cm`. Multiword currencies and places keep
their spaces: `100 US dollars to Indian rupees`, `12pm New York to London`.

Physical symbols preserve case: `1 Mb to MB` is `0.125 MB`, while `1 MB to Mb`
is `8 Mb`. Legacy lowercase byte aliases (`kb`, `mb`, `gb`, `tb`) still mean
bytes; use `b`, `Mb`, `Gb`, `Tb` or full bit names for bits. Data rates
follow internet usage instead: `mbps`, `Mbps` and `Mbit/s` are megabits per
second, while `MBps` and `MB/s` are megabytes per second, so
`100 mbps to MB/s` gives 12.5 megabytes per second. `sq` abbreviates
`square` (`1 sq km to sq mi`). `C` denotes Celsius; a bare `c` denotes the
speed of light. In an explicit temperature
pair such as `1c to f`, the target makes the temperature meaning clear.
`k`/`K` means thousands only after an amount in a conversion (`5k km to miles`),
not in ordinary arithmetic or time offsets.

Named places follow Windows daylight-saving rules, including supported IANA
names such as `America/New_York`, major cities, single-timezone countries
(`japan`, `uk`), and the US zone names `pacific`/`PT`, `eastern`/`ET`,
`central`/`CT`, and `mountain`/`MT`. `usa` and `america` mean US Eastern
time. Explicit abbreviations such as `EST` retain their fixed offsets, as do
`utc+3`, `gmt-8`, and `utc+5:30`, also written sign first as `+5:30 gmt`.
A date may lead the clock with or without `at`, day or month first
(`25-12-26 9am utc to ist`, `nov 19 11pm ET`). `3pm in tokyo` shows the
local 3 PM in Tokyo, and `time in tokyo`, `now in tokyo`, or `tokyo time`
show the current time there.

Months and whole days use the local calendar, while hours, minutes and
seconds are elapsed durations: `in 1 day` and `in 24 hours`
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

While typing, unfinished conversions (`1usd to `, `10 km to kilome`, `26 to
he`), operators, and function arguments preview the complete part of the same
query. The preview's value, conversion labels, and clipboard value all refer
to that complete part. Completed valid units still win over guesses about
partial names. Inputs without a complete value yet, such as `sqrt(` or `in 2
ho`, show a neutral “keep typing” state. Completed invalid expressions still
show the invalid-expression screen; a different query's old result is never
reused.

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
