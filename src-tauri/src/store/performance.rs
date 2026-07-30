use super::{
    Database, HistoryPage, DEFAULT_HISTORY_PAGE_SIZE, MAX_MENU_BAR_ITEM_LIMIT,
    MAX_SUMMARY_DISPLAY_BYTES,
};
use crate::history_mirror::{HistoryMirror, HistoryMirrorConfig};
use copy_event_listener::event::{Data, Event, Item};
use serde::Serialize;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum FixtureProfile {
    Text,
    Mixed,
}

impl FixtureProfile {
    fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Mixed => "mixed",
        }
    }
}

struct FixtureRoot {
    path: PathBuf,
}

impl FixtureRoot {
    fn new(profile: FixtureProfile, item_count: usize) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "copy-stack-perf-{}-{}-{item_count}-{unique}",
            std::process::id(),
            profile.name()
        ));
        fs::create_dir_all(&path).expect("performance fixture directory should be created");
        let data_path = path.join("data");
        fs::create_dir(&data_path).expect("performance data directory should be created");
        #[cfg(unix)]
        for directory in [&path, &data_path] {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
                .expect("performance directory should be private");
        }
        Self { path }
    }
}

impl Drop for FixtureRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Debug, Serialize)]
struct PerformanceRecord {
    fixture_version: u32,
    profile: FixtureProfile,
    item_count: usize,
    cold_startup_micros: u128,
    seed_micros: u128,
    first_page_micros: u128,
    full_page_walk_micros: u128,
    detail_load_micros: u128,
    single_capture_micros: u128,
    duplicate_capture_micros: u128,
    tray_snapshot_micros: u128,
    stats_micros: u128,
    jsonl_refresh_micros: u128,
    warm_startup_micros: u128,
    count_retention_micros: u128,
    byte_retention_micros: u128,
    first_page_items: usize,
    page_count: usize,
    first_page_payload_bytes: usize,
    tray_items: usize,
    stored_items: u64,
    accounted_history_bytes: u64,
    jsonl_bytes: u64,
    retained_items_after_cleanup: u64,
    retained_bytes_after_cleanup: u64,
}

fn data(data_type: &str, value: impl Into<Vec<u8>>) -> Data {
    Data {
        r#type: data_type.to_string(),
        data: value.into(),
    }
}

