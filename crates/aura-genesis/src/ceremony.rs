//! W7 — offline genesis ceremony: deterministic generation, Shamir sharing,
//! witness logs and destruction of temporary media.
//!
//! The ceremony is a sequence of CLI steps run on an air-gapped host
//! (`src/bin/aura-ceremony.rs` drives [`run`]): `open`, `generate`, `split`,
//! `shred`, `seal`, `reconstruct`, `verify`. Secrets live only in `media/`
//! files until `shred` destroys them (D-W7-4); every step is witnessed by a
//! hash-chained log (D-W7-3); the sealed genesis commits to exactly the
//! witnessed pubkey set (D-W7-5) and `verify` scans for leftover private
//! material ("not one private key in the repository/CI").
//!
//! Design of record: `docs/w7-genesis-ceremony-plan.md` (D-W7-1..D-W7-5).
//! Vectors: `tools/gen_w7_vectors.py` ->
//! `crates/aura-genesis/tests/fixtures/w7_ceremony_vectors.json`, asserted
//! byte-for-byte by `tests/w7_ceremony.rs`.
//!
//! NC targets (`tools/nc_w7.py`):
//! 1. shredding skipped -> `no_private_key_in_outputs` red (the named
//!    control: the O5 gate);
//! 2. threshold check dropped -> `shamir_one_share_refused` red;
//! 3. witness chain check dropped -> `witness_log_tamper_detected` red;
//! 4. pubkey-set binding dropped -> `pubkey_set_mismatch_refused` red;
//! 5. reconstruction corrupted -> `shamir_reconstructs_2_of_3` red;
//! 6. media-present check dropped -> `seal_before_shred_refused` red.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use aura_crypto::{blake3_hash, ed25519_public_key};
use aura_tx::signer_address;
use aura_types::{Address, Amount, ChainId, PROTOCOL_VERSION};

use crate::types::{Allocation, Genesis, Validator};

/// Context prefix for ceremony key derivation (D-W7-1).
pub const KEY_CONTEXT: &[u8] = b"AuraLite-Crypto/ceremony-key-v1\x00";

/// Context prefix for Shamir polynomial coefficients (D-W7-2).
pub const SHARE_CONTEXT: &[u8] = b"AuraLite-Crypto/ceremony-share-v1\x00";

/// Context prefix for the witness hash chain (D-W7-3).
pub const WITNESS_CONTEXT: &[u8] = b"AuraLite-Crypto/ceremony-witness-v1\x00";

/// Context prefix for the reconstructed-secret commitment (D-W7-2).
pub const COMMIT_CONTEXT: &[u8] = b"AuraLite-Crypto/ceremony-commit-v1\x00";

/// Exit code for policy refusals.
const EXIT_REFUSED: i32 = 65;

/// Exit code for usage errors.
const EXIT_USAGE: i32 = 64;

/// Mark a message as a usage (argument) error inside `run`.
fn usage(message: impl Into<String>) -> String {
    format!("usage:{}", message.into())
}

// --------------------------------------------------------------------------- GF(256)

/// Arithmetic tables for GF(2^8) with the AES polynomial `0x11b`.
struct Gf {
    exp: [u8; 512],
    log: [u8; 256],
}

impl Gf {
    fn new() -> Self {
        let mut exp = [0u8; 512];
        let mut log = [0u8; 256];
        // Multiplicative generator 3: 2 has order 51 under 0x11b, so the
        // tables must step x *= 3 (x ^ x<<1, reduced) to cover all of
        // GF(256)*.
        let mut x: u16 = 1;
        for (index, slot) in exp.iter_mut().take(255).enumerate() {
            *slot = x as u8;
            log[usize::from(x as u8)] = index as u8;
            x ^= x << 1;
            if x & 0x100 != 0 {
                x ^= 0x11b;
            }
        }
        let (head, tail) = exp.split_at_mut(255);
        tail[..255].copy_from_slice(head);
        tail[255..257].copy_from_slice(&head[..2]);
        Gf { exp, log }
    }

    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            return 0;
        }
        self.exp[usize::from(self.log[usize::from(a)]) + usize::from(self.log[usize::from(b)])]
    }

    fn inv(&self, a: u8) -> u8 {
        debug_assert!(a != 0);
        self.exp[255 - usize::from(self.log[usize::from(a)])]
    }
}

