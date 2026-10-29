//! X0: the determinism census — what a WASM module may expose, and what is
//! banned before a contract can even be deployed.
//!
//! Contracts are the main source of consensus divergence: one NaN bit pattern,
//! one WASI clock read, one host import and two nodes disagree inside the same
//! block. X0 **measures that surface first**. It does not run a VM yet — it
//! parses a module and decides, fail-closed, whether the module can ever be
//! deterministic:
//!
//! * [`CENSUS`] is the published table: every source of nondeterminism we know
//!   of, the policy for it (`Ban` / `Allow`) and the layer that enforces the
//!   policy.
//! * [`ModuleLimits::restrictive`] is the restrictive configuration the rest of
//!   the stack must use: bounded module size, bounded linear memory that MUST
//!   declare a maximum, bounded tables, no imports at all, no start function.
//! * [`census`] is the gate. It walks the module — sections, imports, globals,
//!   memory/table limits and every instruction in every function body — and
//!   returns [`Verdict::Banned`] with a machine-readable reason, or
//!   [`Verdict::Allowed`] for a module whose behaviour is a pure function of
//!   its inputs and its own memory.
//!
//! **Fail-closed by construction:** the instruction scanner is a whitelist.
//! An opcode that is not explicitly listed as deterministic is rejected as
//! [`Ban::UnknownOpcode`]. Adding a feature therefore requires a reviewed
//! change to this file, a new census row and a new gate vector — exactly the
//! discipline the rest of the project uses.
//!
//! ## What X0 deliberately does NOT claim
//!
//! Fuel metering, engine-level limits, memory isolation and the actual
//! execution of a module are X1's job (`wasmtime`, pinned). X0 does not link an
//! engine, so it makes **no runtime claim**: the census rows that depend on the
//! engine carry `enforcement = "X1 (engine)"` and say so.
//!
//! Golden vectors: `tools/gen_x0_vectors.py` (independent Python assembler).
//! Report: `docs/evidence/x0/report.md`.

use std::fmt;

/// Largest accepted module, in bytes.
pub const MAX_MODULE_BYTES: usize = 512 * 1024;
/// Largest accepted linear memory, in 64 KiB pages (4096 pages = 256 MiB).
pub const MAX_MEMORY_PAGES: u32 = 4096;
/// Largest accepted table, in entries.
pub const MAX_TABLE_ENTRIES: u32 = 1024;
/// WASM binary magic.
pub const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d];
/// The only accepted binary version (WASM 1.0 / MVP).
pub const WASM_VERSION: u32 = 1;

/// A policy decision for one source of nondeterminism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// The feature is rejected before deploy.
    Ban,
    /// The feature is allowed and its behaviour is a pure function of state.
    Allow,
}

/// One row of the census: a source, its policy and where the policy is enforced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CensusRow {
    /// Stable identifier (`float_instruction`, `host_import`, ...).
    pub source: &'static str,
    /// The WASM feature or proposal involved.
    pub feature: &'static str,
    /// Ban or allow.
    pub policy: Policy,
    /// The layer that enforces it.
    pub enforcement: &'static str,
    /// Why — one sentence, including what would diverge.
    pub note: &'static str,
}

