//! W1 gates: BIP-39/32 vectors, SLIP-0010 Ed25519 derivation, encrypted
//! keystore roundtrip, 1000 wrong passwords, format fuzz, zeroize/redaction.

use aura_wallet::derivation::{self, ExtendedEdKey};
use aura_wallet::keystore::{KdfParams, Keystore, KeystoreError};
use aura_wallet::secret::{Password, SecretSeed};
use bip32::{Prefix, XPrv};

const BIP39_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const BIP39_PASSPHRASE: &str = "TREZOR";
const BIP39_SEED_HEX: &str = "c55257c360c07c72029aebc1b53c05ed0362ada38ead3e3e9efa3708e53495531f09a6987599d18264c1e1c92f2cf141630c7a3c4ab7c81b2f001698e7463b04";
const BIP32_SEED_HEX: &str = "000102030405060708090a0b0c0d0e0f";

#[cfg(any(
    feature = "insecure-test-kdf",
    not(any(target_os = "redox", target_os = "espidf"))
))]
fn test_kdf() -> KdfParams {
    #[cfg(feature = "insecure-test-kdf")]
    {
        KdfParams::testing()
    }
    #[cfg(not(feature = "insecure-test-kdf"))]
    {
        KdfParams::default_argon2id()
    }
}

fn encrypt_test_seed(
    seed: &SecretSeed,
    password: &Password,
    kdf: KdfParams,
) -> Result<Keystore, KeystoreError> {
    #[cfg(feature = "insecure-test-kdf")]
    {
        Keystore::encrypt_seed_insecure_for_testing(seed, password, kdf)
    }
    #[cfg(not(feature = "insecure-test-kdf"))]
    {
        Keystore::encrypt_seed(seed, password, kdf)
    }
}

fn decrypt_test_seed(store: &Keystore, password: &Password) -> Result<SecretSeed, KeystoreError> {
    #[cfg(feature = "insecure-test-kdf")]
    {
        store.decrypt_seed_insecure_for_testing(password)
    }
    #[cfg(not(feature = "insecure-test-kdf"))]
    {
        store.decrypt_seed(password)
    }
}

fn seed_from_hex(hexstr: &str) -> SecretSeed {
    let bytes = hex::decode(hexstr).unwrap();
    let mut seed = [0u8; 64];
    seed[..bytes.len()].copy_from_slice(&bytes);
    SecretSeed::new(seed)
}

fn seed_v18(phrase: &str, passphrase: &str) -> Result<SecretSeed, derivation::DerivationError> {
    derivation::mnemonic_to_seed_with_profile(
        phrase,
        passphrase,
        derivation::DEFAULT_BIP39_NORMALIZATION_PROFILE,
    )
}

#[test]
fn bip39_official_vector() {
    let seed = seed_v18(BIP39_MNEMONIC, BIP39_PASSPHRASE).unwrap();
    assert_eq!(hex::encode(seed.as_bytes()), BIP39_SEED_HEX);
    assert!(seed_v18("not a valid mnemonic", "").is_err());
}

#[test]
fn bip39_rejects_noncanonical_whitespace_after_nfkd() {
    use aura_wallet::derivation::DerivationError;

    // NFKD maps U+3000 to the ordinary single-space BIP-39 separator.
    let ideographic = BIP39_MNEMONIC.replace(' ', "\u{3000}");
    assert_eq!(
        hex::encode(seed_v18(&ideographic, BIP39_PASSPHRASE).unwrap().as_bytes()),
        BIP39_SEED_HEX
    );
    // These spellings survive NFKD but are silently rejoined with single
    // spaces by bip39::Mnemonic. Reject them rather than returning a seed for
    // a different input than the caller supplied.
    for phrase in [
        BIP39_MNEMONIC.replacen(' ', "  ", 1),
        BIP39_MNEMONIC.replacen(' ', "\t", 1),
        BIP39_MNEMONIC.replacen(' ', "\n", 1),
        format!(" {BIP39_MNEMONIC}"),
        format!("{BIP39_MNEMONIC} "),
    ] {
        assert_eq!(
            seed_v18(&phrase, BIP39_PASSPHRASE).err(),
            Some(DerivationError::InvalidMnemonic),
            "noncanonical mnemonic whitespace must not silently change the BIP-39 input"
        );
    }
}

#[test]
fn bip39_whitespace_matches_independent_nfkd_pbkdf2_reference() {
    use aura_wallet::derivation::DerivationError;

    // Python reference is an NFKD + PBKDF2 oracle, not a mnemonic validator.
    // All inputs and hashes are PUBLIC and must never be used for funds.
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/w1_mnemonic_reference.json")).unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 8);
    let canonical = cases[0]["raw_normalized_pbkdf2_hex"].as_str().unwrap();
    assert_eq!(canonical, BIP39_SEED_HEX);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let input = case["phrase"].as_str().unwrap();
        let passphrase = case["passphrase"].as_str().unwrap();
        let raw_seed = case["raw_normalized_pbkdf2_hex"].as_str().unwrap();
        let accepted = case["accepted"].as_bool().unwrap();
        let actual = seed_v18(input, passphrase);
        if accepted {
            assert_eq!(hex::encode(actual.unwrap().as_bytes()), raw_seed, "{name}");
        } else {
            assert_ne!(
                raw_seed, canonical,
                "reference must show distinct raw PBKDF2 bytes: {name}"
            );
            assert_eq!(
                actual.err(),
                Some(DerivationError::InvalidMnemonic),
                "{name}"
            );
        }
    }
}

#[test]
fn bip39_whitespace_matrix_covers_all_word_counts_and_positions() {
    use aura_wallet::derivation::DerivationError;

    // Deterministic PUBLIC entropy; never use these mnemonics for funds.
    let mut rejected = 0;
    let separators = ["  ", "\t", "\n", "\r\n", "\u{85}", "\u{2028}", "\u{2029}"];
    for entropy_len in [16, 20, 24, 28, 32] {
        let phrase =
            bip39::Mnemonic::from_entropy_in(bip39::Language::English, &vec![0u8; entropy_len])
                .unwrap()
                .to_string();
        let canonical = seed_v18(&phrase, "public-passphrase").unwrap();
        for whitespace in ["\u{a0}", "\u{2003}", "\u{3000}"] {
            let alternative = phrase.replace(' ', whitespace);
            assert_eq!(
                seed_v18(&alternative, "public-passphrase")
                    .unwrap()
                    .as_bytes(),
                canonical.as_bytes(),
                "single separator normalizing to ASCII space"
            );
        }
        let words: Vec<_> = phrase.split(' ').collect();
        for position in 1..words.len() {
            for separator in separators {
                let altered = format!(
                    "{}{}{}",
                    words[..position].join(" "),
                    separator,
                    words[position..].join(" ")
                );
                assert_eq!(
                    seed_v18(&altered, "public-passphrase").err(),
                    Some(DerivationError::InvalidMnemonic),
                    "entropy_len={entropy_len}, position={position}, separator={separator:?}"
                );
                rejected += 1;
            }
        }
        for whitespace in separators {
            for altered in [
                format!("{whitespace}{phrase}"),
                format!("{phrase}{whitespace}"),
            ] {
                assert_eq!(
                    seed_v18(&altered, "public-passphrase").err(),
                    Some(DerivationError::InvalidMnemonic)
                );
                rejected += 1;
            }
        }
    }
    assert_eq!(rejected, 665);
}

#[test]
fn bip39_versioned_profiles_are_stable_and_recovery_is_explicit() {
    use aura_wallet::derivation::Bip39NormalizationProfile::{Unicode15_1, Unicode18_0};

    assert_eq!(unicode_normalization_15_1::UNICODE_VERSION, (15, 1, 0));
    assert_eq!(unicode_normalization::UNICODE_VERSION, (18, 0, 0));
    assert_eq!(Unicode15_1.id(), "bip39-nfkd-unicode-15.1");
    assert_eq!(Unicode18_0.id(), "bip39-nfkd-unicode-18.0");
    assert_eq!(derivation::DEFAULT_BIP39_NORMALIZATION_PROFILE, Unicode18_0);
    assert_eq!(
        aura_wallet::derivation::Bip39NormalizationProfile::from_id(Unicode15_1.id()),
        Some(Unicode15_1)
    );
    assert_eq!(
        aura_wallet::derivation::Bip39NormalizationProfile::from_id(Unicode18_0.id()),
        Some(Unicode18_0)
    );
    assert_eq!(
        aura_wallet::derivation::Bip39NormalizationProfile::from_id("unknown-profile"),
        None
    );

    // Under Unicode 15.1, U+A7F1 is unassigned and remains itself. Under
    // Unicode 18.0 it normalizes to ASCII S. Python 3.13 / Unicode 15.1
    // independently computed both PBKDF2 vectors.
    let legacy =
        derivation::mnemonic_to_seed_with_profile(BIP39_MNEMONIC, "\u{a7f1}", Unicode15_1).unwrap();
    let current = seed_v18(BIP39_MNEMONIC, "\u{a7f1}").unwrap();
    let legacy_ascii =
        derivation::mnemonic_to_seed_with_profile(BIP39_MNEMONIC, "S", Unicode15_1).unwrap();
    assert_ne!(legacy.as_bytes(), legacy_ascii.as_bytes());
    assert_ne!(legacy.as_bytes(), current.as_bytes());
    assert_eq!(
        hex::encode(legacy.as_bytes()),
        "bed061151b51a8251bdb193c1d174b9c8915d2b3bdafcb94216186ee658508e0ee12a4c00540d1b2566037e716648e3754cecdea2e627bc25c722f160b12ceb0"
    );
    assert_eq!(
        hex::encode(current.as_bytes()),
        "88b573b7c92474305b4049a7ea9883c474fc256cdf24937006c29f343e5f0a1bca064f8e53a856536671c6b518b09397e742d9afff93b469b3ea2b5c4d128a9b"
    );

    // Both profiles must normalize the BIP-39 sentence and passphrase using
    // their own tables, not just differ on the version-sensitive scalar.
    let fullwidth_phrase: String = BIP39_MNEMONIC
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                char::from_u32(c as u32 + 0xfee0).unwrap()
            } else {
                c
            }
        })
        .collect();
    let legacy_fullwidth =
        derivation::mnemonic_to_seed_with_profile(&fullwidth_phrase, "pässphraseÅ", Unicode15_1)
            .unwrap();
    let legacy_decomposed = derivation::mnemonic_to_seed_with_profile(
        BIP39_MNEMONIC,
        "pa\u{308}ssphraseA\u{30a}",
        Unicode15_1,
    )
    .unwrap();
    assert_eq!(legacy_fullwidth.as_bytes(), legacy_decomposed.as_bytes());
}