fn text_event(index: usize) -> Event {
    Event {
        items: vec![Item {
            data_list: vec![data(
                "public.utf8-plain-text",
                format!("synthetic text fixture {index:04} {}", "x".repeat(96)),
            )],
        }],
    }
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.to_string_lossy())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320_u32 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn append_png_chunk(png: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(chunk_type);
    png.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(chunk_type.len() + data.len());
    crc_input.extend_from_slice(chunk_type);
    crc_input.extend_from_slice(data);
    png.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn valid_png(index: usize) -> Vec<u8> {
    let pixel = [
        0,
        index as u8,
        (index >> 8) as u8,
        (index >> 16) as u8,
        0xff,
    ];
    let mut adler_a = 1_u32;
    let mut adler_b = 0_u32;
    for byte in pixel {
        adler_a = (adler_a + byte as u32) % 65_521;
        adler_b = (adler_b + adler_a) % 65_521;
    }
    let adler = (adler_b << 16) | adler_a;
    let mut idat = vec![0x78, 0x01, 0x01, 5, 0, 0xfa, 0xff];
    idat.extend_from_slice(&pixel);
    idat.extend_from_slice(&adler.to_be_bytes());

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&1_u32.to_be_bytes());
    ihdr.extend_from_slice(&1_u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    append_png_chunk(&mut png, b"IHDR", &ihdr);
    append_png_chunk(&mut png, b"IDAT", &idat);
    append_png_chunk(&mut png, b"IEND", &[]);
    png
}

fn mixed_event(root: &Path, index: usize) -> Event {
    match index % 6 {
        0 => Event {
            items: vec![Item {
                data_list: vec![
                    data(
                        "public.utf8-plain-text",
                        format!("synthetic formatted fixture {index:04}"),
                    ),
                    data(
                        "public.html",
                        format!(
                            "<p><strong>Synthetic {index:04}</strong> {}</p>",
                            "html".repeat(32)
                        ),
                    ),
                ],
            }],
        },
        1 => Event {
            items: vec![Item {
                data_list: vec![data("public.png", valid_png(index))],
            }],
        },
        2 => {
            let path = root.join(format!("image-{index:04}.png"));
            fs::write(&path, valid_png(index)).expect("synthetic image fixture should be written");
            Event {
                items: vec![Item {
                    data_list: vec![
                        data("public.file-url", file_url(&path)),
                        data("public.tiff", vec![index as u8; 64]),
                    ],
                }],
            }
        }
        3 => {
            let path = root.join(format!("video-{index:04}.mov"));
            let mut bytes = b"\0\0\0\x14ftypqt  ".to_vec();
            bytes.extend((index as u64).to_le_bytes());
            fs::write(&path, bytes).expect("synthetic video fixture should be written");
            Event {
                items: vec![Item {
                    data_list: vec![
                        data("public.file-url", file_url(&path)),
                        data("public.tiff", Vec::new()),
                    ],
                }],
            }
        }
        4 => {
            let file_path = root.join(format!("file-{index:04}-a.txt"));
            fs::write(&file_path, format!("synthetic file fixture {index:04}"))
                .expect("synthetic file fixture should be written");
            let folder_path = root.join(format!("folder-{index:04}"));
            fs::create_dir(&folder_path).expect("synthetic folder fixture should be created");
            Event {
                items: vec![
                    Item {
                        data_list: vec![data("public.file-url", file_url(&file_path))],
                    },
                    Item {
                        data_list: vec![data("public.file-url", file_url(&folder_path))],
                    },
                ],
            }
        }
        _ => text_event(index),
    }
}

fn fixture_event(profile: FixtureProfile, root: &Path, index: usize) -> Event {
    match profile {
        FixtureProfile::Text => text_event(index),
        FixtureProfile::Mixed => mixed_event(root, index),
    }
}

fn run_case(profile: FixtureProfile, item_count: usize) -> PerformanceRecord {
    let fixture_root = FixtureRoot::new(profile, item_count);
    let events = (0..item_count)
        .map(|index| fixture_event(profile, &fixture_root.path, index))
        .collect::<Vec<_>>();
    let database_path = fixture_root.path.join("data").join("history.sqlite3");

    let started = Instant::now();
    let db = Database::open_path(&database_path).expect("fixture database should initialize");
    let cold_startup_micros = started.elapsed().as_micros();
    db.set_max_items(item_count as u32)
        .expect("fixture retention should update");

    let started = Instant::now();
    for event in &events {
        assert!(db
            .insert_event(event)
            .expect("synthetic fixture should insert"));
    }
    let seed_micros = started.elapsed().as_micros();

    let started = Instant::now();
    let first_page = db
        .get_history_page(None, None)
        .expect("first summary page should load");
    let first_page_micros = started.elapsed().as_micros();

    let started = Instant::now();
    let mut page_count = 1usize;
    let mut next_cursor = first_page.next_cursor.clone();
    while let Some(cursor) = next_cursor {
        let page = db
            .get_history_page(Some(&cursor), None)
            .expect("successive summary page should load");
        page_count += 1;
        next_cursor = page.next_cursor;
    }
    let full_page_walk_micros = started.elapsed().as_micros();

    let started = Instant::now();
    if let Some(summary) = first_page.items.iter().find(|item| item.has_detail) {
        let seed = db
            .get_history_detail_seed(&summary.content_hash)
            .expect("detail seed should load")
            .expect("detail seed should exist");
        Database::build_history_detail(seed, false).expect("detail should build");
    }
    let detail_load_micros = started.elapsed().as_micros();

    let capture_event = fixture_event(profile, &fixture_root.path, item_count + 10_000);
    let started = Instant::now();
    assert!(db
        .insert_event(&capture_event)
        .expect("single capture should insert"));
    let single_capture_micros = started.elapsed().as_micros();
    let started = Instant::now();
    assert!(db
        .insert_event(&capture_event)
        .expect("duplicate capture should update"));
    let duplicate_capture_micros = started.elapsed().as_micros();

    let started = Instant::now();
    let tray = db.get_tray_events().expect("tray snapshot should load");
    let tray_snapshot_micros = started.elapsed().as_micros();

    let started = Instant::now();
    let stats = db.get_history_stats().expect("history stats should load");
    let stats_micros = started.elapsed().as_micros();

    assert_structural_budgets(&first_page, tray.len(), item_count);
    let first_page_payload_bytes = serde_json::to_vec(&first_page)
        .expect("page should serialize")
        .len();

    let jsonl_path = fixture_root.path.join("data").join("history.jsonl");
    let mirror = HistoryMirror::start_database(
        HistoryMirrorConfig::new(jsonl_path.clone(), 4_096).with_debounce(Duration::ZERO),
        database_path.clone(),
    )
    .expect("database-backed JSONL mirror should start");
    let started = Instant::now();
    mirror
        .schedule_refresh()
        .expect("JSONL refresh should schedule");
    mirror
        .flush(Duration::from_secs(10))
        .expect("JSONL refresh should complete");
    let jsonl_refresh_micros = started.elapsed().as_micros();
    mirror
        .shutdown(Duration::from_secs(2))
        .expect("JSONL mirror should stop");
    let jsonl_bytes = fs::metadata(&jsonl_path)
        .expect("JSONL snapshot should exist")
        .len();
    let jsonl_rows =
        BufReader::new(fs::File::open(&jsonl_path).expect("JSONL snapshot should be readable"))
            .lines()
            .count() as u64;
    assert_eq!(jsonl_rows, stats.total_items);

    drop(db);
    let started = Instant::now();
    let reopened =
        Database::open_path(&database_path).expect("current-schema fixture should reopen");
    let warm_startup_micros = started.elapsed().as_micros();
    let reopened_stats = reopened
        .get_history_stats()
        .expect("reopened fixture stats should load");
    assert_eq!(reopened_stats.total_items, stats.total_items);
    assert_eq!(reopened_stats.total_bytes, stats.total_bytes);

    let started = Instant::now();
    reopened
        .set_max_items(item_count.saturating_sub(10).max(1) as u32)
        .expect("count retention setting should update");
    reopened
        .cleanup_old_events()
        .expect("count retention should complete");
    let count_retention_micros = started.elapsed().as_micros();

    let after_count_cleanup = reopened
        .get_history_stats()
        .expect("count-retained stats should load");
    let started = Instant::now();
    reopened
        .set_max_history_bytes((after_count_cleanup.total_bytes / 2).max(1))
        .expect("byte retention setting should update");
    reopened
        .cleanup_old_events()
        .expect("byte retention should complete");
    let byte_retention_micros = started.elapsed().as_micros();
    let retained = reopened
        .get_history_stats()
        .expect("retained stats should load");

    PerformanceRecord {
        fixture_version: 3,
        profile,
        item_count,
        cold_startup_micros,
        seed_micros,
        first_page_micros,
        full_page_walk_micros,
        detail_load_micros,
        single_capture_micros,
        duplicate_capture_micros,
        tray_snapshot_micros,
        stats_micros,
        jsonl_refresh_micros,
        warm_startup_micros,
        count_retention_micros,
        byte_retention_micros,
        first_page_items: first_page.items.len(),
        page_count,
        first_page_payload_bytes,
        tray_items: tray.len(),
        stored_items: stats.total_items,
        accounted_history_bytes: stats.total_bytes,
        jsonl_bytes,
        retained_items_after_cleanup: retained.total_items,
        retained_bytes_after_cleanup: retained.total_bytes,
    }
}

fn assert_structural_budgets(page: &HistoryPage, tray_items: usize, item_count: usize) {
    assert_eq!(page.items.len(), item_count.min(DEFAULT_HISTORY_PAGE_SIZE));
    assert!(page
        .items
        .iter()
        .all(|item| item.display.len() <= MAX_SUMMARY_DISPLAY_BYTES));
    assert_eq!(tray_items, item_count.min(MAX_MENU_BAR_ITEM_LIMIT));
}

/// Run with:
///
/// `cargo test --release --manifest-path src-tauri/Cargo.toml performance_matrix_report -- --ignored --nocapture`
///
/// Timings are emitted for comparison but deliberately have no assertions.
#[test]
#[ignore = "manual release-mode performance harness"]
fn performance_matrix_report() {
    for profile in [FixtureProfile::Text, FixtureProfile::Mixed] {
        for item_count in [100, 1_000] {
            let record = run_case(profile, item_count);
            println!(
                "{}",
                serde_json::to_string(&record).expect("performance record should serialize")
            );
        }
    }
}
