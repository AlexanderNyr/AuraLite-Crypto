//! Emits a keystore containing the publicly known W1 test-vector seed.
//! Never use this example or its password for real funds.

use aura_wallet::derivation::DEFAULT_BIP39_NORMALIZATION_PROFILE;
use aura_wallet::keystore::{KdfParams, Keystore};
use aura_wallet::secret::Password;

fn main() {
    let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let kind = std::env::args().nth(1).expect("pass a public vector kind");
    let unicode = kind.ends_with("_unicode");
    let passphrase = if unicode {
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
        "argon2id" | "argon2id_unicode" => KdfParams::default_argon2id(),
        "scrypt" | "scrypt_unicode" => KdfParams::Scrypt {
            log_n: 17,
            r: 8,
            p: 1,
        },
        _ => panic!("unknown vector kind; public test data only"),
    };
    let doc = Keystore::create_from_mnemonic_with_profile(
        phrase,
        passphrase,
        DEFAULT_BIP39_NORMALIZATION_PROFILE,
        &password,
        kdf,
    )
    .expect("test vector encryption");
    println!("{}", doc.to_json_string().expect("serialize test document"));
}
