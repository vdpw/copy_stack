# History Performance Baseline

## Scope

The storage harness creates a temporary on-disk database from deterministic
synthetic clipboard events. It never reads the system clipboard, a user
database, or a user file path. Its mixed fixture rotates through plain text,
HTML, embedded PNG bytes, generated local image and video files, and generated
file/folder URLs. The temporary database and media tree are removed after each
case.

The fixture version is included in every JSON result. Change that version when
the workload shape changes so results from incompatible workloads are not
compared accidentally.

## Run

Run the complete 100/1000 text and mixed matrix in release mode:

```bash
scripts/perf-history.sh
```

The command prints one JSON record per case. Save those records together with:

- commit SHA;
- macOS and hardware model;
- Rust toolchain;
- whether the run was on battery or external power;
- a note about other heavy foreground workloads.

Timing fields are observations, not unit-test assertions:

- `cold_startup_micros`
- `seed_micros`
- `first_page_micros`
- `full_page_walk_micros`
- `detail_load_micros`
- `single_capture_micros`
- `duplicate_capture_micros`
- `tray_snapshot_micros`
- `stats_micros`
- `jsonl_refresh_micros`
- `warm_startup_micros`
- `count_retention_micros`
- `byte_retention_micros`

The history, capture, tray, and retention measurements run with JSONL disabled.
The harness then starts the production database-backed mirror, measures one
complete enabled refresh, verifies its row count, and shuts the worker down.
JSONL application-mutex wait and hold are structurally zero: the refresh signal
carries no rows, and the worker opens its own read-only SQLite connection. The
reported JSONL time includes that independent query, event decoding,
serialization, flush, sync, and atomic replace. The other database timings are
also the relevant upper bounds for their synchronous database critical
sections; scheduler contention is validated by interleaving tests rather than
an unstable wall-clock assertion.
`first_page_payload_bytes`, `accounted_history_bytes`, and `jsonl_bytes` make
payload and storage growth visible without exposing clipboard contents.
`stored_items` confirms the final row count independently from the requested
fixture size.

For peak process memory, run the release test command under `/usr/bin/time -l`
and record `maximum resident set size`. Use Instruments for the desktop app
when measuring first-interaction and scrolling behavior.

## Fixture-v3 reference record (pre-configurable tray count)

On 2026-07-28, the release harness passed on an Apple M3 Pro MacBook Pro
(`arm64`, 36 GB, macOS 26.5.2, rustc 1.93.1) while connected to AC power.
Fixtures were synthetic and the selected already-built test-binary run completed
in 0.90 seconds with 14.42 MiB peak RSS.

| Profile / rows | First page | Full page walk | Detail | Capture | Duplicate |  Tray | Warm start | Count cleanup | Byte cleanup |
| -------------- | ---------: | -------------: | -----: | ------: | --------: | ----: | ---------: | ------------: | -----------: |
| text / 1000    |     348 µs |       6,101 µs |    n/a |  480 µs |    322 µs | 26 µs |     433 µs |      1,079 µs |     2,968 µs |
| mixed / 1000   |     210 µs |       3,165 µs |  72 µs |  504 µs |     72 µs | 24 µs |     490 µs |      1,002 µs |     3,262 µs |

The 100-row text/mixed records also passed. First pages contained 50 summaries,
the 1000-row walks contained 20 pages, and tray snapshots contained 20 rows
because this record predates the all-items default. The largest first-page
payload was 37,265 bytes. Full JSON records belong with the release evidence
rather than this design document.

The same fixture-v3 harness was repeated after adding the production
database-backed JSONL observation. A complete 1000-row text snapshot took
27,696 µs and produced 597,002 bytes; the mixed snapshot took 15,660 µs and
produced 666,601 bytes. Both snapshots contained exactly 1000 newline-delimited
records. The worker streams rows from its independent read-only SQLite
connection into the private temporary output, so the observation does not
materialize or re-sort the full history in memory.

## Committed-HEAD before/after comparison

The before baseline was measured from a detached temporary worktree at
`f9c78e3a0a281b7c6dfc7cb5d921552c48247879`, before the paging, lazy-detail,
bounded-tray, startup-metadata, byte-retention, and storage-stat changes. The
temporary worktree received one uncommitted ignored test harness and was not
used to modify the active frontend or application source. That harness used the
same fixture-v3 generator and 100/1000 text/mixed matrix as the current
harness, an optimized release test binary, temporary on-disk SQLite databases,
and the same machine and power conditions described above.

The legacy API had no page, lazy-detail, stats, or byte-retention operations.
Its faithful initial History operation was `get_all_events()`, which selected
every row, decoded every event blob, eagerly built every available preview, and
serialized the complete result. The legacy tray used the same unbounded
`get_all_events()` database operation before constructing native menu items.
Consequently:

- legacy `history load` is both its initial interaction and its complete
  history walk;
- legacy detail has no separate number because preview work is included in
  `history load`;
- legacy tray numbers cover the database snapshot, not native Tauri menu-item
  construction, matching the scope of the current tray query measurement;
- current `full walk` deliberately requests all cursor pages and is not work
  performed for the first screen;
- byte cleanup has no legacy value because the setting and accounting did not
  exist.

All times below are microseconds from selected already-built binary runs. These
are observations, not portable thresholds.

