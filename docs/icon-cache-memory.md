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
