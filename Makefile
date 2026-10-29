# AuraLite-Crypto — single entry point (landed in C0).
# Every command here is the same command CI runs. No hidden shards.
SHELL := /bin/bash
CARGO ?= cargo
PYTHON ?= python3

.PHONY: help deps-check fmt clippy test test-core test-sanitize \
        test-determinism fuzz-smoke nc-c0 evidence-c0 \
        gen-c1-vectors nc-c1 evidence-c1 \
        gen-c2-vectors nc-c2 evidence-c2 \
        check-deps record-c3-deps gen-c3-vectors nc-c3 evidence-c3 \
        nc-all gen-c4-vectors nc-c4 evidence-c4 \
        gen-c5-vectors nc-c5 evidence-c5 \
        gen-c6-vectors nc-c6 evidence-c6 \
        gen-c7-vectors nc-c7 evidence-c7 \
        record-c8-deps record-c9-deps record-deps gen-c8-vectors nc-c8 evidence-c8 \
        gen-c9-vectors nc-c9 evidence-c9 vector-inventory \
        audit-closeout nc-c10 evidence-c10 \
        gen-s0-vectors nc-s0 evidence-s0 \
        gen-s1-vectors nc-s1 evidence-s1 \
        record-s2-deps gen-s2-vectors nc-s2 evidence-s2 test-s2-native \
        gen-s3-vectors nc-s3 evidence-s3 \
        record-s4-deps gen-s4-vectors nc-s4 evidence-s4 \
        record-s5-deps gen-s5-vectors nc-s5 evidence-s5 \
        record-s6-deps gen-s6-vectors nc-s6 evidence-s6 \
        record-s7-deps gen-s7-vectors nc-s7 evidence-s7 \
        record-x0-deps gen-x0-vectors nc-x0 evidence-x0 \
        record-x1-deps gen-x1-vectors nc-x1 evidence-x1 \
        record-x2-deps gen-x2-vectors nc-x2 evidence-x2 \
        record-x3-deps gen-x3-vectors nc-x3 evidence-x3 \
        record-x4-deps build-contracts nc-x4 evidence-x4 \
        record-x5-deps build-standards check-standards nc-x5 evidence-x5 \
        record-x6-deps build-x6-contracts check-x6-contracts nc-x6 evidence-x6 \
        record-x7-deps gen-x7-vectors nc-x7 x7-matrix evidence-x7 test-x7 \
        gen-x8-mutants nc-x8 evidence-x8 test-x8 \
        gen-d0-vectors nc-d0 evidence-d0 test-d0 \
        gen-d1-vectors nc-d1 evidence-d1 test-d1 \
        devnet clean

