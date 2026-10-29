//! X8 — the static verifier: a module is checked **before** it can be deployed.
//!
//! X0 answers "may this module run at all?" with a fail-closed census: it bans
//! floats, foreign imports, the start section, shared memory, atomics, SIMD and an
//! unbounded memory, and it whitelists opcodes. That is a *policy* question and it
//! stops at the first refusal.
//!
//! X8 asks a different question — "is this module well-formed enough to execute
//! deterministically?" — and it answers with a **report**: every problem found, with
//! a stable code and a byte offset, not just the first one. Concretely it adds what
//! a policy scan cannot:
//!
//! * **Structure.** Section order and uniqueness, `function`/`code` counts that
//!   agree, a body that ends exactly at its declared length, no trailing bytes.
//! * **Stack discipline.** Every function body is walked with its own signature —
//!   parameters and declared locals from the type section, effect tables for the
//!   accepted opcodes — and the operand stack is tracked: underflow, the published
//!   depth ceiling, branch labels that do not exist, and `call`/`local.*`/`global.*`
//!   indices outside their index spaces.
//! * **Budgets.** Instructions per function and locals per function are capped, so
//!   a deploy cannot ship a body that is guaranteed to burn a whole block's budget
//!   in one call.
//!
//! ## What it does not claim
//!
//! This is **depth** verification, not full type checking: the operand stack is
//! tracked as a count, so a module that is well-typed by neither engine but balanced
//! in depth would pass the stack checks and be rejected at instantiation instead.
//! Saying that plainly is cheaper than pretending to be a complete validator, and
//! the engines remain the authority on instantiation: X8 rejects what no engine may
//! accept, it does not replace one.
//!
//! Golden mutants: `tools/gen_x8_mutants.py` (independent Python mutator) writes 100
//! damaged copies of the shipped artifacts with the expected verdict of each.
//! Report: `docs/evidence/x8/report.md`.

use std::fmt;

use crate::census::{MAX_MEMORY_PAGES, MAX_MODULE_BYTES, WASM_MAGIC, WASM_VERSION};

/// Largest number of instructions one function body may contain.
pub const MAX_FUNCTION_OPS: u32 = 200_000;

/// Largest operand-stack depth one function body may reach.
///
/// A `rustc`-produced contract for this SDK peaks in the tens; the limit exists so
/// that "the stack is bounded" is a property of the module, not of the engine's
/// willingness to allocate.
pub const MAX_STACK_DEPTH: u32 = 4_096;

/// Largest number of locals (parameters included) one function may declare.
pub const MAX_LOCALS: u32 = 1_024;

/// Limits the verifier enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyLimits {
    /// Largest accepted module.
    pub max_module_bytes: usize,
    /// Operand-stack ceiling per function body.
    pub max_stack_depth: u32,
    /// Instruction ceiling per function body.
    pub max_function_ops: u32,
    /// Locals ceiling per function body.
    pub max_locals: u32,
    /// Whether the three ABI import modules may appear.
    pub allow_abi_imports: bool,
}

impl VerifyLimits {
    /// The configuration every deploy must use.
    pub const fn hosted() -> Self {
        VerifyLimits {
            max_module_bytes: MAX_MODULE_BYTES,
            max_stack_depth: MAX_STACK_DEPTH,
            max_function_ops: MAX_FUNCTION_OPS,
            max_locals: MAX_LOCALS,
            allow_abi_imports: true,
        }
    }

    /// The strictest configuration: no imports at all, the same budgets.
    pub const fn restrictive() -> Self {
        VerifyLimits {
            allow_abi_imports: false,
            ..VerifyLimits::hosted()
        }
    }
}

impl Default for VerifyLimits {
    fn default() -> Self {
        VerifyLimits::hosted()
    }
}

/// A stable, machine-readable reason. The string form is part of the evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// Not a WASM binary (magic mismatch).
    BadMagic,
    /// A version other than 1.
    BadVersion,
    /// Larger than the published ceiling.
    ModuleTooLarge,
    /// A section id repeated or out of order.
    SectionOrder,
    /// A section id outside 0..=12.
    SectionUnknown,
    /// A section body ended before it should.
    SectionTruncated,
    /// A value type is a float (banned by X0, re-checked with an offset).
    FloatType,
    /// An import is not one of the three ABI modules.
    ImportNotAllowed,
    /// A global is not an integer.
    GlobalType,
    /// Memory declares no maximum, or a maximum above the ceiling.
    MemoryLimits,
    /// The `function` and `code` sections disagree.
    FunctionCodeMismatch,
    /// A body does not end exactly at its declared length.
    BodyTruncated,
    /// An opcode outside the accepted subset.
    OpcodeBanned,
    /// A function body's operand stack went negative.
    StackUnderflow,
    /// A function body's operand stack exceeded [`MAX_STACK_DEPTH`].
    StackTooDeep,
    /// A branch names a label that does not exist.
    LabelDepth,
    /// `local.get/set/tee` names a local that does not exist.
    LocalIndex,
    /// `global.get/set` names a global that does not exist.
    GlobalIndex,
    /// `call` names a function that does not exist.
    CallIndex,
    /// A body contains more instructions than [`MAX_FUNCTION_OPS`].
    OpcodeBudget,
    /// A function declares more locals than [`MAX_LOCALS`].
    LocalsBudget,
    /// The module does not export a usable `run`.
    ExportShape,
}