/// The published census table.
///
/// NC target: `tools/nc_x0.py` flips a row's policy and requires the matching
/// gate vector to go red, proving the table and the parser agree.
pub const CENSUS: &[CensusRow] = &[
    CensusRow {
        source: "float_instruction",
        feature: "f32/f64 arithmetic, comparison, conversion and reinterpret",
        policy: Policy::Ban,
        enforcement: "X0 (this module) + X8 (verifier)",
        note: "NaN payloads and their bit patterns are not fully specified, so a rewritten \
               NaN can diverge between engines and CPUs; integers only.",
    },
    CensusRow {
        source: "host_import",
        feature: "import section (any module, any name, including WASI)",
        policy: Policy::Ban,
        enforcement: "X0 (this module) + X1 (engine) + X2 (ABI allowlist)",
        note: "an import is the only way to reach a clock, an RNG or the OS. Since X2 the only \
               permitted imports are the seven ABI v1 functions of module `env`, and only when \
               the caller asks for `ModuleLimits::hosted()`; everything else is refused.",
    },
    CensusRow {
        source: "start_section",
        feature: "start function (implicit execution on instantiate)",
        policy: Policy::Ban,
        enforcement: "X0 (this module)",
        note: "code that runs without a transaction, gas charge or explicit call makes \
               instantiation itself observable and unaccounted.",
    },
    CensusRow {
        source: "shared_memory",
        feature: "threads proposal, shared linear memory (limits flag 0x03)",
        policy: Policy::Ban,
        enforcement: "X0 (this module)",
        note: "concurrent writes make execution order matter; a single-threaded VM cannot \
               reproduce them deterministically.",
    },
    CensusRow {
        source: "atomics",
        feature: "0xFE-prefixed atomic instructions and memory.atomic.wait",
        policy: Policy::Ban,
        enforcement: "X0 (this module)",
        note: "atomics only exist to coordinate threads; `wait` can also block on the host \
               clock indefinitely.",
    },
    CensusRow {
        source: "simd",
        feature: "0xFD-prefixed v128 instructions",
        policy: Policy::Ban,
        enforcement: "X0 (this module)",
        note: "not needed for Stage 1 contracts; every extra instruction set is extra \
               divergence surface between engine versions and CPU features.",
    },
    CensusRow {
        source: "bulk_memory",
        feature: "0xFC-prefixed bulk memory/table ops and saturating truncation",
        policy: Policy::Ban,
        enforcement: "X0 (this module)",
        note: "trunc_sat is a float conversion in disguise, and bulk ops copy memory without \
               per-byte metering in some engines.",
    },
    CensusRow {
        source: "call_indirect",
        feature: "typed indirect call through a table",
        policy: Policy::Ban,
        enforcement: "X0 (this module) + X6 (call isolation)",
        note: "dynamic dispatch makes the call graph — and therefore gas and depth limits — \
               unverifiable statically; X6 may relax this with an explicit depth budget.",
    },
    CensusRow {
        source: "unbounded_memory",
        feature: "memory section without a declared maximum",
        policy: Policy::Ban,
        enforcement: "X0 (this module) + X1 (engine)",
        note: "without a maximum, memory.grow can consume the whole node; every contract \
               must declare its ceiling.",
    },
    CensusRow {
        source: "memory_too_large",
        feature: "declared memory maximum above the Stage 1 cap",
        policy: Policy::Ban,
        enforcement: "X0 (this module)",
        note: "a declaration above MAX_MEMORY_PAGES cannot be met under the node's limits, \
               so it is rejected at deploy instead of failing at run time.",
    },
    CensusRow {
        source: "table_too_large",
        feature: "table without a maximum, or above the cap",
        policy: Policy::Ban,
        enforcement: "X0 (this module)",
        note: "same argument as memory: bounds must be visible before execution.",
    },
    CensusRow {
        source: "mutable_global",
        feature: "mutable global (module-level state outside linear memory)",
        policy: Policy::Ban,
        enforcement: "X0 (this module)",
        note: "state outside the account's storage namespace survives between calls in some \
               engines and not others; contract state belongs in storage, where it is \
               versioned and metered.",
    },
    CensusRow {
        source: "unknown_opcode",
        feature: "any opcode outside the whitelist (reserved, future or prefix)",
        policy: Policy::Ban,
        enforcement: "X0 (this module), fail-closed",
        note: "the scanner is a whitelist: an opcode we did not review is rejected rather \
               than trusted.",
    },
    CensusRow {
        source: "module_too_large",
        feature: "module size above MAX_MODULE_BYTES",
        policy: Policy::Ban,
        enforcement: "X0 (this module)",
        note: "deploy cost must be bounded before a single byte is executed.",
    },
    CensusRow {
        source: "bad_binary",
        feature: "magic, version, section framing",
        policy: Policy::Ban,
        enforcement: "X0 (this module)",
        note: "malformed input is rejected at the boundary so the engine never sees it.",
    },
    CensusRow {
        source: "integer_arithmetic",
        feature: "i32/i64 arithmetic, comparison, wrap/extend, memory load/store, locals, \
                  blocks, loops, branches, direct calls",
        policy: Policy::Allow,
        enforcement: "X0 whitelist; X1 executes with fuel",
        note: "fully specified by the WASM spec: same bytes in, same bytes out, on every \
               engine that implements MVP.",
    },
];