help:
	@echo "AuraLite-Crypto (Stage 1, C0)"
	@echo "  make deps-check        verify toolchain"
	@echo "  make test              full gate: fmt + clippy + cargo test + docs guard"
	@echo "  make test-core         CORE shard only"
	@echo "  make nc-c0             negative controls for C0 -> docs/evidence/c0/nc.log"
	@echo "  make evidence-c0       C0 historical record (do NOT re-run after C1)"
	@echo "  make gen-c1-vectors    regenerate independent BIP-173 golden vectors"
	@echo "  make nc-c1             C1 negative controls -> docs/evidence/c1/nc.log"
	@echo "  make evidence-c1       regenerate docs/evidence/c1/gates.txt + SHA256SUMS"
	@echo "  make gen-c2-vectors    regenerate independent codec golden vectors"
	@echo "  make nc-c2             C2 negative controls -> docs/evidence/c2/nc.log"
	@echo "  make evidence-c2       regenerate docs/evidence/c2/gates.txt + SHA256SUMS"
	@echo "  make check-deps        exact-pin + checksum + allowlist gate"
	@echo "  make record-c3-deps    re-record external deps (reviewable action)"
	@echo "  make gen-c3-vectors    regenerate independent crypto golden vectors"
	@echo "  make nc-c3             C3 negative controls -> docs/evidence/c3/nc.log"
	@echo "  make evidence-c3       regenerate docs/evidence/c3/gates.txt + SHA256SUMS"
	@echo "  make nc-all            all C0-C4 NCs without rewriting historical logs"
	@echo "  make gen-c4-vectors    regenerate independent tx golden vectors"
	@echo "  make nc-c4             C4 negative controls -> docs/evidence/c4/nc.log"
	@echo "  make evidence-c4       regenerate docs/evidence/c4/gates.txt + SHA256SUMS"
	@echo "  make gen-c5-vectors    regenerate independent block golden vectors"
	@echo "  make nc-c5             C5 negative controls -> docs/evidence/c5/nc.log"
	@echo "  make evidence-c5       regenerate docs/evidence/c5/gates.txt + SHA256SUMS"
	@echo "  make gen-c6-vectors    regenerate independent Merkle-proof vectors"
	@echo "  make nc-c6             C6 negative controls -> docs/evidence/c6/nc.log"
	@echo "  make evidence-c6       regenerate docs/evidence/c6/gates.txt + SHA256SUMS"
	@echo "  make gen-c7-vectors    regenerate independent account/state-key vectors"
	@echo "  make nc-c7             C7 negative controls -> docs/evidence/c7/nc.log"
	@echo "  make evidence-c7       regenerate docs/evidence/c7/gates.txt + SHA256SUMS"
	@echo "  make record-c8-deps    re-record the C8 dependency addendum"
	@echo "  make gen-c8-vectors    regenerate independent genesis vectors"
	@echo "  make nc-c8             C8 negative controls -> docs/evidence/c8/nc.log"
	@echo "  make evidence-c8       regenerate docs/evidence/c8/gates.txt + SHA256SUMS"
	@echo "  make genesis-hash      print the shipped testnet genesis hash"
	@echo "  make vector-inventory  count golden vectors (>= 200 required)"
	@echo "  make audit-closeout    plan<->tree audit (no ✅ without a receipt)"
	@echo "  make nc-c10            C10 negative controls -> docs/evidence/c10/nc.log"
	@echo "  make evidence-c10      regenerate close-out receipts + NC inventory"
	@echo "  make gen-s0-vectors    regenerate independent state-baseline vectors"
	@echo "  make nc-s0             S0 negative controls -> docs/evidence/s0/nc.log"
	@echo "  make evidence-s0       regenerate S0 report + receipts"
	@echo "  make gen-s1-vectors    regenerate independent Merkle-tree vectors"
	@echo "  make nc-s1             S1 negative controls -> docs/evidence/s1/nc.log"
	@echo "  make evidence-s1       regenerate S1 report + receipts"
	@echo "  make record-s2-deps    re-record S2 dependency delta"
	@echo "  make gen-s2-vectors    regenerate independent WAL/RocksDB vectors"
	@echo "  make nc-s2             S2 negative controls -> docs/evidence/s2/nc.log"
	@echo "  make evidence-s2       regenerate S2 report + receipts"
	@echo "  make test-s2-native    optional real-RocksDB backend tests (needs librocksdb)"
	@echo "  make gen-s3-vectors    regenerate independent overlay/diff vectors"
	@echo "  make nc-s3             S3 negative controls -> docs/evidence/s3/nc.log"
	@echo "  make evidence-s3       regenerate S3 report + receipts"
	@echo "  make record-s4-deps    re-record S4 dependency decision"
	@echo "  make gen-s4-vectors    regenerate independent snapshot vectors"
	@echo "  make nc-s4             S4 negative controls -> docs/evidence/s4/nc.log"
	@echo "  make evidence-s4       regenerate S4 report + receipts"
	@echo "  make record-s5-deps    re-record S5 dependency decision"
	@echo "  make gen-s5-vectors    regenerate independent state-sync vectors"
	@echo "  make nc-s5             S5 negative controls -> docs/evidence/s5/nc.log"
	@echo "  make evidence-s5       regenerate S5 report + receipts"
	@echo "  make record-s6-deps    re-record S6 dependency decision"
	@echo "  make gen-s6-vectors    regenerate independent retention vectors"
	@echo "  make nc-s6             S6 negative controls -> docs/evidence/s6/nc.log"
	@echo "  make evidence-s6       regenerate S6 report + receipts"
	@echo "  make record-s7-deps    re-record S7 dependency decision"
	@echo "  make gen-s7-vectors    regenerate independent backup/restore vectors"
	@echo "  make nc-s7             S7 negative controls -> docs/evidence/s7/nc.log"
	@echo "  make evidence-s7       regenerate S7 report + receipts"
	@echo "  make record-x0-deps    re-record X0 dependency decision"
	@echo "  make gen-x0-vectors    regenerate independent determinism-census vectors"
	@echo "  make nc-x0             X0 negative controls -> docs/evidence/x0/nc.log"
	@echo "  make evidence-x0       regenerate X0 census report + receipts"
	@echo "  make record-x1-deps    re-record the X1 engine dependency decision"
	@echo "  make gen-x1-vectors    regenerate X1 vectors + measured fuel (needs cargo)"
	@echo "  make nc-x1             X1 negative controls -> docs/evidence/x1/nc.log"
	@echo "  make evidence-x1       regenerate X1 sandbox report + receipts"
	@echo "  make gen-x2-vectors    regenerate X2 ABI/gas vectors + measured gas (needs cargo)"
	@echo "  make nc-x2             X2 negative controls -> docs/evidence/x2/nc.log"
	@echo "  make evidence-x2       regenerate X2 ABI/gas report + receipts"
	@echo "  make gen-x3-vectors    regenerate X3 lifecycle vectors + measured gas (needs cargo)"
	@echo "  make nc-x3             X3 negative controls -> docs/evidence/x3/nc.log"
	@echo "  make evidence-x3       regenerate X3 lifecycle report + receipts"
	@echo "  make build-contracts   build examples/ to wasm32 and pin size+hash (needs the target)"
	@echo "  make nc-x4             X4 SDK negative controls -> docs/evidence/x4/nc.log"
	@echo "  make evidence-x4       regenerate X4 SDK/contracts report + receipts"
	@echo "  make build-standards   build ARC-20/ARC-721/multisig and pin their artifacts"
	@echo "  make check-standards   rebuild the standards and compare with the pins"
	@echo "  make nc-x5             X5 standards negative controls -> docs/evidence/x5/nc.log"
	@echo "  make evidence-x5       regenerate X5 standards/ABI v2 report + receipts"
	@echo "  make build-x6-contracts build the X6 call contracts + record their gas"
	@echo "  make check-x6-contracts verify the pinned X6 artifacts match the source"
	@echo "  make nc-x6             X6 call-isolation negative controls -> docs/evidence/x6/nc.log"
	@echo "  make evidence-x6       regenerate X6 call-isolation report + receipts"
	@echo "  make gen-x7-vectors    regenerate the pinned X7 determinsm scenario"
	@echo "  make nc-x7             X7 determinism negative controls -> docs/evidence/x7/nc.log"
	@echo "  make x7-matrix         run the X7 scenario under 3 build configurations"
	@echo "  make evidence-x7       regenerate X7 determinism report + receipts"
	@echo "  make gen-x8-mutants    regenerate the 100 golden X8 mutants + manifest"
	@echo "  make test-x8           X8 static-verifier gate (mutants + unit tests)"
	@echo "  make nc-x8             X8 verifier negative controls -> docs/evidence/x8/nc.log"
	@echo "  make evidence-x8       regenerate X8 verifier report + receipts"
	@echo "  make gen-d0-vectors    regenerate the pinned D0 node baseline fixture"
	@echo "  make test-d0           D0 baseline gate (stub refusal + measured survey)"
	@echo "  make nc-d0             D0 node-baseline negative controls -> docs/evidence/d0/nc.log"
	@echo "  make evidence-d0       regenerate D0 baseline report + receipts"
	@echo "  make gen-d1-vectors    regenerate the pinned D1 skeleton fixture"
	@echo "  make test-d1           D1 skeleton gate (config, tracing, PID, health)"
	@echo "  make nc-d1             D1 skeleton negative controls -> docs/evidence/d1/nc.log"
	@echo "  make evidence-d1       regenerate D1 skeleton report + receipts"
	@echo "  make gen-b0-vectors    regenerate the pinned B0 consensus baseline fixture"
	@echo "  make test-b0           B0 baseline gate (no consensus, measured)"
	@echo "  make nc-b0             B0 consensus-baseline negative controls -> docs/evidence/b0/nc.log"
	@echo "  make evidence-b0       regenerate B0 consensus baseline report + receipts"
	@echo "  make gen-b1-vectors    regenerate the pinned B1 PoA baseline fixture"
	@echo "  make test-b1           B1 PoA baseline gate (proposer + block signature)"
	@echo "  make nc-b1             B1 PoA baseline negative controls -> docs/evidence/b1/nc.log"
	@echo "  make evidence-b1       regenerate B1 PoA baseline report + receipts"
	@echo "  make gen-b2-vectors    regenerate the pinned B2 QC baseline fixture"
	@echo "  make test-b2           B2 QC baseline gate (BLS12-381 + quorum cert)"
	@echo "  make nc-b2             B2 QC baseline negative controls -> docs/evidence/b2/nc.log"
	@echo "  make evidence-b2       regenerate B2 QC baseline report + receipts"
	@echo "  make gen-b3-vectors    regenerate the pinned B3 finality fixture"
	@echo "  make test-b3           B3 finality baseline gate (3-chain commit + view change)"
	@echo "  make nc-b3             B3 finality baseline negative controls -> docs/evidence/b3/nc.log"
	@echo "  make evidence-b3       regenerate B3 finality baseline report + receipts"
	@echo "  make record-c9-deps    re-record the C9 dependency addendum"
	@echo "  make gen-c9-vectors    regenerate 512 independent test-rig vectors"
	@echo "  make nc-c9             C9 negative controls -> docs/evidence/c9/nc.log"
	@echo "  make evidence-c9       regenerate docs/evidence/c9/gates.txt + SHA256SUMS"
	@echo "  make test-sanitize     SKIP until H3"
	@echo "  make test-determinism  SKIP until X7"
	@echo "  make fuzz-smoke        real 60 s portable fuzz across 6 decoders"
	@echo "  make devnet            NOT IMPLEMENTED (D7)"