/// Split one 32-byte secret into `shares` shares with quorum `quorum`
/// (Shamir over GF(256), per byte, deterministic coefficients — D-W7-2).
fn split_secret(secret: &[u8; 32], key_index: u32, quorum: u8, shares: u8) -> Vec<(u8, [u8; 32])> {
    let gf = Gf::new();
    (1..=shares)
        .map(|x| {
            let mut y = [0u8; 32];
            for (byte, &plain) in secret.iter().enumerate() {
                let mut input = Vec::with_capacity(SHARE_CONTEXT.len() + 32 + 8);
                input.extend_from_slice(SHARE_CONTEXT);
                input.extend_from_slice(secret);
                input.extend_from_slice(&key_index.to_le_bytes());
                input.extend_from_slice(&(byte as u32).to_le_bytes());
                let block = blake3_hash(&input);
                let mut acc = 0u8;
                for k in (0..quorum as usize).rev() {
                    let coeff = if k == 0 { plain } else { block.as_bytes()[k] };
                    acc = gf.mul(acc, x) ^ coeff;
                }
                y[byte] = acc;
            }
            (x, y)
        })
        .collect()
}

/// Reconstruct the secret from at least `quorum` distinct shares
/// (Lagrange interpolation at 0 — D-W7-2).
fn reconstruct_secret(shares: &[(u8, [u8; 32])]) -> [u8; 32] {
    let gf = Gf::new();
    let mut secret = [0u8; 32];
    for byte in 0..32 {
        let mut acc = 0u8;
        for (j, (x_j, y_j)) in shares.iter().enumerate() {
            let mut num = 1u8;
            let mut den = 1u8;
            for (m, (x_m, _)) in shares.iter().enumerate() {
                if m == j {
                    continue;
                }
                num = gf.mul(num, *x_m);
                den = gf.mul(den, x_m ^ x_j);
            }
            let basis = gf.mul(num, gf.inv(den));
            acc ^= gf.mul(y_j[byte], basis);
        }
        // Interpolation point is 0 (NC-5 target).
        secret[byte] = acc;
    }
    secret
}

// --------------------------------------------------------------------------- witness log

/// The head of the empty witness chain.
fn chain_root() -> [u8; 32] {
    let mut input = Vec::with_capacity(WITNESS_CONTEXT.len() + 21);
    input.extend_from_slice(WITNESS_CONTEXT);
    input.extend_from_slice(b"genesis-ceremony-log-v1");
    blake3_hash(&input).as_bytes().to_owned()
}

/// Next chain head for one entry text.
fn chain_step(prev: &[u8; 32], entry: &str) -> [u8; 32] {
    let mut input = Vec::with_capacity(WITNESS_CONTEXT.len() + 32 + entry.len());
    input.extend_from_slice(WITNESS_CONTEXT);
    input.extend_from_slice(prev);
    input.extend_from_slice(entry.as_bytes());
    blake3_hash(&input).as_bytes().to_owned()
}

/// One parsed witness line.
struct WitnessLine {
    /// The chain head after this entry (hex).
    head: String,
    /// The entry text (without the head).
    entry: String,
}

/// Parse `witness.log` lines of the form `<head-hex> <entry>`.
fn parse_witness(text: &str) -> Result<Vec<WitnessLine>, String> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        if raw.is_empty() {
            continue;
        }
        let (head, entry) = raw.split_once(' ').ok_or("witness chain broken")?;
        if head.len() != 64 {
            return Err("witness chain broken".to_string());
        }
        lines.push(WitnessLine {
            head: head.to_string(),
            entry: entry.to_string(),
        });
    }
    Ok(lines)
}

/// Recompute the witness chain; any tampering is `witness chain broken`.
fn verify_chain(lines: &[WitnessLine]) -> Result<[u8; 32], String> {
    let mut prev = chain_root();
    for line in lines {
        let next = chain_step(&prev, &line.entry);
        if hex(&next) != line.head {
            return Err("witness chain broken".to_string());
        }
        prev = next;
    }
    Ok(prev)
}

/// Append one witnessed entry and return its chain head (hex).
fn witness(dir: &Path, entry: &str) -> Result<String, String> {
    let path = dir.join("witness.log");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let lines = parse_witness(&text)?;
    let prev = verify_chain(&lines)?;
    let head = chain_step(&prev, entry);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|_| "witness write failed".to_string())?;
    writeln!(file, "{} {entry}", hex(&head)).map_err(|_| "witness write failed".to_string())?;
    Ok(hex(&head))
}

