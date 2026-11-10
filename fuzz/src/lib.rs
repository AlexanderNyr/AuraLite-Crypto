//! Shared checks for isolated W1 fuzz targets. Not part of the application.

#[cfg(feature = "insecure-wallet-fuzz")]
pub mod keystore_decrypt {
    use aura_wallet::keystore::{Keystore, KeystoreError};
    use aura_wallet::secret::Password;

    // Public test values. These weak parameters and APIs are deliberately
    // isolated behind the fuzz-only feature and are not for real keystores.
    // BASE was generated once with
    // `Keystore::encrypt_seed_insecure_for_testing(&SecretSeed::new(SEED),
    // &Password::new(PASSWORD), KdfParams::Argon2id { m_cost: 16, t_cost: 1,
    // p_cost: 1 })` and pinned byte-for-byte so every mutation mode stays
    // deterministic and reproducible.
    const SEED: [u8; 64] = [0x42; 64];
    const PASSWORD: &str = "public-fuzz-password";
    const BASE: &str = r#"{
  "accounts": [
    {
      "address": "aura1qzf3c39r4eg47fndmvewprmgs3ysrn70zvyfpyzj",
      "index": 0,
      "path": "m/44'/1'/0'/0'/0'",
      "public_key": "b1ac545246c34724938b46fb2df53184bc825d813dee8e66b10736feecd00b93"
    }
  ],
  "cipher": {
    "name": "aes-256-gcm",
    "nonce": "6dcd025900025067f35c97ee"
  },
  "ciphertext": "47bfccc6d0fae3a2507f3069083c88cf40f28576843932c427e7a8792485e274b1ece9c982f4fae27e2813ac545eef939bf1cf7ae8ce6adbbbd7798ebc450c86f835cd62acbe015483fa4342448b3b6f",
  "derivation": {
    "coin_type": 1,
    "path_template": "m/44'/1'/0'/0'/i'",
    "scheme": "slip10-ed25519"
  },
  "format": "auralite-keystore",
  "id": "345452777079cab672b39d8f078095bb",
  "kdf": {
    "m_cost": 16,
    "name": "argon2id",
    "p_cost": 1,
    "salt": "7ad124ec5bbaab5777bd89ec947b52c8",
    "t_cost": 1
  },
  "seed_derivation": {
    "normalization_profile": null
  },
  "version": 2
}"#;
    const MODES: u8 = 14;

    fn change_hex(value: &str, position: usize, choice: u8) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut bytes = value.as_bytes().to_vec();
        let index = position % bytes.len();
        let mut new = HEX[usize::from(choice % 16)];
        if new == bytes[index] {
            new = HEX[(usize::from(choice % 16) + 1) % 16];
        }
        bytes[index] = new;
        String::from_utf8(bytes).expect("hex is ASCII")
    }

    /// Check the parsed/decrypted result for one bounded, structured input.
    /// An invalid authentication change must return AuthenticationFailed,
    /// not merely any error. No fuzz-controlled KDF can exceed these bounds.
    pub fn check(data: &[u8]) {
        let mode = data.first().copied().unwrap_or(0) % MODES;
        let position = usize::from(data.get(1).copied().unwrap_or(0));
        let choice = data.get(2).copied().unwrap_or(0);
        let mut doc: serde_json::Value = serde_json::from_str(BASE).unwrap();
        let mut password = PASSWORD.to_owned();
        match mode {
            0 => {} // unmodified control
            1 | 11 => {
                // 64 seed ciphertext bytes, then 16 GCM tag bytes.
                let offset = if mode == 1 {
                    position % 128
                } else {
                    128 + position % 32
                };
                doc["ciphertext"] =
                    change_hex(doc["ciphertext"].as_str().unwrap(), offset, choice).into();
            }
            2 => doc["id"] = change_hex(doc["id"].as_str().unwrap(), position, choice).into(),
            3 => password = format!("wrong-public-fuzz-password-{position:02x}"),
            4 => {
                let account = &mut doc["accounts"][0];
                account["public_key"] =
                    change_hex(account["public_key"].as_str().unwrap(), position, choice).into();
            }
            5 => {
                doc["kdf"]["salt"] =
                    change_hex(doc["kdf"]["salt"].as_str().unwrap(), position, choice).into()
            }
            6 => {
                doc["cipher"]["nonce"] =
                    change_hex(doc["cipher"]["nonce"].as_str().unwrap(), position, choice).into()
            }
            7 => {
                doc["accounts"][0]["address"] =
                    format!("invalid-public-address-{position:02x}").into()
            }
            8 => doc["kdf"]["m_cost"] = (17 + position % 48).into(), // 17..64 KiB
            9 => doc["kdf"]["t_cost"] = (2 + position % 2).into(),   // 2..3
            10 => doc["kdf"]["p_cost"] = 2.into(), // 16 KiB is valid for two lanes
            12 => {
                // v2 authenticates the BIP-39 profile marker; the pinned base
                // carries null, so either legal ID is an authenticated change.
                doc["seed_derivation"]["normalization_profile"] = if position % 2 == 0 {
                    "bip39-nfkd-unicode-15.1"
                } else {
                    "bip39-nfkd-unicode-18.0"
                }
                .into();
            }
            13 => {
                // Hex spelling in AAD fields: a case-only change keeps the
                // decoded id/salt/nonce bytes (and therefore the derived key
                // and GCM nonce) identical yet must break authentication.
                let slot = match position % 3 {
                    0 => &mut doc["id"],
                    1 => &mut doc["kdf"]["salt"],
                    _ => &mut doc["cipher"]["nonce"],
                };
                let current = slot.as_str().expect("hex field").to_owned();
                let case_changed = current.to_ascii_uppercase();
                *slot = if case_changed != current {
                    case_changed.into()
                } else {
                    change_hex(&current, position, choice).into()
                };
            }
            _ => unreachable!(),
        }
        // Both encodings represent the same document; formatting is not AAD.
        let text = if data.get(3).copied().unwrap_or(0) & 1 == 0 {
            serde_json::to_string(&doc).unwrap()
        } else {
            serde_json::to_string_pretty(&doc).unwrap()
        };
        let store = Keystore::from_json_str(&text).expect("constructed document must stay valid");
        let result = store.decrypt_seed_insecure_for_testing(&Password::new(password));
        if mode == 0 {
            assert_eq!(result.expect("roundtrip").as_bytes(), &SEED);
        } else {
            assert_eq!(
                result.err(),
                Some(KeystoreError::AuthenticationFailed),
                "mutation mode {mode} must fail authentication"
            );
        }
    }

    #[cfg(test)]
    mod tests {
        use super::check;

        #[test]
        fn all_modes_and_boundaries_are_exercised() {
            check(&[]);
            for mode in 0..super::MODES {
                check(&[mode, 0, 0, 0]);
                check(&[mode, 255, 255, 1]);
            }
            for field in 0..3u8 {
                check(&[13, field, 0, 0]); // id, kdf.salt, cipher.nonce hex case
            }
            check(&[8, 47, 255, 1]); // maximum fuzzed Argon2 memory: 64 KiB
        }
    }
}

