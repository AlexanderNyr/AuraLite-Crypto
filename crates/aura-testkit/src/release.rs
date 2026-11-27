//! O1 — Releases: the checked release manifest (D-O1-1..D-O1-5).
//!
//! The release process is a checked pipeline: [`check`] validates a release
//! directory (`release/manifest.txt` registry, `SHA256SUMS`, detached
//! `.sig` files, `SBOM.txt`, the two-host `repro.txt` record, and the
//! `docker/Dockerfile` policy) against the workspace. The reproducibility
//! gate (D-O1-1) is the named control `repro_mismatch_refused`: two
//! independent build hosts must agree on the binary's sha256. Signature
//! verification (RFC 8032) is the wired H1 tool `tools/sign_release.py`;
//! this module checks the binding layer only and never claims to be
//! cryptographic verification (D-O1-3).
//!
//! Design of record: `docs/o1-releases-plan.md`. Vectors:
//! `tools/gen_o1_vectors.py` -> `tests/fixtures/o1_release_vectors.json`,
//! asserted byte-for-byte by `tests/o1_release.rs`.
//!
//! NC targets (`tools/nc_o1.py`):
//! 1. repro-equality check dropped -> `repro_mismatch_refused` red (the
//!    named control: "two independent hosts -> identical binary sha256");
//! 2. version check dropped -> `version_drift_refused` red;
//! 3. changelog check dropped -> `changelog_missing_refused` red;
//! 4. checksum match check dropped -> `checksum_mismatch_refused` red;
//! 5. signature length check dropped -> `signature_malformed_refused` red;
//! 6. unknown-line strictness dropped -> `foreign_manifest_line_refused`
//!    red (the manifest format cannot carry commands).

use std::io::Write;
use std::path::{Path, PathBuf};

/// Exit code for policy refusals.
const EXIT_REFUSED: i32 = 65;

/// Exit code for usage errors.
const EXIT_USAGE: i32 = 64;

/// The manifest rule pinned by the format (D-O1-2).
const RULE: &str = "release-v1";

/// An ed25519 detached signature is exactly 64 bytes (D-O1-3).
const SIG_BYTES: usize = 64;

// --------------------------------------------------------------------------- sha256

/// The SHA-256 round constants (FIPS 180-4).
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 of `data` (std-only implementation, pinned against Python
/// `hashlib` by the golden vectors — D-O1-5).
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in message.chunks(64) {
        let mut w = [0u32; 64];
        for (index, word) in w.iter_mut().enumerate().take(16) {
            let start = index * 4;
            *word = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (index, word) in h.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Lowercase hex of `bytes`.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Strict base64 decode (standard alphabet, padding required).
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut values: Vec<u8> = Vec::new();
    let mut padding = 0;
    for (index, ch) in text.chars().enumerate() {
        let value = match ch {
            'A'..='Z' => ch as u8 - b'A',
            'a'..='z' => ch as u8 - b'a' + 26,
            '0'..='9' => ch as u8 - b'0' + 52,
            '+' => 62,
            '/' => 63,
            '=' => {
                if index < text.len() - 2 {
                    return None;
                }
                padding += 1;
                0
            }
            _ => return None,
        };
        values.push(value);
    }
    if padding > 2 {
        return None;
    }
    let mut out = Vec::new();
    for group in values.chunks(4) {
        let triple = ((group[0] as u32) << 18)
            | ((group[1] as u32) << 12)
            | ((group[2] as u32) << 6)
            | (group[3] as u32);
        out.push(((triple >> 16) & 0xff) as u8);
        out.push(((triple >> 8) & 0xff) as u8);
        out.push((triple & 0xff) as u8);
    }
    for _ in 0..padding {
        out.pop();
    }
    Some(out)
}

// --------------------------------------------------------------------------- model

/// A strict parse of `release/manifest.txt` (D-O1-2).
#[derive(Debug, Clone)]
struct Manifest {
    /// The release version (`version` line).
    version: String,
    /// The pinned signer key id (`keyid` line).
    keyid: String,
    /// Artifact names in file order (`artifact` lines).
    artifacts: Vec<String>,
    /// `(artifact, sigfile)` pairs in file order (`sig` lines).
    sigs: Vec<(String, String)>,
}

/// One workspace package (from `crates/*/Cargo.toml`).
#[derive(Debug, Clone)]
struct Package {
    /// The package name.
    name: String,
    /// The resolved version (workspace version for `version.workspace`).
    version: String,
    /// The resolved license.
    license: String,
    /// `[dependencies]` keys, sorted.
    deps: Vec<String>,
}

/// Workspace facts read from the manifests (D-O1-4).
#[derive(Debug, Clone)]
struct Workspace {
    /// `[workspace.package] version`.
    version: String,
    /// The packages under `crates/`, sorted by name.
    packages: Vec<Package>,
}

// --------------------------------------------------------------------------- parsing

/// Parse the strict release manifest; unknown lines are refused so the
/// format cannot carry commands.
fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let mut rule = false;
    let mut version = None;
    let mut keyid = None;
    let mut artifacts = Vec::new();
    let mut sigs = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == format!("rule {RULE}") {
            rule = true;
        } else if let Some(value) = line.strip_prefix("version ") {
            version = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("keyid ") {
            keyid = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("artifact ") {
            artifacts.push(value.to_string());
        } else if let Some(value) = line.strip_prefix("sig ") {
            let (artifact, sigfile) = value
                .split_once(' ')
                .ok_or_else(|| format!("unknown manifest line: {line}"))?;
            sigs.push((artifact.to_string(), sigfile.to_string()));
        } else {
            return Err(format!("unknown manifest line: {line}"));
        }
    }
    if !rule {
        return Err("missing rule line".to_string());
    }
    Ok(Manifest {
        version: version.ok_or("missing version line")?,
        keyid: keyid.ok_or("missing keyid line")?,
        artifacts,
        sigs,
    })
}

/// Extract `key = "value"` from a section of a TOML-ish manifest.
fn toml_value(section_text: &str, key: &str) -> Option<String> {
    for raw in section_text.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                let value = value.trim();
                if let Some(inner) = value.strip_prefix('"') {
                    return inner.strip_suffix('"').map(str::to_string);
                }
            }
        }
    }
    None
}