// --------------------------------------------------------------------------- helpers

/// Lowercase hex of a byte slice.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decode a hex string into raw bytes (self-contained: no hex crate).
fn unhex(text: &str) -> Result<Vec<u8>, String> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || !bytes.len().is_multiple_of(2) {
        return Err("bad hex".to_string());
    }
    let nibble = |c: u8| -> Result<u8, String> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err("bad hex".to_string()),
        }
    };
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks(2) {
        out.push(nibble(pair[0])? * 16 + nibble(pair[1])?);
    }
    Ok(out)
}

/// Decode a whitespace-stripped hex file into raw bytes.
fn read_hex_file(path: &str) -> Result<Vec<u8>, String> {
    let text = std::fs::read_to_string(path).map_err(|_| "bad entropy file".to_string())?;
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let out = unhex(&cleaned).map_err(|_| "bad entropy file".to_string())?;
    if out.len() != 16 && out.len() != 24 && out.len() != 32 {
        return Err("bad entropy file".to_string());
    }
    Ok(out)
}

/// Overwrite a file twice (0x00, then 0xFF) and remove it (D-W7-4).
fn shred_file(path: &Path) -> std::io::Result<()> {
    let len = std::fs::metadata(path)?.len() as usize;
    std::fs::write(path, vec![0x00u8; len])?;
    std::fs::write(path, vec![0xFFu8; len])?;
    std::fs::remove_file(path)
}

/// Collect private material under `root`: any `media/` file or `secret=` line.
fn scan_private(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let mut flagged = relative.split('/').any(|part| part == "media");
            if !flagged && let Ok(text) = std::fs::read_to_string(&path) {
                flagged = text.lines().any(|line| line.starts_with("secret="));
            }
            if flagged {
                found.push(relative);
            }
        }
    }
    found.sort();
    found
}

// --------------------------------------------------------------------------- files

/// The parameters recorded by `open`.
struct Params {
    chain: String,
    validators: u32,
    quorum: u8,
    shares: u8,
    power: u64,
    genesis_time_ms: u64,
    allocations: Vec<(String, u64)>,
}

/// The public manifest recorded by `generate`.
struct Manifest {
    /// `(index, address bech32, pubkey hex)` in declaration order.
    validators: Vec<(u32, String, String)>,
}

fn dir_of(map: &BTreeMap<String, Vec<String>>) -> Result<PathBuf, String> {
    one(map, "ceremony-dir").map(PathBuf::from)
}

fn one(map: &BTreeMap<String, Vec<String>>, name: &str) -> Result<String, String> {
    match map.get(name) {
        Some(values) if values.len() == 1 && !values[0].is_empty() => Ok(values[0].clone()),
        Some(values) if values.len() == 1 => Err(usage(format!("empty --{name}"))),
        Some(_) => Err(usage(format!("duplicate --{name}"))),
        None => Err(usage(format!("missing --{name}"))),
    }
}

fn write_params(dir: &Path, params: &Params) -> std::io::Result<()> {
    let mut text = String::from("ceremony params v1\n");
    text.push_str(&format!("chain={}\n", params.chain));
    text.push_str(&format!("validators={}\n", params.validators));
    text.push_str(&format!("quorum={}\n", params.quorum));
    text.push_str(&format!("shares={}\n", params.shares));
    text.push_str(&format!("power={}\n", params.power));
    text.push_str(&format!("genesis_time_ms={}\n", params.genesis_time_ms));
    for (index, (address, balance)) in params.allocations.iter().enumerate() {
        text.push_str(&format!(
            "allocation={index} address={address} balance={balance}\n"
        ));
    }
    std::fs::write(dir.join("params.txt"), text)
}