/// Shared checks for the parser-only W1 fuzz target. Never decrypts and never
/// runs an attacker-selected KDF. Unit/mutation tested.
pub mod keystore_parse {
    use aura_wallet::derivation::Bip39NormalizationProfile;
    use aura_wallet::keystore::{Keystore, MAX_KEYSTORE_BYTES};

    /// Check the strict-parser metamorphic properties for one bounded input.
    /// Every accepted document must serialize stably, report exactly the
    /// normalization profile carried by its raw text, and reject one extra
    /// field, an escape-spelled duplicate key (`\u0066ormat` is `format`) and
    /// one byte over the size limit.
    pub fn check(data: &[u8]) {
        if data.len() > MAX_KEYSTORE_BYTES as usize {
            return;
        }
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        if let Ok(store) = Keystore::from_json_str(text) {
            let serialized = store.to_json_string().expect("valid document serializes");
            let reparsed =
                Keystore::from_json_str(&serialized).expect("serialized document parses");
            assert_eq!(
                serialized,
                reparsed.to_json_string().expect("stable serialization")
            );
            // The profile accessor must agree with the raw document text, not
            // merely with another call to itself.
            let raw: serde_json::Value = serde_json::from_str(text).expect("input parsed");
            let expected = raw["seed_derivation"]["normalization_profile"]
                .as_str()
                .and_then(Bip39NormalizationProfile::from_id);
            assert_eq!(
                store.normalization_profile(),
                expected,
                "profile accessor must match the raw text"
            );
            assert_eq!(
                store.normalization_profile(),
                reparsed.normalization_profile()
            );

            // Strict-parser metamorphic rejects derived from this accepted
            // document.
            let body = serialized.strip_prefix('{').expect("object document");
            let extra_field = format!("{{\"extra-field\":0,{body}");
            assert!(Keystore::from_json_str(&extra_field).is_err());
            let escaped_duplicate = format!("{{\"\\u0066ormat\":null,{body}");
            assert!(Keystore::from_json_str(&escaped_duplicate).is_err());
            let mut oversize = serialized;
            oversize.push_str(&" ".repeat((MAX_KEYSTORE_BYTES as usize + 1) - oversize.len()));
            assert_eq!(oversize.len(), MAX_KEYSTORE_BYTES as usize + 1);
            assert!(Keystore::from_json_str(&oversize).is_err());
        }
    }

