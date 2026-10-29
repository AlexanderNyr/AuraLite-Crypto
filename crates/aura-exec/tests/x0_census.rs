//! X0 gate — 20+ programs, each either banned or deterministic.
//!
//! The DoD line is: *20 programs (NaN, clocks, imports) — either banned or
//! deterministic*. Every program below is assembled by an INDEPENDENT Python
//! assembler (`tools/gen_x0_vectors.py`), never by this crate, and the verdict
//! the Rust census must produce is pinned in the fixture.
//!
//! No engine is involved: X0 is a static census, so these tests make claims
//! about classification only, not about execution.

use std::collections::BTreeMap;

use aura_exec::{Ban, CENSUS, ModuleLimits, Policy, Verdict, census};

const FIXTURE: &str = include_str!("fixtures/x0_census.txt");

fn fixture() -> &'static BTreeMap<String, String> {
    static PARSED: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();
    PARSED.get_or_init(|| {
        let mut map = BTreeMap::new();
        for line in FIXTURE.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let (key, value) = trimmed
                .split_once('=')
                .unwrap_or_else(|| panic!("malformed fixture line: {trimmed}"));
            assert!(map.insert(key.to_owned(), value.to_owned()).is_none());
        }
        map
    })
}

fn program_names() -> Vec<String> {
    let mut names: Vec<String> = fixture()
        .keys()
        .filter_map(|key| key.strip_prefix("program_"))
        .filter_map(|rest| rest.strip_suffix("_hex"))
        .map(str::to_owned)
        .collect();
    names.sort();
    names
}

fn from_hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2));
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let nibble = |byte: u8| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => panic!("bad hex nibble"),
            };
            (nibble(pair[0]) << 4) | nibble(pair[1])
        })
        .collect()
}

fn program(name: &str) -> Vec<u8> {
    from_hex(&fixture()[&format!("program_{name}_hex")])
}

#[test]
fn twenty_programs_are_each_banned_or_deterministic() {
    let limits = ModuleLimits::restrictive();
    let names = program_names();
    let count: usize = fixture()["program_count"].parse().unwrap();
    assert_eq!(names.len(), count, "fixture program count disagrees");
    assert!(
        count >= 20,
        "the DoD asks for at least 20 programs, got {count}"
    );

    let mut allowed = 0usize;
    let mut banned = 0usize;
    let mut rows = Vec::new();
    for name in &names {
        let bytes = program(name);
        let expected = &fixture()[&format!("program_{name}_verdict")];
        let expected_size: usize = fixture()[&format!("program_{name}_size")].parse().unwrap();
        assert_eq!(bytes.len(), expected_size, "{name}: fixture size");

        let report = census(&bytes, &limits);
        assert_eq!(
            &report.verdict.label(),
            expected,
            "{name}: census verdict disagrees with the independent fixture"
        );
        assert_eq!(report.size, bytes.len(), "{name}: reported size");
        if report.is_allowed() {
            allowed += 1;
            assert!(report.ban().is_none());
        } else {
            banned += 1;
            let ban = report.ban().expect("banned verdict carries a reason");
            assert!(
                !ban.to_string().is_empty(),
                "{name}: ban must explain itself"
            );
        }
        rows.push(format!(
            "  {name:<24} {:<28} sections={:?}",
            report.verdict.label(),
            report.sections
        ));
    }
    println!("X0 census: {} programs", names.len());
    for row in &rows {
        println!("{row}");
    }
    assert_eq!(allowed + banned, names.len());
    assert_eq!(allowed.to_string(), fixture()["allowed_count"]);
    assert_eq!(banned.to_string(), fixture()["banned_count"]);
    assert!(
        allowed >= 5,
        "at least the deterministic programs must pass"
    );
    assert!(
        banned >= 15,
        "the DoD wants the nondeterminism surface rejected"
    );
    println!("  allowed {allowed}, banned {banned}");
}