/// The restrictive configuration every deploy must use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleLimits {
    /// Largest accepted module size.
    pub max_module_bytes: usize,
    /// Largest accepted memory, in pages.
    pub max_memory_pages: u32,
    /// Largest accepted table, in entries.
    pub max_table_entries: u32,
    /// Whether the seven ABI v1 functions (`env.*`) may be imported.
    ///
    /// X0/X1 refused **every** import, because the sandbox resolved none. X2
    /// introduced the host ABI, so a contract that uses it must be able to say
    /// so — but only through the closed list in [`crate::abi::ABI_FUNCTIONS`].
    /// The X0 default stays `false`: a module that is merely being *classified*
    /// is still judged with the strictest policy, and the X0 vectors keep their
    /// verdicts.
    pub allow_abi_imports: bool,
}

impl ModuleLimits {
    /// The Stage 1 configuration: bounded size, memory, tables; nothing else.
    pub const fn restrictive() -> Self {
        ModuleLimits {
            max_module_bytes: MAX_MODULE_BYTES,
            max_memory_pages: MAX_MEMORY_PAGES,
            max_table_entries: MAX_TABLE_ENTRIES,
            allow_abi_imports: false,
        }
    }

    /// X2/X3 policy: the same bounds, plus the closed ABI v1 import list.
    pub const fn hosted() -> Self {
        ModuleLimits {
            allow_abi_imports: true,
            ..ModuleLimits::restrictive()
        }
    }

    /// Bytes per page of linear memory.
    pub const fn page_size() -> u32 {
        65_536
    }

    /// Largest accepted memory in bytes.
    pub const fn max_memory_bytes(&self) -> u64 {
        self.max_memory_pages as u64 * 65_536
    }
}

/// Why a module is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ban {
    /// Module is larger than the configured cap.
    ModuleTooLarge(usize),
    /// Missing `\0asm`.
    BadMagic,
    /// Unsupported binary version.
    BadVersion(u32),
    /// The module imports something (clocks, RNG, OS, WASI).
    HostImport {
        /// Import module name.
        module: String,
        /// Import field name.
        name: String,
    },
    /// Shared (threaded) linear memory.
    SharedMemory,
    /// Memory without a declared maximum.
    UnboundedMemory,
    /// Memory maximum above the cap.
    MemoryTooLarge(u32),
    /// Table without a maximum, or above the cap.
    TableTooLarge(u32),
    /// A start function is declared.
    StartSection,
    /// A mutable global is declared.
    MutableGlobal,
    /// A float opcode or float global/local is used.
    FloatInstruction(u8),
    /// SIMD (`0xFD` prefix).
    Simd,
    /// Atomics (`0xFE` prefix).
    Atomics,
    /// Bulk memory / saturating truncation (`0xFC` prefix or data-count section).
    BulkMemory,
    /// Indirect call through a table.
    CallIndirect,
    /// An opcode outside the whitelist.
    UnknownOpcode(u8),
    /// The binary ends early.
    Truncated,
    /// A section is structurally wrong.
    BadSection(&'static str),
    /// A section id appears out of order or twice.
    BadSectionOrder(u8),
}

impl Ban {
    /// Stable machine-readable code, used by the census vectors and evidence.
    pub fn code(&self) -> String {
        match self {
            Ban::ModuleTooLarge(_) => "module_too_large".to_string(),
            Ban::BadMagic => "bad_magic".to_string(),
            Ban::BadVersion(_) => "bad_version".to_string(),
            Ban::HostImport { .. } => "host_import".to_string(),
            Ban::SharedMemory => "shared_memory".to_string(),
            Ban::UnboundedMemory => "unbounded_memory".to_string(),
            Ban::MemoryTooLarge(_) => "memory_too_large".to_string(),
            Ban::TableTooLarge(_) => "table_too_large".to_string(),
            Ban::StartSection => "start_section".to_string(),
            Ban::MutableGlobal => "mutable_global".to_string(),
            Ban::FloatInstruction(_) => "float_instruction".to_string(),
            Ban::Simd => "simd".to_string(),
            Ban::Atomics => "atomics".to_string(),
            Ban::BulkMemory => "bulk_memory".to_string(),
            Ban::CallIndirect => "call_indirect".to_string(),
            Ban::UnknownOpcode(_) => "unknown_opcode".to_string(),
            Ban::Truncated => "truncated".to_string(),
            Ban::BadSection(_) => "bad_section".to_string(),
            Ban::BadSectionOrder(_) => "bad_section_order".to_string(),
        }
    }
}