    #[cfg(test)]
    mod tests {
        use super::check;
        use aura_wallet::keystore::{Keystore, MAX_KEYSTORE_BYTES};

        const FIXTURES: [&str; 9] = [
            include_str!("../../crates/aura-wallet/tests/fixtures/w1_interop_argon2id.json"),
            include_str!("../../crates/aura-wallet/tests/fixtures/w1_interop_scrypt.json"),
            include_str!("../../crates/aura-wallet/tests/fixtures/w1_interop_scrypt_legacy.json"),
            include_str!(
                "../../crates/aura-wallet/tests/fixtures/w1_interop_argon2id_uppercase_hex_header.json"
            ),
            include_str!(
                "../../crates/aura-wallet/tests/fixtures/w1_interop_argon2id_profile_15_1.json"
            ),
            include_str!(
                "../../crates/aura-wallet/tests/fixtures/w1_interop_argon2id_profile_null.json"
            ),
            include_str!(
                "../../crates/aura-wallet/tests/fixtures/w1_interop_argon2id_profile_sensitive_18_0.json"
            ),
            include_str!(
                "../../crates/aura-wallet/tests/fixtures/w1_inconsistent_uppercase_public_key.json"
            ),
            include_str!(
                "../../crates/aura-wallet/tests/fixtures/w1_inconsistent_scrypt_legacy_account.json"
            ),
        ];

        #[test]
        fn accepted_documents_pass_strict_metamorphic_rejects() {
            for text in FIXTURES {
                // A fixture that silently failed to parse would leave every
                // metamorphic assertion unexercised.
                assert!(
                    Keystore::from_json_str(text).is_ok(),
                    "fixture must parse: {text:.40}"
                );
                check(text.as_bytes());
            }
            // Truncated, invalid-UTF-8 and unknown documents must be ignored
            // or fail closed without panicking.
            check(b"");
            check(b"{");
            check(b"[1, 2, 3]");
            check(b"{} {}");
            check(b"{\"format\":\"other\"}");
            check(&[0xff, 0xfe, 0x00]);
            // The raw-size guard returns before parsing.
            check(&vec![b' '; MAX_KEYSTORE_BYTES as usize + 1]);
        }
    }
}

/// Shared checks for the recovery-metadata parser fuzz target. Parser-only;
/// the mnemonic itself is never part of these inputs.
pub mod recovery_metadata {
    use aura_wallet::derivation::Bip39NormalizationProfile;
    use aura_wallet::keystore::RecoveryMetadata;

