#!/usr/bin/env python3
"""W1 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the cryptosystem. Never run
while another process is building the same checkout.
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/aura-wallet/src/keystore.rs"


def run_test(test: str, feature: bool | str) -> subprocess.CompletedProcess[str]:
    cmd = ["cargo", "test", "-p", "aura-wallet"]
    if feature:
        cmd += ["--features", "insecure-test-kdf" if feature is True else feature]
    if "::" in test:
        cmd += ["--lib", "--", "--exact", test]
    else:
        cmd += ["--test", "w1_keystore", test, "--", "--exact"]
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def control(label: str, old: str, new: str, test: str, feature: bool | str, source: Path = SOURCE) -> bool:
    original = source.read_text(encoding="utf-8")
    if original.count(old) != 1:
        print(f"FAIL {label}: mutation target not unique")
        return False
    try:
        source.write_text(original.replace(old, new, 1), encoding="utf-8")
        result = run_test(test, feature)
    finally:
        source.write_text(original, encoding="utf-8")
    # Compilation failure is NOT a valid red test.
    ok = result.returncode != 0 and f"test {test} ... FAILED" in result.stdout
    print(f"{'PASS' if ok else 'FAIL'} {label}: {test}")
    if not ok:
        print((result.stdout + result.stderr)[-2500:])
    return ok


def main() -> int:
    checks = [
        ("BIP-39 whitespace silently rejoined", '    if normalized_phrase.as_str() != canonical_phrase.as_str() {',
         '    if false {',
         'bip39_whitespace_matches_independent_nfkd_pbkdf2_reference', False,
         ROOT / "crates/aura-wallet/src/derivation.rs"),
        ("BIP-39 whitespace matrix bypass", '    if normalized_phrase.as_str() != canonical_phrase.as_str() {',
         '    if false {',
         'bip39_whitespace_matrix_covers_all_word_counts_and_positions', False,
         ROOT / "crates/aura-wallet/src/derivation.rs"),
        ("oversized pretty output bypasses compact fallback", '        if pretty.len() as u64 <= MAX_KEYSTORE_BYTES {',
         '        if true {',
         'parsed_keystore_can_be_serialized_within_loader_limit', False),
        ("oversized compact output is emitted", '        if compact.len() as u64 > MAX_KEYSTORE_BYTES {',
         '        if false {',
         'keystore::serialization_size_tests::oversized_compact_output_fails_closed', False),
        ("AAD omits keystore ID", '        map.remove("ciphertext");',
         '        map.remove("ciphertext");\n        map.remove("id");',
         'test_keystore_authenticates_header_and_every_ciphertext_byte', True),
        ("AAD omits BIP-39 profile marker", '        map.remove("ciphertext");',
         '        map.remove("ciphertext");\n        map.remove("seed_derivation");',
         'keystore_and_recovery_manifest_bind_the_explicit_profile', False),
        ("AAD omits null profile metadata", '        map.remove("ciphertext");',
         '        map.remove("ciphertext");\n        if map.get("seed_derivation").and_then(|v| v.get("normalization_profile")).is_some_and(|v| v.is_null()) { map.remove("seed_derivation"); }',
         'keystore::aad_reference_tests::canonical_aad_matches_independent_python_bytes', False),
        ("AAD omits Unicode 15.1 profile metadata", '        map.remove("ciphertext");',
         '        map.remove("ciphertext");\n        if map.get("seed_derivation").and_then(|v| v.get("normalization_profile")).and_then(Value::as_str) == Some("bip39-nfkd-unicode-15.1") { map.remove("seed_derivation"); }',
         'keystore::aad_reference_tests::canonical_aad_matches_independent_python_bytes', False),
        ("AAD retains ciphertext", '        map.remove("ciphertext");',
         '        let _ = map;',
         'keystore::aad_reference_tests::canonical_aad_matches_independent_python_bytes', False),
        ("AAD object keys reversed with preserve_order enabled", '            keys.sort();',
         '            keys.sort_by(|a, b| b.cmp(a));',
         'keystore::aad_reference_tests::canonical_aad_matches_independent_python_bytes', 'serde_json/preserve_order'),
        ("test-only wrong password bypass", '        self.decrypt_seed_inner(password, false, true)',
         '        if password.as_str().starts_with("wrong-password-") { return Ok(SecretSeed::new([0; 64])); }\n        self.decrypt_seed_inner(password, false, true)',
         'keystore_roundtrip_and_wrong_passwords', True),
        ("ordinary API wrong password bypass", '        self.decrypt_seed_inner(password, false, false)',
         '        if password.as_str() == "wrong-public-password" { return Ok(SecretSeed::new([0; 64])); }\n        self.decrypt_seed_inner(password, false, false)',
         'python_generated_keystore_interoperability_vectors', False),
        ("authenticated public key not checked against seed", 'metadata["public_key"].as_str() != Some(hex::encode(account.public_key).as_str())',
         'false',
         'authenticated_public_key_must_match_decrypted_seed', False),
        ("authenticated address not checked against seed", 'metadata["address"].as_str() != Some(account.address.to_string().as_str())',
         'false',
         'authenticated_address_must_match_decrypted_seed', False),
        ("profile mismatch is ignored during recovery", '        if stored_profile != recovery_metadata.normalization_profile() {',
         '        if false {',
         'keystore_and_recovery_manifest_bind_the_explicit_profile', False),
        ("recovered seed comparison is bypassed", '        if !aura_crypto::constant_time_eq(recovered_seed.as_bytes(), keystore_seed.as_bytes()) {',
         '        if false {',
         'keystore_and_recovery_manifest_bind_the_explicit_profile', False),
        ("unknown normalization profile defaults silently", '            Bip39NormalizationProfile::from_id(id).ok_or(KeystoreError::Unsupported)?;',
         '            Bip39NormalizationProfile::from_id(id).unwrap_or(derivation::DEFAULT_BIP39_NORMALIZATION_PROFILE);',
         'keystore_and_recovery_manifest_bind_the_explicit_profile', False),
        ("recovery metadata accepts oversized JSON", '        if text.len() > 4096 {',
         '        if false {',
         'recovery_metadata_strict_schema_and_byte_limit_matrix', False),
        ("recovery metadata accepts duplicate keys", '        DuplicateKeyChecker::check(text)?;\n        let object = document.as_object().ok_or(KeystoreError::InvalidFormat)?;',
         '        let _ = text;\n        let object = document.as_object().ok_or(KeystoreError::InvalidFormat)?;',
         'recovery_metadata_strict_schema_and_byte_limit_matrix', False),
        ("recovery metadata accepts extra fields", '        exact_fields(&document, &["normalization_profile"])?;',
         '        let _ = &document;',
         'recovery_metadata_strict_schema_and_byte_limit_matrix', False),
        ("recovery metadata misclassifies missing/null profile", '        let id = document["normalization_profile"]\n            .as_str()\n            .ok_or(KeystoreError::RecoveryProfileMissing)?;',
         '        let id = document["normalization_profile"]\n            .as_str()\n            .ok_or(KeystoreError::InvalidFormat)?;',
         'python_recovery_metadata_reference_matches_parser_matrix', False),
        ("post-GCM public_key comparison ignores ASCII case", '        if metadata["public_key"].as_str() != Some(hex::encode(account.public_key).as_str())',
         '        if !metadata["public_key"].as_str().is_some_and(|pk| pk.eq_ignore_ascii_case(&hex::encode(account.public_key)))',
         'reauthenticated_uppercase_hex_header_and_canonical_account_rules', False),
        ("post-GCM address comparison ignores ASCII case", '            || metadata["address"].as_str() != Some(account.address.to_string().as_str())',
         '            || !metadata["address"].as_str().is_some_and(|a| a.eq_ignore_ascii_case(&account.address.to_string()))',
         'reauthenticated_uppercase_hex_header_and_canonical_account_rules', False),
        ("random keystore id bypass", '        OsRng.fill_bytes(&mut id);',
         '        let _ = &mut id;',
         'fresh_encryptions_use_distinct_public_random_values', True),
        ("random KDF salt bypass", '        OsRng.fill_bytes(&mut salt);',
         '        let _ = &mut salt;',
         'fresh_encryptions_use_distinct_public_random_values', True),
        ("random GCM nonce bypass", '        OsRng.fill_bytes(&mut nonce_bytes);',
         '        let _ = &mut nonce_bytes;',
         'fresh_encryptions_use_distinct_public_random_values', True),
        ("W1 transaction accepts different signer", 'if signer_address(&public_key) != self.signer {',
         'if false {',
         'wallet_transaction_rejects_a_different_derived_key', False,
         ROOT / "crates/aura-tx/src/tx.rs"),
        ("W1 transaction accepts foreign chain", 'if self.unsigned.chain_id.as_str() != expected_chain {',
         'if false {',
         'wallet_transaction_rejects_foreign_chain', False,
         ROOT / "crates/aura-tx/src/tx.rs"),
        ("W1 transaction accepts foreign signature domain", 'aura_crypto::verify(pk, chain, Domain::Tx, msg, sig)',
         'aura_crypto::verify(pk, chain, Domain::Vote, msg, sig)',
         'wallet_transaction_rejects_foreign_domain_signature', False,
         ROOT / "crates/aura-tx/src/tx.rs"),
        ("duplicate keys accepted", '        DuplicateKeyChecker::check(text)?;\n        let ks = Self { document };',
         '        let _ = text;\n        let ks = Self { document };',
         'format_rejects_duplicate_unknown_and_malformed_fields', True),
        ("schema matrix permits escaped duplicate keys", '        DuplicateKeyChecker::check(text)?;\n        let ks = Self { document };',
         '        let _ = text;\n        let ks = Self { document };',
         'python_schema_reference_matches_parser_matrix', False),
        ("schema matrix permits extra fields", '    if object.len() != fields.len() || !fields.iter().all(|key| object.contains_key(*key)) {',
         '    if false {',
         'python_schema_reference_matches_parser_matrix', False),
        ("schema matrix omits KDF resource limits", '        params.validate_limits()?;\n        Ok((params, salt))',
         '        let _ = params;\n        Ok((params, salt))',
         'python_schema_reference_matches_parser_matrix', False),
        ("atomic no-overwrite bypass", 'linkat(\n                    &directory,\n                    tmp_name.as_str(),\n                    &directory,\n                    name,\n                    AtFlags::empty(),\n                )',
         'rustix::fs::renameat(&directory, tmp_name.as_str(), &directory, name)',
         'concurrent_save_never_overwrites', True),
        ("parent descriptor anchoring bypass", 'linkat(\n                    &directory,\n                    tmp_name.as_str(),\n                    &directory,\n                    name,\n                    AtFlags::empty(),\n                )',
         'linkat(rustix::fs::CWD, tmp_path.as_path(), rustix::fs::CWD, path, AtFlags::empty())',
         'keystore::durability_tests::parent_directory_replacement_does_not_redirect_publication', False),
        ("Argon2 minimum memory bypass with test feature", 'matches!(\n            self,\n            Self::Argon2id {\n                m_cost: 0..=19_455,\n                ..\n            }\n        )',
         'false',
         'insecure_feature_does_not_weaken_normal_kdf_policy', True),
        ("Argon2 minimum iterations bypass with test feature", 'matches!(self, Self::Argon2id { t_cost: 0..=1, .. })',
         'false',
         'insecure_feature_does_not_weaken_normal_kdf_policy', True),
        ("weak scrypt accepted by ordinary API with test feature", 'if matches!(self, Self::Scrypt { log_n: 14..=16, .. }) {',
         'if false {',
         'insecure_feature_does_not_weaken_normal_kdf_policy', True),
        ("ordinary encrypt selects insecure policy", '        Self::encrypt_seed_inner(seed, password, kdf, None, false)',
         '        Self::encrypt_seed_inner(seed, password, kdf, None, true)',
         'insecure_feature_does_not_weaken_normal_kdf_policy', True),
        ("ordinary decrypt selects insecure policy", '        self.decrypt_seed_inner(password, false, false)',
         '        self.decrypt_seed_inner(password, false, true)',
         'insecure_feature_does_not_weaken_normal_kdf_policy', True),
        ("test feature accepts excessive Argon2 memory", '!(minimum_memory..=262_144).contains(&m_cost)',
         '!(minimum_memory..=u32::MAX).contains(&m_cost)',
         'insecure_test_parser_enforces_resource_caps', True),
        ("test feature accepts excessive scrypt memory", '(1u64 << log_n) * u64::from(r) * 128 > 268_435_456',
         'false',
         'insecure_test_parser_enforces_resource_caps', True),
        ("mnemonic NFKD normalization bypass", 'UnicodeNormalization18::nfkd(input).collect::<String>()',
         'input.to_owned()',
         'bip39_nfkd_phrase_and_passphrase_match_independent_reference', False,
         ROOT / "crates/aura-wallet/src/derivation.rs"),
        ("passphrase NFKD normalization bypass", 'UnicodeNormalization15_1::nfkd(input).collect::<String>()',
         'input.to_owned()',
         'bip39_versioned_profiles_are_stable_and_recovery_is_explicit', False,
         ROOT / "crates/aura-wallet/src/derivation.rs"),
        ("legacy profile silently uses Unicode 18 tables", 'UnicodeNormalization15_1::nfkd(input).collect::<String>()',
         'UnicodeNormalization18::nfkd(input).collect::<String>()',
         'independent_sensitive_profile_fixtures_recover_distinct_seeds', False,
         ROOT / "crates/aura-wallet/src/derivation.rs"),
        ("re-encryption drops normalization-profile marker", '        let seed = self.decrypt_seed(current_password)?;\n        let profile = self.normalization_profile();\n        Self::encrypt_seed_inner(&seed, new_password, new_kdf, profile, false)',
         '        let seed = self.decrypt_seed(current_password)?;\n        let profile = None;\n        Self::encrypt_seed_inner(&seed, new_password, new_kdf, profile, false)',
         'keystore_and_recovery_manifest_bind_the_explicit_profile', False),
        ("keystore password silently normalized", 'password.as_str().as_bytes(),\n                        salt,\n                        &mut key[..],',
         'unicode_normalization::UnicodeNormalization::nfd(password.as_str()).collect::<String>().as_bytes(),\n                        salt,\n                        &mut key[..],',
         'unicode_interop_vectors_use_nfkd_mnemonic_and_raw_password_bytes', False),
        ("directory-like destination accepted", 'if raw.ends_with(b"/") || raw.ends_with(b"/.") {',
         'if false {',
         'save_new_rejects_directory_syntax_without_creating_a_file', True),
        ("unsafe parent directory accepted", 'if meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o022 != 0 {',
         'if false {',
         'save_new_rejects_untrusted_or_symlink_parent', False),
        ("temporary-file identity check bypass", 'if !directory_entry_matches(&directory, tmp_name.as_ref(), &original) {',
         'if false {',
         'keystore::durability_tests::temporary_entry_substitution_is_detected_before_publication', False),
        ("keystore load follows symlink", 'OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC',
         'OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC',
         'load_rejects_fifo_and_final_symlink', False),
        ("directory synchronization bypass", '            sync_parent(parent, Some(&directory))?;',
         '            let _ = sync_parent;',
         'keystore::durability_tests::sync_error_after_publication_is_reported_without_overwrite', False),
    ]
    for check in checks:
        test, feature = check[3:5]
        result = run_test(test, feature)
        if result.returncode != 0 or f"test {test} ... ok" not in result.stdout:
            print(f"FAIL baseline test {test}\n{result.stdout}\n{result.stderr}")
            return 1
    passed = sum(control(*check) for check in checks)
    print(f"RESULT: {passed}/{len(checks)} negative controls PASS")
    return 0 if passed == len(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