deps-check:
	@command -v $(CARGO) >/dev/null || { echo "ERROR: cargo not found (see rust-toolchain.toml)"; exit 1; }
	@command -v rustc >/dev/null || { echo "ERROR: rustc not found"; exit 1; }
	@$(CARGO) fmt --version >/dev/null || { echo "ERROR: rustfmt component missing"; exit 1; }
	@$(CARGO) clippy --version >/dev/null || { echo "ERROR: clippy component missing"; exit 1; }
	@echo "toolchain: $$(rustc --version), $$($(CARGO) --version)"

fmt:
	@$(CARGO) fmt --all -- --check

clippy:
	@$(CARGO) clippy --workspace --all-targets -- -D warnings

# The C0 gate. A gate that cannot go red is not a gate: `make nc-c0` proves
# both the Rust and the docs halves of this target fail when tampered with.
test: deps-check
	@echo "== fmt =="
	@$(CARGO) fmt --all -- --check
	@echo "== clippy =="
	@$(CARGO) clippy --workspace --all-targets -- -D warnings
	@echo "== cargo test =="
	@$(CARGO) test --workspace
	@echo "== docs drift guard =="
	@$(PYTHON) tools/check_claims.py
	@echo "== dependency gate =="
	@$(PYTHON) tools/check_deps.py
	@echo "== vector inventory =="
	@$(PYTHON) tools/count_vectors.py
	@echo "== test rig wiring =="
	@$(PYTHON) tools/check_rig.py
	@echo "== close-out audit =="
	@$(PYTHON) tools/audit_closeout.py
	@echo "ALL GATES GREEN"