#[test]
fn keystore_and_recovery_manifest_bind_the_explicit_profile() {
    use aura_wallet::derivation::Bip39NormalizationProfile::{Unicode15_1, Unicode18_0};
    use aura_wallet::keystore::RecoveryMetadata;

    let password = Password::new("public-profile-keystore-password");
    let new_password = Password::new("public-profile-reencrypted-password");
    let kdf = KdfParams::Argon2id {
        m_cost: 19_456,
        t_cost: 2,
        p_cost: 1,
    };
    let store = Keystore::create_from_mnemonic_with_profile(
        BIP39_MNEMONIC,
        "\u{a7f1}",
        Unicode15_1,
        &password,
        kdf,
    )
    .unwrap();
    assert_eq!(store.normalization_profile(), Some(Unicode15_1));

    let marker = store.recovery_metadata().unwrap();
    assert_eq!(marker.normalization_profile(), Unicode15_1);
    assert_eq!(
        marker.to_json_string().unwrap(),
        r#"{"normalization_profile":"bip39-nfkd-unicode-15.1"}"#
    );
    assert_eq!(
        RecoveryMetadata::from_json_str(&marker.to_json_string().unwrap()).unwrap(),
        marker
    );
    assert_eq!(
        RecoveryMetadata::from_json_str("{}"),
        Err(KeystoreError::RecoveryProfileMissing)
    );
    assert_eq!(
        RecoveryMetadata::from_json_str(r#"{"normalization_profile":"bip39-nfkd-unicode-19.0"}"#),
        Err(KeystoreError::Unsupported)
    );
    assert!(matches!(
        RecoveryMetadata::from_json_str(
            r#"{"normalization_profile":"bip39-nfkd-unicode-15.1","normalization_profile":"bip39-nfkd-unicode-18.0"}"#
        ),
        Err(KeystoreError::InvalidFormat)
    ));

    let parsed = Keystore::from_json_str(&store.to_json_string().unwrap()).unwrap();
    assert_eq!(parsed.normalization_profile(), Some(Unicode15_1));
    parsed
        .verify_mnemonic_recovery(BIP39_MNEMONIC, "\u{a7f1}", marker, &password)
        .unwrap();
    assert_eq!(
        parsed.verify_mnemonic_recovery(
            BIP39_MNEMONIC,
            "\u{a7f1}",
            RecoveryMetadata::new(Unicode18_0),
            &password,
        ),
        Err(KeystoreError::RecoveryProfileMismatch)
    );
    assert_eq!(
        parsed.verify_mnemonic_recovery(BIP39_MNEMONIC, "S", marker, &password,),
        Err(KeystoreError::AuthenticationFailed)
    );

    let reencrypted = parsed.reencrypt(&password, &new_password, kdf).unwrap();
    assert_eq!(reencrypted.normalization_profile(), Some(Unicode15_1));
    assert_eq!(
        reencrypted.decrypt_seed(&new_password).unwrap().as_bytes(),
        parsed.decrypt_seed(&password).unwrap().as_bytes()
    );

    let raw_seed = seed_v18(BIP39_MNEMONIC, "\u{a7f1}").unwrap();
    let markerless = Keystore::encrypt_seed(&raw_seed, &password, kdf).unwrap();
    assert_eq!(markerless.normalization_profile(), None);
    assert_eq!(
        markerless.verify_mnemonic_recovery(BIP39_MNEMONIC, "\u{a7f1}", marker, &password),
        Err(KeystoreError::RecoveryProfileMissing)
    );

    let mut changed: serde_json::Value =
        serde_json::from_str(&store.to_json_string().unwrap()).unwrap();
    changed["seed_derivation"]["normalization_profile"] = serde_json::json!(Unicode18_0.id());
    let changed = Keystore::from_json_str(&changed.to_string()).unwrap();
    assert_eq!(
        changed.decrypt_seed(&password).err(),
        Some(KeystoreError::AuthenticationFailed)
    );
}

#[test]
fn bip39_nfkd_phrase_and_passphrase_match_independent_reference() {
    // Public test data: Python unicodedata.normalize("NFKD") + hashlib.pbkdf2_hmac.
    let fullwidth: String = BIP39_MNEMONIC
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                char::from_u32(c as u32 + 0xfee0).unwrap()
            } else {
                c
            }
        })
        .collect();
    let expected = "e3d85b52995dc685199fc741ef27da2f2af12355990911bb2e1d39ac8e3fa09d4180c28bf153a210568a49541a266317e21a9c9c43ff95316726cce0b0a3e020";
    let composed = seed_v18(&fullwidth, "pässphraseÅ").unwrap();
    let decomposed = seed_v18(BIP39_MNEMONIC, "pa\u{308}ssphraseA\u{30a}").unwrap();
    assert_eq!(hex::encode(composed.as_bytes()), expected);
    assert_eq!(composed.as_bytes(), decomposed.as_bytes());
}

#[test]
fn bip32_official_test_vector_1() {
    let seed = hex::decode(BIP32_SEED_HEX).unwrap();
    let root = XPrv::derive_from_path(&seed, &"m".parse().unwrap()).unwrap();
    assert_eq!(
        root.to_string(Prefix::XPRV).as_str(),
        concat!(
            "xp",
            "rv9s21ZrQH143K3QTDL4LXw2F7HEK3wJUD2nW2nRk4stbPy6cq3jPPqjiChkVvvNKmPGJxWUtg6LnF5kejMRNNU3TGtRBeJgk33yuGBxrMPHi"
        )
    );
    let child = XPrv::derive_from_path(&seed, &"m/0'".parse().unwrap()).unwrap();
    assert_eq!(
        child.to_string(Prefix::XPRV).as_str(),
        concat!(
            "xp",
            "rv9uHRZZhk6KAJC1avXpDAp4MDc3sQKNxDiPvvkX8Br5ngLNv1TxvUxt4cV1rGL5hj6KCesnDYUhd7oWgT11eZG7XnxHrnYeSvkzY7d2bhkJ7"
        )
    );
}

#[test]
fn slip10_ed25519_master_vector_and_hardened_path() {
    let raw_seed = hex::decode(BIP32_SEED_HEX).unwrap();
    let master = ExtendedEdKey::master_from_bytes(&raw_seed).unwrap();
    let seed = seed_from_hex(BIP32_SEED_HEX);
    assert_eq!(
        hex::encode(master.signing_key().as_bytes()),
        "2b4be7f19ee27bbf30c667b642d5f4aa69fd169872f8fc3059c08ebae2eb19e7"
    );
    assert_eq!(
        hex::encode(master.public_key()),
        "a4b2856bfec510abab89753fac1ac0e1112364e7d250545963f135f2a33188ed"
    );
    let a0 = derivation::derive_account(&seed, 0).unwrap();
    let a1 = derivation::derive_account(&seed, 1).unwrap();
    assert_eq!(a0.path, "m/44'/1'/0'/0'/0'");
    assert_ne!(a0.public_key, a1.public_key);
    assert_ne!(a0.address, a1.address);
    assert!(ExtendedEdKey::derive_path(&seed, "m/44'/1'/0'/0/0").is_err());
}

#[test]
fn accepted_testnet_paths_match_independent_python_openssl_vectors() {
    // BIP-39's published test mnemonic and TREZOR passphrase; these are public
    // test values, not usable secrets. Python hashlib/hmac plus OpenSSL Ed25519
    // independently generate the expected public keys (see the reference tool).
    let seed = seed_v18(BIP39_MNEMONIC, BIP39_PASSPHRASE).unwrap();
    for (index, expected) in [
        (
            0,
            "80304784f9593856e1ca54b514ec68397b001f88b534fc3862405b9e368ed8ff",
        ),
        (
            1,
            "2f9616add565b4c9692fa646c9461a58b0eb496d0cb550427b7ee29d7679cac4",
        ),
        (
            42,
            "cc05b53b876eb60fd25a706c5c99cbc3659220b993e22f5f6a5d5b2120bbf6c4",
        ),
    ] {
        let account = derivation::derive_account(&seed, index).unwrap();
        assert_eq!(hex::encode(account.public_key), expected);
        assert_eq!(account.path, format!("m/44'/1'/0'/0'/{index}'"));
    }
    assert!(derivation::derive_account(&seed, 1 << 31).is_err());
}

