//! X4 gate (policy side): what the census had to relax for Rust contracts, and
//! what it refuses to relax.
//!
//! The X4 engine gate found three X0 rules that reject **every** `rustc`
//! contract — a mutable global (`__stack_pointer`), `memory.copy`/`memory.fill`
//! (`memcpy`/`memset`) and `call_indirect` (`core::fmt`'s integer formatting).
//! All three were relaxed under `ModuleLimits::hosted()` and none under
//! `ModuleLimits::restrictive()`, so the X0 verdicts stay exactly as shipped.
//!
//! The important half of this file is the **bound**: relaxing `call_indirect`
//! would be indefensible if a function pointer could reach an import, i.e. host
//! code. The census checks the element table for exactly that, and the module
//! below is assembled by hand to prove it — a compiler cannot be trusted to
//! produce the counter-example.

use aura_exec::census::{self, Ban, ModuleLimits, Verdict};

/// Assemble a module with one ABI import, one defined function, a table, and
/// one `call_indirect` whose table entry is `table_target`.
///
/// `table_target == 0` points at the **import** (forbidden); `1` points at the
/// module's own function (allowed under the hosted policy).
fn module_with_indirect_call(table_target: u32) -> Vec<u8> {
    let mut wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    // 1. types: t0 = () -> (), t1 = (i32, i32, i32, i32) -> i32
    let mut types = vec![0x02];
    types.extend([0x60, 0x00, 0x00]);
    types.extend([0x60, 0x04, 0x7f, 0x7f, 0x7f, 0x7f, 0x01, 0x7f]);
    wasm.push(1);
    wasm.push(types.len() as u8);
    wasm.extend(types);

    // 2. imports: env.storage_write, func, type 1
    let name = b"storage_write";
    let mut imports = vec![0x01, 0x03];
    imports.extend(b"env");
    imports.push(name.len() as u8);
    imports.extend(name);
    imports.extend([0x00, 0x01]);
    wasm.push(2);
    wasm.push(imports.len() as u8);
    wasm.extend(imports);

    // 3. functions: one defined function of type 0
    wasm.extend([3, 2, 1, 0]);

    // 4. table: funcref, min 1, max 1 (X0 requires a declared maximum)
    wasm.extend([4, 5, 1, 0x70, 0x01, 0x01, 0x01]);

    // 5. memory: min 1, max 256
    wasm.extend([5, 5, 1, 0x01, 0x01, 0x80, 0x02]);

    // 7. exports: "run" -> function 1
    let mut exports = vec![0x01, 0x03];
    exports.extend(b"run");
    exports.extend([0x00, 0x01]);
    wasm.push(7);
    wasm.push(exports.len() as u8);
    wasm.extend(exports);

    // 9. elements: active segment, table 0, offset 0, one function
    wasm.push(9);
    wasm.push(7);
    wasm.extend([0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, table_target as u8]);

    // 10. code: i32.const 0; call_indirect type 0 table 0; end
    let body = vec![0x00, 0x41, 0x00, 0x11, 0x00, 0x00, 0x0b];
    wasm.push(10);
    wasm.push((body.len() + 2) as u8);
    wasm.push(0x01);
    wasm.push(body.len() as u8);
    wasm.extend(body);

    wasm
}

#[test]
fn an_indirect_call_may_reach_this_module_but_never_an_import() {
    // The counter-example: the table exposes the import, so a function pointer
    // could call host code.
    let hostile = module_with_indirect_call(0);
    let verdict = census::census(&hostile, &ModuleLimits::hosted()).verdict;
    assert_eq!(
        verdict,
        Verdict::Banned(Ban::IndirectIntoImport(0)),
        "the census must refuse a table that exposes an import"
    );

    // Same shape, but the table points at the module's own function.
    let fine = module_with_indirect_call(1);
    let report = census::census(&fine, &ModuleLimits::hosted());
    assert_eq!(report.verdict, Verdict::Allowed, "{:?}", report.verdict);
    assert_eq!(report.indirect_calls, 1, "the call is counted, not ignored");
    assert_eq!(report.table_functions, vec![1]);
}

#[test]
fn the_restrictive_policy_still_bans_what_x0_banned() {
    let fine = module_with_indirect_call(1);
    // Under the restrictive policy this module is banned twice over: the import
    // table is scanned before the code, so `HostImport` is what a caller sees.
    // Both verdicts are X0 bans, and the point is that nothing changed for a
    // module that does not opt into the hosted policy.
    let verdict = census::census(&fine, &ModuleLimits::restrictive()).verdict;
    assert!(
        matches!(
            verdict,
            Verdict::Banned(Ban::HostImport { .. }) | Verdict::Banned(Ban::CallIndirect)
        ),
        "expected an X0 ban under the restrictive policy, got {verdict:?}"
    );
}

#[test]
fn the_hosted_policy_is_a_strict_superset_of_the_restrictive_one() {
    let hosted = ModuleLimits::hosted();
    let restrictive = ModuleLimits::restrictive();
    assert!(hosted.allow_abi_imports && !restrictive.allow_abi_imports);
    assert_eq!(hosted.max_mutable_globals, 1);
    assert_eq!(restrictive.max_mutable_globals, 0);
    assert!(hosted.allow_bulk_memory && !restrictive.allow_bulk_memory);
    assert!(hosted.allow_indirect_calls && !restrictive.allow_indirect_calls);
    // Everything else is the same policy: the relaxation is enumerated, not a
    // second policy that drifts.
    assert_eq!(hosted.max_module_bytes, restrictive.max_module_bytes);
    assert_eq!(hosted.max_memory_pages, restrictive.max_memory_pages);
    assert_eq!(hosted.max_table_entries, restrictive.max_table_entries);
}