impl Code {
    /// The stable string form written into reports and evidence.
    pub fn as_str(self) -> &'static str {
        match self {
            Code::BadMagic => "bad_magic",
            Code::BadVersion => "bad_version",
            Code::ModuleTooLarge => "module_too_large",
            Code::SectionOrder => "section_order",
            Code::SectionUnknown => "section_unknown",
            Code::SectionTruncated => "section_truncated",
            Code::FloatType => "float_type",
            Code::ImportNotAllowed => "import_not_allowed",
            Code::GlobalType => "global_type",
            Code::MemoryLimits => "memory_limits",
            Code::FunctionCodeMismatch => "function_code_mismatch",
            Code::BodyTruncated => "body_truncated",
            Code::OpcodeBanned => "opcode_banned",
            Code::StackUnderflow => "stack_underflow",
            Code::StackTooDeep => "stack_too_deep",
            Code::LabelDepth => "label_depth",
            Code::LocalIndex => "local_index",
            Code::GlobalIndex => "global_index",
            Code::CallIndex => "call_index",
            Code::OpcodeBudget => "opcode_budget",
            Code::LocalsBudget => "locals_budget",
            Code::ExportShape => "export_shape",
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One problem, with the byte offset it was found at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Stable reason.
    pub code: Code,
    /// Byte offset in the module (0 = before the header).
    pub offset: usize,
    /// Human-readable detail — never used to make a decision.
    pub detail: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at 0x{:x}: {}", self.code, self.offset, self.detail)
    }
}

/// The verifier's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// True when `errors` is empty.
    pub ok: bool,
    /// Module size in bytes.
    pub size: usize,
    /// Defined functions (imports excluded).
    pub functions: usize,
    /// Instructions in all bodies.
    pub instructions: u32,
    /// Largest operand-stack depth reached by any body.
    pub max_stack_depth: u32,
    /// Largest instruction count of any single body.
    pub max_function_ops: u32,
    /// Every problem found, in the order they were found.
    pub errors: Vec<Problem>,
}

impl Report {
    /// The stable codes of every problem, in order.
    pub fn codes(&self) -> Vec<&'static str> {
        self.errors
            .iter()
            .map(|problem| problem.code.as_str())
            .collect()
    }

    /// The first problem, if any.
    pub fn first(&self) -> Option<&Problem> {
        self.errors.first()
    }
}

// ------------------------------------------------------------------ cursor

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
    base: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8], base: usize) -> Self {
        Cursor { bytes, at: 0, base }
    }

    fn offset(&self) -> usize {
        self.base + self.at
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }

    fn u8(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        if self.remaining() < len {
            return None;
        }
        let slice = &self.bytes[self.at..self.at + len];
        self.at += len;
        Some(slice)
    }

    fn u32(&mut self) -> Option<u32> {
        let mut result: u32 = 0;
        let mut shift = 0;
        loop {
            let byte = self.u8()?;
            result |= ((byte & 0x7f) as u32) << shift;
            if byte & 0x80 == 0 {
                return Some(result);
            }
            shift += 7;
            if shift >= 35 {
                return None;
            }
        }
    }

    fn skip_signed(&mut self) -> Option<()> {
        loop {
            let byte = self.u8()?;
            if byte & 0x80 == 0 {
                return Some(());
            }
        }
    }

    fn name(&mut self) -> Option<String> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

// ------------------------------------------------------------------- types

/// The two value types the accepted subset uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    /// 32-bit integer.
    I32,
    /// 64-bit integer.
    I64,
}