# C9: the CORE shard. Golden vectors + property tests + a short deterministic
# fuzz pass; `make fuzz-smoke` is the full 60-second campaign.
test-core:
	@$(PYTHON) tools/count_vectors.py
	@$(PYTHON) tools/check_rig.py
	@$(CARGO) test -p aura-types -p aura-codec -p aura-crypto -p aura-tx -p aura-block -p aura-state -p aura-storage -p aura-genesis -p aura-testkit
	@$(CARGO) run --quiet -p aura-testkit --bin aura-fuzz -- --seconds 0 --min-iterations 5000 --target all
	@$(PYTHON) tools/check_claims.py
	@$(PYTHON) tools/check_deps.py

vector-inventory:
	@$(PYTHON) tools/count_vectors.py

# Honest skips, not silent successes. These become real gates in their phases.
test-sanitize:
	@echo "SKIP: Miri/ASan/UBSan gates land with H3. Not claimed at C9."

test-determinism:
	@echo "SKIP: byte-identical state-root gate lands with X7. Not claimed at C9."

# C9: stable-Rust portable fuzz gate. H3 adds the 24 h libFuzzer campaign.
fuzz-smoke:
	@$(CARGO) run --quiet -p aura-testkit --bin aura-fuzz -- --seconds 60 --min-iterations 1000 --target all

nc-c0:
	@mkdir -p docs/evidence/c0
	@$(PYTHON) tools/nc_c0.py > docs/evidence/c0/nc.log 2>&1 || { cat docs/evidence/c0/nc.log; exit 1; }
	@cat docs/evidence/c0/nc.log

evidence-c0:
	@$(PYTHON) tools/write_c0_evidence.py

gen-c1-vectors:
	@$(PYTHON) tools/gen_c1_vectors.py

nc-c1:
	@mkdir -p docs/evidence/c1
	@$(PYTHON) tools/nc_c1.py > docs/evidence/c1/nc.log 2>&1 || { cat docs/evidence/c1/nc.log; exit 1; }
	@cat docs/evidence/c1/nc.log

evidence-c1:
	@$(PYTHON) tools/write_c1_evidence.py

gen-c2-vectors:
	@$(PYTHON) tools/gen_c2_vectors.py

nc-c2:
	@mkdir -p docs/evidence/c2
	@$(PYTHON) tools/nc_c2.py > docs/evidence/c2/nc.log 2>&1 || { cat docs/evidence/c2/nc.log; exit 1; }
	@cat docs/evidence/c2/nc.log

evidence-c2:
	@$(PYTHON) tools/write_c2_evidence.py

check-deps:
	@$(PYTHON) tools/check_deps.py

record-c3-deps:
	@$(PYTHON) tools/record_c3_deps.py

gen-c3-vectors:
	@$(PYTHON) tools/gen_c3_vectors.py