impl fmt::Display for Ban {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ban::ModuleTooLarge(size) => write!(f, "module of {size} bytes is over the cap"),
            Ban::BadMagic => f.write_str("missing \\0asm magic"),
            Ban::BadVersion(version) => write!(f, "unsupported WASM version {version}"),
            Ban::HostImport { module, name } => {
                write!(
                    f,
                    "module imports {module}::{name}; contracts may not import anything"
                )
            }
            Ban::SharedMemory => f.write_str("shared (threaded) linear memory"),
            Ban::UnboundedMemory => f.write_str("linear memory without a declared maximum"),
            Ban::MemoryTooLarge(pages) => write!(f, "memory of {pages} pages is over the cap"),
            Ban::TableTooLarge(entries) => write!(f, "table of {entries} entries is over the cap"),
            Ban::StartSection => f.write_str("start function would run without a transaction"),
            Ban::MutableGlobal => f.write_str("mutable global (state belongs in storage)"),
            Ban::FloatInstruction(opcode) => {
                write!(f, "float instruction 0x{opcode:02x} (NaN payloads diverge)")
            }
            Ban::Simd => f.write_str("SIMD instruction"),
            Ban::Atomics => f.write_str("atomic instruction"),
            Ban::BulkMemory => f.write_str("bulk memory / saturating truncation instruction"),
            Ban::CallIndirect => f.write_str("indirect call through a table"),
            Ban::UnknownOpcode(opcode) => write!(f, "opcode 0x{opcode:02x} is not whitelisted"),
            Ban::Truncated => f.write_str("binary ends early"),
            Ban::BadSection(name) => write!(f, "malformed {name} section"),
            Ban::BadSectionOrder(id) => write!(f, "section {id} is out of order or duplicated"),
        }
    }
}

impl std::error::Error for Ban {}

/// The verdict for one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The module is a pure function of its inputs and its own memory.
    Allowed,
    /// The module is rejected, with the reason.
    Banned(Ban),
}

impl Verdict {
    /// True for [`Verdict::Allowed`].
    pub fn is_allowed(&self) -> bool {
        matches!(self, Verdict::Allowed)
    }

    /// Machine-readable label used by the golden vectors: `allowed` or
    /// `banned:<code>`.
    pub fn label(&self) -> String {
        match self {
            Verdict::Allowed => "allowed".to_string(),
            Verdict::Banned(ban) => format!("banned:{}", ban.code()),
        }
    }
}

/// What the census learned about a module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CensusReport {
    /// The verdict.
    pub verdict: Verdict,
    /// Module size in bytes.
    pub size: usize,
    /// Section ids in order of appearance.
    pub sections: Vec<u8>,
    /// Function bodies scanned.
    pub functions: u32,
    /// Instructions scanned across all bodies.
    pub instructions: u32,
    /// Imports found `(module, name)`.
    pub imports: Vec<(String, String)>,
    /// Declared memory maximum, if a memory was declared.
    pub memory_pages: Option<u32>,
    /// Declared table maximum, if a table was declared.
    pub table_entries: Option<u32>,
}

impl CensusReport {
    /// True when the module may be deployed.
    pub fn is_allowed(&self) -> bool {
        self.verdict.is_allowed()
    }