fn read_params(dir: &Path) -> Result<Params, String> {
    let text =
        std::fs::read_to_string(dir.join("params.txt")).map_err(|_| "not open".to_string())?;
    if !text.starts_with("ceremony params v1\n") {
        return Err("not open".to_string());
    }
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut allocations = Vec::new();
    for line in text.lines().skip(1) {
        if let Some(rest) = line.strip_prefix("allocation=") {
            let mut address = String::new();
            let mut balance = 0u64;
            for item in rest.split_whitespace() {
                if let Some(value) = item.strip_prefix("address=") {
                    address = value.to_string();
                } else if let Some(value) = item.strip_prefix("balance=") {
                    balance = value.parse().map_err(|_| "bad params".to_string())?;
                }
            }
            if address.is_empty() {
                return Err("bad params".to_string());
            }
            allocations.push((address, balance));
        } else if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.to_string(), value.to_string());
        }
    }
    let get = |key: &str| -> Result<String, String> {
        fields
            .get(key)
            .cloned()
            .ok_or_else(|| "bad params".to_string())
    };
    Ok(Params {
        chain: get("chain")?,
        validators: get("validators")?
            .parse()
            .map_err(|_| "bad params".to_string())?,
        quorum: get("quorum")?
            .parse()
            .map_err(|_| "bad params".to_string())?,
        shares: get("shares")?
            .parse()
            .map_err(|_| "bad params".to_string())?,
        power: get("power")?
            .parse()
            .map_err(|_| "bad params".to_string())?,
        genesis_time_ms: get("genesis_time_ms")?
            .parse()
            .map_err(|_| "bad params".to_string())?,
        allocations,
    })
}

fn write_manifest(dir: &Path, manifest: &Manifest) -> std::io::Result<()> {
    let mut text = String::from("ceremony manifest v1\n");
    for (index, address, pubkey) in &manifest.validators {
        text.push_str(&format!(
            "validator={index} address={address} pubkey={pubkey}\n"
        ));
    }
    std::fs::write(dir.join("manifest.txt"), text)
}

fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let text =
        std::fs::read_to_string(dir.join("manifest.txt")).map_err(|_| "not open".to_string())?;
    if !text.starts_with("ceremony manifest v1\n") {
        return Err("not open".to_string());
    }
    let mut validators = Vec::new();
    for line in text.lines().skip(1) {
        let rest = line
            .strip_prefix("validator=")
            .ok_or_else(|| "bad manifest".to_string())?;
        let mut parts = rest.split_whitespace();
        let index: u32 = parts
            .next()
            .and_then(|first| first.parse().ok())
            .ok_or_else(|| "bad manifest".to_string())?;
        let mut address = String::new();
        let mut pubkey = String::new();
        for item in parts {
            if let Some(value) = item.strip_prefix("address=") {
                address = value.to_string();
            } else if let Some(value) = item.strip_prefix("pubkey=") {
                pubkey = value.to_string();
            }
        }
        if address.is_empty() || pubkey.len() != 64 {
            return Err("bad manifest".to_string());
        }
        validators.push((index, address, pubkey));
    }
    if validators.is_empty() {
        return Err("bad manifest".to_string());
    }
    Ok(Manifest { validators })
}

// --------------------------------------------------------------------------- steps

fn cmd_open(map: &BTreeMap<String, Vec<String>>, out: &mut dyn Write) -> Result<(), String> {
    let dir = dir_of(map)?;
    let chain = one(map, "chain")?;
    let parse = |name: &str| -> Result<u64, String> {
        one(map, name)?
            .parse()
            .map_err(|_| usage(format!("bad --{name}")))
    };
    let validators = parse("validators")?;
    let quorum = parse("quorum")?;
    let shares = parse("shares")?;
    let power = parse("power")?;
    let genesis_time_ms = parse("genesis-time")?;
    if validators == 0 || validators > 64 {
        return Err("invalid validators".to_string());
    }
    if power == 0 {
        return Err("invalid power".to_string());
    }
    if quorum == 0 || quorum > shares || shares > 255 {
        return Err("invalid quorum".to_string());
    }
    let mut allocations = Vec::new();
    for item in map.get("allocation").cloned().unwrap_or_default() {
        let (address, balance) = item
            .split_once(':')
            .ok_or_else(|| "bad allocation".to_string())?;
        let balance: u64 = balance.parse().map_err(|_| "bad allocation".to_string())?;
        Address::from_bech32(address).map_err(|_| "bad allocation".to_string())?;
        allocations.push((address.to_string(), balance));
    }
    let params = Params {
        chain: chain.clone(),
        validators: validators as u32,
        quorum: quorum as u8,
        shares: shares as u8,
        power,
        genesis_time_ms,
        allocations,
    };
    std::fs::create_dir_all(dir.join("media")).map_err(|_| "io error".to_string())?;
    std::fs::create_dir_all(dir.join("shares")).map_err(|_| "io error".to_string())?;
    write_params(&dir, &params).map_err(|_| "io error".to_string())?;
    let entry = format!(
        "open chain={chain} validators={validators} quorum={quorum} shares={shares} power={power} genesis_time_ms={genesis_time_ms}"
    );
    witness(&dir, &entry)?;
    let _ = writeln!(out, "status=opened");
    Ok(())
}

