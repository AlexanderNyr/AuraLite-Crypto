#!/bin/sh
# Print shell `export` lines for the OPTIONAL native RocksDB backend.
# Sourced with eval by `make test-s2-native`. Fails loudly when the system
# librocksdb-dev / libclang are absent instead of silently building the C++
# library (which needs GB of RAM and is not reproducible in CI).
set -e

find_first() {
    find /usr/lib /usr/local/lib /usr/lib/x86_64-linux-gnu -name "$1" -print -quit 2>/dev/null || true
}

rocksdb_so=$(find_first 'librocksdb.so*')
if [ -z "$rocksdb_so" ]; then
    echo "ERROR: librocksdb.so not found. Install librocksdb-dev (>= 9.x) and clang/libclang-dev." >&2
    exit 1
fi

clang_so=$(find_first 'libclang.so*')
if [ -z "$clang_so" ]; then
    echo "ERROR: libclang.so not found. Install clang/libclang-dev." >&2
    exit 1
fi

rocksdb_dir=$(dirname "$rocksdb_so")
clang_dir=$(dirname "$clang_so")
tmpdir="${TMPDIR:-/tmp}"

printf 'export ROCKSDB_LIB_DIR=%s\n' "$rocksdb_dir"
printf 'export LIBCLANG_PATH=%s\n' "$clang_dir"
printf 'export ROCKSDB_INCLUDE_DIR=/usr/include\n'
printf 'export LD_LIBRARY_PATH=%s${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}\n' "$rocksdb_dir"
printf 'export TMPDIR=%s\n' "$tmpdir"
printf 'export CARGO_BUILD_JOBS=1\n'
