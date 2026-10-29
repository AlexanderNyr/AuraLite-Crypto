//! X8 — the static verifier gate.
//!
//! The claim: **a module either satisfies the accepted subset or is refused with a
//! stable, machine-readable code — and the refusal is the one the damage deserves.**
//!
//! Two halves, deliberately unequal:
//!
//! * [`manifest_is_measured`] replays the 100 golden mutants that
//!   `tools/gen_x8_mutants.py` produces. The mutator is a separate Python
//!   implementation: it parses the module itself, damages one thing, and states the
//!   verdict it expects. If the Rust verifier and the Python mutator agreed by
//!   sharing code, agreement would prove nothing.
//! * the unit tests at the bottom build one tiny module per accepted code with a
//!   hand-assembled WASM binary, so every one of the 22 codes has a witness that
//!   does not depend on the shipped artifacts looking the way they look today.
//!
//! The shipped artifacts are pinned too: if `kv.wasm` starts reporting a different
//! function count or stack depth, this gate fails before any evidence is written.

use std::collections::BTreeMap;

use aura_exec::verify::{
    Code, MAX_FUNCTION_OPS, MAX_LOCALS, MAX_STACK_DEPTH, VerifyLimits, verify,
};

/// The manifest the Python mutator writes: one `file=… expect=…` line per module.
const MANIFEST: &str = include_str!("fixtures/x8_mutants.txt");

/// Per-artifact pin, measured by `x8-verify` and reproduced by this gate.
///
/// `(path, functions, instructions, max_stack, max_ops)`. A change here is a change
/// in a shipped contract, and it has to be re-measured on purpose.
const SHIPPED: [(&str, usize, u32, u32, u32); 6] = [
    ("examples/kv/wasm/kv.wasm", 33, 1_905, 7, 476),
    ("examples/store/wasm/store.wasm", 29, 1_921, 7, 476),
    ("examples/events/wasm/events.wasm", 25, 1_871, 7, 476),
    ("examples/vault/wasm/vault.wasm", 53, 3_692, 7, 535),
    ("examples/evil/wasm/evil.wasm", 61, 4_235, 7, 630),
    ("examples/recurse/wasm/recurse.wasm", 51, 3_331, 7, 476),
];

/// One manifest line: the file, the verdict the mutator expects, and why.
struct Row {
    file: String,
    expect: String,
    kind: String,
    source: String,
}

fn manifest() -> Vec<Row> {
    let mut rows = Vec::new();
    for line in MANIFEST.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: BTreeMap<&str, &str> = line
            .split_whitespace()
            .filter_map(|field| field.split_once('='))
            .collect();
        rows.push(Row {
            file: fields.get("file").copied().unwrap_or_default().to_string(),
            expect: fields
                .get("expect")
                .copied()
                .unwrap_or("accept")
                .to_string(),
            kind: fields.get("kind").copied().unwrap_or_default().to_string(),
            source: fields
                .get("source")
                .copied()
                .unwrap_or_default()
                .to_string(),
        });
    }
    rows
}

/// Directory the manifest's paths are relative to — the same base `x8-verify` uses.
fn fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Repository root. `CARGO_MANIFEST_DIR` is `crates/aura-exec`.
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> Vec<u8> {
    let full = fixtures().join(path);
    std::fs::read(&full).unwrap_or_else(|error| panic!("{} is readable: {error}", full.display()))
}

fn read_repo(path: &str) -> Vec<u8> {
    let full = repo_root().join(path);
    std::fs::read(&full).unwrap_or_else(|error| panic!("{} is readable: {error}", full.display()))
}