fn cmd_generate(map: &BTreeMap<String, Vec<String>>, out: &mut dyn Write) -> Result<(), String> {
    let dir = dir_of(map)?;
    let params = read_params(&dir)?;
    let entropy = read_hex_file(&one(map, "entropy-file")?)?;
    let mut manifest = Manifest {
        validators: Vec::new(),
    };
    for index in 0..params.validators {
        let mut input = Vec::with_capacity(KEY_CONTEXT.len() + entropy.len() + 4);
        input.extend_from_slice(KEY_CONTEXT);
        input.extend_from_slice(&entropy);
        input.extend_from_slice(&index.to_le_bytes());
        let secret = blake3_hash(&input);
        let public = ed25519_public_key(secret.as_bytes());
        let address = signer_address(&public);
        let media = dir.join("media").join(format!("key-{index:02}.key"));
        std::fs::write(&media, format!("secret={}\n", hex(secret.as_bytes())))
            .map_err(|_| "io error".to_string())?;
        let entry = format!(
            "generate i={index} pubkey={} address={}",
            hex(&public),
            address.to_bech32()
        );
        witness(&dir, &entry)?;
        manifest
            .validators
            .push((index, address.to_bech32(), hex(&public)));
    }
    write_manifest(&dir, &manifest).map_err(|_| "io error".to_string())?;
    let _ = writeln!(out, "status=generated");
    for (index, address, pubkey) in &manifest.validators {
        let _ = writeln!(out, "validator={index} address={address} pubkey={pubkey}");
    }
    Ok(())
}

fn cmd_split(map: &BTreeMap<String, Vec<String>>, out: &mut dyn Write) -> Result<(), String> {
    let dir = dir_of(map)?;
    let params = read_params(&dir)?;
    let manifest = read_manifest(&dir)?;
    let mut share_files = 0u32;
    for (index, _, _) in &manifest.validators {
        let media = dir.join("media").join(format!("key-{index:02}.key"));
        let text = std::fs::read_to_string(&media).map_err(|_| "no keys".to_string())?;
        let hex_secret = text
            .trim()
            .strip_prefix("secret=")
            .ok_or("bad media".to_string())?;
        let raw = unhex(hex_secret).map_err(|_| "bad media".to_string())?;
        let secret: [u8; 32] = raw.try_into().map_err(|_| "bad media".to_string())?;
        for (x, y) in split_secret(&secret, *index, params.quorum, params.shares) {
            let path = dir
                .join("shares")
                .join(format!("key-{index:02}-share-{x:02}.share"));
            let body = format!(
                "ceremony share v1\nkey={index}\nx={x}\nquorum={}\ny={}\n",
                params.quorum,
                hex(&y)
            );
            std::fs::write(&path, body).map_err(|_| "io error".to_string())?;
            share_files += 1;
        }
        witness(
            &dir,
            &format!(
                "split i={index} quorum={} shares={}",
                params.quorum, params.shares
            ),
        )?;
    }
    let _ = writeln!(
        out,
        "status=split keys={} share_files={share_files}",
        params.validators
    );
    Ok(())
}

fn cmd_shred(map: &BTreeMap<String, Vec<String>>, out: &mut dyn Write) -> Result<(), String> {
    let dir = dir_of(map)?;
    let media_dir = dir.join("media");
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&media_dir) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    for path in &files {
        shred_file(path).map_err(|_| "io error".to_string())?;
    }
    witness(&dir, &format!("shred files={}", files.len()))?;
    let _ = writeln!(out, "status=shredded files={}", files.len());
    Ok(())
}

