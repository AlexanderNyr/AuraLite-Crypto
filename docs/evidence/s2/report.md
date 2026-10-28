# S2 — RocksDB + WAL durable state (measured)

Generated (UTC): 2026-10-26T22:02:34+00:00. Numbers come from the independent Python
reference `tools/gen_s2_vectors.py` and are re-verified by
`crates/aura-storage/tests/s2_storage.rs`.

## Architecture

- **Two backends behind one `StateBackend` trait.** The default is a
  pure-Rust WAL + `BTreeMap` backend so `make test` is reproducible
  anywhere. The `rocksdb` Cargo feature enables the real RocksDB
  backend with four column families (`state`, `nonce`, `meta`,
  `wal_index`) and a single atomic, `sync = true` `WriteBatch`.
  Column families are an explicit part of the backend API for both.

- **WAL is the durability authority.** A framed record
  `varint(len) || payload || BLAKE3(S2_WAL_CONTEXT || payload)` is
  appended and `fsync`ed BEFORE head publication. `payload` carries
  version, parent root, state root, and the ordered changes with
  per-key nonces. `head.bin` is written atomically (tmp + rename +
  directory fsync) with a checksummed version/root.

- **State root** = `BLAKE3(S2_ROOT_CONTEXT || version || tree_root)`
  where `tree_root` is the S1 sparse Merkle root over
  `BLAKE3(S2_KEY_CONTEXT || len || key)` paths.

## Measured gates

- golden commits: **11**
- `kill -9` commit points: **10** (plus 10 more in
  the WAL/head crash window)
- WAL bytes (all commits): **4065**
- final leaves: **44**
- 8 independent rejection vectors: bad checksum, bad magic,
  truncated frame, trailing bytes, non-sequential version, wrong
  parent root, zeroed root, mutated value.
- `missing_wal_record_is_rejected`: publishing a head without a
  backing WAL record is refused.

Two `Storage::open` paths are verified: a clean reopen after all
commits, and recovery that replays a WAL record fsynced but never
published. Both must reproduce the same head and root.

## Native RocksDB backend

- feature `rocksdb`: VERIFIED in this run.
- `rocksdb = 0.23.0`, `librocksdb-sys = 0.17.1+9.9.3` pinned;
  linked dynamically to the system `librocksdb-dev` (the bundled C++
  build is disabled: it needs GB of RAM and is not reproducible in CI).
  `bindgen-runtime` generates C-API bindings from the installed headers.
- Ubuntu 24.04 ships librocksdb 8.9.1, whose C API is incompatible
  with `rocksdb 0.23`. Use Debian trixie (9.10) or a container with
  librocksdb >= 9.9; see `tools/s2_native_env.sh`.

## Honest scope

- No execution overlay / rollback (S3), no chunked snapshots (S4),
  no state sync (S5), no pruning (S6), no backup (S7).
- No network, no VM, no consensus.

Validators / blocks / transactions remain **0 / 0 / 0**.