    /// Check the strict-marker metamorphic properties for one bounded input.
    /// Every accepted marker must round-trip canonically, report exactly the
    /// profile carried by its raw text, stay within the 4096-byte cap, and
    /// reject a duplicate key (plain or escape-spelled), an unknown member and
    /// a document over the cap.
    pub fn check(data: &[u8]) {
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        if let Ok(marker) = RecoveryMetadata::from_json_str(text) {
            assert!(data.len() <= 4096, "oversized marker accepted");
            let canonical = marker.to_json_string().expect("marker serializes");
            let reparsed =
                RecoveryMetadata::from_json_str(&canonical).expect("canonical marker parses");
            assert_eq!(marker, reparsed);
            assert_eq!(
                canonical,
                reparsed.to_json_string().expect("stable serialization")
            );
            // The profile accessor must agree with the raw document text, not
            // merely with another call to itself.
            let raw: serde_json::Value = serde_json::from_str(text).expect("input parsed");
            let expected = Bip39NormalizationProfile::from_id(
                raw["normalization_profile"]
                    .as_str()
                    .expect("validated profile id"),
            )
            .expect("validated profile id");
            assert_eq!(
                marker.normalization_profile(),
                expected,
                "profile accessor must match the raw text"
            );
            // Metamorphic invalid inputs constructed from a known-good marker.
            let body = canonical.strip_prefix('{').expect("object serialization");
            let duplicate = format!("{{\"normalization_profile\":null,{body}");
            let escaped_duplicate = format!("{{\"\\u006eormalization_profile\":null,{body}");
            let extra = format!("{{\"unexpected\":null,{body}");
            let oversized = format!("{canonical}{}", " ".repeat(4097 - canonical.len()));
            for invalid in [&duplicate, &escaped_duplicate, &extra, &oversized] {
                assert!(
                    RecoveryMetadata::from_json_str(invalid).is_err(),
                    "invalid marker accepted"
                );
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::check;
        use aura_wallet::keystore::RecoveryMetadata;

        const MATRIX: &str = include_str!(
            "../../crates/aura-wallet/tests/fixtures/w1_recovery_metadata_reference.json"
        );

        #[test]
        fn accepted_markers_pass_strict_metamorphic_and_profile_checks() {
            let cases: Vec<serde_json::Value> =
                serde_json::from_str(MATRIX).expect("matrix parses");
            for case in &cases {
                let document = case["document"].as_str().expect("document text");
                let accepted = case["accepted"].as_bool().expect("accepted flag");
                if accepted {
                    assert!(
                        RecoveryMetadata::from_json_str(document).is_ok(),
                        "fixture must parse: {}",
                        case["name"]
                    );
                    check(document.as_bytes());
                } else {
                    assert!(
                        RecoveryMetadata::from_json_str(document).is_err(),
                        "fixture must fail: {}",
                        case["name"]
                    );
                }
            }
            // Explicit boundary and syntax cases; these double as tripwires
            // for the mutation controls.
            let marker = r#"{"normalization_profile":"bip39-nfkd-unicode-18.0"}"#;
            assert!(RecoveryMetadata::from_json_str(marker).is_ok());
            check(marker.as_bytes());
            let mut at_limit = marker.to_owned();
            at_limit.push_str(&" ".repeat(4096 - at_limit.len()));
            assert!(RecoveryMetadata::from_json_str(&at_limit).is_ok());
            check(at_limit.as_bytes());
            let mut over_limit = marker.to_owned();
            over_limit.push_str(&" ".repeat(4097 - over_limit.len()));
            assert!(RecoveryMetadata::from_json_str(&over_limit).is_err());
            for invalid in [
                "",
                "{}",
                r#"{"normalization_profile":null}"#,
                r#"{"normalization_profile":"unknown-profile-id"}"#,
                r#"{"normalization_profile":123}"#,
                r#"{"normalization_profile":"bip39-nfkd-unicode-15.1","extra":null}"#,
                r#"{"\u006eormalization_profile":"bip39-nfkd-unicode-15.1","normalization_profile":"bip39-nfkd-unicode-18.0"}"#,
                "[1, 2]",
                "{\"normalization_profile\":\"bip39-nfkd-unicode-15.1\"",
            ] {
                assert!(
                    RecoveryMetadata::from_json_str(invalid).is_err(),
                    "must reject: {invalid}"
                );
            }
            check(b"");
            check(&[0xff, 0xfe]);
            check(b"{");
        }
    }
}

/// Shared check for the scrypt differential fuzz target: the local wiping fork
/// must byte-match upstream `scrypt` 0.11.0 for every bounded test input. Tiny
/// work factors only; this does not test or benchmark production KDF policy.
pub mod scrypt_differential {
    /// Check one bounded input: derived test-only parameters, then an exact
    /// byte comparison of fork and upstream outputs.
    pub fn check(data: &[u8]) {
        if data.len() < 4 {
            return;
        }
        let log_n = 4 + data[0] % 6;
        let r = u32::from(1 + data[1] % 4);
        let p = u32::from(1 + data[2] % 3);
        let len = usize::from(16 + data[3] % 49);
        let rest = &data[4..];
        let split = rest.len() / 2;
        let (password, salt) = rest.split_at(split);
        let patched = scrypt_w1_wipe::Params::new(log_n, r, p, len).unwrap();
        let original = scrypt::Params::new(log_n, r, p, len).unwrap();
        let mut a = vec![0u8; len];
        let mut b = vec![0u8; len];
        scrypt_w1_wipe::scrypt(password, salt, &patched, &mut a).unwrap();
        scrypt::scrypt(password, salt, &original, &mut b).unwrap();
        assert_eq!(a, b, "fork diverged from upstream scrypt 0.11.0");
    }

    #[cfg(test)]
    mod tests {
        use super::check;

        #[test]
        fn fork_matches_upstream_on_deterministic_inputs() {
            for data in [
                &[][..],
                &[0u8, 0, 0, 0][..],
                &[5, 3, 2, 48, b'p', b'a', b's', b's'][..],
                &[2, 1, 1, 16][..],
                &[4, 2, 0, 64, 0xff][..],
                &[1, 3, 2, 33, b's'][..],
                &[255, 255, 255, 255, 1, 2, 3, 4, 5][..],
            ] {
                check(data);
            }
        }
    }
}