    /// The reason, if banned.
    pub fn ban(&self) -> Option<&Ban> {
        match &self.verdict {
            Verdict::Banned(ban) => Some(ban),
            Verdict::Allowed => None,
        }
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Cursor { bytes, position: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }

    fn u8(&mut self) -> Result<u8, Ban> {
        let byte = *self.bytes.get(self.position).ok_or(Ban::Truncated)?;
        self.position += 1;
        Ok(byte)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], Ban> {
        if self.remaining() < len {
            return Err(Ban::Truncated);
        }
        let slice = &self.bytes[self.position..self.position + len];
        self.position += len;
        Ok(slice)
    }

    /// Unsigned LEB128, at most `u32`.
    fn u32(&mut self) -> Result<u32, Ban> {
        let mut value: u32 = 0;
        let mut shift = 0u32;
        loop {
            let byte = self.u8()?;
            let bits = u32::from(byte & 0x7f);
            if shift >= 32 || (shift == 28 && bits > 0x0f) {
                return Err(Ban::BadSection("varint"));
            }
            value |= bits << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
        }
    }

    /// Signed LEB128 of the given byte width (32 or 64).
    fn signed(&mut self, bits: u32) -> Result<i64, Ban> {
        let mut value: i64 = 0;
        let mut shift = 0u32;
        loop {
            let byte = self.u8()?;
            let part = i64::from(byte & 0x7f);
            if shift >= bits {
                return Err(Ban::BadSection("signed varint"));
            }
            value |= part << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                if shift < 64 && byte & 0x40 != 0 {
                    value |= -1i64 << shift;
                }
                return Ok(value);
            }
        }
    }