nc-c3:
	@mkdir -p docs/evidence/c3
	@$(PYTHON) tools/nc_c3.py > docs/evidence/c3/nc.log 2>&1 || { cat docs/evidence/c3/nc.log; exit 1; }
	@cat docs/evidence/c3/nc.log

evidence-c3:
	@$(PYTHON) tools/write_c3_evidence.py

# Runs every NC script WITHOUT rewriting the historical C0-C3 evidence logs,
# so their committed SHA256SUMS stay valid. CI uses this instead of nc-cX.
nc-all:
	@for phase in c0 c1 c2 c3 c4 c5 c6 c7 c8 c9 c10 s0 s1 s2 s3 s4 s5 s6 s7 x0 x1 x2 x3 x4 x5 x6 x7 x8 d0 d1 b0 b1 b2 b3; do \
	    $(PYTHON) tools/nc_$$phase.py > /tmp/aura-nc-$$phase.log 2>&1 || \
	      { echo "$$phase NEGATIVE CONTROLS FAILED"; tail -30 /tmp/aura-nc-$$phase.log; exit 1; }; \
	    grep '^RESULT:' /tmp/aura-nc-$$phase.log; \
	done

gen-c4-vectors:
	@$(PYTHON) tools/gen_c4_vectors.py

nc-c4:
	@mkdir -p docs/evidence/c4
	@$(PYTHON) tools/nc_c4.py > docs/evidence/c4/nc.log 2>&1 || { cat docs/evidence/c4/nc.log; exit 1; }
	@cat docs/evidence/c4/nc.log

evidence-c4:
	@$(PYTHON) tools/write_c4_evidence.py

gen-c5-vectors:
	@$(PYTHON) tools/gen_c5_vectors.py

nc-c5:
	@mkdir -p docs/evidence/c5
	@$(PYTHON) tools/nc_c5.py > docs/evidence/c5/nc.log 2>&1 || { cat docs/evidence/c5/nc.log; exit 1; }
	@cat docs/evidence/c5/nc.log

evidence-c5:
	@$(PYTHON) tools/write_c5_evidence.py

gen-c6-vectors:
	@$(PYTHON) tools/gen_c6_vectors.py

nc-c6:
	@mkdir -p docs/evidence/c6
	@$(PYTHON) tools/nc_c6.py > docs/evidence/c6/nc.log 2>&1 || { cat docs/evidence/c6/nc.log; exit 1; }
	@cat docs/evidence/c6/nc.log

evidence-c6:
	@$(PYTHON) tools/write_c6_evidence.py

gen-c7-vectors:
	@$(PYTHON) tools/gen_c7_vectors.py

nc-c7:
	@mkdir -p docs/evidence/c7
	@$(PYTHON) tools/nc_c7.py > docs/evidence/c7/nc.log 2>&1 || { cat docs/evidence/c7/nc.log; exit 1; }
	@cat docs/evidence/c7/nc.log

evidence-c7:
	@$(PYTHON) tools/write_c7_evidence.py

record-c8-deps:
	@$(PYTHON) tools/record_c8_deps.py

record-c9-deps:
	@$(PYTHON) tools/record_c9_deps.py

# Generic alias for the current phase's dependency addendum.
record-deps: record-c9-deps

gen-c8-vectors:
	@$(PYTHON) tools/gen_c8_vectors.py

nc-c8:
	@mkdir -p docs/evidence/c8
	@$(PYTHON) tools/nc_c8.py > docs/evidence/c8/nc.log 2>&1 || { cat docs/evidence/c8/nc.log; exit 1; }
	@cat docs/evidence/c8/nc.log

evidence-c8:
	@$(PYTHON) tools/write_c8_evidence.py

genesis-hash:
	@$(CARGO) run --quiet -p aura-genesis --bin aura -- genesis hash --file docs/spec/genesis-testnet.json

gen-c9-vectors:
	@$(PYTHON) tools/gen_c9_vectors.py

nc-c9:
	@mkdir -p docs/evidence/c9
	@$(PYTHON) tools/nc_c9.py > docs/evidence/c9/nc.log 2>&1 || { cat docs/evidence/c9/nc.log; exit 1; }
	@cat docs/evidence/c9/nc.log

evidence-c9:
	@$(PYTHON) tools/write_c9_evidence.py

audit-closeout:
	@$(PYTHON) tools/audit_closeout.py

nc-c10:
	@mkdir -p docs/evidence/c10
	@$(PYTHON) tools/nc_c10.py > docs/evidence/c10/nc.log 2>&1 || { cat docs/evidence/c10/nc.log; exit 1; }
	@cat docs/evidence/c10/nc.log

evidence-c10:
	@$(PYTHON) tools/write_c10_evidence.py

gen-s0-vectors:
	@$(PYTHON) tools/gen_s0_vectors.py

