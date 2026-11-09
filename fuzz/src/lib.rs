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
