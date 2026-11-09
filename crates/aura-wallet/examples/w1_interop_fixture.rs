//! Emits a keystore containing the publicly known W1 test-vector seed.
//! Never use this example or its password for real funds.

use aura_wallet::derivation::{
    self, Bip39NormalizationProfile, DEFAULT_BIP39_NORMALIZATION_PROFILE,
};
use aura_wallet::keystore::{KdfParams, Keystore};
use aura_wallet::secret::Password;

fn main() {
    let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let kind = std::env::args().nth(1).expect("pass a public vector kind");
    let unicode = kind.ends_with("_unicode");
    let passphrase = if kind.contains("_profile_sensitive_") {
        "\u{a7f1}"
    } else if unicode {
        "pa\u{308}ssphraseA\u{30a}"
    } else {
        "TREZOR"
    };
    let password = Password::new(if unicode {
        "public-café-🔐"
    } else {
        "public-interop-password"
    });
    let kdf = match kind.as_str() {
        "argon2id"
        | "argon2id_unicode"
        | "argon2id_profile_null"
        | "argon2id_profile_15_1"
        | "argon2id_profile_sensitive_15_1"
        | "argon2id_profile_sensitive_18_0" => KdfParams::default_argon2id(),
        "scrypt" | "scrypt_unicode" | "scrypt_profile_null" | "scrypt_profile_15_1" => {
            KdfParams::Scrypt {
                log_n: 17,
                r: 8,
                p: 1,
            }
        }
        _ => panic!("unknown vector kind; public test data only"),
    };
    let doc = if kind.ends_with("_profile_null") {
        let seed = derivation::mnemonic_to_seed_with_profile(
            phrase,
            passphrase,
            DEFAULT_BIP39_NORMALIZATION_PROFILE,
        )
        .expect("public test seed");
        Keystore::encrypt_seed(&seed, &password, kdf)
    } else {
        let profile =
            if kind.ends_with("_profile_15_1") || kind.ends_with("_profile_sensitive_15_1") {
                Bip39NormalizationProfile::Unicode15_1
            } else {
                DEFAULT_BIP39_NORMALIZATION_PROFILE
            };
        Keystore::create_from_mnemonic_with_profile(phrase, passphrase, profile, &password, kdf)
    }
    .expect("test vector encryption");
    println!("{}", doc.to_json_string().expect("serialize test document"));
}