| Fixture      | Legacy history load | Current first page | Current full walk | Legacy full payload | Current first-page payload |
| ------------ | ------------------: | -----------------: | ----------------: | ------------------: | -------------------------: |
| text / 100   |                 124 |                111 |                92 |            66,601 B |                   37,263 B |
| text / 1000  |                 791 |                348 |             6,101 |           666,001 B |                   37,265 B |
| mixed / 100  |                 425 |                 80 |                67 |            50,088 B |                   21,858 B |
| mixed / 1000 |               4,001 |                210 |             3,165 |           504,048 B |                   21,860 B |

| Fixture      | Legacy rows loaded for tray | Then-current tray rows | Legacy tray query | Then-current tray query | Legacy warm start | Current warm start |
| ------------ | --------------------------: | ---------------------: | ----------------: | ----------------------: | ----------------: | -----------------: |
| text / 100   |                         100 |                     20 |                99 |                      22 |            24,622 |                478 |
| text / 1000  |                        1000 |                     20 |               786 |                      26 |           251,786 |                433 |
| mixed / 100  |                         100 |                     20 |               363 |                      19 |            26,461 |                402 |
| mixed / 1000 |                        1000 |                     20 |             3,110 |                      24 |           252,944 |                490 |

| Fixture      | Legacy capture | Current capture | Legacy duplicate | Current duplicate | Legacy count cleanup | Current count cleanup | Current byte cleanup |
| ------------ | -------------: | --------------: | ---------------: | ----------------: | -------------------: | --------------------: | -------------------: |
| text / 100   |            547 |             356 |               17 |               281 |                  582 |                   616 |                  791 |
| text / 1000  |            595 |             480 |               17 |               322 |                  764 |                 1,079 |                2,968 |
| mixed / 100  |            498 |             321 |               17 |                28 |                  730 |                   524 |                  638 |
| mixed / 1000 |            584 |             504 |               19 |                72 |                  714 |                 1,002 |                3,262 |

| Fixture      | Legacy cold start | Current cold start | Legacy seed | Current seed |
| ------------ | ----------------: | -----------------: | ----------: | -----------: |
| text / 100   |             3,851 |              1,100 |      25,950 |       28,909 |
| text / 1000  |             3,013 |                538 |     322,748 |      367,587 |
| mixed / 100  |             3,639 |                803 |      25,822 |       30,560 |
| mixed / 1000 |             2,992 |                673 |     337,490 |      358,438 |

At 1000 rows, the bounded first response reduced serialized payload by about
17.9 times for text and 23.1 times for mixed history. Current-schema warm
startup improved from roughly 252 ms to less than 0.5 ms because it no longer
rebuilds metadata from every event. The legacy and current direct harness
processes peaked at 16,187,392 bytes and 15,122,432 bytes RSS respectively.
These recorded tray timings predate the configurable tray count and its
all-items default, so they are not current release evidence for tray rebuild
cost.

The comparison also preserves non-improvements: seeding was slightly slower in
the current implementation, and duplicate/count-retention timings vary by
profile. The current values remain within the same-machine review budgets and
include byte accounting and stricter invariants that did not exist in the
legacy source.

For same-machine, fixture-v3 review, use these non-blocking comparison budgets:

| Scenario                       | Review budget |
| ------------------------------ | ------------: |
| First 50-summary page          |        1.5 ms |
| Full 1000-row page walk        |         25 ms |
| One mixed detail build         |        0.3 ms |
| One capture                    |          2 ms |
| Duplicate capture              |        1.5 ms |
| Configured 20-row tray query   |       0.15 ms |
| Current-schema warm start      |        2.5 ms |
| Count retention                |          5 ms |
| Byte retention                 |         15 ms |
| Enabled 1000-row JSONL refresh |        100 ms |

These budgets are roughly four times the observed maxima to absorb
microbenchmark jitter. They are review signals, not portable guarantees:
compare only the same fixture version and a comparable environment.

## Deterministic Structural Budgets

Normal tests enforce stable structural budgets that do not depend on machine
speed:

- the default first page returns at most 50 summaries;
- requested page sizes are capped at 100;
- a summary display is at most 512 bytes;
- summary and tray-construction queries never decode event blobs or read media
  files;
- macOS tray hover loads at most one 64 KiB display-only preview at a time;
- the tray snapshot returns the configured number of events, defaults to all
  retained events, and is capped at the 1000-item retention ceiling;
- a database-backed JSONL refresh streams one row at a time outside the
  application database mutex;
- a current schema/classifier version does not rebuild or scan history during
  startup;
- compact-mode canonical paging emits the newest row for effective text once,
  including across cursor boundaries;
- retention enforces both item count and accounted byte budget.

Do not add wall-clock thresholds to ordinary unit tests. Record timing budgets
as release-harness expectations after measuring a reference machine, and compare
before/after records using the same fixture version and environment.

## Desktop Regression Matrix

Use a generated synthetic database when integrating the page and detail
commands into Tauri. Validate:

1. cold start and a second current-schema start;
2. first History page and successive cursor pages;
3. background refresh while the list is scrolled;
4. detail expansion for HTML, PNG, local image, video, and multiple files;
5. capture, duplicate update, count cleanup, and byte cleanup;
6. compact-mode paging and restore ordering;
7. tray rebuild with 1000 stored rows;
8. JSONL disabled and enabled.

The baseline contains no real clipboard content or user paths and may be shared
in development reports.

CI runs the deterministic structural tests natively on both `macos-15` Apple
Silicon and `macos-15-intel`. The ignored timing harness is intentionally
manual so its JSON results can be recorded with controlled machine context.
Neither automated architecture run nor the harness replaces the native desktop
evidence required by `docs/security-release-checklist.md`.