/// Split a TOML-ish file into `(section, body)` pairs (stdlib-only; the
/// fixtures and the workspace manifests stay within this subset).
fn toml_sections(text: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut current = String::new();
    let mut body = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            if !current.is_empty() {
                sections.push((current.clone(), body.clone()));
            }
            current = line[1..line.len() - 1].to_string();
            body = String::new();
        } else {
            body.push_str(raw);
            body.push('\n');
        }
    }
    if !current.is_empty() {
        sections.push((current, body));
    }
    sections
}

/// Read the workspace facts: `[workspace.package]` version/license and the
/// packages under `crates/` (name, resolved version/license, dependency
/// keys).
fn parse_workspace(root: &Path) -> Result<Workspace, String> {
    let manifest_path = root.join("Cargo.toml");
    let manifest_text =
        std::fs::read_to_string(&manifest_path).map_err(|_| "cannot read workspace".to_string())?;
    let mut version = None;
    let mut license = None;
    for (section, body) in toml_sections(&manifest_text) {
        if section == "workspace.package" {
            version = toml_value(&body, "version");
            license = toml_value(&body, "license");
        }
    }
    let version = version.ok_or("workspace version missing")?;
    let license = license.unwrap_or_default();
    let crates = root.join("crates");
    let entries = std::fs::read_dir(&crates).map_err(|_| "bad workspace".to_string())?;
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    dirs.sort();
    let mut packages = Vec::new();
    for dir in dirs {
        let manifest_path = dir.join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let mut name = None;
        let mut pkg_version = None;
        let mut pkg_license = None;
        let mut deps = Vec::new();
        for (section, body) in toml_sections(&text) {
            if section == "package" {
                name = toml_value(&body, "name");
                pkg_version = toml_value(&body, "version");
                pkg_license = toml_value(&body, "license");
                if body.contains("version.workspace = true") {
                    pkg_version = Some(version.clone());
                }
                if body.contains("license.workspace = true") {
                    pkg_license = Some(license.clone());
                }
            } else if section == "dependencies" {
                for raw in body.lines() {
                    let line = raw.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((key, _)) = line.split_once(" =") {
                        let key = key.trim();
                        if !key.is_empty() {
                            deps.push(key.to_string());
                        }
                    }
                }
            }
        }
        let Some(name) = name else {
            continue;
        };
        deps.sort();
        deps.dedup();
        packages.push(Package {
            name,
            version: pkg_version.unwrap_or_else(|| version.clone()),
            license: pkg_license.unwrap_or_else(|| license.clone()),
            deps,
        });
    }
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Workspace { version, packages })
}