#[test]
fn census_table_policies_match_the_gate() {
    let limits = ModuleLimits::restrictive();
    // Every banned row with a program must be banned for that exact code.
    let expectations = [
        ("f64_add", "float_instruction"),
        ("f64_nan_const", "float_instruction"),
        ("import_wasi_clock", "host_import"),
        ("import_env_random", "host_import"),
        ("shared_memory", "shared_memory"),
        ("start_section", "start_section"),
        ("mutable_global", "mutable_global"),
        ("memory_no_max", "unbounded_memory"),
        ("memory_too_large", "memory_too_large"),
        ("simd_v128", "simd"),
        ("atomic_wait", "atomics"),
        ("bulk_memory_copy", "bulk_memory"),
        ("call_indirect", "call_indirect"),
        ("unknown_opcode", "unknown_opcode"),
    ];
    for (name, code) in expectations {
        let report = census(&program(name), &limits);
        let ban = report
            .ban()
            .unwrap_or_else(|| panic!("{name} must be banned"));
        assert_eq!(ban.code(), code, "{name}: wrong ban code");
        assert!(
            CENSUS.iter().any(|row| row.source == code),
            "{code} is not a published census row"
        );
    }
    // The allowed programs prove the whitelist is usable, not vacuous.
    for name in [
        "empty",
        "i32_arith",
        "i64_memory_roundtrip",
        "locals_and_calls",
        "block_loop_branch",
    ] {
        assert!(
            census(&program(name), &limits).is_allowed(),
            "{name} must be allowed: integer-only WASM is deterministic"
        );
    }
    // Every Allowed census row must be reachable, with a stated enforcement.
    for row in CENSUS {
        assert!(!row.source.is_empty());
        assert!(!row.feature.is_empty());
        assert!(!row.enforcement.is_empty());
        assert!(
            !row.note.is_empty(),
            "{}: census row must justify itself",
            row.source
        );
    }
    assert!(
        CENSUS
            .iter()
            .filter(|row| row.policy == Policy::Ban)
            .count()
            >= 14
    );
    assert!(CENSUS.iter().any(|row| row.policy == Policy::Allow));
}

#[test]
fn restrictive_limits_are_the_published_ones() {
    let limits = ModuleLimits::restrictive();
    assert_eq!(limits.max_module_bytes, 512 * 1024);
    assert_eq!(limits.max_memory_pages, 4_096);
    assert_eq!(limits.max_table_entries, 1_024);
    assert_eq!(ModuleLimits::page_size(), 65_536);
    assert_eq!(limits.max_memory_bytes(), 256 * 1024 * 1024);
    assert_eq!(
        limits.max_memory_pages.to_string(),
        fixture()["max_memory_pages"]
    );
    assert_eq!(
        limits.max_module_bytes.to_string(),
        fixture()["max_module_bytes"]
    );
    assert_eq!(
        limits.max_table_entries.to_string(),
        fixture()["max_table_entries"]
    );

    // One page above the cap is rejected with the documented reason.
    let over_cap = census(&wasm_with_memory(limits.max_memory_pages + 1), &limits);
    assert_eq!(
        over_cap.ban().map(Ban::code),
        Some("memory_too_large".to_string())
    );
}

/// Build a minimal module whose single memory declares `pages` as its maximum.
fn wasm_with_memory(pages: u32) -> Vec<u8> {
    let mut payload = vec![0x01, 0x01, 0x01]; // count, limits flag (min+max), min
    payload.extend_from_slice(&leb(pages as u64));
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.push(0x05);
    module.extend_from_slice(&leb(payload.len() as u64));
    module.extend_from_slice(&payload);
    module
}

fn leb(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

#[test]
fn verdict_labels_are_stable() {
    assert_eq!(Verdict::Allowed.label(), "allowed");
    assert_eq!(
        Verdict::Banned(Ban::StartSection).label(),
        "banned:start_section"
    );
    assert_eq!(
        Verdict::Banned(Ban::HostImport {
            module: "wasi_snapshot_preview1".into(),
            name: "clock_time_get".into()
        })
        .label(),
        "banned:host_import"
    );
    assert!(!Verdict::Banned(Ban::Simd).is_allowed());
}