nc-s0:
	@mkdir -p docs/evidence/s0
	@$(PYTHON) tools/nc_s0.py > docs/evidence/s0/nc.log 2>&1 || { cat docs/evidence/s0/nc.log; exit 1; }
	@cat docs/evidence/s0/nc.log

evidence-s0:
	@$(PYTHON) tools/write_s0_evidence.py

gen-s1-vectors:
	@$(PYTHON) tools/gen_s1_vectors.py

nc-s1:
	@mkdir -p docs/evidence/s1
	@$(PYTHON) tools/nc_s1.py > docs/evidence/s1/nc.log 2>&1 || { cat docs/evidence/s1/nc.log; exit 1; }
	@cat docs/evidence/s1/nc.log

evidence-s1:
	@$(PYTHON) tools/write_s1_evidence.py

record-s2-deps:
	@$(PYTHON) tools/record_s2_deps.py

gen-s2-vectors:
	@$(PYTHON) tools/gen_s2_vectors.py

nc-s2:
	@mkdir -p docs/evidence/s2
	@$(PYTHON) tools/nc_s2.py > docs/evidence/s2/nc.log 2>&1 || { cat docs/evidence/s2/nc.log; exit 1; }
	@cat docs/evidence/s2/nc.log

evidence-s2: record-s2-deps
	@$(PYTHON) tools/write_s2_evidence.py

test-s2-native:
	@eval "$$(sh tools/s2_native_env.sh)" && $(CARGO) test -p aura-storage --test s2_storage --features rocksdb
	@eval "$$(sh tools/s2_native_env.sh)" && $(CARGO) clippy -p aura-storage --all-targets --features rocksdb -- -D warnings

gen-s3-vectors:
	@$(PYTHON) tools/gen_s3_vectors.py

nc-s3:
	@mkdir -p docs/evidence/s3
	@$(PYTHON) tools/nc_s3.py > docs/evidence/s3/nc.log 2>&1 || { cat docs/evidence/s3/nc.log; exit 1; }
	@cat docs/evidence/s3/nc.log

evidence-s3:
	@$(PYTHON) tools/write_s3_evidence.py

record-s4-deps:
	@$(PYTHON) tools/record_s4_deps.py

gen-s4-vectors:
	@$(PYTHON) tools/gen_s4_vectors.py

nc-s4:
	@mkdir -p docs/evidence/s4
	@$(PYTHON) tools/nc_s4.py > docs/evidence/s4/nc.log 2>&1 || { cat docs/evidence/s4/nc.log; exit 1; }
	@cat docs/evidence/s4/nc.log

evidence-s4: record-s4-deps
	@$(PYTHON) tools/write_s4_evidence.py

record-s5-deps:
	@$(PYTHON) tools/record_s5_deps.py

gen-s5-vectors:
	@$(PYTHON) tools/gen_s5_vectors.py

nc-s5:
	@mkdir -p docs/evidence/s5
	@$(PYTHON) tools/nc_s5.py > docs/evidence/s5/nc.log 2>&1 || { cat docs/evidence/s5/nc.log; exit 1; }
	@cat docs/evidence/s5/nc.log

evidence-s5: record-s5-deps
	@$(PYTHON) tools/write_s5_evidence.py

record-s6-deps:
	@$(PYTHON) tools/record_s6_deps.py

gen-s6-vectors:
	@$(PYTHON) tools/gen_s6_vectors.py

nc-s6:
	@mkdir -p docs/evidence/s6
	@$(PYTHON) tools/nc_s6.py > docs/evidence/s6/nc.log 2>&1 || { cat docs/evidence/s6/nc.log; exit 1; }
	@cat docs/evidence/s6/nc.log

evidence-s6: record-s6-deps
	@$(PYTHON) tools/write_s6_evidence.py

record-s7-deps:
	@$(PYTHON) tools/record_s7_deps.py

gen-s7-vectors:
	@$(PYTHON) tools/gen_s7_vectors.py

nc-s7:
	@mkdir -p docs/evidence/s7
	@$(PYTHON) tools/nc_s7.py > docs/evidence/s7/nc.log 2>&1 || { cat docs/evidence/s7/nc.log; exit 1; }
	@cat docs/evidence/s7/nc.log

evidence-s7: record-s7-deps
	@$(PYTHON) tools/write_s7_evidence.py

record-x0-deps:
	@$(PYTHON) tools/record_x0_deps.py

gen-x0-vectors:
	@$(PYTHON) tools/gen_x0_vectors.py

nc-x0:
	@mkdir -p docs/evidence/x0
	@$(PYTHON) tools/nc_x0.py > docs/evidence/x0/nc.log 2>&1 || { cat docs/evidence/x0/nc.log; exit 1; }
	@cat docs/evidence/x0/nc.log