/// The deterministic SBOM (D-O1-4): `sbom v1` + one `pkg` line per crate.
fn sbom_text(workspace: &Workspace) -> String {
    let mut out = String::from("sbom v1\n");
    for package in &workspace.packages {
        out.push_str(&format!(
            "pkg name={} version={} license={} deps={}\n",
            package.name,
            package.version,
            package.license,
            package.deps.join(",")
        ));
    }
    out
}

/// The deterministic `SHA256SUMS` text for the manifest artifacts.
fn checksums_text(release: &Path, manifest: &Manifest) -> Result<String, String> {
    let mut names = manifest.artifacts.clone();
    names.sort();
    names.dedup();
    let mut out = String::new();
    for name in names {
        let data =
            std::fs::read(release.join(&name)).map_err(|_| format!("artifact missing: {name}"))?;
        out.push_str(&format!("{}  {name}\n", hex(&sha256(&data))));
    }
    Ok(out)
}

// --------------------------------------------------------------------------- checks

/// The release check (D-O1-2), in the pinned order; returns the ok line.
fn check(workspace: &Workspace, release: &Path) -> Result<String, String> {
    let manifest_path = release.join("manifest.txt");
    let manifest_text =
        std::fs::read_to_string(&manifest_path).map_err(|_| "cannot read manifest".to_string())?;
    let manifest = parse_manifest(&manifest_text)?;

    // The manifest's key id must match the pinned release key (D-O1-3).
    let pinned = std::fs::read_to_string(release.join("../docs/release-pubkey.txt"))
        .or_else(|_| std::fs::read_to_string("docs/release-pubkey.txt"))
        .map_err(|_| "cannot read release pubkey".to_string())?;
    if manifest.keyid != pinned.trim() {
        return Err(format!("key mismatch: {}", manifest.keyid));
    }

    // Semver + workspace version match (D-O1-4).
    let semver_ok = {
        let parts: Vec<&str> = manifest.version.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
    };
    if !semver_ok {
        return Err(format!("bad version: {}", manifest.version));
    }
    if manifest.version != workspace.version {
        return Err(format!("version drift: {}", manifest.version));
    }

    // CHANGELOG carries the release entry (D-O1-4).
    let changelog = std::fs::read_to_string(release.join("../CHANGELOG.md"))
        .or_else(|_| std::fs::read_to_string("CHANGELOG.md"))
        .map_err(|_| "cannot read changelog".to_string())?;
    if !changelog.contains(&format!("## [{}", manifest.version)) {
        return Err(format!("changelog entry missing: {}", manifest.version));
    }

    // Artifacts exist; digests match (D-O1-2).
    for name in &manifest.artifacts {
        if !release.join(name).is_file() {
            return Err(format!("artifact missing: {name}"));
        }
    }
    let expected_sums = checksums_text(release, &manifest)?;
    let sums_text = std::fs::read_to_string(release.join("SHA256SUMS"))
        .map_err(|_| "cannot read SHA256SUMS".to_string())?;
    let expected_lines: Vec<&str> = expected_sums.lines().collect();
    let actual_lines: Vec<&str> = sums_text.lines().collect();
    for index in 0..expected_lines.len().max(actual_lines.len()) {
        match (expected_lines.get(index), actual_lines.get(index)) {
            (Some(expected), Some(actual)) => {
                if expected != actual {
                    let name = expected.split("  ").nth(1).unwrap_or("?");
                    return Err(format!("checksum mismatch: {name}"));
                }
            }
            (Some(expected), None) => {
                let name = expected.split("  ").nth(1).unwrap_or("?");
                return Err(format!("checksum stale: {name}"));
            }
            (None, Some(actual)) => {
                let name = actual.split("  ").nth(1).unwrap_or("?");
                return Err(format!("checksum stale: {name}"));
            }
            (None, None) => {}
        }
    }

    // Signature binding: every artifact carries a 64-byte detached ed25519
    // signature (D-O1-3; the RFC 8032 verify is tools/sign_release.py).
    for name in &manifest.artifacts {
        let Some((_, sigfile)) = manifest.sigs.iter().find(|(artifact, _)| artifact == name) else {
            return Err(format!("signature missing: {name}"));
        };
        let data = std::fs::read(release.join(sigfile))
            .map_err(|_| format!("signature missing: {name}"))?;
        let text = String::from_utf8_lossy(&data).trim().to_string();
        let decoded = base64_decode(&text).ok_or_else(|| format!("signature malformed: {name}"))?;
        if decoded.len() != SIG_BYTES {
            return Err(format!("signature malformed: {name}"));
        }
    }

    // The SBOM matches the workspace regeneration (D-O1-4).
    let sbom = std::fs::read_to_string(release.join("SBOM.txt"))
        .map_err(|_| "cannot read SBOM".to_string())?;
    if sbom != sbom_text(workspace) {
        return Err("sbom drift".to_string());
    }

    // The two-host reproducibility record (D-O1-1) — the named gate.
    let repro = std::fs::read_to_string(release.join("repro.txt"))
        .map_err(|_| "cannot read repro record".to_string())?;
    let mut binary = None;
    let mut hosts: Vec<(String, String)> = Vec::new();
    for raw in repro.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "repro v1" {
            continue;
        } else if let Some(value) = line.strip_prefix("binary ") {
            binary = Some(value.to_string());
        } else if let Some(rest) = line.strip_prefix("toolchain ") {
            let _ = rest;
        } else if let Some(rest) = line.strip_prefix("source_date_epoch ") {
            let _ = rest;
        } else if let Some(rest) = line.strip_prefix("host ") {
            let (host, digest) = rest
                .split_once(" sha256 ")
                .ok_or_else(|| format!("unknown repro line: {line}"))?;
            hosts.push((host.to_string(), digest.to_string()));
        } else {
            return Err(format!("unknown repro line: {line}"));
        }
    }
    let binary = binary.ok_or("missing binary line")?;
    if hosts.len() < 2 {
        return Err(format!("repro mismatch: {binary}"));
    }
    let first = &hosts[0].1;
    if hosts.iter().any(|(_, digest)| digest != first) {
        return Err(format!("repro mismatch: {binary}"));
    }
    let sums = checksums_text(release, &manifest)?;
    let pinned_digest = sums
        .lines()
        .find_map(|line| line.split_once("  ").filter(|(_, name)| *name == binary))
        .map(|(digest, _)| digest.to_string())
        .ok_or_else(|| format!("repro mismatch: {binary}"))?;
    if pinned_digest != *first {
        return Err(format!("repro mismatch: {binary}"));
    }

    // Docker policy: multi-stage with digest-pinned bases (D-O1-4).
    let dockerfile = std::fs::read_to_string(release.join("../docker/Dockerfile"))
        .or_else(|_| std::fs::read_to_string("docker/Dockerfile"))
        .map_err(|_| "cannot read dockerfile".to_string())?;
    let mut bases = Vec::new();
    for raw in dockerfile.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("FROM ") {
            let image = rest.split_whitespace().next().unwrap_or("");
            if !image.is_empty() {
                bases.push(image.to_string());
            }
        }
    }
    if bases.len() < 2 {
        return Err("docker single stage".to_string());
    }
    for image in &bases {
        let pinned = image
            .split_once("@sha256:")
            .map(|(_, digest)| {
                digest.len() == 64 && digest.chars().all(|ch| ch.is_ascii_hexdigit())
            })
            .unwrap_or(false);
        if !pinned {
            let shown = image.split('@').next().unwrap_or(image);
            return Err(format!("docker base unpinned: {shown}"));
        }
    }

    Ok(format!(
        "release=ok version={} artifacts={}",
        manifest.version,
        manifest.artifacts.len()
    ))
}