    fn name(&mut self) -> Result<String, Ban> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// Classify one WASM binary under `limits`.
pub fn census(bytes: &[u8], limits: &ModuleLimits) -> CensusReport {
    let mut report = CensusReport {
        verdict: Verdict::Allowed,
        size: bytes.len(),
        sections: Vec::new(),
        functions: 0,
        instructions: 0,
        imports: Vec::new(),
        memory_pages: None,
        table_entries: None,
    };
    match classify(bytes, limits, &mut report) {
        Ok(()) => report.verdict = Verdict::Allowed,
        Err(ban) => report.verdict = Verdict::Banned(ban),
    }
    report
}

fn classify(bytes: &[u8], limits: &ModuleLimits, report: &mut CensusReport) -> Result<(), Ban> {
    if bytes.len() > limits.max_module_bytes {
        return Err(Ban::ModuleTooLarge(bytes.len()));
    }
    if bytes.len() < 8 {
        return Err(Ban::Truncated);
    }
    if bytes[..4] != WASM_MAGIC {
        return Err(Ban::BadMagic);
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != WASM_VERSION {
        return Err(Ban::BadVersion(version));
    }

    let mut cursor = Cursor::new(&bytes[8..]);
    let mut last_id = 0u8;
    while cursor.remaining() > 0 {
        let id = cursor.u8()?;
        let size = cursor.u32()? as usize;
        let body = cursor.take(size)?;
        report.sections.push(id);
        if id != 0 {
            if id <= last_id || id > 12 {
                return Err(Ban::BadSectionOrder(id));
            }
            last_id = id;
        }
        let mut section = Cursor::new(body);
        match id {
            0 => {
                // Custom section: names and producers are informational only.
                while section.remaining() > 0 {
                    let _ = section.u8()?;
                }
            }
            1 => {
                // Type section: function signatures may mention floats.
                scan_types(&mut section)?;
            }
            2 => scan_imports(&mut section, limits, report)?,
            3 => {
                let count = section.u32()?;
                for _ in 0..count {
                    let _ = section.u32()?;
                }
            }
            4 => scan_tables(&mut section, limits, report)?,
            5 => scan_memories(&mut section, limits, report)?,
            6 => scan_globals(&mut section)?,
            7 => skip_section(&mut section)?,
            8 => return Err(Ban::StartSection),
            9 => skip_section(&mut section)?,
            10 => scan_code(&mut section, report)?,
            11 => skip_section(&mut section)?,
            12 => return Err(Ban::BulkMemory),
            _ => return Err(Ban::BadSectionOrder(id)),
        }
    }
    Ok(())
}

/// The single choke point of the float policy.
///
/// NC target: `tools/nc_x0.py` turns this into `Ok(())` and requires the
/// float programs (`f32_add`, `f64_add`, `f64_nan_const`, `f64_reinterpret`)
/// to stop being banned — i.e. the policy must live in exactly one place.
fn float_ban(opcode: u8) -> Result<(), Ban> {
    Err(Ban::FloatInstruction(opcode))
}

fn valtype(byte: u8) -> Result<(), Ban> {
    match byte {
        0x7f | 0x7e => Ok(()),
        0x7d | 0x7c => float_ban(byte),
        0x7b => Err(Ban::Simd),
        _ => Err(Ban::BadSection("value type")),
    }
}

fn scan_types(section: &mut Cursor<'_>) -> Result<(), Ban> {
    let count = section.u32()?;
    for _ in 0..count {
        let form = section.u8()?;
        if form != 0x60 {
            return Err(Ban::BadSection("function type"));
        }
        for _ in 0..2 {
            let arity = section.u32()?;
            for _ in 0..arity {
                valtype(section.u8()?)?;
            }
        }
    }
    Ok(())
}

fn scan_imports(
    section: &mut Cursor<'_>,
    limits: &ModuleLimits,
    report: &mut CensusReport,
) -> Result<(), Ban> {
    let count = section.u32()?;
    // Every entry is parsed in full, so a malformed import table keeps reporting
    // `bad_binary` instead of leaking through as a policy denial.
    let mut refused: Option<Ban> = None;
    for _ in 0..count {
        let module = section.name()?;
        let name = section.name()?;
        let kind = section.u8()?;
        match kind {
            0x00 => {
                let _ = section.u32()?;
            }
            0x01 => {
                let _ = section.u8()?;
                let flag = section.u8()?;
                let _ = section.u32()?;
                if flag == 0x01 {
                    let _ = section.u32()?;
                }
            }
            0x02 => {
                let flag = section.u8()?;
                let _ = section.u32()?;
                if flag == 0x01 {
                    let _ = section.u32()?;
                }
            }
            0x03 => {
                let _ = section.u8()?;
                let _ = section.u8()?;
            }
            _ => return Err(Ban::BadSection("import kind")),
        }
        report.imports.push((module.clone(), name.clone()));
        // A function import from the closed ABI v1 list is the only allowed one.
        let allowed =
            limits.allow_abi_imports && kind == 0x00 && crate::abi::is_abi_import(&module, &name);
        if !allowed && refused.is_none() {
            refused = Some(Ban::HostImport { module, name });
        }
    }
    match refused {
        Some(ban) => Err(ban),
        None => Ok(()),
    }
}

fn scan_tables(
    section: &mut Cursor<'_>,
    limits: &ModuleLimits,
    report: &mut CensusReport,
) -> Result<(), Ban> {
    let count = section.u32()?;
    for _ in 0..count {
        let element = section.u8()?;
        if element != 0x70 {
            return Err(Ban::BadSection("table element type"));
        }
        let flag = section.u8()?;
        let min = section.u32()?;
        let max = match flag {
            0x00 => None,
            0x01 => Some(section.u32()?),
            _ => return Err(Ban::SharedMemory),
        };
        let Some(max) = max else {
            return Err(Ban::TableTooLarge(min));
        };
        if max > limits.max_table_entries || min > max {
            return Err(Ban::TableTooLarge(max));
        }
        report.table_entries = Some(max);
    }
    Ok(())
}

fn scan_memories(
    section: &mut Cursor<'_>,
    limits: &ModuleLimits,
    report: &mut CensusReport,
) -> Result<(), Ban> {
    let count = section.u32()?;
    for _ in 0..count {
        let flag = section.u8()?;
        let min = section.u32()?;
        let max = match flag {
            0x00 => None,
            0x01 => Some(section.u32()?),
            0x03 => return Err(Ban::SharedMemory),
            _ => return Err(Ban::BadSection("memory limits")),
        };
        let Some(max) = max else {
            return Err(Ban::UnboundedMemory);
        };
        if max > limits.max_memory_pages || min > max {
            return Err(Ban::MemoryTooLarge(max));
        }
        report.memory_pages = Some(max);
    }
    Ok(())
}

fn scan_globals(section: &mut Cursor<'_>) -> Result<(), Ban> {
    let count = section.u32()?;
    for _ in 0..count {
        let kind = section.u8()?;
        valtype(kind)?;
        let mutable = section.u8()?;
        if mutable == 0x01 {
            return Err(Ban::MutableGlobal);
        }
        // init expression: one const instruction plus `end`
        loop {
            let opcode = section.u8()?;
            match opcode {
                0x0b => break,
                0x41 => {
                    let _ = section.signed(32)?;
                }
                0x42 => {
                    let _ = section.signed(64)?;
                }
                0x00 | 0x01 => {}
                _ => return Err(Ban::BadSection("global init")),
            }
        }
    }
    Ok(())
}

fn skip_section(section: &mut Cursor<'_>) -> Result<(), Ban> {
    section.take(section.remaining())?;
    Ok(())
}

fn scan_code(section: &mut Cursor<'_>, report: &mut CensusReport) -> Result<(), Ban> {
    let count = section.u32()?;
    for _ in 0..count {
        let size = section.u32()? as usize;
        let body = section.take(size)?;
        report.functions += 1;
        let mut cursor = Cursor::new(body);
        let locals = cursor.u32()?;
        for _ in 0..locals {
            let repeat = cursor.u32()?;
            if repeat > 50_000 {
                return Err(Ban::BadSection("local count"));
            }
            valtype(cursor.u8()?)?;
        }
        scan_instructions(&mut cursor, report)?;
    }
    Ok(())
}

/// The whitelist. Anything not listed here is `UnknownOpcode` (fail-closed).
fn scan_instructions(cursor: &mut Cursor<'_>, report: &mut CensusReport) -> Result<(), Ban> {
    let mut depth = 0i64;
    while cursor.remaining() > 0 {
        let opcode = cursor.u8()?;
        report.instructions += 1;
        match opcode {
            // control
            0x00 | 0x01 | 0x05 | 0x0f | 0x1a | 0x1b => {}
            0x02..=0x04 => {
                scan_block_type(cursor)?;
                depth += 1;
            }
            0x0b => {
                // `end` closes a block; the body's final `end` brings us to zero.
                if depth == 0 {
                    if cursor.remaining() > 0 {
                        return Err(Ban::BadSection("trailing bytes after body end"));
                    }
                    return Ok(());
                }
                depth -= 1;
            }
            0x0c | 0x0d | 0x10 | 0x20 | 0x21 | 0x22 | 0x23 | 0x24 => {
                let _ = cursor.u32()?;
            }
            0x0e => {
                let labels = cursor.u32()?;
                for _ in 0..labels {
                    let _ = cursor.u32()?;
                }
                let _ = cursor.u32()?;
            }
            0x11 => return Err(Ban::CallIndirect),
            0x28..=0x3e => {
                let _ = cursor.u32()?;
                let _ = cursor.u32()?;
            }
            0x3f | 0x40 => {
                let reserved = cursor.u8()?;
                if reserved != 0x00 {
                    return Err(Ban::BadSection("memory immediate"));
                }
            }
            0x41 => {
                let _ = cursor.signed(32)?;
            }
            0x42 => {
                let _ = cursor.signed(64)?;
            }
            // f32.const / f64.const carry float immediates, NaN payloads included
            0x43 | 0x44 => return float_ban(opcode),
            // i32/i64 comparison: deterministic
            0x45..=0x5a => {}
            // f32/f64 comparison: banned
            0x5b..=0x66 => return float_ban(opcode),
            // i32/i64 numeric: deterministic
            0x67..=0x8a => {}
            // f32/f64 numeric: banned
            0x8b..=0x98 => return float_ban(opcode),
            // conversions: 0xa7 wrap, 0xac/0xad extend are integer-only
            0xa7 | 0xac | 0xad => {}
            0xa8..=0xab | 0xae..=0xbb => return float_ban(opcode),
            // sign extension proposal (integer)
            0xc0..=0xc4 => {}
            0xfc => return Err(Ban::BulkMemory),
            0xfd => return Err(Ban::Simd),
            0xfe => return Err(Ban::Atomics),
            other => return Err(Ban::UnknownOpcode(other)),
        }
    }
    if depth == 0 {
        Err(Ban::Truncated)
    } else {
        Err(Ban::BadSection("unterminated block"))
    }
}

fn scan_block_type(cursor: &mut Cursor<'_>) -> Result<(), Ban> {
    let byte = cursor.u8()?;
    match byte {
        0x40 => Ok(()),
        0x7f | 0x7e => Ok(()),
        0x7d | 0x7c => float_ban(byte),
        0x7b => Err(Ban::Simd),
        // a positive type index is encoded as a signed LEB128 >= 0
        other => {
            let mut shift = 7u32;
            let mut value = i64::from(other & 0x7f);
            let mut current = other;
            while current & 0x80 != 0 {
                current = cursor.u8()?;
                value |= i64::from(current & 0x7f) << shift;
                shift += 7;
                if shift > 35 {
                    return Err(Ban::BadSection("block type"));
                }
            }
            if value < 0 {
                return Err(Ban::BadSection("block type"));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn census_table_covers_every_ban_variant() {
        // Every Ban must be reachable from a documented census row: the table
        // and the parser must not drift apart.
        let codes: Vec<String> = CENSUS.iter().map(|row| row.source.to_string()).collect();
        let bans = [
            Ban::ModuleTooLarge(0),
            Ban::BadMagic,
            Ban::BadVersion(0),
            Ban::HostImport {
                module: String::new(),
                name: String::new(),
            },
            Ban::SharedMemory,
            Ban::UnboundedMemory,
            Ban::MemoryTooLarge(0),
            Ban::TableTooLarge(0),
            Ban::StartSection,
            Ban::MutableGlobal,
            Ban::FloatInstruction(0),
            Ban::Simd,
            Ban::Atomics,
            Ban::BulkMemory,
            Ban::CallIndirect,
            Ban::UnknownOpcode(0),
        ];
        for ban in bans {
            let code = ban.code();
            if code == "bad_section"
                || code == "bad_section_order"
                || code == "truncated"
                || code == "bad_magic"
                || code == "bad_version"
            {
                assert!(codes.iter().any(|row| row == "bad_binary"));
                continue;
            }
            assert!(
                codes.iter().any(|row| row == &code),
                "census table has no row for {code}"
            );
        }
        assert!(codes.iter().any(|row| row == "call_indirect"));
        assert!(
            CENSUS
                .iter()
                .any(|row| row.source == "integer_arithmetic" && row.policy == Policy::Allow)
        );
    }

    #[test]
    fn empty_module_is_allowed_and_empty_input_is_not() {
        let empty = [WASM_MAGIC, WASM_VERSION.to_le_bytes()].concat();
        let report = census(&empty, &ModuleLimits::restrictive());
        assert!(report.is_allowed(), "{:?}", report.verdict);
        assert_eq!(report.size, 8);
        assert_eq!(report.functions, 0);

        assert_eq!(
            census(&[], &ModuleLimits::restrictive()).verdict,
            Verdict::Banned(Ban::Truncated)
        );
        assert_eq!(
            census(b"\0asx\x01\0\0\0", &ModuleLimits::restrictive()).verdict,
            Verdict::Banned(Ban::BadMagic)
        );
        let mut bad_version = b"\0asm".to_vec();
        bad_version.extend_from_slice(&2u32.to_le_bytes());
        assert_eq!(
            census(&bad_version, &ModuleLimits::restrictive()).verdict,
            Verdict::Banned(Ban::BadVersion(2))
        );
    }

    #[test]
    fn module_size_cap_is_enforced_before_parsing() {
        let mut limits = ModuleLimits::restrictive();
        limits.max_module_bytes = 16;
        let empty = [WASM_MAGIC, WASM_VERSION.to_le_bytes()].concat();
        assert!(census(&empty, &limits).is_allowed());
        let padded = [empty.clone(), vec![0u8; 32]].concat();
        assert!(matches!(
            census(&padded, &limits).verdict,
            Verdict::Banned(Ban::ModuleTooLarge(_))
        ));
    }
}