#[test]
#[cfg(feature = "insecure-test-kdf")]
fn keystore_roundtrip_and_wrong_passwords() {
    let seed = seed_v18(BIP39_MNEMONIC, "").unwrap();
    let password = Password::new("correct horse battery staple");
    let ks = encrypt_test_seed(&seed, &password, test_kdf()).unwrap();
    let decrypted = decrypt_test_seed(&ks, &password).unwrap();
    assert_eq!(decrypted.as_bytes(), seed.as_bytes());

    let mut failures = 0;
    for i in 0..1000u32 {
        let wrong = Password::new(format!("wrong-password-{i}"));
        if decrypt_test_seed(&ks, &wrong).is_err() {
            failures += 1;
        }
    }
    assert_eq!(failures, 1000, "1000 wrong passwords -> 0 decryptions");
}

#[test]
#[cfg(feature = "insecure-test-kdf")]
fn keystore_tamper_and_format_fuzz() {
    let seed = seed_v18(BIP39_MNEMONIC, "").unwrap();
    let password = Password::new("pw");
    let ks = encrypt_test_seed(&seed, &password, test_kdf()).unwrap();
    let valid = ks.to_json_string().unwrap();

    // Deterministic format fuzz: mutate bytes and require fail-closed or identical plaintext.
    let mut state = 0x1234_5678_9abc_def0u64;
    let bytes = valid.as_bytes();
    for _ in 0..5000 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let mut mutated = bytes.to_vec();
        let idx = (state as usize) % mutated.len();
        mutated[idx] ^= ((state >> 32) as u8) | 1;
        let text = String::from_utf8_lossy(&mutated);
        if let Ok(parsed) = Keystore::from_json_str(&text)
            && let Ok(decrypted) = decrypt_test_seed(&parsed, &password)
        {
            assert_eq!(
                decrypted.as_bytes(),
                seed.as_bytes(),
                "any accepted mutation must decrypt to the original seed"
            );
        }
    }

    // Explicit header tamper must fail authentication.
    let mut doc: serde_json::Value = serde_json::from_str(&valid).unwrap();
    doc["id"] = serde_json::json!("00000000000000000000000000000000");
    let tampered = Keystore::from_json_str(&doc.to_string()).unwrap();
    assert!(matches!(
        decrypt_test_seed(&tampered, &password),
        Err(KeystoreError::AuthenticationFailed)
    ));

    // A v2 profile marker cannot be relabelled as a v1 markerless schema.
    let mut downgraded: serde_json::Value = serde_json::from_str(&valid).unwrap();
    downgraded["version"] = serde_json::json!(1);
    assert!(matches!(
        Keystore::from_json_str(&downgraded.to_string()),
        Err(KeystoreError::InvalidFormat)
    ));
}

// All inputs are public test values. This changes one decoded byte without
// altering the schema or relying on a particular original hex character.
fn flipped_hex(text: &str, byte_index: usize) -> String {
    let mut bytes = hex::decode(text).unwrap();
    bytes[byte_index] ^= 1;
    hex::encode(bytes)
}

#[test]
fn production_keystores_reject_modified_ciphertext_and_gcm_tag() {
    let password = Password::new("public-interop-password");
    let expected = seed_v18(BIP39_MNEMONIC, BIP39_PASSPHRASE).unwrap();
    for (name, fixture) in [
        (
            "argon2id",
            include_str!("fixtures/w1_interop_argon2id.json"),
        ),
        ("scrypt", include_str!("fixtures/w1_interop_scrypt.json")),
    ] {
        let original: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let valid = Keystore::from_json_str(fixture).unwrap();
        assert_eq!(
            valid.decrypt_seed(&password).unwrap().as_bytes(),
            expected.as_bytes()
        );
        // Ciphertext bytes 0..63 encrypt the seed; 64..79 are the GCM tag.
        for index in [0, 63, 64, 79] {
            let mut changed = original.clone();
            changed["ciphertext"] =
                serde_json::json!(flipped_hex(original["ciphertext"].as_str().unwrap(), index));
            let parsed = Keystore::from_json_str(&changed.to_string()).unwrap();
            assert_eq!(
                parsed.decrypt_seed(&password).err(),
                Some(KeystoreError::AuthenticationFailed),
                "{name} ciphertext/tag byte {index}"
            );
        }
    }
}

#[test]
fn keystore_rejects_changes_to_fixed_schema_fields() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/w1_interop_argon2id.json")).unwrap();
    let mut changed_fields = Vec::new();
    for (label, path, value) in [
        ("format", "/format", serde_json::json!("other-format")),
        (
            "unknown normalization profile",
            "/seed_derivation/normalization_profile",
            serde_json::json!("bip39-nfkd-unicode-19.0"),
        ),
        ("version", "/version", serde_json::json!(3)),
        (
            "cipher name",
            "/cipher/name",
            serde_json::json!("other-cipher"),
        ),
        ("KDF name", "/kdf/name", serde_json::json!("other-kdf")),
        (
            "derivation scheme",
            "/derivation/scheme",
            serde_json::json!("other-scheme"),
        ),
        ("coin type", "/derivation/coin_type", serde_json::json!(2)),
        (
            "path template",
            "/derivation/path_template",
            serde_json::json!("other-path"),
        ),
        ("account index", "/accounts/0/index", serde_json::json!(1)),
        (
            "account path",
            "/accounts/0/path",
            serde_json::json!("other-path"),
        ),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(path).unwrap() = value;
        changed_fields.push((label, changed));
    }
    let mut missing_seed_derivation = original.clone();
    missing_seed_derivation
        .as_object_mut()
        .unwrap()
        .remove("seed_derivation");
    changed_fields.push(("missing seed derivation metadata", missing_seed_derivation));
    let mut missing_profile_id = original.clone();
    missing_profile_id["seed_derivation"]
        .as_object_mut()
        .unwrap()
        .remove("normalization_profile");
    changed_fields.push(("missing profile ID", missing_profile_id));
    for (label, accounts) in [
        ("no accounts", serde_json::json!([])),
        (
            "two accounts",
            serde_json::json!([
                original["accounts"][0].clone(),
                original["accounts"][0].clone()
            ]),
        ),
    ] {
        let mut changed = original.clone();
        changed["accounts"] = accounts;
        changed_fields.push((label, changed));
    }
    for (label, changed) in changed_fields {
        assert!(
            matches!(
                Keystore::from_json_str(&changed.to_string()),
                Err(KeystoreError::InvalidFormat | KeystoreError::Unsupported)
            ),
            "{label} must be rejected by schema validation"
        );
    }
}

#[cfg(feature = "insecure-test-kdf")]
#[test]
fn test_keystore_authenticates_header_and_every_ciphertext_byte() {
    // Deliberately weak test-only KDFs keep this mutation matrix practical.
    // Neither their parameters nor these public credentials are for real funds.
    let seed = seed_v18(BIP39_MNEMONIC, "").unwrap();
    let password = Password::new("public-auth-matrix");
    for (name, kdf) in [
        (
            "argon2id",
            KdfParams::Argon2id {
                m_cost: 32,
                t_cost: 1,
                p_cost: 1,
            },
        ),
        (
            "scrypt",
            KdfParams::Scrypt {
                log_n: 14,
                r: 8,
                p: 1,
            },
        ),
    ] {
        let keystore = Keystore::encrypt_seed_insecure_for_testing(&seed, &password, kdf).unwrap();
        assert_eq!(
            keystore
                .decrypt_seed_insecure_for_testing(&password)
                .unwrap()
                .as_bytes(),
            seed.as_bytes()
        );
        let original: serde_json::Value =
            serde_json::from_str(&keystore.to_json_string().unwrap()).unwrap();
        let mut changes: Vec<(&str, serde_json::Value)> = Vec::new();
        for (label, section, field) in [
            ("id", "", "id"),
            ("salt", "kdf", "salt"),
            ("nonce", "cipher", "nonce"),
            ("public key", "account", "public_key"),
        ] {
            let mut changed = original.clone();
            let item = match section {
                "" => &mut changed[field],
                "account" => &mut changed["accounts"][0][field],
                _ => &mut changed[section][field],
            };
            *item = serde_json::json!(flipped_hex(item.as_str().unwrap(), 0));
            changes.push((label, changed));
        }
        let mut changed = original.clone();
        changed["accounts"][0]["address"] = serde_json::json!("invalid-but-nonempty-address");
        changes.push(("address", changed));
        for (field, value) in if name == "argon2id" {
            [("m_cost", 64), ("t_cost", 2), ("p_cost", 2)]
        } else {
            [("log_n", 15), ("r", 9), ("p", 2)]
        } {
            let mut changed = original.clone();
            changed["kdf"][field] = serde_json::json!(value);
            changes.push((field, changed));
        }
        // Exercise every byte for fast Argon2, and both body/tag boundaries
        // for the more expensive weak-scrypt reference.
        let indices: Vec<usize> = if name == "argon2id" {
            (0..80).collect()
        } else {
            vec![0, 63, 64, 79]
        };
        for index in indices {
            let mut changed = original.clone();
            changed["ciphertext"] =
                serde_json::json!(flipped_hex(original["ciphertext"].as_str().unwrap(), index));
            let parsed = Keystore::from_json_str(&changed.to_string()).unwrap();
            assert_eq!(
                parsed.decrypt_seed_insecure_for_testing(&password).err(),
                Some(KeystoreError::AuthenticationFailed),
                "{name} ciphertext/tag byte {index}"
            );
        }
        for (label, changed) in changes {
            // These mutations remain structurally and resource-policy valid:
            // rejection must come from authentication, not from the parser.
            let parsed = Keystore::from_json_str(&changed.to_string())
                .unwrap_or_else(|e| panic!("{name} {label} did not parse: {e}"));
            assert_eq!(
                parsed.decrypt_seed_insecure_for_testing(&password).err(),
                Some(KeystoreError::AuthenticationFailed),
                "{name} modified {label}"
            );
        }
    }
}