fn cmd_seal(map: &BTreeMap<String, Vec<String>>, out: &mut dyn Write) -> Result<(), String> {
    let dir = dir_of(map)?;
    let out_path = one(map, "out")?;
    let params = read_params(&dir)?;
    let manifest = read_manifest(&dir)?;
    let text =
        std::fs::read_to_string(dir.join("witness.log")).map_err(|_| "witness chain broken")?;
    let lines = parse_witness(&text)?;
    verify_chain(&lines)?;

    // The witnessed `generate` entries commit to the pubkey set (D-W7-5).
    let mut witnessed: Vec<(u32, String, String)> = Vec::new();
    let mut saw_shred = false;
    let mut saw_split = 0u32;
    for line in &lines {
        if let Some(rest) = line.entry.strip_prefix("generate ") {
            let mut index = None;
            let mut pubkey = String::new();
            let mut address = String::new();
            for item in rest.split_whitespace() {
                if let Some(value) = item.strip_prefix("i=") {
                    index = value.parse::<u32>().ok();
                } else if let Some(value) = item.strip_prefix("pubkey=") {
                    pubkey = value.to_string();
                } else if let Some(value) = item.strip_prefix("address=") {
                    address = value.to_string();
                }
            }
            if let Some(index) = index {
                witnessed.push((index, address, pubkey));
            }
        } else if line.entry.starts_with("split ") {
            saw_split += 1;
        } else if line.entry.starts_with("shred ") {
            saw_shred = true;
        }
    }
    if witnessed.len() != params.validators as usize {
        return Err("witness log incomplete".to_string());
    }
    // A manifest that disagrees with the witnessed keys is refused
    // (NC-4 target).
    if witnessed != manifest.validators {
        return Err("validator set mismatch".to_string());
    }
    // D-W7-4: temporary media must be destroyed before sealing (NC-6 target).
    if !scan_private(&dir).is_empty() {
        return Err("temporary media present".to_string());
    }
    if saw_split != params.validators || !saw_shred {
        return Err("witness log incomplete".to_string());
    }

    let mut validators = Vec::new();
    for (_, address, pubkey) in &manifest.validators {
        let raw_pub = unhex(pubkey).map_err(|_| "bad manifest".to_string())?;
        let public: [u8; 32] = raw_pub.try_into().map_err(|_| "bad manifest".to_string())?;
        let address = Address::from_bech32(address).map_err(|_| "bad manifest".to_string())?;
        validators.push(Validator {
            address,
            power: params.power,
            public_key: public,
        });
    }
    let mut allocations = Vec::new();
    for (address, balance) in &params.allocations {
        allocations.push(Allocation {
            address: Address::from_bech32(address).map_err(|_| "bad allocation".to_string())?,
            balance: Amount::from_units(u128::from(*balance)),
        });
    }
    let chain_id = ChainId::new(params.chain.as_str()).map_err(|_| "bad params".to_string())?;
    let genesis = Genesis::new(
        chain_id,
        PROTOCOL_VERSION,
        params.genesis_time_ms,
        validators,
        allocations,
    )
    .map_err(|_| "bad genesis".to_string())?;
    std::fs::write(&out_path, genesis.canonical_bytes()).map_err(|_| "io error".to_string())?;
    let genesis_hash = hex(genesis.hash().as_bytes());
    let validator_set = hex(genesis.validator_set_hash().as_bytes());
    let tip = witness(
        &dir,
        &format!("seal genesis={genesis_hash} validator_set={validator_set}"),
    )?;
    let _ = writeln!(out, "status=sealed");
    let _ = writeln!(out, "genesis={genesis_hash}");
    let _ = writeln!(out, "validator_set={validator_set}");
    let _ = writeln!(out, "witness_tip={tip}");
    Ok(())
}

