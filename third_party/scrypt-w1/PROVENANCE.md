# Experimental local scrypt fork

This is a maintenance fork of the **published RustCrypto `scrypt` 0.11.0**
crate, crates.io SHA-256
`0516a385866c09368f0b5bcd1caff3366aace790fcd46e2bb032697bb172fd1f`,
source commit `7e84684d711846230b265c45da0da0bb40513fea`. The original MIT and
Apache 2.0 license texts are included. This is not an upstream release.

The source is unchanged except for a `zeroize::Zeroizing` import and wrappers
around the three password-derived B, V, and T heap allocations in `src/lib.rs`.
The manifest is locally adapted for exact pinned dependencies. A guard verifies
that reversing exactly this small source patch reproduces the upstream file
hash and that the other source files, licenses and manifest match pinned hashes.
The wrapper wipes the buffers on normal drop and unwinding, but this is
best-effort: stack/register copies, allocator and library internals, swap,
core dumps, process compromise, abort, or platform effects remain outside that
guarantee. The fork increases maintenance and security-review burden; remove
it only after a compatible upstream fix has been reviewed and verified.

The original crates.io `scrypt` is retained only as a Rust test dependency
for differential tests. See RustCrypto issue
<https://github.com/RustCrypto/password-hashes/issues/938> for upstream context.
