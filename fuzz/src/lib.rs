//! Shared checks for isolated W1 fuzz targets. Not part of the application.

#[cfg(feature = "insecure-wallet-fuzz")]
pub mod keystore_decrypt {
    use aura_wallet::keystore::{KdfParams, Keystore, KeystoreError};
    use aura_wallet::secret::{Password, SecretSeed};
    use std::sync::OnceLock;

    // Public test values. These weak parameters and APIs are deliberately
    // isolated behind the fuzz-only feature and are not for real keystores.
    const SEED: [u8; 64] = [0x42; 64];
    const PASSWORD: &str = "public-fuzz-password";
    const MODES: u8 = 12;
    static BASE: OnceLock<String> = OnceLock::new();

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
        let base = BASE.get_or_init(|| {
            Keystore::encrypt_seed_insecure_for_testing(
                &SecretSeed::new(SEED),
                &Password::new(PASSWORD),
                KdfParams::Argon2id {
                    m_cost: 16,
                    t_cost: 1,
                    p_cost: 1,
                },
            )
            .expect("bounded test KDF")
            .to_json_string()
            .expect("test document serializes")
        });
        let mode = data.first().copied().unwrap_or(0) % MODES;
        let position = usize::from(data.get(1).copied().unwrap_or(0));
        let choice = data.get(2).copied().unwrap_or(0);
        let mut doc: serde_json::Value = serde_json::from_str(base).unwrap();
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
            for mode in 0..12 {
                check(&[mode, 0, 0, 0]);
                check(&[mode, 255, 255, 1]);
            }
            check(&[8, 47, 255, 1]); // maximum fuzzed Argon2 memory: 64 KiB
        }
    }
}
