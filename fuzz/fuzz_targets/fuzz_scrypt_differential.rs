#![no_main]

use libfuzzer_sys::fuzz_target;

// Public test-only inputs: tiny work factors keep the differential campaign
// bounded. This does not test or benchmark wallet production KDF policy.
fuzz_target!(|data: &[u8]| {
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
});