/// Regenerate `manifest.txt`, `SHA256SUMS` and `SBOM.txt` from the release
/// directory (artifact discovery: files that are not `.sig`, the manifest
/// records or `repro.txt`) and the workspace.
fn sync(workspace: &Workspace, release: &Path) -> Result<String, String> {
    let previous = std::fs::read_to_string(release.join("manifest.txt"))
        .ok()
        .and_then(|text| parse_manifest(&text).ok());
    let pinned = std::fs::read_to_string(release.join("../docs/release-pubkey.txt"))
        .or_else(|_| std::fs::read_to_string("docs/release-pubkey.txt"))
        .map_err(|_| "cannot read release pubkey".to_string())?
        .trim()
        .to_string();
    let entries = std::fs::read_dir(release).map_err(|_| "cannot read release".to_string())?;
    let mut names: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".sig")
            || matches!(
                name.as_str(),
                "manifest.txt" | "SHA256SUMS" | "SBOM.txt" | "repro.txt"
            )
        {
            continue;
        }
        names.push(name);
    }
    names.sort();
    if names.is_empty() {
        return Err("release empty".to_string());
    }
    let mut out = String::new();
    out.push_str("# AuraLite release manifest (O1) — checked by `aura-release check`.\n");
    out.push_str(
        "# Regenerate with `make release-sync`. Format: docs/o1-releases-plan.md (D-O1-2).\n",
    );
    out.push_str(&format!("rule {RULE}\n"));
    out.push_str(&format!("version {}\n", workspace.version));
    out.push_str(&format!("keyid {pinned}\n"));
    for name in &names {
        out.push_str(&format!("artifact {name}\n"));
    }
    for name in &names {
        if release.join(format!("{name}.sig")).is_file() {
            out.push_str(&format!("sig {name} {name}.sig\n"));
        }
    }
    let _ = previous;
    std::fs::write(release.join("manifest.txt"), &out)
        .map_err(|_| "cannot write manifest".to_string())?;
    let manifest = parse_manifest(&out)?;
    let sums = checksums_text(release, &manifest)?;
    std::fs::write(release.join("SHA256SUMS"), sums)
        .map_err(|_| "cannot write SHA256SUMS".to_string())?;
    std::fs::write(release.join("SBOM.txt"), sbom_text(workspace))
        .map_err(|_| "cannot write SBOM".to_string())?;
    Ok(format!("synced artifacts={}", names.len()))
}