impl ValType {
    fn from_byte(byte: u8) -> Result<ValType, Code> {
        match byte {
            0x7f => Ok(ValType::I32),
            0x7e => Ok(ValType::I64),
            0x7d | 0x7c => Err(Code::FloatType),
            0x7b => Err(Code::OpcodeBanned),
            _ => Err(Code::SectionTruncated),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct FuncType {
    params: Vec<ValType>,
    results: Vec<ValType>,
}

// ------------------------------------------------------------------- API

/// Verify `bytes` under `limits`, collecting **every** problem.
pub fn verify(bytes: &[u8], limits: &VerifyLimits) -> Report {
    let mut report = Report {
        ok: false,
        size: bytes.len(),
        functions: 0,
        instructions: 0,
        max_stack_depth: 0,
        max_function_ops: 0,
        errors: Vec::new(),
    };

    if bytes.len() > limits.max_module_bytes {
        push(
            &mut report,
            Code::ModuleTooLarge,
            0,
            format!("{} bytes > {} limit", bytes.len(), limits.max_module_bytes),
        );
    }
    if bytes.len() < 8 {
        push(
            &mut report,
            Code::SectionTruncated,
            0,
            "shorter than the wasm header".to_string(),
        );
        return report;
    }
    if bytes[..4] != WASM_MAGIC {
        push(&mut report, Code::BadMagic, 0, "magic mismatch".to_string());
        return report;
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != WASM_VERSION {
        push(
            &mut report,
            Code::BadVersion,
            4,
            format!("version {version}, expected {WASM_VERSION}"),
        );
        return report;
    }

    let mut types: Vec<FuncType> = Vec::new();
    let mut function_types: Vec<u32> = Vec::new();
    let mut imported: Vec<FuncType> = Vec::new();
    let mut imported_functions: u32 = 0;
    let mut globals: u32 = 0;
    let mut code_bodies: Vec<(usize, usize)> = Vec::new(); // (offset, len)
    let mut exports_run = false;
    let mut memory_max: Option<u32> = None;

    let mut cursor = Cursor::new(&bytes[8..], 8);
    let mut last_id = 0u8;
    let mut seen = [false; 13];

    while cursor.remaining() > 0 {
        let id_offset = cursor.offset();
        let Some(id) = cursor.u8() else {
            push(
                &mut report,
                Code::SectionTruncated,
                cursor.offset(),
                "section id".to_string(),
            );
            break;
        };
        let Some(size) = cursor.u32() else {
            push(
                &mut report,
                Code::SectionTruncated,
                id_offset,
                "section size".to_string(),
            );
            break;
        };
        let Some(body) = cursor.take(size as usize) else {
            push(
                &mut report,
                Code::SectionTruncated,
                id_offset,
                format!(
                    "section {id} claims {size} bytes, {} remain",
                    cursor.remaining()
                ),
            );
            break;
        };
        let body_base = cursor.offset() - size as usize;

        if id > 12 {
            push(
                &mut report,
                Code::SectionUnknown,
                id_offset,
                format!("section id {id}"),
            );
            continue;
        }
        if id != 0 {
            if seen[id as usize] {
                push(
                    &mut report,
                    Code::SectionOrder,
                    id_offset,
                    format!("section {id} appears twice"),
                );
            }
            if id < last_id {
                push(
                    &mut report,
                    Code::SectionOrder,
                    id_offset,
                    format!("section {id} after section {last_id}"),
                );
            }
            seen[id as usize] = true;
            last_id = last_id.max(id);
        }

        let mut section = Cursor::new(body, body_base);
        match id {
            1 => types = read_types(&mut section, &mut report),
            2 => {
                imported_functions =
                    read_imports(&mut section, limits, &mut report, &types, &mut imported)
            }
            3 => function_types = read_functions(&mut section, &mut report),
            5 => read_memory(&mut section, &mut report, &mut memory_max),
            6 => globals += read_globals(&mut section, &mut report),
            7 => exports_run = read_exports(&mut section, &mut report),
            10 => code_bodies = read_code(&mut section, &mut report),
            _ => {}
        }
    }

    report.functions = function_types.len();
    if !code_bodies.is_empty() && code_bodies.len() != function_types.len() {
        push(
            &mut report,
            Code::FunctionCodeMismatch,
            cursor.offset(),
            format!(
                "{} functions but {} bodies",
                function_types.len(),
                code_bodies.len()
            ),
        );
    }
    if memory_max.is_none() && seen[5] {
        push(
            &mut report,
            Code::MemoryLimits,
            0,
            "memory declares no maximum".to_string(),
        );
    }
    if !exports_run {
        push(
            &mut report,
            Code::ExportShape,
            0,
            "no exported `run` function".to_string(),
        );
    }

    // Bodies. Every function index is `imported + position`.
    let total_functions = imported_functions + function_types.len() as u32;
    for (index, (offset, len)) in code_bodies.iter().enumerate() {
        if index >= function_types.len() {
            break;
        }
        let type_index = function_types[index] as usize;
        let Some(signature) = types.get(type_index) else {
            push(
                &mut report,
                Code::CallIndex,
                *offset,
                format!("function {index} names type {type_index}, which does not exist"),
            );
            continue;
        };
        // `offset` is absolute (the section cursor carries the module base), so the
        // body can be sliced straight out of the module.
        let start = *offset;
        let slice = if start <= bytes.len() && start + len <= bytes.len() {
            &bytes[start..start + len]
        } else {
            push(
                &mut report,
                Code::BodyTruncated,
                *offset,
                format!("function {index} body runs past the module"),
            );
            continue;
        };
        check_body(
            slice,
            *offset,
            signature,
            &types,
            &imported,
            &function_types,
            total_functions,
            globals,
            limits,
            &mut report,
        );
    }

    report.ok = report.errors.is_empty();
    report
}

fn push(report: &mut Report, code: Code, offset: usize, detail: String) {
    report.errors.push(Problem {
        code,
        offset,
        detail,
    });
}

// -------------------------------------------------------------- read stage

fn read_types(section: &mut Cursor<'_>, report: &mut Report) -> Vec<FuncType> {
    let mut types = Vec::new();
    let Some(count) = section.u32() else {
        push(
            report,
            Code::SectionTruncated,
            section.offset(),
            "type count".to_string(),
        );
        return types;
    };
    for _ in 0..count {
        let offset = section.offset();
        let Some(form) = section.u8() else {
            push(
                report,
                Code::SectionTruncated,
                offset,
                "type form".to_string(),
            );
            return types;
        };
        if form != 0x60 {
            push(
                report,
                Code::SectionTruncated,
                offset,
                format!("type form 0x{form:02x}, expected a function type"),
            );
            return types;
        }
        let mut ty = FuncType::default();
        let Some(params) = section.u32() else {
            return types;
        };
        for _ in 0..params {
            let at = section.offset();
            let Some(byte) = section.u8() else {
                return types;
            };
            match ValType::from_byte(byte) {
                Ok(value) => ty.params.push(value),
                Err(code) => push(report, code, at, format!("parameter type 0x{byte:02x}")),
            }
        }
        let Some(results) = section.u32() else {
            return types;
        };
        for _ in 0..results {
            let at = section.offset();
            let Some(byte) = section.u8() else {
                return types;
            };
            match ValType::from_byte(byte) {
                Ok(value) => ty.results.push(value),
                Err(code) => push(report, code, at, format!("result type 0x{byte:02x}")),
            }
        }
        types.push(ty);
    }
    types
}

fn read_imports(
    section: &mut Cursor<'_>,
    limits: &VerifyLimits,
    report: &mut Report,
    types: &[FuncType],
    imported: &mut Vec<FuncType>,
) -> u32 {
    let mut functions = 0;
    let Some(count) = section.u32() else {
        push(
            report,
            Code::SectionTruncated,
            section.offset(),
            "import count".to_string(),
        );
        return functions;
    };
    for _ in 0..count {
        let at = section.offset();
        let Some(module) = section.name() else {
            push(
                report,
                Code::SectionTruncated,
                at,
                "import module".to_string(),
            );
            return functions;
        };
        let Some(name) = section.name() else {
            push(
                report,
                Code::SectionTruncated,
                at,
                "import name".to_string(),
            );
            return functions;
        };
        let Some(kind) = section.u8() else {
            push(
                report,
                Code::SectionTruncated,
                at,
                "import kind".to_string(),
            );
            return functions;
        };
        let allowed = limits.allow_abi_imports
            && kind == 0x00
            && (crate::abi::is_abi_import(&module, &name)
                || crate::abi_v2::is_abi2_import(&module, &name)
                || crate::abi_v3::is_abi3_import(&module, &name));
        if !allowed {
            push(
                report,
                Code::ImportNotAllowed,
                at,
                format!("import {module}.{name} (kind {kind})"),
            );
        }
        match kind {
            0x00 => {
                let Some(type_index) = section.u32() else {
                    return functions;
                };
                match types.get(type_index as usize) {
                    Some(signature) => imported.push(signature.clone()),
                    None => push(
                        report,
                        Code::CallIndex,
                        at,
                        format!("import {module}.{name} names type {type_index}"),
                    ),
                }
                functions += 1;
            }
            0x01 => {
                let Some(_element) = section.u8() else {
                    return functions;
                };
                if skip_limits(section).is_none() {
                    return functions;
                }
            }
            0x02 => {
                if skip_limits(section).is_none() {
                    return functions;
                }
            }
            0x03 => {
                let Some(_element) = section.u8() else {
                    return functions;
                };
                let Some(_mutable) = section.u8() else {
                    return functions;
                };
            }
            _ => {
                push(
                    report,
                    Code::SectionTruncated,
                    at,
                    format!("import kind {kind}"),
                );
                return functions;
            }
        }
    }
    functions
}

fn skip_limits(section: &mut Cursor<'_>) -> Option<()> {
    let _flag = section.u8()?;
    let _min = section.u32()?;
    let _max = section.u32()?;
    Some(())
}

fn read_functions(section: &mut Cursor<'_>, report: &mut Report) -> Vec<u32> {
    let mut functions = Vec::new();
    let Some(count) = section.u32() else {
        push(
            report,
            Code::SectionTruncated,
            section.offset(),
            "function count".to_string(),
        );
        return functions;
    };
    for _ in 0..count {
        let Some(index) = section.u32() else {
            push(
                report,
                Code::SectionTruncated,
                section.offset(),
                "function type".to_string(),
            );
            return functions;
        };
        functions.push(index);
    }
    functions
}

fn read_memory(section: &mut Cursor<'_>, report: &mut Report, max: &mut Option<u32>) {
    let Some(count) = section.u32() else {
        push(
            report,
            Code::SectionTruncated,
            section.offset(),
            "memory count".to_string(),
        );
        return;
    };
    for _ in 0..count {
        let at = section.offset();
        let Some(flag) = section.u8() else { return };
        let Some(_min) = section.u32() else { return };
        match flag {
            0x00 | 0x01 | 0x03 => {
                if flag == 0x00 {
                    push(
                        report,
                        Code::MemoryLimits,
                        at,
                        "linear memory declares no maximum".to_string(),
                    );
                    return;
                }
                let Some(declared) = section.u32() else {
                    return;
                };
                if declared > MAX_MEMORY_PAGES {
                    push(
                        report,
                        Code::MemoryLimits,
                        at,
                        format!("maximum {declared} pages > {MAX_MEMORY_PAGES}"),
                    );
                }
                *max = Some(declared);
            }
            other => push(
                report,
                Code::SectionTruncated,
                at,
                format!("memory limit flag {other}"),
            ),
        }
    }
}

fn read_globals(section: &mut Cursor<'_>, report: &mut Report) -> u32 {
    let mut count = 0;
    let Some(declared) = section.u32() else {
        push(
            report,
            Code::SectionTruncated,
            section.offset(),
            "global count".to_string(),
        );
        return 0;
    };
    for _ in 0..declared {
        let at = section.offset();
        let Some(ty) = section.u8() else { return count };
        if ValType::from_byte(ty).is_err() {
            push(
                report,
                Code::GlobalType,
                at,
                format!("global type 0x{ty:02x} is not an integer"),
            );
        }
        let Some(_mutable) = section.u8() else {
            return count;
        };
        // The initialiser expression: one instruction plus `end` in practice, but
        // the spec allows a longer constant expression, so skip to the `end`.
        loop {
            let Some(opcode) = section.u8() else {
                return count;
            };
            match opcode {
                0x0b => break,
                0x41 | 0x42 => {
                    if section.skip_signed().is_none() {
                        return count;
                    }
                }
                0x23 => {
                    if section.u32().is_none() {
                        return count;
                    }
                }
                _ => {
                    push(
                        report,
                        Code::GlobalType,
                        at,
                        format!("global initialiser opcode 0x{opcode:02x}"),
                    );
                    return count;
                }
            }
        }
        count += 1;
    }
    count
}

fn read_exports(section: &mut Cursor<'_>, report: &mut Report) -> bool {
    let mut run = false;
    let Some(count) = section.u32() else {
        push(
            report,
            Code::SectionTruncated,
            section.offset(),
            "export count".to_string(),
        );
        return false;
    };
    for _ in 0..count {
        let Some(name) = section.name() else {
            push(
                report,
                Code::SectionTruncated,
                section.offset(),
                "export name".to_string(),
            );
            return run;
        };
        let Some(_kind) = section.u8() else {
            return run;
        };
        let Some(_index) = section.u32() else {
            return run;
        };
        if name == "run" {
            run = true;
        }
    }
    run
}

/// Collect `(body_offset, body_len)` for every function body.
///
/// The offset recorded is the absolute position of the body's length prefix, so a
/// problem inside a body can be reported against the module, not against a slice.
fn read_code(section: &mut Cursor<'_>, report: &mut Report) -> Vec<(usize, usize)> {
    let mut bodies = Vec::new();
    let Some(count) = section.u32() else {
        push(
            report,
            Code::SectionTruncated,
            section.offset(),
            "code count".to_string(),
        );
        return bodies;
    };
    for _ in 0..count {
        let at = section.offset();
        let Some(len) = section.u32() else {
            push(
                report,
                Code::SectionTruncated,
                at,
                "body length".to_string(),
            );
            return bodies;
        };
        // `at` points at the length prefix; the payload starts after it.
        let payload = section.offset();
        if section.take(len as usize).is_none() {
            push(
                report,
                Code::BodyTruncated,
                at,
                format!("body claims {len} bytes, {} remain", section.remaining()),
            );
            return bodies;
        }
        bodies.push((payload, len as usize));
    }
    bodies
}

// ------------------------------------------------------------- body stage

/// One control frame of the body's block stack.
struct Frame {
    /// Operand-stack depth when the frame opened.
    base: u32,
    /// Whether the frame leaves one result on the stack.
    results: u32,
    /// True once the rest of the frame is unreachable.
    dead: bool,
}

#[allow(clippy::too_many_arguments)]
fn check_body(
    body: &[u8],
    base: usize,
    signature: &FuncType,
    types: &[FuncType],
    imported: &[FuncType],
    defined: &[u32],
    functions: u32,
    globals: u32,
    limits: &VerifyLimits,
    report: &mut Report,
) {
    let mut cursor = Cursor::new(body, base);
    let Some(locals) = read_locals(&mut cursor, limits, report) else {
        return;
    };
    let total_locals = signature.params.len() as u32 + locals;

    let mut depth: u32 = 0;
    let mut peak: u32 = 0;
    let mut frames: Vec<Frame> = vec![Frame {
        base: 0,
        results: signature.results.len() as u32,
        dead: false,
    }];
    let mut ops: u32 = 0;

    while cursor.remaining() > 0 {
        let at = cursor.offset();
        let Some(opcode) = cursor.u8() else { break };
        ops += 1;
        if ops > limits.max_function_ops {
            push(
                report,
                Code::OpcodeBudget,
                at,
                format!("more than {} instructions", limits.max_function_ops),
            );
            break;
        }
        let dead = frames.last().map(|frame| frame.dead).unwrap_or(false);

        // Every pop/push goes through these two helpers so the "unreachable code
        // has a polymorphic stack" rule lives in one place.
        macro_rules! pop {
            ($count:expr) => {{
                let count: u32 = $count;
                if !dead {
                    if depth < count {
                        push(
                            report,
                            Code::StackUnderflow,
                            at,
                            format!("opcode 0x{opcode:02x} pops {count}, depth is {depth}"),
                        );
                        return;
                    }
                    depth -= count;
                }
            }};
        }
        macro_rules! pushv {
            ($count:expr) => {{
                let count: u32 = $count;
                if !dead {
                    depth += count;
                    peak = peak.max(depth);
                    if depth > limits.max_stack_depth {
                        push(
                            report,
                            Code::StackTooDeep,
                            at,
                            format!("depth {depth} > {}", limits.max_stack_depth),
                        );
                        return;
                    }
                }
            }};
        }

        match opcode {
            // control
            0x00 => {
                if let Some(frame) = frames.last_mut() {
                    frame.dead = true;
                    depth = frame.base;
                }
            }
            0x01 => {}
            0x02..=0x04 => {
                let Some(results) = block_results(&mut cursor, signature, types, &mut *report, at)
                else {
                    return;
                };
                if opcode == 0x04 {
                    pop!(1);
                }
                frames.push(Frame {
                    base: depth,
                    results,
                    dead: dead && opcode != 0x04,
                });
            }
            0x05 => {
                // `else`: close the `if` arm, reopen the frame for the else arm.
                if frames.len() < 2 {
                    push(
                        report,
                        Code::LabelDepth,
                        at,
                        "`else` outside `if`".to_string(),
                    );
                    return;
                }
                let frame = frames.last_mut().unwrap();
                depth = frame.base;
                frame.dead = false;
            }
            0x0b => {
                // `end`
                if frames.len() == 1 {
                    if cursor.remaining() > 0 {
                        push(
                            report,
                            Code::BodyTruncated,
                            at,
                            format!("{} bytes after the final `end`", cursor.remaining()),
                        );
                    }
                    frames.pop();
                    break;
                }
                let frame = frames.pop().unwrap();
                depth = frame.base;
                if !frame.dead {
                    pushv!(frame.results);
                }
            }
            0x0c => {
                let Some(label) = cursor.u32() else { return };
                if label as usize >= frames.len() {
                    push(
                        report,
                        Code::LabelDepth,
                        at,
                        format!("branch to label {label}, depth {}", frames.len()),
                    );
                    return;
                }
                if let Some(frame) = frames.last_mut() {
                    frame.dead = true;
                    depth = frame.base;
                }
            }
            0x0d => {
                let Some(label) = cursor.u32() else { return };
                if label as usize >= frames.len() {
                    push(
                        report,
                        Code::LabelDepth,
                        at,
                        format!("branch to label {label}, depth {}", frames.len()),
                    );
                    return;
                }
                pop!(1);
            }
            0x0e => {
                let Some(count) = cursor.u32() else { return };
                for _ in 0..count {
                    let Some(label) = cursor.u32() else { return };
                    if label as usize >= frames.len() {
                        push(
                            report,
                            Code::LabelDepth,
                            at,
                            format!("branch to label {label}, depth {}", frames.len()),
                        );
                        return;
                    }
                }
                let Some(_default) = cursor.u32() else { return };
                pop!(1);
                if let Some(frame) = frames.last_mut() {
                    frame.dead = true;
                    depth = frame.base;
                }
            }
            0x0f => {
                if let Some(frame) = frames.last_mut() {
                    frame.dead = true;
                    depth = frame.base;
                }
            }
            0x10 => {
                let Some(index) = cursor.u32() else { return };
                if index >= functions {
                    push(
                        report,
                        Code::CallIndex,
                        at,
                        format!("call {index} of {functions} functions"),
                    );
                    return;
                }
                // The callee's signature comes from the type section: imported
                // functions carry theirs in the import section, defined ones in the
                // function section. This is what makes the stack check real rather
                // than a count of opcodes.
                let signature = if (index as usize) < imported.len() {
                    imported.get(index as usize)
                } else {
                    defined
                        .get(index as usize - imported.len())
                        .and_then(|type_index| types.get(*type_index as usize))
                };
                match signature {
                    Some(signature) => {
                        pop!(signature.params.len() as u32);
                        pushv!(signature.results.len() as u32);
                    }
                    None => pop!(0),
                }
            }
            0x11 => {
                // call_indirect: type index, then table index
                let Some(type_index) = cursor.u32() else {
                    return;
                };
                let Some(table) = cursor.u32() else { return };
                if table != 0 {
                    push(report, Code::OpcodeBanned, at, format!("table {table}"));
                    return;
                }
                let Some(ty) = types.get(type_index as usize) else {
                    push(
                        report,
                        Code::CallIndex,
                        at,
                        format!("call_indirect names type {type_index}"),
                    );
                    return;
                };
                pop!(ty.params.len() as u32 + 1);
                pushv!(ty.results.len() as u32);
            }
            // parametric
            0x1a => pop!(1),
            0x1b => {
                pop!(3);
                pushv!(1);
            }
            // variables
            0x20 => {
                let Some(index) = cursor.u32() else { return };
                check_local(&mut *report, at, index, total_locals);
                pushv!(1);
            }
            0x21 => {
                let Some(index) = cursor.u32() else { return };
                check_local(&mut *report, at, index, total_locals);
                pop!(1);
            }
            0x22 => {
                let Some(index) = cursor.u32() else { return };
                check_local(&mut *report, at, index, total_locals);
                pop!(1);
                pushv!(1);
            }
            0x23 => {
                let Some(index) = cursor.u32() else { return };
                if index >= globals {
                    push(
                        report,
                        Code::GlobalIndex,
                        at,
                        format!("global {index} of {globals}"),
                    );
                    return;
                }
                pushv!(1);
            }
            0x24 => {
                let Some(index) = cursor.u32() else { return };
                if index >= globals {
                    push(
                        report,
                        Code::GlobalIndex,
                        at,
                        format!("global {index} of {globals}"),
                    );
                    return;
                }
                pop!(1);
            }
            // memory
            0x28..=0x35 => {
                let Some(_align) = cursor.u32() else { return };
                let Some(_offset) = cursor.u32() else { return };
                pop!(1);
                pushv!(1);
            }
            0x36..=0x3e => {
                let Some(_align) = cursor.u32() else { return };
                let Some(_offset) = cursor.u32() else { return };
                pop!(2);
            }
            0x3f => {
                let Some(reserved) = cursor.u8() else { return };
                if reserved != 0 {
                    push(
                        report,
                        Code::OpcodeBanned,
                        at,
                        "memory.size immediate".to_string(),
                    );
                    return;
                }
                pushv!(1);
            }
            0x40 => {
                let Some(reserved) = cursor.u8() else { return };
                if reserved != 0 {
                    push(
                        report,
                        Code::OpcodeBanned,
                        at,
                        "memory.grow immediate".to_string(),
                    );
                    return;
                }
                pop!(1);
                pushv!(1);
            }
            // constants
            0x41 | 0x42 => {
                if cursor.skip_signed().is_none() {
                    return;
                }
                pushv!(1);
            }
            // i32 comparisons
            0x45 => {
                pop!(1);
                pushv!(1);
            }
            0x46..=0x4f => {
                pop!(2);
                pushv!(1);
            }
            // i64 comparisons
            0x50 => {
                pop!(1);
                pushv!(1);
            }
            0x51..=0x5a => {
                pop!(2);
                pushv!(1);
            }
            // i32 numeric: clz/ctz/popcnt then binary ops
            0x67..=0x69 => {
                pop!(1);
                pushv!(1);
            }
            0x6a..=0x78 => {
                pop!(2);
                pushv!(1);
            }
            // i64 numeric
            0x79..=0x7b => {
                pop!(1);
                pushv!(1);
            }
            0x7c..=0x8a => {
                pop!(2);
                pushv!(1);
            }
            // integer conversions and sign extension
            0xa7 | 0xac | 0xad | 0xc0..=0xc4 => {
                pop!(1);
                pushv!(1);
            }
            // bulk memory: the two deterministic byte operations only
            0xfc => {
                let Some(sub) = cursor.u32() else { return };
                match sub {
                    10 => {
                        if cursor.u32().is_none() || cursor.u32().is_none() {
                            return;
                        }
                        pop!(3);
                    }
                    11 => {
                        if cursor.u32().is_none() {
                            return;
                        }
                        pop!(3);
                    }
                    other => {
                        push(
                            report,
                            Code::OpcodeBanned,
                            at,
                            format!("0xfc sub-opcode {other}"),
                        );
                        return;
                    }
                }
            }
            other => {
                push(
                    report,
                    Code::OpcodeBanned,
                    at,
                    format!("opcode 0x{other:02x}"),
                );
                return;
            }
        }
    }

    if !frames.is_empty() && frames.len() > 1 {
        push(
            report,
            Code::BodyTruncated,
            cursor.offset(),
            format!("{} unclosed block(s)", frames.len() - 1),
        );
    }
    report.instructions += ops;
    report.max_function_ops = report.max_function_ops.max(ops);
    report.max_stack_depth = report.max_stack_depth.max(peak);
}

fn read_locals(cursor: &mut Cursor<'_>, limits: &VerifyLimits, report: &mut Report) -> Option<u32> {
    let count = cursor.u32()?;
    let mut total: u32 = 0;
    for _ in 0..count {
        let at = cursor.offset();
        let _repeat = cursor.u32()?;
        let ty = cursor.u8()?;
        if ValType::from_byte(ty).is_err() {
            push(
                report,
                Code::FloatType,
                at,
                format!("local type 0x{ty:02x}"),
            );
        }
        total = total.saturating_add(_repeat);
    }
    if total > limits.max_locals {
        push(
            report,
            Code::LocalsBudget,
            cursor.offset(),
            format!("{total} locals > {}", limits.max_locals),
        );
    }
    Some(total)
}

fn check_local(report: &mut Report, at: usize, index: u32, locals: u32) {
    if index >= locals {
        push(
            report,
            Code::LocalIndex,
            at,
            format!("local {index} of {locals}"),
        );
    }
}

fn block_results(
    cursor: &mut Cursor<'_>,
    signature: &FuncType,
    types: &[FuncType],
    report: &mut Report,
    at: usize,
) -> Option<u32> {
    let byte = cursor.u8()?;
    if byte == 0x40 {
        return Some(0);
    }
    if let Ok(ty) = ValType::from_byte(byte) {
        let _ = ty;
        return Some(1);
    }
    // A signed LEB32 type index starts with a byte that is not a valtype; decode it
    // as one (the encodings never collide for the indices we accept).
    if byte & 0x80 != 0 {
        let mut value = (byte & 0x7f) as u32;
        let mut shift = 7;
        loop {
            let next = cursor.u8()?;
            value |= ((next & 0x7f) as u32) << shift;
            if next & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift >= 35 {
                push(report, Code::SectionTruncated, at, "block type".to_string());
                return None;
            }
        }
        let Some(ty) = types.get(value as usize) else {
            push(
                report,
                Code::CallIndex,
                at,
                format!("block type index {value}"),
            );
            return None;
        };
        return Some(ty.results.len() as u32);
    }
    let _ = signature;
    push(
        report,
        Code::SectionTruncated,
        at,
        format!("block type 0x{byte:02x}"),
    );
    None
}
