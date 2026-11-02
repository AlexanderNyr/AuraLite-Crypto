# W0 Report — Wallet Baseline & Key-Exposure Audit

## Summary
Phase **W0** establishes the `aura-wallet` crate (`crates/aura-wallet`), the
key-exposure audit ("where keys could end up") across 8 concrete exposure
surfaces (`EXPOSURE_SURFACES`), 6 threat model notes (`THREAT_NOTES`,
`T-W0-1`..`T-W0-6`) aligned with decisions `D-W1`..`D-W4`, and the repo-wide
tree/log secret-material scanner (`scan_text_for_key_leaks` and
`audit_tree_for_key_leaks`).

## What W0 Delivers
1. **Honest baseline refusal and survey (`crates/aura-wallet/src/baseline.rs`, `src/bin/aura-wallet.rs`)**:
   - `aura-wallet` without arguments or invoked with `new/import/list/balance/transfer/sign/broadcast/stake/vote` exits `EXIT_STUB` (`2`) and names the missing W1–W4 capabilities.
   - `aura-wallet report [--fixture]` measures the 4 Stage 1 wallet components (`keystore`, `wallet-cli`, `offline-signing`, `multisig`): `0` implemented, `4` placeholders, `0` missing, pinned in `crates/aura-wallet/tests/fixtures/w0_baseline.txt`.
2. **Audit of where keys could end up (`EXPOSURE_SURFACES`, `THREAT_NOTES`, `docs/evidence/w0/threat-notes.md`)**:
   - Documents 8 exposure surfaces (`logs-and-tracing`, `cli-argv-and-env`, `plaintext-disk-files`, `process-memory-and-core-dumps`, `rpc-and-network-frames`, `online-hot-machine-signing`, `single-key-treasury-or-validator`, `ci-and-repository-fixtures`) and 6 threat notes (`T-W0-1`..`T-W0-6`), locking the BIP-44 derivation path `m/44'/ALCRY'/0'/0/i` (`D-W1`).
3. **Tree-search log scanner gate (`audit_tree_for_key_leaks`, `scan_text_for_key_leaks`)**:
   - Scans workspace `.log` files, `gates.txt`, fixtures, docs, and source files for leaked hex private keys (`private_key`, `secret_key`, `privkey`, `ed25519_secret`, `signing_key`, `bls_secret`, `xprv`, `aura1priv`), BIP-39 mnemonics (`mnemonic`, `seed_phrase`, `recovery_phrase`), PEM private key blocks, and raw seeds (`raw_seed`, `master_seed`, `secret_seed`, `bip39_seed`), while ignoring public identifiers (`public_key`, `block_hash`, `state_root`, `tx_hash`, `signature`).
   - Verified against the live workspace (`0` findings across all repository logs and files), a poisoned-log tree-search negative test (`key in a log -> gate red`), and 64 deterministic vectors in `crates/aura-wallet/tests/fixtures/w0_key_audit.txt`.
4. **Negative controls (`tools/nc_w0.py`)**:
   - 6/6 library unit-test mutation controls PASS (`339` cumulative negative controls across the workspace).