// --------------------------------------------------------------------------- CLI driver

/// Read a whole file to text.
fn read_text(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|_| format!("cannot read {path}"))
}

/// Collect `--flag value` pairs from the argument tail.
fn flags(rest: &[String]) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut map = std::collections::BTreeMap::new();
    let mut index = 0;
    while index < rest.len() {
        let flag = &rest[index];
        let name = flag
            .strip_prefix("--")
            .ok_or_else(|| format!("expected --flag, got {flag}"))?;
        if index + 1 >= rest.len() {
            return Err(format!("missing value for --{name}"));
        }
        map.insert(name.to_string(), rest[index + 1].clone());
        index += 2;
    }
    Ok(map)
}

/// Execute one `aura-release` command against the given streams; returns the
/// exit code (0 ok, 64 usage, 65 refusal).
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut dispatch = || -> Result<(), String> {
        let (command, rest) = args.split_first().ok_or("usage:missing command")?;
        let map = flags(rest).map_err(|message| format!("usage:{message}"))?;
        let workspace_dir = map
            .get("workspace")
            .cloned()
            .unwrap_or_else(|| ".".to_string());
        let release_dir = map
            .get("release")
            .cloned()
            .unwrap_or_else(|| "release".to_string());
        let workspace = parse_workspace(Path::new(&workspace_dir))?;
        match command.as_str() {
            "check" => {
                let ok = check(&workspace, Path::new(&release_dir))?;
                let _ = writeln!(out, "{ok}");
                Ok(())
            }
            "sbom" => {
                let _ = write!(out, "{}", sbom_text(&workspace));
                Ok(())
            }
            "checksums" => {
                let manifest = parse_manifest(&read_text(&format!("{release_dir}/manifest.txt"))?)?;
                let sums = checksums_text(Path::new(&release_dir), &manifest)?;
                let _ = write!(out, "{sums}");
                Ok(())
            }
            "digest" => {
                let path = map
                    .get("file")
                    .ok_or_else(|| "usage:missing --file".to_string())?;
                let data = std::fs::read(path).map_err(|_| format!("cannot read {path}"))?;
                let _ = writeln!(out, "{}", hex(&sha256(&data)));
                Ok(())
            }
            "sync" => {
                let message = sync(&workspace, Path::new(&release_dir))?;
                let _ = writeln!(out, "{message}");
                Ok(())
            }
            other => Err(format!("usage:unknown command {other}")),
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
                let _ = writeln!(err, "error: release: {message}");
                EXIT_REFUSED
            }
        },
    }
}