evidence-x0: record-x0-deps
	@$(PYTHON) tools/write_x0_evidence.py

record-x1-deps:
	@$(PYTHON) tools/record_x1_deps.py

gen-x1-vectors:
	@$(PYTHON) tools/gen_x1_vectors.py
	@$(CARGO) run -q -p aura-exec --bin x1-record

nc-x1:
	@mkdir -p docs/evidence/x1
	@$(PYTHON) tools/nc_x1.py > docs/evidence/x1/nc.log 2>&1 || { cat docs/evidence/x1/nc.log; exit 1; }
	@cat docs/evidence/x1/nc.log

evidence-x1: record-x1-deps
	@$(PYTHON) tools/write_x1_evidence.py

record-x2-deps:
	@$(PYTHON) tools/record_x2_deps.py

gen-x2-vectors:
	@$(PYTHON) tools/gen_x2_vectors.py
	@$(CARGO) run -q -p aura-exec --bin x2-record

nc-x2:
	@mkdir -p docs/evidence/x2
	@$(PYTHON) tools/nc_x2.py > docs/evidence/x2/nc.log 2>&1 || { cat docs/evidence/x2/nc.log; exit 1; }
	@cat docs/evidence/x2/nc.log

evidence-x2: record-x2-deps
	@$(PYTHON) tools/write_x2_evidence.py

record-x3-deps:
	@$(PYTHON) tools/record_x3_deps.py

gen-x3-vectors:
	@$(PYTHON) tools/gen_x3_vectors.py
	@$(CARGO) run -q -p aura-exec --bin x3-record

nc-x3:
	@mkdir -p docs/evidence/x3
	@$(PYTHON) tools/nc_x3.py > docs/evidence/x3/nc.log 2>&1 || { cat docs/evidence/x3/nc.log; exit 1; }
	@cat docs/evidence/x3/nc.log

evidence-x3: record-x3-deps
	@$(PYTHON) tools/write_x3_evidence.py

record-x4-deps:
	@$(PYTHON) tools/record_x4_deps.py

# X4: build the reference contracts, pin their size+hash, and measure their gas.
# `examples/` is a separate workspace, so this is a real cross-build, not a
# cargo feature of the node.
build-contracts:
	@$(PYTHON) tools/build_contracts.py
	@$(CARGO) run -q -p aura-exec --bin x4-record

check-contracts:
	@$(PYTHON) tools/build_contracts.py --check

nc-x4:
	@mkdir -p docs/evidence/x4
	@$(PYTHON) tools/nc_x4.py > docs/evidence/x4/nc.log 2>&1 || { cat docs/evidence/x4/nc.log; exit 1; }
	@cat docs/evidence/x4/nc.log

evidence-x4: record-x4-deps
	@$(PYTHON) tools/write_x4_evidence.py

record-x5-deps:
	@$(PYTHON) tools/record_x5_deps.py

# X5: build the reference *standards*, pin their size+hash, measure their gas.
# `standards/` is a separate workspace per contract, and the pinned fixture also
# carries the ABI v2 surface the census enforces.
build-standards:
	@$(PYTHON) tools/build_standards.py

check-standards:
	@$(PYTHON) tools/build_standards.py --check

nc-x5:
	@mkdir -p docs/evidence/x5
	@$(PYTHON) tools/nc_x5.py > docs/evidence/x5/nc.log 2>&1 || { cat docs/evidence/x5/nc.log; exit 1; }
	@cat docs/evidence/x5/nc.log

evidence-x5: record-x5-deps
	@$(PYTHON) tools/write_x5_evidence.py

record-x6-deps:
	@$(PYTHON) tools/record_x6_deps.py

# X6: build the *call* contracts (a vault, an attacker, a chain), pin their
# size+sha256, then let them run against each other through the real Lifecycle
# so the gas block is measured rather than assumed.
build-x6-contracts:
	@$(PYTHON) tools/build_x6_contracts.py

check-x6-contracts:
	@$(PYTHON) tools/build_x6_contracts.py --check

nc-x6:
	@mkdir -p docs/evidence/x6
	@$(PYTHON) tools/nc_x6.py > docs/evidence/x6/nc.log 2>&1 || { cat docs/evidence/x6/nc.log; exit 1; }
	@cat docs/evidence/x6/nc.log

evidence-x6: record-x6-deps
	@$(PYTHON) tools/write_x6_evidence.py

record-x7-deps:
	@$(PYTHON) tools/record_x7_deps.py

# X7: the determinism gate. `gen-x7-vectors` re-runs the scenario and rewrites the
# pinned fixture; `x7-matrix` replays it under three build configurations.
gen-x7-vectors:
	@$(PYTHON) tools/gen_x7_vectors.py

