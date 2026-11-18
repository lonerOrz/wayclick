//! README key-code table: generated from the translators, verified against
//! the file.
//!
//! The table between `<!-- key-table:start -->` and `<!-- key-table:end -->`
//! in `README.md` is produced by exhaustively probing each platform
//! translator. This test regenerates the table and fails if the README has
//! drifted from what the code actually does — the table can never lie about
//! a mapping.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use wayclick::domain::PhysicalKey;
use wayclick::input::translator::{linux, macos, windows};

/// How far each translator is probed. Linux evdev codes top out at KEY_MAX
/// (0x2ff = 767); Windows scan codes fit a byte plus the extended marker at
/// 0x100; macOS virtual keycodes fit 7 bits.
const LINUX_MAX: u32 = 767;
const WINDOWS_MAX: u32 = 0x1FF;
const MACOS_MAX: u32 = 127;

/// Human-readable key name for table rows, derived from the variant name.
fn display_name(key: PhysicalKey) -> Option<String> {
    let name = match key {
        PhysicalKey::Unknown { .. } => return None,
        other => format!("{other:?}"),
    };
    Some(name)
}

/// Probe one platform: map every recognised code to its `PhysicalKey`.
/// Unknown codes are skipped (they are not "keys", just fallbacks).
fn probe(max: u32, translate: impl Fn(u32) -> PhysicalKey) -> BTreeMap<PhysicalKey, u32> {
    let mut found = BTreeMap::new();
    for code in 0..=max {
        let key = translate(code);
        if !matches!(key, PhysicalKey::Unknown { .. }) && !found.contains_key(&key) {
            found.insert(key, code);
        }
    }
    found
}

/// The generated markdown table.
fn generate_table() -> String {
    let linux = probe(LINUX_MAX, |c| linux::translate_key(c as u16));
    let windows = probe(WINDOWS_MAX, |c| {
        // Codes at 0x100+ encode the extended (E0-prefix) scan codes, which is
        // how the runtime surfaces them.
        windows::translate_key((c & 0xFF) as u16, c >= 0x100)
    });
    let macos = probe(MACOS_MAX, |c| macos::translate_key(c as u16));

    // Union of every key any platform recognises, ordered by the Linux code
    // (the config's native dialect) then by name.
    let mut all: Vec<(PhysicalKey, String)> = linux
        .keys()
        .chain(windows.keys())
        .chain(macos.keys())
        .filter_map(|k| display_name(*k).map(|n| (*k, n)))
        .collect();
    all.sort_by_cached_key(|(k, n)| (linux.get(k).copied().unwrap_or(u32::MAX), n.clone()));
    all.dedup();

    let mut out = String::new();
    let _ = writeln!(
        out,
        "| Key | Linux (`evdev`) | Windows (`scan code`) | macOS (`CGKeyCode`) |"
    );
    let _ = writeln!(
        out,
        "| --- | --------------: | --------------------: | ------------------: |"
    );
    for (key, name) in &all {
        let _ = writeln!(
            out,
            "| {name} | {} | {} | {} |",
            fmt_cell(linux.get(key)),
            fmt_cell(windows.get(key)),
            fmt_cell(macos.get(key)),
        );
    }
    out
}

/// Format a code cell; `–` marks keys the platform cannot recognise.
fn fmt_cell(code: Option<&u32>) -> String {
    match code {
        Some(c) => format!("{c}"),
        None => "–".into(),
    }
}

#[test]
fn readme_key_table_matches_translators() {
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))
        .expect("README.md must exist (the key table is verified against it)");

    let start = "<!-- key-table:start -->";
    let end = "<!-- key-table:end -->";
    let (Some(from), Some(to)) = (readme.find(start), readme.find(end)) else {
        panic!("README.md must contain {start} and {end} markers around the key table");
    };

    let table = generate_table();
    let actual = &readme[from + start.len()..to];
    let expected = format!("\n{table}");
    assert!(
        actual == expected,
        "README key table drifted from the translators.\n--- expected ---\n{expected}\n--- actual ---\n{actual}"
    );
}

#[test]
fn generated_table_is_sane() {
    let table = generate_table();
    // Letters must exist on all three platforms.
    for name in ["A", "Z", "Space", "Enter", "ArrowUp"] {
        let row = table
            .lines()
            .find(|l| l.starts_with(&format!("| {name} |")))
            .unwrap_or_else(|| panic!("{name} missing from table"));
        assert!(
            !row.contains("–"),
            "{name} should map on all platforms: {row}"
        );
    }
    // F13-F24 exist only on Linux and macOS (extended scan codes are limited).
    assert!(table.contains("| F13 |"));
}

// Helper test (not part of CI verification): print the table for README
// authoring. Run with `cargo test --test readme_key_table print_table -- --nocapture`.
#[test]
fn print_table() {
    println!("{}", generate_table());
}