fn cmd_reconstruct(map: &BTreeMap<String, Vec<String>>, out: &mut dyn Write) -> Result<(), String> {
    let dir = dir_of(map)?;
    let params = read_params(&dir)?;
    let key: u32 = one(map, "key")?
        .parse()
        .map_err(|_| usage("bad --key".to_string()))?;
    let out_path = one(map, "out")?;
    let share_paths = map.get("share").cloned().unwrap_or_default();
    let mut shares: Vec<(u8, [u8; 32])> = Vec::new();
    for path in &share_paths {
        let text = std::fs::read_to_string(path).map_err(|_| "bad share file".to_string())?;
        if !text.starts_with("ceremony share v1\n") {
            return Err("bad share file".to_string());
        }
        let mut x = 0u8;
        let mut y = [0u8; 32];
        let mut seen_y = false;
        let mut key_index = None;
        for line in text.lines().skip(1) {
            if let Some(value) = line.strip_prefix("key=") {
                key_index = value.parse::<u32>().ok();
            } else if let Some(value) = line.strip_prefix("x=") {
                x = value.parse().map_err(|_| "bad share file".to_string())?;
            } else if let Some(value) = line.strip_prefix("y=") {
                let raw = unhex(value).map_err(|_| "bad share file".to_string())?;
                y = raw.try_into().map_err(|_| "bad share file".to_string())?;
                seen_y = true;
            }
        }
        if x == 0 || !seen_y || key_index != Some(key) {
            return Err("bad share file".to_string());
        }
        shares.push((x, y));
    }
    // Fewer than `quorum` shares cannot reconstruct (NC-2 target).
    if (shares.len() as u8) < params.quorum {
        return Err("not enough shares".to_string());
    }
    for (i, (x_i, _)) in shares.iter().enumerate() {
        if shares.iter().skip(i + 1).any(|(x_j, _)| x_j == x_i) {
            return Err("duplicate share".to_string());
        }
    }
    let secret = reconstruct_secret(&shares);
    std::fs::write(&out_path, format!("secret={}\n", hex(&secret)))
        .map_err(|_| "io error".to_string())?;
    let mut input = Vec::with_capacity(COMMIT_CONTEXT.len() + 32);
    input.extend_from_slice(COMMIT_CONTEXT);
    input.extend_from_slice(&secret);
    let commitment = blake3_hash(&input);
    let _ = writeln!(out, "secret_commitment={}", hex(commitment.as_bytes()));
    Ok(())
}

fn cmd_verify(map: &BTreeMap<String, Vec<String>>, out: &mut dyn Write) -> Result<(), String> {
    let dir = dir_of(map)?;
    let params = read_params(&dir)?;
    let text =
        std::fs::read_to_string(dir.join("witness.log")).map_err(|_| "witness chain broken")?;
    let lines = parse_witness(&text)?;
    let tip = verify_chain(&lines)?;
    let mut generates = 0u32;
    let mut splits = 0u32;
    let mut shred = false;
    for line in &lines {
        if line.entry.starts_with("generate ") {
            generates += 1;
        } else if line.entry.starts_with("split ") {
            splits += 1;
        } else if line.entry.starts_with("shred ") {
            shred = true;
        }
    }
    if generates != params.validators || splits != params.validators || !shred {
        return Err("witness log incomplete".to_string());
    }
    let root = map
        .get("root")
        .and_then(|values| values.first())
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.clone());
    let found = scan_private(&root);
    match found.first() {
        Some(path) => Err(format!("private material found: {path}")),
        None => {
            let _ = writeln!(out, "verify=ok");
            let _ = writeln!(out, "scan=clean");
            let _ = writeln!(out, "witness_tip={}", hex(&tip));
            Ok(())
        }
    }
}

// --------------------------------------------------------------------------- CLI driver

/// Execute one ceremony command against the given streams; returns the exit
/// code (0 ok, 64 usage, 65 refusal).
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut dispatch = || -> Result<(), String> {
        let (command, rest) = args.split_first().ok_or(usage("missing command"))?;
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut index = 0;
        while index < rest.len() {
            let flag = &rest[index];
            let name = flag
                .strip_prefix("--")
                .ok_or_else(|| usage(format!("expected --flag, got {flag}")))?;
            if index + 1 >= rest.len() {
                return Err(usage(format!("missing value for --{name}")));
            }
            map.entry(name.to_string())
                .or_default()
                .push(rest[index + 1].clone());
            index += 2;
        }
        match command.as_str() {
            "open" => cmd_open(&map, out),
            "generate" => cmd_generate(&map, out),
            "split" => cmd_split(&map, out),
            "shred" => cmd_shred(&map, out),
            "seal" => cmd_seal(&map, out),
            "reconstruct" => cmd_reconstruct(&map, out),
            "verify" => cmd_verify(&map, out),
            other => Err(usage(format!("unknown command {other}"))),
        }
    };
    match dispatch() {
        Ok(()) => 0,
        Err(message) => match message.strip_prefix("usage:") {
            Some(detail) => {
                let _ = writeln!(err, "error: usage: {detail}");
                EXIT_USAGE
            }
            None => {
                let _ = writeln!(err, "error: ceremony: {message}");
                EXIT_REFUSED
            }
        },
    }
}