test-x7:
	@$(CARGO) test -p aura-exec --test x7_determinism

nc-x7:
	@mkdir -p docs/evidence/x7
	@$(PYTHON) tools/nc_x7.py > docs/evidence/x7/nc.log 2>&1 || { cat docs/evidence/x7/nc.log; exit 1; }
	@cat docs/evidence/x7/nc.log

x7-matrix:
	@$(PYTHON) tools/x7_matrix.py

evidence-x7: record-x7-deps
	@$(PYTHON) tools/write_x7_evidence.py

# X8: the static verifier. `gen-x8-mutants` rewrites the golden mutants from the
# shipped artifacts; the gate replays them and the 23 unit tests that witness every
# refusal code.
gen-x8-mutants:
	@$(PYTHON) tools/gen_x8_mutants.py

test-x8:
	@$(CARGO) test -p aura-exec --test x8_verify

nc-x8:
	@mkdir -p docs/evidence/x8
	@$(PYTHON) tools/nc_x8.py > docs/evidence/x8/nc.log 2>&1 || { cat docs/evidence/x8/nc.log; exit 1; }
	@cat docs/evidence/x8/nc.log

evidence-x8:
	@$(PYTHON) tools/write_x8_evidence.py

# D0: the node baseline. `gen-d0-vectors` rewrites the pinned survey from the
# binary's own measurement; the gate proves the binary refuses and the survey
# matches the tree.
gen-d0-vectors:
	@$(PYTHON) tools/gen_d0_vectors.py

test-d0:
	@$(CARGO) test -p aura-node

nc-d0:
	@mkdir -p docs/evidence/d0
	@$(PYTHON) tools/nc_d0.py > docs/evidence/d0/nc.log 2>&1 || { cat docs/evidence/d0/nc.log; exit 1; }
	@cat docs/evidence/d0/nc.log

evidence-d0:
	@$(PYTHON) tools/write_d0_evidence.py

gen-d1-vectors:
	@$(PYTHON) tools/gen_d1_vectors.py

test-d1:
	@$(CARGO) test -p aura-node --test d1_skeleton

nc-d1:
	@mkdir -p docs/evidence/d1
	@$(PYTHON) tools/nc_d1.py > docs/evidence/d1/nc.log 2>&1 || { cat docs/evidence/d1/nc.log; exit 1; }
	@cat docs/evidence/d1/nc.log

evidence-d1:
	@$(PYTHON) tools/write_d1_evidence.py

gen-b0-vectors:
	@$(PYTHON) tools/gen_b0_vectors.py

test-b0:
	@$(CARGO) test -p aura-consensus

nc-b0:
	@mkdir -p docs/evidence/b0
	@$(PYTHON) tools/nc_b0.py > docs/evidence/b0/nc.log 2>&1 || { cat docs/evidence/b0/nc.log; exit 1; }
	@cat docs/evidence/b0/nc.log

evidence-b0:
	@$(PYTHON) tools/write_b0_evidence.py

gen-b1-vectors:
	@$(PYTHON) tools/gen_b1_vectors.py

test-b1:
	cargo test -p aura-consensus

nc-b1:
	@mkdir -p docs/evidence/b1
	@$(PYTHON) tools/nc_b1.py > docs/evidence/b1/nc.log 2>&1 || { cat docs/evidence/b1/nc.log; exit 1; }
	@cat docs/evidence/b1/nc.log

evidence-b1:
	@$(PYTHON) tools/write_b1_evidence.py

gen-b2-vectors:
	@$(PYTHON) tools/gen_b2_vectors.py

test-b2:
	@$(CARGO) test -p aura-consensus

nc-b2:
	@mkdir -p docs/evidence/b2
	@$(PYTHON) tools/nc_b2.py > docs/evidence/b2/nc.log 2>&1 || { cat docs/evidence/b2/nc.log; exit 1; }
	@cat docs/evidence/b2/nc.log

evidence-b2:
	@$(PYTHON) tools/write_b2_evidence.py

gen-b3-vectors:
	@$(PYTHON) tools/gen_b3_vectors.py

test-b3:
	@$(CARGO) test -p aura-consensus

nc-b3:
	@mkdir -p docs/evidence/b3
	@$(PYTHON) tools/nc_b3.py > docs/evidence/b3/nc.log 2>&1 || { cat docs/evidence/b3/nc.log; exit 1; }
	@cat docs/evidence/b3/nc.log

evidence-b3:
	@$(PYTHON) tools/write_b3_evidence.py

devnet:
	@echo "ERROR: devnet is D7 (NODE_PLAN). It is not implemented at C0." >&2
	@exit 1

clean:
	@$(CARGO) clean
