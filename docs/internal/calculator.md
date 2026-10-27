# Calculator invariants

The provider exposes the calculator through the existing `CommandProvider`
interface. Detection and search evaluate through the same query plan. A
single-query memo expires after one second or a rate revision; it never grows
with search history.

## Parsing and numeric evaluation

`grammar` preserves decimal text and symbol case. Its floating-point adapter
is reserved for bounded time offsets. Physical conversions and currency
arithmetic use `engine`, including compound expressions. `units` contains
aliases and display labels, with no conversion factors or temperature math.
Aliases are also normalized inside compound expressions.

`QueryPlan` chooses currency, time, or numeric intent. Completed invalid
currency/time requests cannot fall through and acquire a different numeric
meaning. `editing` recognizes unfinished suffixes across domains and evaluates
one complete prefix using the same plan. Missing closing delimiters may be
balanced for a preview. This uses the current input, never a previous query;
metadata and clipboard precision are taken from the preview source too.
Unfinished starts without a value use a distinct neutral `PendingState` notice.
Word operators require a right operand, while completed postfix percentages
and factorials remain valid. Unit completions use the engine's registry plus
Hayai aliases, rather than another conversion-factor table. The
picometer/PM collision is resolved by an explicit physical length target.
Input length, nesting, output size, and a shared cooperative engine deadline
bound evaluation. Recursive aggregates and comparisons share that deadline.

Currency presentation rounds decimal strings rather than integer cents or
floating-point amounts. The clipboard retains the engine value before
currency presentation rounding. Engine output precision and the precision of
the upstream FX feed still apply; this is not an accounting ledger.

Invalid calculations return an `EmptyState` notice. The list consumes notices
before selection/indexing and renders the existing empty-state layout with a
calculator icon. Enter and action shortcuts have no result to execute; a
successful query clears the notice.

The spacing and case contract is documented in the user guide and tested
through `CalcProvider`, so parser changes must preserve both automatic
detection and result behavior.

## Rate lifecycle

Each engine context captures one immutable `Arc` to a validated rate table.
A refresh cannot mix old and new rates within a calculation. Publishing a
new table increments the provider memo revision.

Creating or dropping a background job acquires no refresh state. When a job
runs, a worker-only mutex serializes refreshes. A waiting worker rechecks
freshness, then evaluates its own query. Thus cancellation before debounce
cannot strand a fetching flag, and an older query cannot consume the newer
query's completion. The launcher still discards obsolete generations.

Failed refreshes leave the previous snapshot intact and apply a 30-second
retry backoff. The rate date is separate from download time; expired cache
results carry a stale label. Rate parsing rejects nonpositive/nonfinite rates
and snapshots without the expected USD base before publication.

## Dates and clocks

All date arithmetic validates the calendar and stays within years 1–9999.
Offsets must be finite and bounded before integer conversion, and additions
are checked. Relative evaluation accepts an injected clock and timezone for
deterministic boundary tests.

Months and whole days operate on the local calendar. Hours, minutes, seconds,
and the fractional part of a day are elapsed durations, applied in UTC before
conversion back to local time. Seconds survive calculation and formatting.
Windows round-trip validation rejects nonexistent local times; ambiguous
local hours use Windows' selected offset.

Explicit abbreviations remain fixed offsets. Named places, supported IANA
identities, and matching Windows zone names use Windows rules. Native lookup
keeps at most four resolved definitions per thread for 60 seconds, avoiding
repeated enumeration without retaining the entire zone database or polling.

## Verification

Run `cargo fmt --all -- --check`,
`cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked`.
Calculator tests use in-memory rate snapshots; they require no live FX server.
Semantic tests allow two seconds per evaluation to tolerate unoptimized builds
and CI scheduling contention. Normal builds retain the 12 ms deadline; explicit
expired-budget tests verify cancellation without relying on machine speed.
Concurrency tests cover shared refresh and independent query completion.
Windows tests cover named zones and DST transitions.

For architecture changes, compare warmed idle and active RSS using release
builds of the base commit and the changed code, with the same query workload.
Record the workload and measurements in the PR. No calculator dependency was
added for this redesign.
