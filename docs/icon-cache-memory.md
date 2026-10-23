# Bounded icon cache measurement

Scope: opportunity 1 only. The catalog now retains at most 128 entries,
including failures and pending decodes, with least recently used completed
entries evicted first. Pending requests cannot be evicted or duplicated.
Eviction also calls `Window::drop_image` to release the corresponding sprite
atlas entry. Atlas page reclamation itself remains GPUI's responsibility.
No dependencies were added. Results remain uncapped and icons decode asynchronously.
Hits move an existing path in a small deque without allocating another path.

## Measurement (Windows, 2026-10-23)

Command: `cargo test --release icon_cache_memory -- --ignored --nocapture --test-threads=1`

The same ignored test ran before and after the production change, each in a
fresh optimized test process. It requests and completes 1,000 distinct synthetic
48x48 RGBA images, writing every pixel, then keeps the catalog alive while
PowerShell reads WorkingSet64 (RSS) and PrivateMemorySize64 for the test PID.
There is no window, shell extraction, or GPU upload in this controlled workload.

Before: empty RSS 8,347,648 bytes; populated RSS 18,300,928 bytes;
increase 9,953,280 bytes (9.49 MiB). Private bytes increased from 1,560,576
to 11,485,184. Retained entries: 1,000.

After: empty RSS 8,355,840 bytes; populated RSS 9,789,440 bytes;
increase 1,433,600 bytes (1.37 MiB). Private bytes increased from 1,605,632
to 2,961,408. Retained entries: 128.

RSS growth fell by 8,519,680 bytes (8.13 MiB, 85.6%) in this workload.
These are single-run CPU measurements, not full-launcher idle/active RSS or GPU
memory measurements. Visible rows can hold additional image references briefly;
128 entries is a catalog bound, not a whole-process byte limit. Revisiting an
evicted icon requires another background decode. Manual scrolling and full-app
idle/active/GPU measurements remain unverified.

## Verification

`cargo test --release`: 124 passed, 0 failed, 2 ignored (manual memory workload
and live Scry integration). Regression tests cover saturation with pending
requests, failed-result eviction, recency, unsolicited completions, renderer
cleanup notification, and release of an evicted image's final reference.

## Follow-up: prefilter row icons for display DPI

The initial DPI follow-up kept rows at 22 logical pixels and used area averaging
to reduce decoded icons to ceil(22 * window scale) physical pixels: 22 at 100%,
28 at 125%, 33 at 150%, and 44 at 200%. Small sources are not upscaled on the CPU.
This avoids relying on the renderer's bilinear sampler to minify 48px images;
the pinned Windows atlas has one mip level. The cache keeps one size per path,
replacing it on the next request after a DPI change. An old pending request
finishes first, then its rendition is replaced; completions notify the list.

Same 1,000-source workload, with the 128-entry bound enabled in both cases:

- 48px retained images: empty RSS 8,359,936 bytes; populated 9,826,304;
  growth 1,466,368 bytes. Private bytes: 1,601,536 to 2,977,792.
- 22px retained images: empty RSS 8,355,840 bytes; populated 8,736,768;
  growth 380,928 bytes. Private bytes: 1,593,344 to 1,863,680.

RSS growth decreased by 1,085,440 bytes (1.04 MiB) in this single-run workload.
Retained pixel data per 48px source falls from 9,216 to 1,936 bytes at 100% DPI
(79% less). This is CPU cache memory, not full-app idle/active RSS or GPU usage.

To reproduce the second case in PowerShell, set
`$env:HAYAI_ICON_BENCH_PIXELS = '22'`, run the same ignored release test above,
then `Remove-Item Env:HAYAI_ICON_BENCH_PIXELS`. The default is 48 for comparison.

Regression tests cover DPI changes while decoding and averaging a fine
checkerboard into a uniform icon, rather than aliasing its alternating pixels.
`cargo test --release`: 126 passed, 2 ignored. Manual visual checks on real app
icons across monitors remain unverified. Sources still come from the existing
48px shell extraction, so this change does not add detail for scales above 200%.

## Revision: scroll prefetch and sharper fractional-DPI filtering

The box average looked soft at 125-150% DPI. Row preparation now uses Lanczos3
at the same target dimensions, preserving edge contrast with a wider reconstruction
filter. DPI cache handling and the 128-entry limit are unchanged.

Each virtualized result row warms up to 16 neighboring rows in either direction.
Prefetch protects a region of 32 rows on either side of that rendered row from
its own evictions, and does not promote speculative cache hits over demand hits.
It stops admitting speculative loads when eight decodes are pending, leaving
visible-row requests unrestricted by that speculative threshold. Completion
repaints refill nearby icons without timers or a retained work queue. The bound
still includes all pending and completed entries. A fast jump into uncached
results or the first cold open can still briefly show missing icons; prefetch
cannot guarantee instantaneous shell extraction.

The same controlled 1,000-icon workload after this revision retained 128 entries:

- 125% / 28px: empty RSS 8,355,840 bytes; populated RSS 9,162,752 bytes;
  growth 806,912 bytes (0.77 MiB). Private bytes: 1,605,632 to 2,277,376.
- 150% / 33px: empty RSS 8,347,648 bytes; populated RSS 9,306,112 bytes;
  growth 958,464 bytes (0.91 MiB). Private bytes: 1,593,344 to 2,416,640.

Use HAYAI_ICON_BENCH_PIXELS=28 or 33 with the ignored test to reproduce. These
measure the retained CPU cache and filter allocations, not full-window idle/active
RSS, speculative task peaks, scroll latency, GPU storage, or perceived sharpness.
The previous 22px box-filter measurement is historical, not a like-for-like
fractional-DPI comparison. Full-app visual validation remains outstanding.

`cargo test --release`: 129 passed, 2 ignored. Added tests check protected-region
eviction, no LRU promotion on prefetch hits, speculative admission limits with
visible-request priority, and neighborhood bounds at the start/end of results.
The fine checkerboard test continues to verify antialiasing in the image interior.