#[cfg(not(any(target_os = "redox", target_os = "espidf")))]
#[test]
fn keystore_file_permissions_and_no_overwrite() {
    let seed = seed_v18(BIP39_MNEMONIC, "").unwrap();
    let password = Password::new("pw");
    let ks = encrypt_test_seed(&seed, &password, test_kdf()).unwrap();
    let dir = std::env::temp_dir().join(format!("auralite-w1-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("keystore.json");
    ks.save_new(&path).unwrap();
    let loaded = Keystore::load(&path).unwrap();
    assert_eq!(
        decrypt_test_seed(&loaded, &password).unwrap().as_bytes(),
        seed.as_bytes()
    );
    assert_eq!(ks.save_new(&path), Err(KeystoreError::AlreadyExists));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn parsed_keystore_can_be_serialized_within_loader_limit() {
    use aura_wallet::keystore::MAX_KEYSTORE_BYTES;
    let mut doc: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/w1_interop_argon2id.json")).unwrap();
    // The address is deliberately unauthenticated invalid test data: parser
    // size/serialization invariants must hold even before decryption.
    doc["accounts"][0]["address"] = serde_json::json!("x");
    let one_character_size = serde_json::to_string(&doc).unwrap().len();
    let max = MAX_KEYSTORE_BYTES as usize;
    let exact_address_len = max - one_character_size + 1;
    doc["accounts"][0]["address"] = serde_json::json!("x".repeat(exact_address_len));
    let compact_at_limit = serde_json::to_string(&doc).unwrap();
    assert_eq!(compact_at_limit.len(), max);
    assert!(serde_json::to_string_pretty(&doc).unwrap().len() > max);
    let parsed = Keystore::from_json_str(&compact_at_limit).unwrap();
    let serialized = parsed.to_json_string().unwrap();
    assert_eq!(serialized.len(), max);
    assert!(!serialized.contains('\n'), "must use compact fallback");
    #[cfg(not(any(target_os = "redox", target_os = "espidf")))]
    {
        let dir = std::env::temp_dir().join(format!("auralite-w1-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keystore.json");
        parsed.save_new(&path).unwrap();
        let loaded = Keystore::load(&path);
        std::fs::remove_dir_all(dir).unwrap();
        assert!(
            loaded.is_ok(),
            "saved document cannot be loaded: {loaded:?}, {} bytes",
            serialized.len()
        );
    }
    assert!(serialized.len() <= max);
    assert!(Keystore::from_json_str(&serialized).is_ok());

    // Escaped controls and multi-byte UTF-8 cannot break the byte-size bound.
    let special = "\0\n\t\u{0008}\\\"/é\u{2028}🔐";
    doc["accounts"][0]["address"] = serde_json::json!(special);
    let special_base_len = serde_json::to_string(&doc).unwrap().len();
    doc["accounts"][0]["address"] =
        serde_json::json!(format!("{special}{}", "x".repeat(max - special_base_len)));
    let special_at_limit = serde_json::to_string(&doc).unwrap();
    assert_eq!(special_at_limit.len(), max);
    let reencoded = Keystore::from_json_str(&special_at_limit)
        .unwrap()
        .to_json_string()
        .unwrap();
    assert!(reencoded.len() <= max);
    assert!(Keystore::from_json_str(&reencoded).is_ok());

    // A sizable document with enough space for pretty output still roundtrips.
    doc["accounts"][0]["address"] = serde_json::json!("x".repeat(exact_address_len - 4096));
    let below_limit = serde_json::to_string(&doc).unwrap();
    let parsed = Keystore::from_json_str(&below_limit).unwrap();
    let serialized = parsed.to_json_string().unwrap();
    assert!(serialized.len() <= max);
    assert!(
        serialized.contains('\n'),
        "normally retains pretty formatting"
    );
    assert!(Keystore::from_json_str(&serialized).is_ok());
}

#[test]
fn secrets_are_redacted_in_debug_and_display() {
    let seed = SecretSeed::new([0xAB; 64]);
    let password = Password::new("super-secret-password");
    assert!(!format!("{seed:?}").contains("ab"));
    assert!(!format!("{password:?}").contains("super-secret"));
    assert_eq!(format!("{password}"), "[REDACTED]");
}

#[test]
fn production_rejects_weak_kdf_parameters() {
    let seed = seed_v18(BIP39_MNEMONIC, "").unwrap();
    let password = Password::new("pw");
    let weak = KdfParams::Argon2id {
        m_cost: 8,
        t_cost: 1,
        p_cost: 1,
    };
    assert!(matches!(
        Keystore::encrypt_seed(&seed, &password, weak),
        Err(KeystoreError::WeakKdfParameters)
    ));
}

#[test]
#[cfg(all(
    feature = "insecure-test-kdf",
    not(any(target_os = "redox", target_os = "espidf"))
))]
fn concurrent_save_never_overwrites() {
    use std::sync::{Arc, Barrier};
    let seed = seed_v18(BIP39_MNEMONIC, "").unwrap();
    let ks = Arc::new(encrypt_test_seed(&seed, &Password::new("pw"), test_kdf()).unwrap());
    let dir = std::env::temp_dir().join(format!("auralite-w1-race-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("race.json");
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let ks = Arc::clone(&ks);
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                ks.save_new(&path)
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(KeystoreError::AlreadyExists))
            .count(),
        7
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(feature = "insecure-test-kdf")]
fn malformed_kdf_integer_does_not_wrap() {
    let seed = seed_v18(BIP39_MNEMONIC, "").unwrap();
    let ks = encrypt_test_seed(&seed, &Password::new("pw"), test_kdf()).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&ks.to_json_string().unwrap()).unwrap();
    value["kdf"]["m_cost"] = serde_json::json!(4_294_967_304u64);
    assert!(matches!(
        Keystore::from_json_str(&value.to_string()),
        Err(KeystoreError::InvalidFormat)
    ));
}

#[test]
fn slip10_ed25519_official_hardened_child_vector() {
    // SLIP-0010 test vector 2, m/0H and m/0H/2147483647H.
    let raw = hex::decode(concat!(
        "fffcf9f6f3f0edeae7e4e1dedbd8d5d2cfccc9c6c3c0bdbab7b4b1aeaba8a5a2",
        "9f9c999693908d8a8784817e7b7875726f6c696663605d5a5754514e4b484542"
    ))
    .unwrap();
    let first = ExtendedEdKey::master_from_bytes(&raw)
        .unwrap()
        .derive_hardened(0)
        .unwrap();
    assert_eq!(
        hex::encode(first.signing_key().as_bytes()),
        "1559eb2bbec5790b0c65d8693e4d0875b1747f4970ae8b650486ed7470845635"
    );
    assert_eq!(
        hex::encode(first.public_key()),
        "86fab68dcb57aa196c77c5f264f215a112c22a912c10d123b0d03c3c28ef1037"
    );
    let second = first.derive_hardened(2_147_483_647).unwrap();
    assert_eq!(
        hex::encode(second.signing_key().as_bytes()),
        "ea4f5bfe8694d8bb74b7b59404632fd5968b774ed545e810de9c32a4fb4192f4"
    );
    assert_eq!(
        hex::encode(second.public_key()),
        "5ba3b9ac6e90e83effcd25ac4e58a1365a9e35a3d3ae5eb07b9e4d90bcf7506d"
    );
}

#[test]
fn scrypt_roundtrip_and_wrong_password() {
    let seed = seed_v18(BIP39_MNEMONIC, "").unwrap();
    let password = Password::new("scrypt test");
    let ks = encrypt_test_seed(
        &seed,
        &password,
        KdfParams::Scrypt {
            log_n: if cfg!(feature = "insecure-test-kdf") {
                14
            } else {
                17
            },
            r: 8,
            p: 1,
        },
    )
    .unwrap();
    let parsed = Keystore::from_json_str(&ks.to_json_string().unwrap()).unwrap();
    assert_eq!(
        decrypt_test_seed(&parsed, &password).unwrap().as_bytes(),
        seed.as_bytes()
    );
    assert!(matches!(
        decrypt_test_seed(&parsed, &Password::new("incorrect")),
        Err(KeystoreError::AuthenticationFailed)
    ));
}

#[test]
#[cfg(feature = "insecure-test-kdf")]
fn format_rejects_duplicate_unknown_and_malformed_fields() {
    let seed = seed_v18(BIP39_MNEMONIC, "").unwrap();
    let ks = encrypt_test_seed(&seed, &Password::new("pw"), test_kdf()).unwrap();
    let valid = ks.to_json_string().unwrap();
    let duplicate = valid.replacen('{', r#"{"form\u0061t":"auralite-keystore", "#, 1);
    assert!(matches!(
        Keystore::from_json_str(&duplicate),
        Err(KeystoreError::InvalidFormat)
    ));
    let duplicate_nested = valid.replacen("\"kdf\": {", r#""kdf": {"na\u006de":"argon2id", "#, 1);
    assert!(matches!(
        Keystore::from_json_str(&duplicate_nested),
        Err(KeystoreError::InvalidFormat)
    ));
    let mut unknown: serde_json::Value = serde_json::from_str(&valid).unwrap();
    unknown["cipher"]["unexpected"] = serde_json::json!(42);
    assert!(matches!(
        Keystore::from_json_str(&unknown.to_string()),
        Err(KeystoreError::InvalidFormat)
    ));
    let mut malformed: serde_json::Value = serde_json::from_str(&valid).unwrap();
    malformed["cipher"]["nonce"] = serde_json::json!("00");
    assert!(matches!(
        Keystore::from_json_str(&malformed.to_string()),
        Err(KeystoreError::InvalidFormat)
    ));
}

#[test]
fn mnemonic_generation_and_explicit_zeroization() {
    use zeroize::Zeroize;
    for words in [12, 15, 18, 21, 24] {
        let mut phrase = derivation::generate_mnemonic(words).unwrap();
        assert_eq!(phrase.split_whitespace().count(), words);
        let mut seed = seed_v18(&phrase, "").unwrap();
        let mut key = derivation::derive_account(&seed, 0).unwrap().secret;
        key.zeroize();
        assert_eq!(key.as_bytes(), &[0; 32]);
        seed.zeroize();
        assert_eq!(seed.as_bytes(), &[0; 64]);
        phrase.zeroize();
        assert!(phrase.is_empty());
    }
    assert!(derivation::generate_mnemonic(11).is_err());
    assert!(ExtendedEdKey::master_from_bytes(&[0; 15]).is_err());
    assert!(ExtendedEdKey::master_from_bytes(&[0; 65]).is_err());
    let mut password = Password::new("erase me");
    password.zeroize();
    assert!(password.as_str().is_empty());
}

#[test]
fn python_generated_keystore_interoperability_vectors() {
    // Public BIP-39 example seed; never use this password or data for funds.
    let expected = seed_v18(BIP39_MNEMONIC, BIP39_PASSPHRASE).unwrap();
    let password = Password::new("public-interop-password");
    let wrong_password = Password::new("wrong-public-password");
    for fixture in [
        include_str!("fixtures/w1_interop_argon2id.json"),
        include_str!("fixtures/w1_interop_scrypt.json"),
    ] {
        let keystore = Keystore::from_json_str(fixture).unwrap();
        assert_eq!(
            keystore.normalization_profile(),
            Some(derivation::DEFAULT_BIP39_NORMALIZATION_PROFILE)
        );
        assert_eq!(
            keystore.decrypt_seed(&password).unwrap().as_bytes(),
            expected.as_bytes()
        );
        assert!(keystore.decrypt_seed(&wrong_password).is_err());
        let mut header: serde_json::Value = serde_json::from_str(fixture).unwrap();
        header["id"] = serde_json::json!("00000000000000000000000000000000");
        let modified = Keystore::from_json_str(&header.to_string()).unwrap();
        assert!(modified.decrypt_seed(&password).is_err());
    }
}

#[cfg(not(feature = "insecure-test-kdf"))]
#[test]
fn python_schema_reference_matches_parser_matrix() {
    // Parse-only: accepted documents are unauthenticated and may have stale
    // GCM tags. This oracle deliberately uses the default parser KDF limits.
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/w1_schema_reference.json")).unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 71);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let input = case["document"].as_str().unwrap();
        let expected = case["accepted"].as_bool().unwrap();
        assert_eq!(
            Keystore::from_json_str(input).is_ok(),
            expected,
            "schema case: {name}"
        );
    }
}

#[test]
fn python_profile_vectors_decrypt_and_authenticate_marker() {
    use aura_wallet::derivation::Bip39NormalizationProfile::Unicode15_1;

    // Public vectors: ASCII mnemonic/passphrase deliberately derive the same
    // seed under both frozen profiles. They do not test version-sensitive NFKD.
    let expected = seed_v18(BIP39_MNEMONIC, BIP39_PASSPHRASE).unwrap();
    let password = Password::new("public-interop-password");
    let cases = [
        (
            include_str!("fixtures/w1_interop_argon2id_profile_null.json"),
            None,
        ),
        (
            include_str!("fixtures/w1_interop_scrypt_profile_null.json"),
            None,
        ),
        (
            include_str!("fixtures/w1_interop_argon2id_profile_15_1.json"),
            Some(Unicode15_1),
        ),
        (
            include_str!("fixtures/w1_interop_scrypt_profile_15_1.json"),
            Some(Unicode15_1),
        ),
    ];
    for (fixture, profile) in cases {
        let store = Keystore::from_json_str(fixture).unwrap();
        assert_eq!(store.normalization_profile(), profile);
        assert_eq!(
            store.decrypt_seed(&password).unwrap().as_bytes(),
            expected.as_bytes()
        );
        let mut altered: serde_json::Value = serde_json::from_str(fixture).unwrap();
        altered["seed_derivation"]["normalization_profile"] = if profile.is_some() {
            serde_json::Value::Null
        } else {
            serde_json::json!("bip39-nfkd-unicode-15.1")
        };
        let tampered = Keystore::from_json_str(&altered.to_string()).unwrap();
        assert!(matches!(
            tampered.decrypt_seed(&password),
            Err(KeystoreError::AuthenticationFailed)
        ));
    }
}

#[test]
fn independent_sensitive_profile_fixtures_recover_distinct_seeds() {
    use aura_wallet::derivation::Bip39NormalizationProfile::{Unicode15_1, Unicode18_0};
    use aura_wallet::keystore::RecoveryMetadata;

    let password = Password::new("public-interop-password");
    let cases = [
        (
            include_str!("fixtures/w1_interop_argon2id_profile_sensitive_15_1.json"),
            Unicode15_1,
        ),
        (
            include_str!("fixtures/w1_interop_argon2id_profile_sensitive_18_0.json"),
            Unicode18_0,
        ),
    ];
    let mut previous: Option<SecretSeed> = None;
    for (fixture, profile) in cases {
        let store = Keystore::from_json_str(fixture).unwrap();
        let seed = store.decrypt_seed(&password).unwrap();
        let expected =
            derivation::mnemonic_to_seed_with_profile(BIP39_MNEMONIC, "\u{a7f1}", profile).unwrap();
        assert_eq!(seed.as_bytes(), expected.as_bytes());
        if let Some(ref previous) = previous {
            assert_ne!(seed.as_bytes(), previous.as_bytes());
        }
        previous = Some(seed);
        store
            .verify_mnemonic_recovery(
                BIP39_MNEMONIC,
                "\u{a7f1}",
                RecoveryMetadata::new(profile),
                &password,
            )
            .unwrap();
        let other = if profile == Unicode15_1 {
            Unicode18_0
        } else {
            Unicode15_1
        };
        assert_eq!(
            store.verify_mnemonic_recovery(
                BIP39_MNEMONIC,
                "\u{a7f1}",
                RecoveryMetadata::new(other),
                &password
            ),
            Err(KeystoreError::RecoveryProfileMismatch)
        );
    }
}

#[test]
fn unicode_interop_vectors_use_nfkd_mnemonic_and_raw_password_bytes() {
    // All seed, password and KDF inputs below are PUBLIC test data.
    let expected_seed = "e3d85b52995dc685199fc741ef27da2f2af12355990911bb2e1d39ac8e3fa09d4180c28bf153a210568a49541a266317e21a9c9c43ff95316726cce0b0a3e020";
    let seed = seed_v18(BIP39_MNEMONIC, "pässphraseÅ").unwrap();
    assert_eq!(hex::encode(seed.as_bytes()), expected_seed);
    let password = Password::new("public-café-🔐");
    let visually_equal_but_different_bytes = Password::new("public-cafe\u{301}-🔐");
    for fixture in [
        include_str!("fixtures/w1_interop_argon2id_unicode.json"),
        include_str!("fixtures/w1_interop_scrypt_unicode.json"),
    ] {
        let keystore = Keystore::from_json_str(fixture).unwrap();
        assert_eq!(
            keystore.decrypt_seed(&password).unwrap().as_bytes(),
            seed.as_bytes()
        );
        assert!(matches!(
            keystore.decrypt_seed(&visually_equal_but_different_bytes),
            Err(KeystoreError::AuthenticationFailed)
        ));
    }
}

#[test]
fn keystore_parser_and_file_loader_enforce_size_and_depth_limits() {
    use aura_wallet::keystore::MAX_KEYSTORE_BYTES;
    let valid = include_str!("fixtures/w1_interop_scrypt.json");
    let limit = MAX_KEYSTORE_BYTES as usize;
    let mut at_limit = valid.to_owned();
    at_limit.extend(std::iter::repeat_n(' ', limit - valid.len()));
    assert!(Keystore::from_json_str(&at_limit).is_ok());
    at_limit.push(' ');
    assert!(matches!(
        Keystore::from_json_str(&at_limit),
        Err(KeystoreError::InvalidFormat)
    ));

    #[cfg(not(any(target_os = "redox", target_os = "espidf")))]
    {
        let directory =
            std::env::temp_dir().join(format!("auralite-w1-size-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("keystore.json");
        std::fs::write(&path, at_limit.as_bytes()).unwrap();
        assert!(matches!(
            Keystore::load(&path),
            Err(KeystoreError::InvalidFormat)
        ));
        std::fs::write(&path, b"\xff\xfe").unwrap();
        assert!(matches!(
            Keystore::load(&path),
            Err(KeystoreError::InvalidFormat)
        ));
        std::fs::remove_dir_all(directory).unwrap();
    }

    let deep = format!("{}0{}", "[".repeat(256), "]".repeat(256));
    assert!(matches!(
        Keystore::from_json_str(&deep),
        Err(KeystoreError::InvalidFormat)
    ));
}

#[test]
fn production_rejects_weak_scrypt_and_requires_explicit_migration() {
    let fixture = include_str!("fixtures/w1_interop_scrypt_legacy.json");
    let expected = seed_v18(BIP39_MNEMONIC, BIP39_PASSPHRASE).unwrap();
    let password = Password::new("public-interop-password");
    let legacy = Keystore::from_json_str(fixture).unwrap();
    assert!(matches!(
        legacy.decrypt_seed(&password),
        Err(KeystoreError::WeakKdfParameters)
    ));
    assert!(matches!(
        legacy.decrypt_legacy_scrypt_for_migration(&Password::new("wrong-public-password")),
        Err(KeystoreError::AuthenticationFailed)
    ));
    let mut tampered: serde_json::Value = serde_json::from_str(fixture).unwrap();
    tampered["id"] = serde_json::json!("202132435465768798a9bacbdcedfe0f");
    let tampered = Keystore::from_json_str(&tampered.to_string()).unwrap();
    assert!(matches!(
        tampered.decrypt_legacy_scrypt_for_migration(&password),
        Err(KeystoreError::AuthenticationFailed)
    ));
    let recovered = legacy
        .decrypt_legacy_scrypt_for_migration(&password)
        .unwrap();
    assert_eq!(recovered.as_bytes(), expected.as_bytes());
    for log_n in [14, 15, 16] {
        assert!(matches!(
            Keystore::encrypt_seed(
                &recovered,
                &password,
                KdfParams::Scrypt { log_n, r: 8, p: 1 }
            ),
            Err(KeystoreError::WeakKdfParameters)
        ));
    }
    // Even a high work factor is refused when memory would exceed 256 MiB.
    assert!(matches!(
        Keystore::encrypt_seed(
            &recovered,
            &password,
            KdfParams::Scrypt {
                log_n: 18,
                r: 16,
                p: 1
            }
        ),
        Err(KeystoreError::WeakKdfParameters)
    ));
    let replacement = legacy
        .reencrypt_legacy_scrypt_for_migration(&password, &password, KdfParams::default_argon2id())
        .unwrap();
    assert_eq!(replacement.normalization_profile(), None);
    assert_eq!(
        replacement.decrypt_seed(&password).unwrap().as_bytes(),
        expected.as_bytes()
    );
    assert!(matches!(
        replacement.decrypt_legacy_scrypt_for_migration(&password),
        Err(KeystoreError::Unsupported)
    ));
}

#[test]
fn scrypt_wiping_fork_matches_rfc7914_and_upstream() {
    // RFC 7914 §12 test vector. Public values only, deliberately weak cost.
    let params = scrypt_w1_wipe::Params::new(4, 1, 1, 64).unwrap();
    let mut result = [0u8; 64];
    scrypt_w1_wipe::scrypt(b"", b"", &params, &mut result).unwrap();
    assert_eq!(
        hex::encode(result),
        concat!(
            "77d6576238657b203b19ca42c18a0497f16b4844e3074ae8dfdffa3fede21442",
            "fcd0069ded0948f8326a753a0fc81f17e8d3e0fb2e0d3628cf35e20c38d18906"
        )
    );

    // Compare the exact upstream release, used only by tests, for several
    // parameters and output lengths including parallel lanes and UTF-8 bytes.
    for (log_n, r, p, len, password, salt) in [
        (4, 1, 1, 32, &b""[..], &b""[..]),
        (8, 2, 3, 32, &b"password"[..], &b"NaCl"[..]),
        (10, 8, 2, 64, "café-🔐".as_bytes(), &b"public-salt"[..]),
    ] {
        let fork_params = scrypt_w1_wipe::Params::new(log_n, r, p, len).unwrap();
        let original_params = scrypt::Params::new(log_n, r, p, len).unwrap();
        let mut fork = vec![0u8; len];
        let mut original = vec![0u8; len];
        scrypt_w1_wipe::scrypt(password, salt, &fork_params, &mut fork).unwrap();
        scrypt::scrypt(password, salt, &original_params, &mut original).unwrap();
        assert_eq!(fork, original);
    }
}

#[cfg(all(unix, not(any(target_os = "redox", target_os = "espidf"))))]
#[test]
fn save_new_rejects_untrusted_or_symlink_parent() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root =
        std::env::temp_dir().join(format!("auralite-w1-parent-safety-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let untrusted = root.join("writable-by-others");
    std::fs::create_dir(&untrusted).unwrap();
    std::fs::set_permissions(&untrusted, std::fs::Permissions::from_mode(0o777)).unwrap();
    let ks = Keystore::from_json_str(include_str!("fixtures/w1_interop_argon2id.json")).unwrap();
    let untrusted_result = ks.save_new(&untrusted.join("danger.json"));
    let untrusted_exists = untrusted.join("danger.json").exists();

    let real = root.join("real");
    std::fs::create_dir(&real).unwrap();
    let link = root.join("symlink");
    symlink(&real, &link).unwrap();
    let symlink_result = ks.save_new(&link.join("danger.json"));
    let symlink_target_exists = real.join("danger.json").exists();

    let new_parent = root.join("new").join("nested");
    let new_result = ks.save_new(&new_parent.join("safe.json"));
    let new_mode = std::fs::metadata(&new_parent).unwrap().permissions().mode() & 0o777;
    std::fs::remove_dir_all(root).unwrap();
    assert!(untrusted_result.is_err());
    assert!(!untrusted_exists);
    assert!(symlink_result.is_err());
    assert!(!symlink_target_exists);
    assert!(new_result.is_ok());
    assert_eq!(new_mode & 0o077, 0, "new directory is private");
}

#[cfg(all(unix, not(any(target_os = "redox", target_os = "espidf"))))]
#[test]
fn load_rejects_fifo_and_final_symlink() {
    use rustix::fs::{CWD, Mode, mkfifoat};
    use std::os::unix::fs::symlink;
    let dir = std::env::temp_dir().join(format!("auralite-w1-load-safety-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("pipe");
    mkfifoat(CWD, &fifo, Mode::RUSR | Mode::WUSR).unwrap();
    let fifo_result = Keystore::load(&fifo);
    let target = dir.join("target.json");
    std::fs::write(&target, include_str!("fixtures/w1_interop_argon2id.json")).unwrap();
    let link = dir.join("link.json");
    symlink(&target, &link).unwrap();
    let link_result = Keystore::load(&link);
    std::fs::remove_dir_all(dir).unwrap();
    assert!(fifo_result.is_err());
    assert!(link_result.is_err());
}

#[cfg(all(unix, not(any(target_os = "redox", target_os = "espidf"))))]
#[test]
fn save_new_rejects_directory_syntax_without_creating_a_file() {
    let seed = SecretSeed::new([42; 64]);
    let ks = encrypt_test_seed(&seed, &Password::new("pw"), test_kdf()).unwrap();
    let root = std::env::temp_dir().join(format!(
        "auralite-w1-directory-syntax-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (path, leaf) in [
        ("slash/", "slash"),
        ("dot/.", "dot"),
        ("slashes//", "slashes"),
    ] {
        let result = ks.save_new(&root.join(path));
        let created = root.join(leaf).exists();
        if created {
            std::fs::remove_file(root.join(leaf)).unwrap();
        }
        assert_eq!(result, Err(KeystoreError::InvalidFormat), "{path}");
        assert!(!created, "a directory-like path must not create a file");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(any(target_os = "redox", target_os = "espidf"))]
#[test]
fn unsupported_platform_filesystem_operations_fail_closed() {
    let ks = Keystore::from_json_str(include_str!("fixtures/w1_interop_argon2id.json")).unwrap();
    let path = std::path::Path::new("not-created.json");
    assert_eq!(ks.save_new(path), Err(KeystoreError::UnsupportedPlatform));
    assert!(matches!(
        Keystore::load(path),
        Err(KeystoreError::UnsupportedPlatform)
    ));
}

#[test]
fn authenticated_public_key_must_match_decrypted_seed() {
    // Python independently generated a valid GCM tag over the wrong public key.
    let store =
        Keystore::from_json_str(include_str!("fixtures/w1_inconsistent_public_key.json")).unwrap();
    assert!(matches!(
        store.decrypt_seed(&Password::new("public-interop-password")),
        Err(KeystoreError::AuthenticationFailed)
    ));
}

#[test]
fn authenticated_address_must_match_decrypted_seed() {
    // This address belongs to a different valid test account, with a valid tag.
    let store =
        Keystore::from_json_str(include_str!("fixtures/w1_inconsistent_address.json")).unwrap();
    assert!(matches!(
        store.decrypt_seed(&Password::new("public-interop-password")),
        Err(KeystoreError::AuthenticationFailed)
    ));
}

#[test]
fn equivalent_json_encodings_authenticate_the_same_header() {
    // The authenticated bytes are the canonical parsed header, not the input
    // file's whitespace, field order, or JSON escape spelling.
    let original = include_str!("fixtures/w1_interop_argon2id.json");
    let document: serde_json::Value = serde_json::from_str(original).unwrap();
    let reversed = document
        .as_object()
        .unwrap()
        .iter()
        .rev()
        .map(|(key, value)| {
            format!(
                "  {} : {}",
                serde_json::to_string(key).unwrap(),
                serde_json::to_string(value).unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let alternate = format!("{{\n{reversed}\n}}")
        .replacen("\"format\"", "\"\\u0066ormat\"", 1)
        .replace('/', "\\/");
    assert_ne!(original, alternate);
    let store = Keystore::from_json_str(&alternate).unwrap();
    let seed = store
        .decrypt_seed(&Password::new("public-interop-password"))
        .unwrap();
    let expected = seed_v18(BIP39_MNEMONIC, BIP39_PASSPHRASE).unwrap();
    assert_eq!(seed.as_bytes(), expected.as_bytes());
}

#[cfg(feature = "insecure-test-kdf")]
#[test]
fn fresh_encryptions_use_distinct_public_random_values() {
    // Regression check, not proof of RNG quality or statistical independence.
    let seed = SecretSeed::new([0x42; 64]);
    let password = Password::new("public-test-password");
    let one = encrypt_test_seed(&seed, &password, test_kdf()).unwrap();
    let two = encrypt_test_seed(&seed, &password, test_kdf()).unwrap();
    let a: serde_json::Value = serde_json::from_str(&one.to_json_string().unwrap()).unwrap();
    let b: serde_json::Value = serde_json::from_str(&two.to_json_string().unwrap()).unwrap();
    assert_ne!(a["id"], b["id"]);
    assert_ne!(a["kdf"]["salt"], b["kdf"]["salt"]);
    assert_ne!(a["cipher"]["nonce"], b["cipher"]["nonce"]);
    assert_eq!(
        decrypt_test_seed(&one, &password).unwrap().as_bytes(),
        seed.as_bytes()
    );
    assert_eq!(
        decrypt_test_seed(&two, &password).unwrap().as_bytes(),
        seed.as_bytes()
    );
}

// These tests exercise integration with the existing C4 transaction primitives.
// They do not implement a wallet signing API, CLI or W3 offline workflow.
fn public_w1_transaction() -> (
    serde_json::Value,
    SecretSeed,
    derivation::DerivedAccount,
    aura_tx::UnsignedTx,
) {
    use aura_tx::{TxKind, TxParams, UnsignedTx};
    use aura_types::{Amount, ChainId, Gas, Height, Nonce};
    let vector: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/w1_tx_reference.json")).unwrap();
    let store = Keystore::from_json_str(include_str!("fixtures/w1_interop_argon2id.json")).unwrap();
    let seed = store
        .decrypt_seed(&Password::new("public-interop-password"))
        .unwrap();
    let account = derivation::derive_account(&seed, 0).unwrap();
    let tx = UnsignedTx::new(TxParams {
        chain_id: ChainId::new(vector["chain"].as_str().unwrap()).unwrap(),
        signer: account.address,
        nonce: Nonce::new(vector["nonce"].as_u64().unwrap()),
        kind: TxKind::Transfer,
        payload: hex::decode(vector["payload"].as_str().unwrap()).unwrap(),
        gas_limit: Gas::new(vector["gas"].as_u64().unwrap()),
        max_fee: Amount::from_units(u128::from(vector["max_fee"].as_u64().unwrap())),
        tip: Amount::from_units(u128::from(vector["tip"].as_u64().unwrap())),
        expiry: Height::new(vector["expiry"].as_u64().unwrap()),
    })
    .unwrap();
    (vector, seed, account, tx)
}

#[test]
fn wallet_keystore_to_transaction_matches_independent_reference() {
    use aura_tx::Tx;
    use aura_types::{Domain, Height};
    let (vector, _seed, account, unsigned) = public_w1_transaction();
    assert_eq!(account.path, vector["path"].as_str().unwrap());
    assert_eq!(
        hex::encode(account.public_key),
        vector["public_key"].as_str().unwrap()
    );
    assert_eq!(
        account.address.to_string(),
        vector["address"].as_str().unwrap()
    );
    assert_eq!(
        hex::encode(unsigned.signing_bytes()),
        vector["signing_bytes"].as_str().unwrap()
    );
    let digest = aura_crypto::hash_domain(
        unsigned.chain_id.as_str(),
        Domain::Tx,
        &unsigned.signing_bytes(),
    );
    assert_eq!(digest.to_hex(), vector["signing_hash"].as_str().unwrap());
    let tx = unsigned.sign(account.secret.as_bytes()).unwrap();
    assert_eq!(
        hex::encode(tx.sig.bytes),
        vector["signature"].as_str().unwrap()
    );
    assert_eq!(hex::encode(tx.to_bytes()), vector["tx"].as_str().unwrap());
    assert_eq!(tx.txid().to_hex(), vector["txid"].as_str().unwrap());
    let decoded = Tx::from_canonical_bytes(&tx.to_bytes()).unwrap();
    assert_eq!(decoded, tx);
    assert!(decoded.validate("auralite-test-1", Height::new(1)).is_ok());
    let mut trailing = tx.to_bytes();
    trailing.push(0);
    assert!(Tx::from_canonical_bytes(&trailing).is_err());
}

#[test]
fn wallet_transaction_rejects_foreign_chain() {
    use aura_tx::TxError;
    use aura_types::{ChainId, Height};
    let (_, _seed, account, unsigned) = public_w1_transaction();
    let tx = unsigned.sign(account.secret.as_bytes()).unwrap();
    assert!(matches!(
        tx.validate("auralite-1", Height::new(1)),
        Err(TxError::WrongChain(_, _))
    ));
    let mut rewritten = tx;
    rewritten.unsigned.chain_id = ChainId::new("auralite-1").unwrap();
    assert!(matches!(
        rewritten.validate("auralite-1", Height::new(1)),
        Err(TxError::InvalidSignature)
    ));
}

#[test]
fn wallet_transaction_rejects_foreign_domain_signature() {
    use aura_tx::TxError;
    use aura_types::Domain;
    let (_, _seed, account, unsigned) = public_w1_transaction();
    let mut tx = unsigned.sign(account.secret.as_bytes()).unwrap();
    tx.sig.bytes = aura_crypto::sign(
        account.secret.as_bytes(),
        tx.unsigned.chain_id.as_str(),
        Domain::Vote,
        &tx.signing_bytes(),
    );
    assert_eq!(tx.verify_signature(), Err(TxError::InvalidSignature));
}

#[test]
fn wallet_transaction_rejects_a_different_derived_key() {
    use aura_tx::TxError;
    let (_, seed, _account, unsigned) = public_w1_transaction();
    let other_account = derivation::derive_account(&seed, 1).unwrap();
    assert!(matches!(
        unsigned.sign(other_account.secret.as_bytes()),
        Err(TxError::SignerMismatch)
    ));
}

#[cfg(feature = "insecure-test-kdf")]
#[test]
fn insecure_feature_does_not_weaken_normal_kdf_policy() {
    let seed = SecretSeed::new([0x42; 64]);
    let password = Password::new("public-test-password");
    let weak = Keystore::encrypt_seed_insecure_for_testing(&seed, &password, KdfParams::testing())
        .unwrap();
    let parsed = Keystore::from_json_str(&weak.to_json_string().unwrap()).unwrap();
    assert!(matches!(
        parsed.decrypt_seed(&password),
        Err(KeystoreError::WeakKdfParameters)
    ));
    assert_eq!(
        parsed
            .decrypt_seed_insecure_for_testing(&password)
            .unwrap()
            .as_bytes(),
        seed.as_bytes()
    );
    assert!(matches!(
        Keystore::encrypt_seed(&seed, &password, KdfParams::testing()),
        Err(KeystoreError::WeakKdfParameters)
    ));
    assert!(matches!(
        Keystore::encrypt_seed(
            &seed,
            &password,
            KdfParams::Scrypt {
                log_n: 14,
                r: 8,
                p: 1
            }
        ),
        Err(KeystoreError::WeakKdfParameters)
    ));
    // A single too-low dimension must be rejected even when the other is valid.
    for weak in [
        KdfParams::Argon2id {
            m_cost: 19_455,
            t_cost: 2,
            p_cost: 1,
        },
        KdfParams::Argon2id {
            m_cost: 19_456,
            t_cost: 1,
            p_cost: 1,
        },
    ] {
        assert!(matches!(
            Keystore::encrypt_seed(&seed, &password, weak),
            Err(KeystoreError::WeakKdfParameters)
        ));
    }
    let legacy =
        Keystore::from_json_str(include_str!("fixtures/w1_interop_scrypt_legacy.json")).unwrap();
    assert!(matches!(
        legacy.decrypt_seed(&Password::new("public-interop-password")),
        Err(KeystoreError::WeakKdfParameters)
    ));
}

#[cfg(feature = "insecure-test-kdf")]
#[test]
fn insecure_test_parser_enforces_resource_caps() {
    // Parsing must reject oversized parameters before allocating a KDF buffer,
    // including when the explicitly unsafe testing feature is present.
    let seed = SecretSeed::new([0x42; 64]);
    let password = Password::new("public-test-password");
    let store = Keystore::encrypt_seed_insecure_for_testing(&seed, &password, KdfParams::testing())
        .unwrap();
    let mut argon: serde_json::Value =
        serde_json::from_str(&store.to_json_string().unwrap()).unwrap();
    argon["kdf"]["m_cost"] = serde_json::json!(262_145);
    assert!(matches!(
        Keystore::from_json_str(&argon.to_string()),
        Err(KeystoreError::WeakKdfParameters)
    ));
    let mut scrypt: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/w1_interop_scrypt.json")).unwrap();
    scrypt["kdf"]["log_n"] = serde_json::json!(18);
    scrypt["kdf"]["r"] = serde_json::json!(16);
    assert!(matches!(
        Keystore::from_json_str(&scrypt.to_string()),
        Err(KeystoreError::WeakKdfParameters)
    ));
}

#[test]
fn recovery_metadata_strict_schema_and_byte_limit_matrix() {
    use aura_wallet::derivation::Bip39NormalizationProfile::{Unicode15_1, Unicode18_0};
    use aura_wallet::keystore::RecoveryMetadata;

    // Explicit public cases: parsing the marker is not authentication, and
    // must never choose a profile for malformed or ambiguous metadata.
    let cases = [
        (
            "legacy",
            r#"{"normalization_profile":"bip39-nfkd-unicode-15.1"}"#,
            Some(Unicode15_1),
        ),
        (
            "current",
            r#"{"normalization_profile":"bip39-nfkd-unicode-18.0"}"#,
            Some(Unicode18_0),
        ),
        (
            "escaped field",
            r#"{"normalization_\u0070rofile":"bip39-nfkd-unicode-18.0"}"#,
            Some(Unicode18_0),
        ),
        (
            "escaped value",
            r#"{"normalization_profile":"bip39-nfkd-unicode-\u0031\u0035.1"}"#,
            Some(Unicode15_1),
        ),
        (
            "JSON whitespace",
            " \n\t{ \"normalization_profile\" : \"bip39-nfkd-unicode-18.0\" }\r",
            Some(Unicode18_0),
        ),
        ("missing", "{}", None),
        ("top null", "null", None),
        ("top array", "[]", None),
        ("top scalar", "123", None),
        ("null profile", r#"{"normalization_profile":null}"#, None),
        ("number profile", r#"{"normalization_profile":18}"#, None),
        ("array profile", r#"{"normalization_profile":[]}"#, None),
        ("object profile", r#"{"normalization_profile":{}}"#, None),
        ("empty profile", r#"{"normalization_profile":""}"#, None),
        (
            "unsupported profile",
            r#"{"normalization_profile":"bip39-nfkd-unicode-19.0"}"#,
            None,
        ),
        (
            "case sensitive",
            r#"{"normalization_profile":"BIP39-nfkd-unicode-18.0"}"#,
            None,
        ),
        (
            "trailing profile space",
            r#"{"normalization_profile":"bip39-nfkd-unicode-18.0 "}"#,
            None,
        ),
        (
            "unknown field",
            r#"{"normalization_profile":"bip39-nfkd-unicode-18.0","extra":null}"#,
            None,
        ),
        (
            "same duplicate",
            r#"{"normalization_profile":"bip39-nfkd-unicode-18.0","normalization_profile":"bip39-nfkd-unicode-18.0"}"#,
            None,
        ),
        (
            "escaped duplicate",
            r#"{"normalization_profile":"bip39-nfkd-unicode-15.1","normalization_\u0070rofile":"bip39-nfkd-unicode-18.0"}"#,
            None,
        ),
        (
            "escaped duplicate reversed",
            r#"{"normalization_\u0070rofile":"bip39-nfkd-unicode-18.0","normalization_profile":"bip39-nfkd-unicode-15.1"}"#,
            None,
        ),
        (
            "trailing comma",
            r#"{"normalization_profile":"bip39-nfkd-unicode-18.0",}"#,
            None,
        ),
        (
            "trailing JSON",
            r#"{"normalization_profile":"bip39-nfkd-unicode-18.0"}{}"#,
            None,
        ),
        (
            "unpaired surrogate",
            r#"{"normalization_profile":"\ud800"}"#,
            None,
        ),
        (
            "BOM",
            "\u{feff}{\"normalization_profile\":\"bip39-nfkd-unicode-18.0\"}",
            None,
        ),
    ];
    for (name, input, expected) in cases {
        let parsed = RecoveryMetadata::from_json_str(input);
        assert_eq!(
            parsed.as_ref().ok().map(|m| m.normalization_profile()),
            expected,
            "{name}"
        );
        if let Ok(marker) = parsed {
            let canonical = marker.to_json_string().unwrap();
            assert_eq!(
                RecoveryMetadata::from_json_str(&canonical).unwrap(),
                marker,
                "{name}"
            );
        }
    }

    let canonical = RecoveryMetadata::new(Unicode18_0).to_json_string().unwrap();
    let at_limit = format!("{}{}", canonical, " ".repeat(4096 - canonical.len()));
    assert_eq!(at_limit.len(), 4096);
    assert_eq!(
        RecoveryMetadata::from_json_str(&at_limit)
            .unwrap()
            .normalization_profile(),
        Unicode18_0
    );
    let oversized = format!("{at_limit} ");
    assert_eq!(oversized.len(), 4097);
    assert!(
        RecoveryMetadata::from_json_str(&oversized).is_err(),
        "4097 bytes must be rejected"
    );
}

#[test]
fn python_recovery_metadata_reference_matches_parser_matrix() {
    use aura_wallet::keystore::RecoveryMetadata;

    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/w1_recovery_metadata_reference.json")).unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 37);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let input = case["document"].as_str().unwrap();
        let accepted = case["accepted"].as_bool().unwrap();
        match RecoveryMetadata::from_json_str(input) {
            Ok(marker) => {
                assert!(accepted, "unexpected acceptance for {name}");
                assert_eq!(
                    marker.normalization_profile().id(),
                    case["profile"].as_str().unwrap(),
                    "profile mismatch for {name}"
                );
                assert_eq!(
                    marker.to_json_string().unwrap(),
                    case["canonical"].as_str().unwrap(),
                    "canonical mismatch for {name}"
                );
            }
            Err(err) => {
                assert!(!accepted, "unexpected error {err:?} for {name}");
                let actual_kind = match err {
                    KeystoreError::InvalidFormat => "invalid_format",
                    KeystoreError::RecoveryProfileMissing => "missing_profile",
                    KeystoreError::Unsupported => "unsupported",
                    other => panic!("unexpected error variant {other:?} for {name}"),
                };
                assert_eq!(
                    actual_kind,
                    case["error"].as_str().unwrap(),
                    "error kind mismatch for {name}"
                );
            }
        }
    }
}

#[test]
fn reauthenticated_uppercase_hex_header_and_canonical_account_rules() {
    let expected = seed_v18(BIP39_MNEMONIC, BIP39_PASSPHRASE).unwrap();
    let password = Password::new("public-interop-password");

    // Uppercase hex in id/salt/nonce/ciphertext is valid when authenticated,
    // while accounts[0] remains canonical lowercase.
    let upper_raw = include_str!("fixtures/w1_interop_argon2id_uppercase_hex_header.json");
    let upper_store = Keystore::from_json_str(upper_raw).unwrap();
    assert_eq!(
        upper_store.decrypt_seed(&password).unwrap().as_bytes(),
        expected.as_bytes()
    );

    // Header hex case is part of AAD: changing case in id, salt or nonce fails GCM.
    let base_doc: serde_json::Value = serde_json::from_str(upper_raw).unwrap();
    for mutate in [
        |d: &mut serde_json::Value| {
            d["id"] = d["id"].as_str().unwrap().to_ascii_lowercase().into();
        },
        |d: &mut serde_json::Value| {
            d["kdf"]["salt"] = d["kdf"]["salt"]
                .as_str()
                .unwrap()
                .to_ascii_lowercase()
                .into();
        },
        |d: &mut serde_json::Value| {
            d["cipher"]["nonce"] = d["cipher"]["nonce"]
                .as_str()
                .unwrap()
                .to_ascii_lowercase()
                .into();
        },
    ] {
        let mut altered = base_doc.clone();
        mutate(&mut altered);
        let parsed = Keystore::from_json_str(&altered.to_string()).unwrap();
        assert_eq!(
            parsed.decrypt_seed(&password).err(),
            Some(KeystoreError::AuthenticationFailed)
        );
    }

    // Ciphertext is excluded from AAD and decoded case-insensitively.
    let mut lower_ct = base_doc;
    lower_ct["ciphertext"] = lower_ct["ciphertext"]
        .as_str()
        .unwrap()
        .to_ascii_lowercase()
        .into();
    assert_eq!(
        Keystore::from_json_str(&lower_ct.to_string())
            .unwrap()
            .decrypt_seed(&password)
            .unwrap()
            .as_bytes(),
        expected.as_bytes()
    );

    // Validly GCM-authenticated documents whose account public_key or address
    // uses uppercase spelling of the true derived account must still fail closed.
    for fixture in [
        include_str!("fixtures/w1_inconsistent_uppercase_public_key.json"),
        include_str!("fixtures/w1_inconsistent_uppercase_address.json"),
    ] {
        let store = Keystore::from_json_str(fixture).unwrap();
        assert_eq!(
            store.decrypt_seed(&password).err(),
            Some(KeystoreError::AuthenticationFailed)
        );
    }

    // Legacy weak-scrypt migration must also reject a validly GCM-authenticated
    // document whose account metadata disagrees with the decrypted seed.
    let legacy_bad = Keystore::from_json_str(include_str!(
        "fixtures/w1_inconsistent_scrypt_legacy_account.json"
    ))
    .unwrap();
    assert_eq!(
        legacy_bad.decrypt_seed(&password).err(),
        Some(KeystoreError::WeakKdfParameters)
    );
    assert_eq!(
        legacy_bad
            .decrypt_legacy_scrypt_for_migration(&password)
            .err(),
        Some(KeystoreError::AuthenticationFailed)
    );
    assert_eq!(
        legacy_bad
            .reencrypt_legacy_scrypt_for_migration(
                &password,
                &Password::new("migrated-public-password"),
                KdfParams::default_argon2id(),
            )
            .err(),
        Some(KeystoreError::AuthenticationFailed)
    );
}