/// The verdict of a report in the manifest's notation.
fn verdict(codes: &[&'static str]) -> String {
    match codes.first() {
        Some(code) => format!("reject:{code}"),
        None => "accept".to_string(),
    }
}

#[test]
fn manifest_is_measured() {
    let rows = manifest();
    assert_eq!(rows.len(), 106, "100 mutants + 6 shipped artifacts");
    let mutants: Vec<&Row> = rows.iter().filter(|row| row.kind != "shipped").collect();
    assert_eq!(mutants.len(), 100, "the plan is 100 mutants");
    assert_eq!(
        rows.len() - mutants.len(),
        6,
        "every artifact is replayed as-is"
    );

    let limits = VerifyLimits::hosted();
    let mut failures = Vec::new();
    for row in &rows {
        assert!(!row.kind.is_empty(), "{} names a kind", row.file);
        assert!(!row.source.is_empty(), "{} names its source", row.file);
        let report = verify(&read(&row.file), &limits);
        let got = verdict(&report.codes());
        if got != row.expect {
            failures.push(format!(
                "{} ({} on {}) expect={} got={}{}",
                row.file,
                row.kind,
                row.source,
                row.expect,
                got,
                report
                    .first()
                    .map(|problem| format!(" [{}]", problem))
                    .unwrap_or_default()
            ));
        }
        if row.expect == "accept" {
            assert!(report.ok, "{} accepted but carries errors", row.file);
            assert!(report.functions > 0, "{} reports no functions", row.file);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} modules got the wrong verdict:\n{}",
        failures.len(),
        rows.len(),
        failures.join("\n")
    );

    // The plan has to cover the surface, not just count to a hundred.
    let mut per_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows {
        *per_kind.entry(row.kind.as_str()).or_default() += 1;
    }
    assert_eq!(
        per_kind.len(),
        16,
        "15 damage kinds + the shipped replay: {per_kind:?}"
    );
    assert_eq!(per_kind["shipped"], 6);
    for kind in [
        "magic",
        "version",
        "float_type",
        "float_local",
        "import",
        "global_type",
        "memory_max",
        "opcode",
        "call_index",
        "local_index",
        "label_depth",
        "export_shape",
        "function_code",
        "too_large",
        "truncated",
    ] {
        assert!(
            per_kind.get(kind).copied().unwrap_or(0) >= 6,
            "{kind} is under-represented: {per_kind:?}"
        );
    }
}

#[test]
fn mutants_are_distinct_files() {
    let rows = manifest();
    let mut files: Vec<&str> = rows
        .iter()
        .filter(|row| row.kind != "shipped")
        .map(|row| row.file.as_str())
        .collect();
    files.sort_unstable();
    let before = files.len();
    files.dedup();
    assert_eq!(before, files.len(), "two manifest rows name the same file");
    for file in files {
        assert!(
            file.starts_with("x8/"),
            "mutants live under the X8 fixture directory: {file}"
        );
    }
}

#[test]
fn shipped_artifacts_are_pinned() {
    let limits = VerifyLimits::hosted();
    for (path, functions, instructions, stack, ops) in SHIPPED {
        let report = verify(&read_repo(path), &limits);
        assert!(report.ok, "{path}: {:?}", report.errors);
        assert_eq!(report.functions, functions, "{path} functions");
        assert_eq!(report.instructions, instructions, "{path} instructions");
        assert_eq!(report.max_stack_depth, stack, "{path} stack");
        assert_eq!(report.max_function_ops, ops, "{path} ops");
        assert!(
            report.instructions > report.max_function_ops,
            "{path} aggregate > peak"
        );
    }
}

// ------------------------------------------------------------------ builders
//
// A hand-assembled WASM module. The encodings below are the ones the spec uses for
// everything rustc emits; hand-rolling them keeps the fixture independent of any
// assembler crate (this workspace does not depend on `wat`).

const HEADER: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

fn leb(mut value: u32) -> Vec<u8> {
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

fn section(id: u8, body: Vec<u8>) -> Vec<u8> {
    let mut out = vec![id];
    out.extend(leb(body.len() as u32));
    out.extend(body);
    out
}

fn module(sections: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = HEADER.to_vec();
    for body in sections {
        out.extend(body);
    }
    out
}

/// One function type: `(params) -> (results)`.
fn types(params: &[u8], results: &[u8]) -> Vec<u8> {
    let mut body = leb(1);
    body.push(0x60);
    body.extend(leb(params.len() as u32));
    body.extend_from_slice(params);
    body.extend(leb(results.len() as u32));
    body.extend_from_slice(results);
    section(1, body)
}

/// A type section with several function types, so a test can reference more than
/// one signature.
fn types_many(entries: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut body = leb(entries.len() as u32);
    for (params, results) in entries {
        body.push(0x60);
        body.extend(leb(params.len() as u32));
        body.extend_from_slice(params);
        body.extend(leb(results.len() as u32));
        body.extend_from_slice(results);
    }
    section(1, body)
}

/// Functions with explicit type indices.
fn functions_typed(indices: &[u32]) -> Vec<u8> {
    let mut body = leb(indices.len() as u32);
    for index in indices {
        body.extend(leb(*index));
    }
    section(3, body)
}

/// `n` functions, all of type 0.
fn functions(n: u32) -> Vec<u8> {
    let mut body = leb(n);
    body.extend(std::iter::repeat_n(0x00, n as usize));
    section(3, body)
}

fn exports_run() -> Vec<u8> {
    let mut body = leb(1);
    body.extend(leb(3));
    body.extend(b"run");
    body.push(0x00);
    body.push(0x00);
    section(7, body)
}

/// A body: local declarations followed by instructions.
fn body(locals: &[(u32, u8)], ops: &[u8]) -> Vec<u8> {
    let mut out = leb(locals.len() as u32);
    for (count, ty) in locals {
        out.extend(leb(*count));
        out.push(*ty);
    }
    out.extend_from_slice(ops);
    out
}

fn code(bodies: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = leb(bodies.len() as u32);
    for body in bodies {
        out.extend(leb(body.len() as u32));
        out.extend(body);
    }
    section(10, out)
}

/// A module that returns `()`, has no parameters, exports `run` and runs `ops`.
fn one_function(ops: &[u8]) -> Vec<u8> {
    module(vec![
        types(&[], &[]),
        functions(1),
        exports_run(),
        code(vec![body(&[], ops)]),
    ])
}

/// A module that is valid on its own terms, so a test can add exactly one defect.
fn valid() -> Vec<u8> {
    one_function(&[0x0b])
}

fn first_code(bytes: &[u8]) -> Option<&'static str> {
    let report = verify(bytes, &VerifyLimits::hosted());
    report.first().map(|problem| problem.code.as_str())
}

// ------------------------------------------------------------------- header

#[test]
fn accepts_a_minimal_module() {
    let report = verify(&valid(), &VerifyLimits::hosted());
    assert!(report.ok, "{:?}", report.errors);
    assert_eq!(report.functions, 1);
    assert_eq!(report.instructions, 1, "just the final `end`");
    assert_eq!(report.max_stack_depth, 0);
    assert_eq!(report.size, valid().len());
    assert!(report.codes().is_empty());
    assert!(report.first().is_none());
}

#[test]
fn code_bad_magic() {
    let mut bytes = valid();
    bytes[1] = 0x01;
    assert_eq!(first_code(&bytes), Some("bad_magic"));
    let report = verify(&bytes, &VerifyLimits::hosted());
    assert_eq!(
        report.first().unwrap().offset,
        0,
        "the header is where the damage is"
    );
}

#[test]
fn code_bad_version() {
    let mut bytes = valid();
    bytes[4] = 0x02;
    assert_eq!(first_code(&bytes), Some("bad_version"));
}

#[test]
fn code_module_too_large() {
    let limits = VerifyLimits {
        max_module_bytes: 8,
        ..VerifyLimits::hosted()
    };
    let report = verify(&valid(), &limits);
    assert_eq!(
        report.first().map(|problem| problem.code),
        Some(Code::ModuleTooLarge)
    );
}

#[test]
fn code_section_truncated() {
    let mut bytes = valid();
    bytes.truncate(bytes.len() - 2);
    assert_eq!(first_code(&bytes), Some("section_truncated"));
}

#[test]
fn code_section_unknown() {
    let mut bytes = valid();
    bytes.extend(section(13, Vec::new()));
    assert_eq!(first_code(&bytes), Some("section_unknown"));
}

#[test]
fn code_section_order() {
    // The export section belongs after the function section; swap them.
    let bytes = module(vec![
        types(&[], &[]),
        exports_run(),
        functions(1),
        code(vec![body(&[], &[0x0b])]),
    ]);
    assert_eq!(first_code(&bytes), Some("section_order"));
}

#[test]
fn code_section_order_on_a_duplicate() {
    let bytes = module(vec![
        types(&[], &[]),
        functions(1),
        exports_run(),
        code(vec![body(&[], &[0x0b])]),
        types(&[], &[]),
    ]);
    assert_eq!(first_code(&bytes), Some("section_order"));
}

// -------------------------------------------------------------------- types

#[test]
fn code_float_type() {
    let bytes = module(vec![
        types(&[0x7d], &[]),
        functions(1),
        exports_run(),
        code(vec![body(&[], &[0x0b])]),
    ]);
    assert_eq!(first_code(&bytes), Some("float_type"));
}

#[test]
fn code_float_local() {
    let bytes = module(vec![
        types(&[], &[]),
        functions(1),
        exports_run(),
        code(vec![body(&[(1, 0x7c)], &[0x0b])]),
    ]);
    assert_eq!(first_code(&bytes), Some("float_type"));
}

#[test]
fn code_global_type() {
    let mut globals = leb(1);
    globals.push(0x7c); // f64
    globals.push(0x00); // immutable
    globals.extend_from_slice(&[0x44, 0x00, 0x00]); // f64.const 0
    globals.push(0x0b);
    let mut bytes = valid();
    bytes.truncate(8);
    let mut rebuilt = bytes;
    rebuilt.extend(section(1, {
        let mut b = leb(1);
        b.push(0x60);
        b.extend(leb(0));
        b.extend(leb(0));
        b
    }));
    rebuilt.extend(section(3, {
        let mut b = leb(1);
        b.push(0x00);
        b
    }));
    rebuilt.extend(section(6, globals));
    rebuilt.extend(exports_run());
    rebuilt.extend(code(vec![body(&[], &[0x0b])]));
    assert_eq!(first_code(&rebuilt), Some("global_type"));
}

#[test]
fn code_memory_limits_without_a_maximum() {
    let mut memory = leb(1);
    memory.push(0x00); // no maximum
    memory.extend(leb(1));
    let mut bytes = valid();
    bytes.extend(section(5, memory));
    // The memory section must precede the code section, so build it in order.
    let ordered = module(vec![
        types(&[], &[]),
        section(5, {
            let mut m = leb(1);
            m.push(0x00);
            m.extend(leb(1));
            m
        }),
        functions(1),
        exports_run(),
        code(vec![body(&[], &[0x0b])]),
    ]);
    assert_eq!(first_code(&ordered), Some("memory_limits"));
}

#[test]
fn code_memory_limits_above_the_ceiling() {
    let ordered: Vec<u8> = {
        let mut m = leb(1);
        m.push(0x01);
        m.extend(leb(1));
        m.extend(leb(8192));
        m
    };
    let bytes = module(vec![
        types(&[], &[]),
        section(5, ordered),
        functions(1),
        exports_run(),
        code(vec![body(&[], &[0x0b])]),
    ]);
    let report = verify(&bytes, &VerifyLimits::hosted());
    assert_eq!(
        report.first().map(|problem| problem.code),
        Some(Code::MemoryLimits)
    );
}

// ------------------------------------------------------------------ imports

#[test]
fn code_import_not_allowed() {
    let mut imports = leb(1);
    imports.extend(leb(3));
    imports.extend(b"env");
    imports.extend(leb(9));
    imports.extend(b"not_in_abi");
    imports.push(0x00); // function
    imports.push(0x00); // type 0
    let bytes = module(vec![
        types(&[], &[]),
        section(2, imports),
        functions(1),
        exports_run(),
        code(vec![body(&[], &[0x0b])]),
    ]);
    assert_eq!(first_code(&bytes), Some("import_not_allowed"));
}

#[test]
fn abi_imports_pass_in_the_hosted_configuration_and_fail_in_the_restrictive_one() {
    // `env.storage_read` is ABI v1: four i32 parameters, one i32 result.
    let mut imports = leb(1);
    imports.extend(leb(3));
    imports.extend(b"env");
    imports.extend(leb(12));
    imports.extend(b"storage_read");
    imports.push(0x00);
    imports.push(0x00);
    let bytes = module(vec![
        types(&[0x7f, 0x7f, 0x7f, 0x7f], &[0x7f]),
        section(2, imports),
        functions(1),
        exports_run(),
        code(vec![body(&[], &[0x0b])]),
    ]);
    assert!(verify(&bytes, &VerifyLimits::hosted()).ok);

    let strict = verify(&bytes, &VerifyLimits::restrictive());
    assert_eq!(
        strict.first().map(|problem| problem.code),
        Some(Code::ImportNotAllowed),
        "restrictive refuses every import, ABI or not"
    );
}

// -------------------------------------------------------------------- code

#[test]
fn code_function_code_mismatch() {
    let bytes = module(vec![
        types(&[], &[]),
        functions(2),
        exports_run(),
        code(vec![body(&[], &[0x0b])]),
    ]);
    assert_eq!(first_code(&bytes), Some("function_code_mismatch"));
}

#[test]
fn code_body_truncated() {
    // The code section holds one body whose declared length runs past the section.
    let mut inner = leb(1);
    inner.extend(leb(9));
    inner.extend(body(&[], &[0x0b]));
    let bytes = module(vec![
        types(&[], &[]),
        functions(1),
        exports_run(),
        section(10, inner),
    ]);
    assert_eq!(first_code(&bytes), Some("body_truncated"));
}

#[test]
fn code_body_truncated_on_trailing_bytes() {
    let bytes = one_function(&[0x0b, 0x01]);
    assert_eq!(first_code(&bytes), Some("body_truncated"));
}

#[test]
fn code_opcode_banned() {
    let bytes = one_function(&[0x43, 0x00, 0x00, 0x00, 0x00, 0x0b]); // f32.const
    assert_eq!(first_code(&bytes), Some("opcode_banned"));
}

#[test]
fn code_opcode_banned_on_a_bulk_sub_opcode() {
    let bytes = one_function(&[0xfc, 0x08, 0x00, 0x00, 0x0b]); // memory.init
    assert_eq!(first_code(&bytes), Some("opcode_banned"));
}

// ------------------------------------------------------------------- stack

#[test]
fn code_stack_underflow() {
    let bytes = one_function(&[0x6a, 0x0b]); // i32.add with an empty stack
    let report = verify(&bytes, &VerifyLimits::hosted());
    assert_eq!(
        report.first().map(|problem| problem.code),
        Some(Code::StackUnderflow)
    );
    assert!(report.first().unwrap().detail.contains("pops 2"));
}

#[test]
fn code_stack_too_deep() {
    let limits = VerifyLimits {
        max_stack_depth: 1,
        ..VerifyLimits::hosted()
    };
    let bytes = one_function(&[0x41, 0x01, 0x41, 0x02, 0x6a, 0x1a, 0x0b]);
    let report = verify(&bytes, &limits);
    assert_eq!(
        report.first().map(|problem| problem.code),
        Some(Code::StackTooDeep)
    );
    // The same module is fine under the published ceiling.
    assert!(verify(&bytes, &VerifyLimits::hosted()).ok);
}

#[test]
fn stack_depth_is_bounded_by_a_real_body() {
    let report = verify(&one_function(&[0x41, 0x01]), &VerifyLimits::hosted());
    assert_eq!(
        report.max_stack_depth, 1,
        "the body pushes one value and ends"
    );
    let with_value = verify(
        &module(vec![
            types(&[], &[0x7f]),
            functions(1),
            exports_run(),
            code(vec![body(&[], &[0x41, 0x07, 0x0b])]),
        ]),
        &VerifyLimits::hosted(),
    );
    assert!(with_value.ok, "{:?}", with_value.errors);
    assert_eq!(with_value.max_stack_depth, 1);
}

#[test]
fn code_label_depth() {
    let bytes = one_function(&[0x0c, 0x01, 0x0b]); // br 1 out of the function frame
    assert_eq!(first_code(&bytes), Some("label_depth"));
}

#[test]
fn a_branch_to_the_function_frame_is_allowed() {
    let bytes = one_function(&[0x0c, 0x00, 0x0b]);
    assert!(verify(&bytes, &VerifyLimits::hosted()).ok);
}

// ---------------------------------------------------------------- index space

#[test]
fn code_local_index() {
    let bytes = one_function(&[0x20, 0x00, 0x1a, 0x0b]); // local.get 0, no locals
    assert_eq!(first_code(&bytes), Some("local_index"));
}

#[test]
fn code_global_index() {
    let bytes = one_function(&[0x23, 0x00, 0x1a, 0x0b]); // global.get 0, no globals
    assert_eq!(first_code(&bytes), Some("global_index"));
}

#[test]
fn code_call_index() {
    let bytes = one_function(&[0x10, 0x03, 0x0b]); // call 3 of 1 function
    assert_eq!(first_code(&bytes), Some("call_index"));
}

#[test]
fn a_call_feeds_the_callee_signature_into_the_stack_check() {
    // Function 0 is exported and takes nothing; function 1 takes two i32 and returns
    // one. Calling function 1 from an empty stack is an underflow only if the
    // callee's arity is actually read out of the type section.
    let bytes = module(vec![
        types_many(&[(&[], &[]), (&[0x7f, 0x7f], &[0x7f])]),
        functions_typed(&[0, 1]),
        exports_run(),
        code(vec![
            body(&[], &[0x10, 0x01, 0x0b]),
            body(&[], &[0x41, 0x01, 0x0b]),
        ]),
    ]);
    let report = verify(&bytes, &VerifyLimits::hosted());
    assert_eq!(
        report.first().map(|problem| problem.code),
        Some(Code::StackUnderflow)
    );
    assert!(report.first().unwrap().detail.contains("pops 2"));

    // The same call with its arguments in place passes, and leaves one value.
    let satisfied = module(vec![
        types_many(&[(&[], &[]), (&[0x7f, 0x7f], &[0x7f])]),
        functions_typed(&[0, 1]),
        exports_run(),
        code(vec![
            body(&[], &[0x41, 0x01, 0x41, 0x02, 0x10, 0x01, 0x1a, 0x0b]),
            body(&[], &[0x41, 0x01, 0x0b]),
        ]),
    ]);
    let report = verify(&satisfied, &VerifyLimits::hosted());
    assert!(report.ok, "{:?}", report.errors);
    assert_eq!(report.max_stack_depth, 2);
}

// ------------------------------------------------------------------ budgets

#[test]
fn code_opcode_budget() {
    let limits = VerifyLimits {
        max_function_ops: 2,
        ..VerifyLimits::hosted()
    };
    let bytes = one_function(&[0x41, 0x01, 0x41, 0x02, 0x1a, 0x1a, 0x0b]);
    let report = verify(&bytes, &limits);
    assert_eq!(
        report.first().map(|problem| problem.code),
        Some(Code::OpcodeBudget)
    );
    // The published ceiling is the shipped value, not a hidden one.
    assert_eq!(VerifyLimits::hosted().max_function_ops, MAX_FUNCTION_OPS);
}

#[test]
fn code_locals_budget() {
    let limits = VerifyLimits {
        max_locals: 1,
        ..VerifyLimits::hosted()
    };
    let bytes = module(vec![
        types(&[], &[]),
        functions(1),
        exports_run(),
        code(vec![body(&[(2, 0x7f)], &[0x0b])]),
    ]);
    let report = verify(&bytes, &limits);
    assert_eq!(
        report.first().map(|problem| problem.code),
        Some(Code::LocalsBudget)
    );
    assert_eq!(VerifyLimits::hosted().max_locals, MAX_LOCALS);
    assert_eq!(VerifyLimits::hosted().max_stack_depth, MAX_STACK_DEPTH);
    // Under the published limits two `i32` locals are ordinary.
    assert!(verify(&bytes, &VerifyLimits::hosted()).ok);
}

// ------------------------------------------------------------------- exports

#[test]
fn code_export_shape() {
    let bytes = module(vec![
        types(&[], &[]),
        functions(1),
        code(vec![body(&[], &[0x0b])]),
    ]);
    assert_eq!(first_code(&bytes), Some("export_shape"));
}

#[test]
fn code_export_shape_on_a_wrong_name() {
    // An export section that is present but exports everything except `run`.
    let mut exports = leb(1);
    exports.extend(leb(5));
    exports.extend(b"start");
    exports.push(0x00);
    exports.push(0x00);
    let bytes = module(vec![
        types(&[], &[]),
        functions(1),
        section(7, exports),
        code(vec![body(&[], &[0x0b])]),
    ]);
    assert_eq!(first_code(&bytes), Some("export_shape"));
}

#[test]
fn every_code_has_a_stable_string() {
    // The 22 codes are part of the evidence format; a rename is a breaking change.
    let codes = [
        Code::BadMagic,
        Code::BadVersion,
        Code::ModuleTooLarge,
        Code::SectionOrder,
        Code::SectionUnknown,
        Code::SectionTruncated,
        Code::FloatType,
        Code::ImportNotAllowed,
        Code::GlobalType,
        Code::MemoryLimits,
        Code::FunctionCodeMismatch,
        Code::BodyTruncated,
        Code::OpcodeBanned,
        Code::StackUnderflow,
        Code::StackTooDeep,
        Code::LabelDepth,
        Code::LocalIndex,
        Code::GlobalIndex,
        Code::CallIndex,
        Code::OpcodeBudget,
        Code::LocalsBudget,
        Code::ExportShape,
    ];
    assert_eq!(codes.len(), 22);
    let mut names: Vec<&str> = codes.iter().map(|code| code.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 22, "codes must be distinct");
    for name in names {
        assert!(
            name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{name} is not a stable identifier"
        );
    }
}
