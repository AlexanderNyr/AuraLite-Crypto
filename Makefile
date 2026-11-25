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
        test-b0 test-b1 test-b2 test-b3 test-b4 test-b5 test-b6 test-b7 test-b8 test-b9 test-b10 nc-b0 nc-b1 nc-b2 nc-b3 nc-b4 nc-b5 nc-b6 nc-b7 nc-b8 nc-b9 nc-b10 evidence-b0 evidence-b1 evidence-b2 evidence-b3 evidence-b4 evidence-b5 evidence-b6 evidence-b7 evidence-b8 evidence-b9 evidence-b10 \
        gen-b0-vectors gen-b1-vectors gen-b2-vectors gen-b3-vectors gen-b4-vectors gen-b5-vectors gen-b6-vectors gen-b7-vectors gen-b8-vectors gen-b9-vectors gen-b10-vectors \
        test-d2 nc-d2 evidence-d2 \
        test-d3 nc-d3 evidence-d3 \
        test-d4 nc-d4 evidence-d4 \
        test-d5 nc-d5 evidence-d5 \
        test-d6 nc-d6 evidence-d6 \
        test-d7 nc-d7 evidence-d7 \
        gen-d8-vectors test-d8 nc-d8 evidence-d8 \
        gen-n0-vectors test-n0 nc-n0 evidence-n0 \
        gen-n1-vectors test-n1 nc-n1 nc-p2p-review evidence-n1 \
        gen-n2-vectors test-n2 nc-n2 evidence-n2 \
        gen-n3-vectors test-n3 nc-n3 evidence-n3 \
        gen-n4-vectors test-n4 nc-n4 evidence-n4 \
        gen-n5-vectors test-n5 nc-n5 evidence-n5 \
        gen-n6-vectors test-n6 nc-n6 evidence-n6 \
        gen-n7-vectors test-n7 nc-n7 evidence-n7 \
        gen-n8-vectors test-n8 nc-n8 evidence-n8 \
        gen-w0-vectors test-w0 nc-w0 evidence-w0 \
        test-w1 test-w1-interop nc-w1 evidence-w1 \
        gen-w2-vectors test-w2 nc-w2 evidence-w2 \
        gen-w3-vectors test-w3 nc-w3 evidence-w3 \
        gen-w4-vectors test-w4 nc-w4 evidence-w4 \
        gen-w5-vectors test-w5 nc-w5 evidence-w5 \
        gen-h0-vectors check-h0 nc-h0 evidence-h0 \
        gen-h1-vectors gen-h1-sbom check-h1 nc-h1 evidence-h1 \
        gen-h2-vectors check-h2 nc-h2 evidence-h2 \
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
	@echo "  make nc-all            all phase NCs without rewriting historical logs"
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
	@echo "  make test-d2           D2 transfer pipeline + 1,000-transaction acceptance"
	@echo "  make nc-d2             D2 mutation controls -> docs/evidence/d2/nc.log"
	@echo "  make evidence-d2       run D2 and full workspace gates; write evidence receipts"
	@echo "  make test-d3           D3 mempool plus 10,000-transaction acceptance"
	@echo "  make nc-d3             D3 mutation controls -> docs/evidence/d3/nc.log"
	@echo "  make evidence-d3       run D3/full gates and write evidence receipts"
	@echo "  make test-d4           D4 JSON-RPC contract vectors + node-backed API tests"
	@echo "  make nc-d4             D4 mutation controls -> docs/evidence/d4/nc.log"
	@echo "  make evidence-d4       aggregate completed D4 gates into evidence receipts"
	@echo "  make test-d5           D5 REST/gRPC versioning and live transport tests"
	@echo "  make nc-d5             D5 unit-test mutation controls -> docs/evidence/d5/nc.log"
	@echo "  make evidence-d5       aggregate completed D5 gates into evidence receipts"
	@echo "  make test-d6           D6 health, Prometheus and tracing contract tests"
	@echo "  make nc-d6             D6 unit-test mutation controls -> docs/evidence/d6/nc.log"
	@echo "  make evidence-d6       aggregate completed D6 gates into evidence receipts"
	@echo "  make gen-n0-vectors    regenerate the pinned N0 P2P baseline fixture"
	@echo "  make test-n0           N0 baseline gate (no P2P survey + 2-node RPC isolation)"
	@echo "  make nc-n0             N0 unit-test mutation controls -> docs/evidence/n0/nc.log"
	@echo "  make evidence-n0       aggregate completed N0 gates into evidence receipts"
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
	@echo "== P2P pre-auth ban review mutation controls =="
	@$(PYTHON) tools/nc_p2p_review.py
	@echo "== docs drift guard =="
	@$(PYTHON) tools/check_claims.py
	@echo "== threat model review gate =="
	@$(PYTHON) tools/check_threat_model.py
	@echo "== crypto review checklist gate =="
	@$(PYTHON) tools/check_h2_review.py
	@echo "== supply chain gate =="
	@$(PYTHON) tools/check_supply_chain.py --selftest
	@$(PYTHON) tools/check_supply_chain.py
	@echo "== patch archive scope guard =="
	@$(PYTHON) tools/test_patch_scope_guard.py
	@echo "== dependency gate =="
	@$(PYTHON) tools/check_deps.py
	@echo "== local scrypt fork provenance =="
	@$(PYTHON) tools/check_scrypt_fork.py
	@$(PYTHON) tools/test_scrypt_fork_guard.py
	@echo "== vector inventory =="
	@$(PYTHON) tools/count_vectors.py
	@echo "== test rig wiring =="
	@$(PYTHON) tools/check_rig.py
	@echo "== historical checksum policy tests =="
	@$(PYTHON) tools/test_historical_drift.py
	@echo "== opsec checks (H7) =="
	@$(PYTHON) tools/check_opsec.py
	@echo "== re-audit (H8) =="
	@$(PYTHON) tools/h8_reaudit.py
	@echo "== incident drills (H9) =="
	@$(PYTHON) tools/h9_incident_drill.py
	@echo "== ecosystem baseline (E0) =="
	@$(PYTHON) tools/e0_baseline_check.py
	@echo "== indexer gate (E1) =="
	@$(CARGO) test --locked -p aura-storage --test e1_indexer
	@echo "== explorer gate (E2) =="
	@$(PYTHON) tools/e2_explorer_check.py
	@$(CARGO) test --locked -p aura-storage --test e2_explorer
	@echo "== faucet + sdk gate (E3) =="
	@$(PYTHON) tools/e3_faucet_check.py
	@$(CARGO) test --locked -p aura-storage --test e3_faucet
	@$(PYTHON) tools/e3_examples_check.py
	@echo "== economics baseline gate (T0) =="
	@$(PYTHON) tools/t0_economics_check.py
	@$(CARGO) test --locked -p aura-storage --test t0_economics
	@echo "== issuance model gate (T1) =="
	@$(PYTHON) tools/t1_issuance_check.py
	@$(CARGO) test --locked -p aura-storage --test t1_issuance
	@echo "== allocation gate (T2) =="
	@$(PYTHON) tools/t2_allocation_check.py
	@$(CARGO) test --locked -p aura-storage --test t2_allocation
	@echo "== fees gate (T3) =="
	@$(PYTHON) tools/t3_fees_check.py
	@$(CARGO) test --locked -p aura-storage --test t3_fees
	@echo "== staking gate (T4) =="
	@$(PYTHON) tools/t4_staking_check.py
	@$(CARGO) test --locked -p aura-storage --test t4_staking
	@echo "== slashing gate (T5) =="
	@$(PYTHON) tools/t5_slashing_check.py
	@$(CARGO) test --locked -p aura-storage --test t5_slashing
	@echo "== governance gate (T6) =="
	@$(PYTHON) tools/t6_governance_check.py
	@$(CARGO) test --locked -p aura-storage --test t6_governance
	@echo "== treasury gate (T7) =="
	@$(PYTHON) tools/t7_treasury_check.py
	@$(CARGO) test --locked -p aura-storage --test t7_treasury
	@echo "== white paper gate (T8) =="
	@$(PYTHON) tools/t8_whitepaper_check.py
	@$(CARGO) test --locked -p aura-storage --test t8_whitepaper
	@echo "== oracle gate (E5) =="
	@$(PYTHON) tools/e5_oracle_check.py
	@$(CARGO) test --locked -p aura-storage --test e5_oracle
	@echo "== amm gate (E4) =="
	@$(PYTHON) tools/e4_amm_check.py
	@$(CARGO) test --locked -p aura-storage --test e4_amm
	@echo "== stablecoin gate (E6) =="
	@$(PYTHON) tools/e6_cdp_check.py
	@$(CARGO) test --locked -p aura-storage --test e6_cdp
	@echo "== lending gate (E7) =="
	@$(PYTHON) tools/e7_lending_check.py
	@$(CARGO) test --locked -p aura-storage --test e7_lending
	@echo "== nft + dao gate (E8) =="
	@$(PYTHON) tools/e8_nft_check.py
	@$(CARGO) test --locked -p aura-storage --test e8_nft
	@echo "== bridge gate (E9) =="
	@$(PYTHON) tools/e9_bridge_check.py
	@$(CARGO) test --locked -p aura-storage --test e9_bridge
	@$(PYTHON) tools/e9_emulator_lane.py
	@echo "== baseline bench gate (P0) =="
	@$(PYTHON) tools/p0_bench_check.py
	@$(CARGO) test --locked -p aura-bench --test p0_bench
	@echo "== codec/crypto gate (P1) =="
	@$(PYTHON) tools/p1_crypto_check.py
	@$(CARGO) test --locked -p aura-bench --test p1_crypto
	@echo "== parallel execution gate (P2) =="
	@$(PYTHON) tools/p2_parallel_check.py
	@$(CARGO) test --locked -p aura-bench --test p2_parallel
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
	@for phase in c0 c1 c2 c3 c4 c5 c6 c7 c8 c9 c10 s0 s1 s2 s3 s4 s5 s6 s7 x0 x1 x2 x3 x4 x5 x6 x7 x8 d0 d1 b0 b1 b2 b3 d2 d3 d4 d5 d6 n0 n1 n2 b4 b5 d7 d8 n3 n4 n5 n6 n7 n8 b6 b7 b8 b9 b10 w0; do \
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

gen-b4-vectors:
	@$(PYTHON) tools/gen_b4_vectors.py

test-b4:
	@$(CARGO) test -p aura-consensus --lib
	@$(CARGO) test -p aura-consensus --test b4_validator_set

nc-b4:
	@mkdir -p docs/evidence/b4
	@$(PYTHON) tools/nc_b4.py > docs/evidence/b4/nc.log 2>&1 || { cat docs/evidence/b4/nc.log; exit 1; }
	@cat docs/evidence/b4/nc.log

evidence-b4:
	@$(PYTHON) tools/write_b4_evidence.py

gen-b5-vectors:
	@$(PYTHON) tools/gen_b5_vectors.py

test-b5:
	@$(CARGO) test -p aura-consensus --lib
	@$(CARGO) test -p aura-consensus --test b5_vrf_leader

nc-b5:
	@mkdir -p docs/evidence/b5
	@$(PYTHON) tools/nc_b5.py > docs/evidence/b5/nc.log 2>&1 || { cat docs/evidence/b5/nc.log; exit 1; }
	@cat docs/evidence/b5/nc.log

evidence-b5:
	@$(PYTHON) tools/write_b5_evidence.py

gen-b6-vectors:
	@$(PYTHON) tools/gen_b6_vectors.py

test-b6:
	@$(CARGO) test -p aura-consensus --lib
	@$(CARGO) test -p aura-consensus --test b6_slashing

nc-b6:
	@mkdir -p docs/evidence/b6
	@$(PYTHON) tools/nc_b6.py > docs/evidence/b6/nc.log 2>&1 || { cat docs/evidence/b6/nc.log; exit 1; }
	@cat docs/evidence/b6/nc.log

evidence-b6:
	@$(PYTHON) tools/write_b6_evidence.py

gen-b7-vectors:
	@$(PYTHON) tools/gen_b7_vectors.py

test-b7:
	@$(CARGO) test -p aura-consensus --lib
	@$(CARGO) test -p aura-consensus --test b7_rewards

nc-b7:
	@mkdir -p docs/evidence/b7
	@$(PYTHON) tools/nc_b7.py > docs/evidence/b7/nc.log 2>&1 || { cat docs/evidence/b7/nc.log; exit 1; }
	@cat docs/evidence/b7/nc.log

evidence-b7:
	@$(PYTHON) tools/write_b7_evidence.py

gen-b8-vectors:
	@$(PYTHON) tools/gen_b8_vectors.py

test-b8:
	@$(CARGO) test -p aura-consensus --lib
	@$(CARGO) test -p aura-consensus --test b8_fork_choice

nc-b8:
	@mkdir -p docs/evidence/b8
	@$(PYTHON) tools/nc_b8.py > docs/evidence/b8/nc.log 2>&1 || { cat docs/evidence/b8/nc.log; exit 1; }
	@cat docs/evidence/b8/nc.log

evidence-b8:
	@$(PYTHON) tools/write_b8_evidence.py

gen-b9-vectors:
	@$(PYTHON) tools/gen_b9_vectors.py

test-b9:
	@$(CARGO) test -p aura-consensus --lib
	@$(CARGO) test -p aura-consensus --test b9_governance

nc-b9:
	@mkdir -p docs/evidence/b9
	@$(PYTHON) tools/nc_b9.py > docs/evidence/b9/nc.log 2>&1 || { cat docs/evidence/b9/nc.log; exit 1; }
	@cat docs/evidence/b9/nc.log

evidence-b9:
	@$(PYTHON) tools/write_b9_evidence.py

gen-b10-vectors:
	@$(PYTHON) tools/gen_b10_vectors.py

test-b10:
	@$(CARGO) test -p aura-consensus --lib
	@$(CARGO) test -p aura-consensus --test b10_safety_evidence

nc-b10:
	@mkdir -p docs/evidence/b10
	@$(PYTHON) tools/nc_b10.py > docs/evidence/b10/nc.log 2>&1 || { cat docs/evidence/b10/nc.log; exit 1; }
	@cat docs/evidence/b10/nc.log

evidence-b10:
	@$(PYTHON) tools/write_b10_evidence.py

test-d2:
	@$(CARGO) test -p aura-node --test d2_pipeline

nc-d2:
	@mkdir -p docs/evidence/d2
	@$(PYTHON) tools/nc_d2.py > docs/evidence/d2/nc.log 2>&1 || { cat docs/evidence/d2/nc.log; exit 1; }
	@cat docs/evidence/d2/nc.log

evidence-d2:
	@$(PYTHON) tools/write_d2_evidence.py

test-d3:
	@$(CARGO) test -p aura-mempool
	@$(CARGO) test -p aura-node --test d3_mempool

nc-d3:
	@mkdir -p docs/evidence/d3
	@$(PYTHON) tools/nc_d3.py > docs/evidence/d3/nc.log 2>&1 || { cat docs/evidence/d3/nc.log; exit 1; }
	@cat docs/evidence/d3/nc.log

evidence-d3:
	@$(PYTHON) tools/write_d3_evidence.py

test-d4:
	@$(CARGO) test -p aura-rpc --lib
	@$(CARGO) test -p aura-rpc --test d4_vectors
	@$(CARGO) test -p aura-node --test d4_rpc
	@$(CARGO) test -p aura-node --lib pipeline::tests::managed_subscriptions_are_pruned_after_disconnect -- --exact

nc-d4:
	@mkdir -p docs/evidence/d4
	@$(PYTHON) tools/nc_d4.py > docs/evidence/d4/nc.log 2>&1 || { cat docs/evidence/d4/nc.log; exit 1; }
	@cat docs/evidence/d4/nc.log

evidence-d4:
	@$(PYTHON) tools/write_d4_evidence.py

test-d5:
	@$(CARGO) test -p aura-grpc-rest --lib
	@$(CARGO) test -p aura-grpc-rest --test d5_contract
	@$(CARGO) test -p aura-node --bin aurad

nc-d5:
	@mkdir -p docs/evidence/d5
	@$(PYTHON) tools/nc_d5.py > docs/evidence/d5/nc.log 2>&1 || { cat docs/evidence/d5/nc.log; exit 1; }
	@cat docs/evidence/d5/nc.log

evidence-d5:
	@$(PYTHON) tools/write_d5_evidence.py

test-d6:
	@$(CARGO) test -p aura-node --lib observability::tests
	@$(CARGO) test -p aura-node --lib lifecycle::tests::structured_log_events_are_json -- --exact
	@$(CARGO) test -p aura-node --lib lifecycle::tests::otel_layer_exports_recorded_spans -- --exact
	@$(CARGO) test -p aura-node --test d6_observability

nc-d6:
	@mkdir -p docs/evidence/d6
	@$(PYTHON) tools/nc_d6.py > docs/evidence/d6/nc.log 2>&1 || { cat docs/evidence/d6/nc.log; exit 1; }
	@cat docs/evidence/d6/nc.log

evidence-d6:
	@$(PYTHON) tools/write_d6_evidence.py

test-d7:
	@$(CARGO) test -p aura-node --lib devnet::tests
	@$(CARGO) test -p aura-node --test d7_devnet

nc-d7:
	@mkdir -p docs/evidence/d7
	@$(PYTHON) tools/nc_d7.py > docs/evidence/d7/nc.log 2>&1 || { cat docs/evidence/d7/nc.log; exit 1; }
	@cat docs/evidence/d7/nc.log

evidence-d7:
	@$(PYTHON) tools/write_d7_evidence.py

gen-d8-vectors:
	@$(PYTHON) tools/gen_d8_vectors.py

test-d8:
	@$(CARGO) test -p aura-node --lib cli::tests
	@$(CARGO) test -p aura-node --test d8_cli

nc-d8:
	@mkdir -p docs/evidence/d8
	@$(PYTHON) tools/nc_d8.py > docs/evidence/d8/nc.log 2>&1 || { cat docs/evidence/d8/nc.log; exit 1; }
	@cat docs/evidence/d8/nc.log

evidence-d8:
	@$(PYTHON) tools/write_d8_evidence.py

gen-n0-vectors:
	@$(PYTHON) tools/gen_n0_vectors.py

test-n0:
	@$(CARGO) test -p aura-p2p --lib
	@$(CARGO) test -p aura-node --test n0_two_nodes_no_p2p

nc-n0:
	@mkdir -p docs/evidence/n0
	@$(PYTHON) tools/nc_n0.py > docs/evidence/n0/nc.log 2>&1 || { cat docs/evidence/n0/nc.log; exit 1; }
	@cat docs/evidence/n0/nc.log

evidence-n0:
	@$(PYTHON) tools/write_n0_evidence.py

gen-n1-vectors:
	@$(PYTHON) tools/gen_n1_vectors.py

test-n1:
	@$(CARGO) test -p aura-p2p --lib
	@$(CARGO) test -p aura-p2p --test n1_transport
	@$(CARGO) test -p aura-p2p --test n1_spoofed_peer_ban

nc-p2p-review:
	@$(PYTHON) tools/nc_p2p_review.py

nc-n1:
	@$(PYTHON) tools/nc_n1.py > /tmp/aura-nc-n1.log 2>&1 || { cat /tmp/aura-nc-n1.log; exit 1; }
	@cat /tmp/aura-nc-n1.log

evidence-n1:
	@$(PYTHON) tools/write_n1_evidence.py

gen-n2-vectors:
	@$(PYTHON) tools/gen_n2_vectors.py

test-n2:
	@$(CARGO) test -p aura-p2p --lib
	@$(CARGO) test -p aura-p2p --test n2_gossipsub

nc-n2:
	@mkdir -p docs/evidence/n2
	@$(PYTHON) tools/nc_n2.py > docs/evidence/n2/nc.log 2>&1 || { cat docs/evidence/n2/nc.log; exit 1; }
	@cat docs/evidence/n2/nc.log

evidence-n2:
	@$(PYTHON) tools/write_n2_evidence.py

gen-n3-vectors:
	@$(PYTHON) tools/gen_n3_vectors.py

test-n3:
	@$(CARGO) test -p aura-p2p --lib
	@$(CARGO) test -p aura-p2p --test n3_discovery

nc-n3:
	@mkdir -p docs/evidence/n3
	@$(PYTHON) tools/nc_n3.py > docs/evidence/n3/nc.log 2>&1 || { cat docs/evidence/n3/nc.log; exit 1; }
	@cat docs/evidence/n3/nc.log

evidence-n3:
	@$(PYTHON) tools/write_n3_evidence.py

gen-n4-vectors:
	@$(PYTHON) tools/gen_n4_vectors.py

test-n4:
	@$(CARGO) test -p aura-p2p --lib
	@$(CARGO) test -p aura-p2p --test n4_block_sync

nc-n4:
	@mkdir -p docs/evidence/n4
	@$(PYTHON) tools/nc_n4.py > docs/evidence/n4/nc.log 2>&1 || { cat docs/evidence/n4/nc.log; exit 1; }
	@cat docs/evidence/n4/nc.log

evidence-n4:
	@$(PYTHON) tools/write_n4_evidence.py

gen-n5-vectors:
	@$(PYTHON) tools/gen_n5_vectors.py

test-n5:
	@$(CARGO) test -p aura-p2p --lib
	@$(CARGO) test -p aura-p2p --test n5_state_sync

nc-n5:
	@mkdir -p docs/evidence/n5
	@$(PYTHON) tools/nc_n5.py > docs/evidence/n5/nc.log 2>&1 || { cat docs/evidence/n5/nc.log; exit 1; }
	@cat docs/evidence/n5/nc.log

evidence-n5:
	@$(PYTHON) tools/write_n5_evidence.py

gen-n6-vectors:
	@$(PYTHON) tools/gen_n6_vectors.py

test-n6:
	@$(CARGO) test -p aura-p2p --lib
	@$(CARGO) test -p aura-p2p --test n6_dos_defence

nc-n6:
	@$(PYTHON) tools/nc_n6.py > /tmp/aura-nc-n6.log 2>&1 || { cat /tmp/aura-nc-n6.log; exit 1; }
	@cat /tmp/aura-nc-n6.log

evidence-n6:
	@$(PYTHON) tools/write_n6_evidence.py

gen-n7-vectors:
	@$(PYTHON) tools/gen_n7_vectors.py

test-n7:
	@$(CARGO) test -p aura-p2p --lib
	@$(CARGO) test -p aura-p2p --test n7_light_client

nc-n7:
	@mkdir -p docs/evidence/n7
	@$(PYTHON) tools/nc_n7.py > docs/evidence/n7/nc.log 2>&1 || { cat docs/evidence/n7/nc.log; exit 1; }
	@cat docs/evidence/n7/nc.log

evidence-n7:
	@$(PYTHON) tools/write_n7_evidence.py

gen-n8-vectors:
	@$(PYTHON) tools/gen_n8_vectors.py

test-n8:
	@$(CARGO) test -p aura-p2p --lib
	@$(CARGO) test -p aura-p2p --test n8_chaos

nc-n8:
	@mkdir -p docs/evidence/n8
	@$(PYTHON) tools/nc_n8.py > docs/evidence/n8/nc.log 2>&1 || { cat docs/evidence/n8/nc.log; exit 1; }
	@cat docs/evidence/n8/nc.log

evidence-n8:
	@$(PYTHON) tools/write_n8_evidence.py

gen-w0-vectors:
	@$(PYTHON) tools/gen_w0_vectors.py

test-w0:
	@$(CARGO) test -p aura-wallet --lib
	@$(CARGO) test -p aura-wallet --test w0_key_audit

# Historical W0 receipts are immutable; print current results instead.
nc-w0:
	@$(PYTHON) tools/nc_w0.py

evidence-w0:
	@$(PYTHON) tools/write_w0_evidence.py

evidence-w1:
	@$(PYTHON) tools/write_w1_evidence.py

devnet:
	@$(PYTHON) tools/run_devnet.py

clean:
	@$(CARGO) clean

# W1 crypto component gates. The explicit feature enables deliberately weak
# Argon2 parameters ONLY for the 1000-negative-password/format-fuzz tests.
.PHONY: test-w1 nc-w1
test-w1:
	@$(PYTHON) tools/w1_schema_reference.py
	@$(PYTHON) tools/test_w1_schema_reference_guard.py
	@$(PYTHON) tools/w1_recovery_metadata_reference.py
	@$(PYTHON) tools/test_w1_recovery_metadata_reference_guard.py
	@$(PYTHON) tools/test_w1_fuzz_campaign.py
	@$(PYTHON) tools/w1_mnemonic_reference.py
	@$(PYTHON) tools/test_w1_mnemonic_reference_guard.py
	@$(PYTHON) tools/w1_aad_reference.py
	@$(PYTHON) tools/check_scrypt_fork.py
	@$(PYTHON) tools/test_scrypt_fork_guard.py
	@$(PYTHON) tools/check_unicode_15_1_fork.py
	@$(PYTHON) tools/test_unicode_15_1_fork_guard.py
	@$(PYTHON) tools/check_w1_test_api.py
	@$(CARGO) test -p aura-wallet --lib
	@$(CARGO) test -p aura-wallet --lib --features serde_json/preserve_order keystore::aad_reference_tests::canonical_aad_matches_independent_python_bytes -- --exact
	@$(CARGO) test -p aura-wallet --test w1_keystore
	@$(CARGO) test -p aura-wallet --features insecure-test-kdf --test w1_keystore
	@$(CARGO) test --locked --manifest-path fuzz/Cargo.toml --lib --features insecure-wallet-fuzz

# Mutation controls do not overwrite historical receipts. They temporarily
# modify source, so do not run concurrently with builds or other controls.
nc-w1:
	@$(PYTHON) tools/nc_w1.py
	@$(PYTHON) tools/nc_w1_fuzz.py

# W2: the 40 CLI vectors gate (tools/gen_w2_vectors.py oracle; see docs/w2-cli-plan.md).
.PHONY: gen-w2-vectors test-w2 nc-w2

gen-w2-vectors:
	@$(PYTHON) tools/gen_w2_vectors.py

test-w2:
	@cargo test -p aura-wallet --features insecure-test-kdf --test w2_cli

nc-w2:
	@$(PYTHON) tools/nc_w2.py

evidence-w2:
	@$(PYTHON) tools/write_w2_evidence.py

# W3: the 24 air-gapped offline-signing vectors gate (tools/gen_w3_vectors.py oracle).
.PHONY: gen-w3-vectors test-w3 nc-w3 evidence-w3

gen-w3-vectors:
	@$(PYTHON) tools/gen_w3_vectors.py

test-w3:
	@cargo test -p aura-wallet --features insecure-test-kdf --test w3_offline

nc-w3:
	@$(PYTHON) tools/nc_w3.py

evidence-w3:
	@$(PYTHON) tools/write_w3_evidence.py

# W4: the 22 multisig vectors gate (tools/gen_w4_vectors.py oracle).
.PHONY: gen-w4-vectors test-w4 nc-w4 evidence-w4

gen-w4-vectors:
	@$(PYTHON) tools/gen_w4_vectors.py

test-w4:
	@cargo test -p aura-wallet --features insecure-test-kdf --test w4_multisig

nc-w4:
	@$(PYTHON) tools/nc_w4.py

evidence-w4:
	@$(PYTHON) tools/write_w4_evidence.py

# W5: the 10 hardware scenarios gate (tools/gen_w5_vectors.py oracle).
.PHONY: gen-w5-vectors test-w5 nc-w5 evidence-w5

gen-w5-vectors:
	@$(PYTHON) tools/gen_w5_vectors.py

test-w5:
	@cargo test -p aura-wallet --features insecure-test-kdf --test w5_hardware

nc-w5:
	@$(PYTHON) tools/nc_w5.py

evidence-w5:
	@$(PYTHON) tools/write_w5_evidence.py

# H0: the threat-model review gate (tools/check_threat_model.py).
.PHONY: gen-h0-vectors check-h0 nc-h0 evidence-h0

gen-h0-vectors:
	@$(PYTHON) tools/gen_h0_vectors.py

check-h0:
	@$(PYTHON) tools/check_threat_model.py

nc-h0:
	@$(PYTHON) tools/nc_h0.py

evidence-h0:
	@$(PYTHON) tools/write_h0_evidence.py

# H1: the supply-chain gate (tools/check_supply_chain.py).
.PHONY: gen-h1-vectors gen-h1-sbom check-h1 nc-h1 evidence-h1

gen-h1-vectors:
	@$(PYTHON) tools/gen_h1_vectors.py

gen-h1-sbom:
	@$(PYTHON) tools/gen_h1_sbom.py

check-h1:
	@$(PYTHON) tools/check_supply_chain.py --selftest
	@$(PYTHON) tools/check_supply_chain.py

nc-h1:
	@$(PYTHON) tools/nc_h1.py

evidence-h1:
	@$(PYTHON) tools/write_h1_evidence.py

# H2: the crypto review gate (docs/h2-crypto-review-plan.md).
.PHONY: gen-h2-vectors check-h2 nc-h2 evidence-h2

gen-h2-vectors:
	@$(PYTHON) tools/gen_h2_vectors.py

check-h2:
	@$(PYTHON) tools/check_h2_review.py

nc-h2:
	@$(PYTHON) tools/nc_h2.py

evidence-h2:
	@$(PYTHON) tools/write_h2_evidence.py

# H3: recorded fuzz campaigns (docs/h3-fuzz-plan.md §3) and controls.
fuzz-h3:
	@$(PYTHON) tools/h3_fuzz_campaign.py --schedule smoke

fuzz-h3-full:
	@$(PYTHON) tools/h3_fuzz_campaign.py --schedule full

nc-h3:
	@$(PYTHON) tools/nc_h3.py

evidence-h3:
	@$(PYTHON) tools/write_h3_evidence.py

# H4: property suites run inside `make test` (cargo test --workspace); these
# targets are the recorded 1M-tx campaign and the phase controls.
property-h4:
	@$(CARGO) test --locked -p aura-testkit --test h4_properties
	@$(CARGO) test --locked -p aura-state --test h4_tree
	@$(CARGO) test --locked -p aura-exec --test h4_model_vm

property-h4-full:
	@$(PYTHON) tools/h4_property_campaign.py --schedule full

nc-h4:
	@$(PYTHON) tools/nc_h4.py

evidence-h4:
	@$(PYTHON) tools/write_h4_evidence.py

# H5: consensus safety (docs/h5-consensus-plan.md). The model-table and
# chaos-matrix tests run inside `make test` (cargo test --workspace); the
# full campaign records them together with the TLC runs (safe / weakened).
model-h5:
	@$(CARGO) test --locked -p aura-consensus --test h5_safety_model
	@$(CARGO) test --locked -p aura-p2p --test h5_jepsen

model-h5-full:
	@$(PYTHON) tools/h5_consensus_campaign.py --schedule full

nc-h5:
	@$(PYTHON) tools/nc_h5.py

evidence-h5:
	@$(PYTHON) tools/write_h5_evidence.py

# H6: economic-attack simulations (docs/h6-economic-plan.md). The sims run
# inside `make test` (cargo test --workspace); these targets record the
# campaign and the phase controls.
econ-h6:
	@$(CARGO) test --locked -p aura-consensus --test h6_economic
	@$(CARGO) test --locked -p aura-mempool --test h6_mev

econ-h6-full:
	@$(PYTHON) tools/h6_economic_campaign.py --schedule full

nc-h6:
	@$(PYTHON) tools/nc_h6.py

evidence-h6:
	@$(PYTHON) tools/write_h6_evidence.py

# H7: OpSec / reproducible builds (docs/h7-opsec-plan.md). The OpSec
# checks are wired into `make test`; these targets record the campaign and
# the phase controls.
opsec-h7:
	@$(PYTHON) tools/check_opsec.py
	@$(CARGO) test --locked -p aura-wallet --features insecure-test-kdf --test h7_opsec

repro-h7:
	@$(PYTHON) tools/h7_repro_build.py

nc-h7:
	@$(PYTHON) tools/nc_h7.py

evidence-h7:
	@$(PYTHON) tools/write_h7_evidence.py

.PHONY: reaudit-h8 nc-h8 evidence-h8
reaudit-h8:
	@$(PYTHON) tools/h8_reaudit.py

nc-h8:
	@$(PYTHON) tools/nc_h8.py

evidence-h8:
	@$(PYTHON) tools/write_h8_evidence.py

.PHONY: drill-h9 nc-h9 evidence-h9
drill-h9:
	@$(PYTHON) tools/h9_incident_drill.py

nc-h9:
	@$(PYTHON) tools/nc_h9.py

evidence-h9:
	@$(PYTHON) tools/write_h9_evidence.py

.PHONY: check-e0 nc-e0 evidence-e0
check-e0:
	@$(PYTHON) tools/e0_baseline_check.py

nc-e0:
	@$(PYTHON) tools/nc_e0.py

evidence-e0:
	@$(PYTHON) tools/write_e0_evidence.py

.PHONY: test-e1 nc-e1 evidence-e1
test-e1:
	@$(CARGO) test --locked -p aura-storage --test e1_indexer

nc-e1:
	@$(PYTHON) tools/nc_e1.py

evidence-e1:
	@$(PYTHON) tools/write_e1_evidence.py

.PHONY: test-e2 nc-e2 evidence-e2
test-e2:
	@$(CARGO) test --locked -p aura-storage --test e2_explorer

nc-e2:
	@$(PYTHON) tools/nc_e2.py

evidence-e2:
	@$(PYTHON) tools/write_e2_evidence.py

.PHONY: test-t6 check-t6 examples-t6 nc-t6 evidence-t6
test-t6:
	@$(CARGO) test --locked -p aura-storage --test t6_governance

check-t6:
	@$(PYTHON) tools/t6_governance_check.py

examples-t6:
	@$(CARGO) run --quiet --locked -p aura-storage --example t6_governance_example

nc-t6:
	@$(PYTHON) tools/nc_t6.py

evidence-t6:
	@$(PYTHON) tools/write_t6_evidence.py

.PHONY: test-t7 check-t7 examples-t7 nc-t7 evidence-t7
test-t7:
	@$(CARGO) test --locked -p aura-storage --test t7_treasury

check-t7:
	@$(PYTHON) tools/t7_treasury_check.py

examples-t7:
	@$(CARGO) run --quiet --locked -p aura-storage --example t7_treasury_example

nc-t7:
	@$(PYTHON) tools/nc_t7.py

evidence-t7:
	@$(PYTHON) tools/write_t7_evidence.py

.PHONY: test-t8 check-t8 examples-t8 nc-t8 evidence-t8
test-t8:
	@$(CARGO) test --locked -p aura-storage --test t8_whitepaper

check-t8:
	@$(PYTHON) tools/t8_whitepaper_check.py

examples-t8:
	@$(CARGO) run --quiet --locked -p aura-storage --example t8_whitepaper_example

nc-t8:
	@$(PYTHON) tools/nc_t8.py

evidence-t8:
	@$(PYTHON) tools/write_t8_evidence.py

.PHONY: test-e5 check-e5 examples-e5 nc-e5 evidence-e5
test-e5:
	@$(CARGO) test --locked -p aura-storage --test e5_oracle

check-e5:
	@$(PYTHON) tools/e5_oracle_check.py

examples-e5:
	@$(CARGO) run --quiet --locked -p aura-storage --example e5_oracle_example sim/e5-oracle.json

nc-e5:
	@$(PYTHON) tools/nc_e5.py

evidence-e5:
	@$(PYTHON) tools/write_e5_evidence.py

.PHONY: test-e4 check-e4 examples-e4 nc-e4 evidence-e4
test-e4:
	@$(CARGO) test --locked -p aura-storage --test e4_amm

check-e4:
	@$(PYTHON) tools/e4_amm_check.py

examples-e4:
	@$(CARGO) run --quiet --locked -p aura-storage --example e4_amm_example sim/e4-amm.json

nc-e4:
	@$(PYTHON) tools/nc_e4.py

evidence-e4:
	@$(PYTHON) tools/write_e4_evidence.py

.PHONY: test-e6 check-e6 examples-e6 nc-e6 evidence-e6
test-e6:
	@$(CARGO) test --locked -p aura-storage --test e6_cdp

check-e6:
	@$(PYTHON) tools/e6_cdp_check.py

examples-e6:
	@$(CARGO) run --quiet --locked -p aura-storage --example e6_cdp_example sim/e6-cdp.json

nc-e6:
	@$(PYTHON) tools/nc_e6.py

evidence-e6:
	@$(PYTHON) tools/write_e6_evidence.py

.PHONY: test-e7 check-e7 examples-e7 nc-e7 evidence-e7
test-e7:
	@$(CARGO) test --locked -p aura-storage --test e7_lending

check-e7:
	@$(PYTHON) tools/e7_lending_check.py

examples-e7:
	@$(CARGO) run --quiet --locked -p aura-storage --example e7_lending_example sim/e7-lending.json

nc-e7:
	@$(PYTHON) tools/nc_e7.py

evidence-e7:
	@$(PYTHON) tools/write_e7_evidence.py

.PHONY: test-e8 check-e8 examples-e8 nc-e8 evidence-e8
test-e8:
	@$(CARGO) test --locked -p aura-storage --test e8_nft

check-e8:
	@$(PYTHON) tools/e8_nft_check.py

examples-e8:
	@$(CARGO) run --quiet --locked -p aura-storage --example e8_nft_example

nc-e8:
	@$(PYTHON) tools/nc_e8.py

evidence-e8:
	@$(PYTHON) tools/write_e8_evidence.py

.PHONY: test-e9 check-e9 examples-e9 nc-e9 evidence-e9 emulator-lane
test-e9:
	@$(CARGO) test --locked -p aura-storage --test e9_bridge

check-e9:
	@$(PYTHON) tools/e9_bridge_check.py

examples-e9:
	@$(CARGO) run --quiet --locked -p aura-storage --example e9_bridge_example

nc-e9:
	@$(PYTHON) tools/nc_e9.py

evidence-e9:
	@$(PYTHON) tools/write_e9_evidence.py

# E9 emulator lane: a deterministic CI run of `aurad` (the AuraLite-Emulator
# lane contract, in-repo harness).
emulator-lane:
	@$(PYTHON) tools/e9_emulator_lane.py

.PHONY: test-p0 check-p0 examples-p0 nc-p0 evidence-p0
test-p0:
	@$(CARGO) test --locked -p aura-bench --test p0_bench

check-p0:
	@$(PYTHON) tools/p0_bench_check.py

examples-p0:
	@$(CARGO) run --quiet --locked -p aura-bench --example p0_bench_example

nc-p0:
	@$(PYTHON) tools/nc_p0.py

evidence-p0:
	@$(PYTHON) tools/write_p0_evidence.py

.PHONY: test-p1 check-p1 examples-p1 nc-p1 evidence-p1
test-p1:
	@$(CARGO) test --locked -p aura-bench --test p1_crypto

check-p1:
	@$(PYTHON) tools/p1_crypto_check.py

examples-p1:
	@$(CARGO) run --quiet --locked -p aura-bench --example p1_crypto_example

nc-p1:
	@$(PYTHON) tools/nc_p1.py

evidence-p1:
	@$(PYTHON) tools/write_p1_evidence.py

.PHONY: test-p2 check-p2 examples-p2 nc-p2 evidence-p2
test-p2:
	@$(CARGO) test --locked -p aura-bench --test p2_parallel

check-p2:
	@$(PYTHON) tools/p2_parallel_check.py

examples-p2:
	@$(CARGO) run --quiet --locked -p aura-bench --example p2_parallel_example

nc-p2:
	@$(PYTHON) tools/nc_p2.py

evidence-p2:
	@$(PYTHON) tools/write_p2_evidence.py

.PHONY: test-p3 check-p3 examples-p3 nc-p3 evidence-p3
test-p3:
	@$(CARGO) test --locked -p aura-bench --test p3_storage

check-p3:
	@$(PYTHON) tools/p3_storage_check.py

examples-p3:
	@$(CARGO) run --quiet --locked -p aura-bench --example p3_storage_example | diff - <$(PYTHON) tools/p3_storage_check.py

nc-p3:
	@$(PYTHON) tools/nc_p3.py

evidence-p3:
	@$(PYTHON) tools/write_p3_evidence.py

.PHONY: test-p4 check-p4 examples-p4 nc-p4 evidence-p4
test-p4:
	@$(CARGO) test --locked -p aura-bench --test p4_network

check-p4:
	@$(PYTHON) tools/p4_network_check.py

examples-p4:
	@$(CARGO) run --quiet --locked -p aura-bench --example p4_network_example | diff - <$(PYTHON) tools/p4_network_check.py

nc-p4:
	@$(PYTHON) tools/nc_p4.py

evidence-p4:
	@$(PYTHON) tools/write_p4_evidence.py

.PHONY: test-p5 check-p5 examples-p5 nc-p5 evidence-p5
.PHONY: test-p6 check-p6 examples-p6 nc-p6 evidence-p6
test-p5:
	@$(CARGO) test --locked -p aura-bench --test p5_consensus

check-p5:
	@$(PYTHON) tools/p5_consensus_check.py

examples-p5:
	@$(CARGO) run --quiet --locked -p aura-bench --example p5_consensus_example | diff - <$(PYTHON) tools/p5_consensus_check.py

nc-p5:
	@$(PYTHON) tools/nc_p5.py

evidence-p5:
	@$(PYTHON) tools/write_p5_evidence.py

nc-p6:
	@$(PYTHON) tools/nc_p6.py

test-p6:
	@$(CARGO) test --locked -p aura-bench --test p6_slo

check-p6:
	@$(PYTHON) tools/p6_slo_check.py
	@$(CARGO) run --quiet --locked -p aura-bench --example p6_slo_example | diff - <$(PYTHON) tools/p6_slo_check.py

examples-p6:
	@$(CARGO) run --quiet --locked -p aura-bench --example p6_slo_example

evidence-p6:
	@$(PYTHON) tools/write_p6_evidence.py

.PHONY: test-t5 check-t5 examples-t5 nc-t5 evidence-t5
test-t5:
	@$(CARGO) test --locked -p aura-storage --test t5_slashing

check-t5:
	@$(PYTHON) tools/t5_slashing_check.py

examples-t5:
	@$(CARGO) run --quiet --locked -p aura-storage --example t5_slashing_example

nc-t5:
	@$(PYTHON) tools/nc_t5.py

evidence-t5:
	@$(PYTHON) tools/write_t5_evidence.py

.PHONY: test-t4 check-t4 examples-t4 nc-t4 evidence-t4
test-t4:
	@$(CARGO) test --locked -p aura-storage --test t4_staking

check-t4:
	@$(PYTHON) tools/t4_staking_check.py

examples-t4:
	@$(CARGO) run --quiet --locked -p aura-storage --example t4_staking_example

nc-t4:
	@$(PYTHON) tools/nc_t4.py

evidence-t4:
	@$(PYTHON) tools/write_t4_evidence.py

.PHONY: test-t3 check-t3 examples-t3 nc-t3 evidence-t3
test-t3:
	@$(CARGO) test --locked -p aura-storage --test t3_fees

check-t3:
	@$(PYTHON) tools/t3_fees_check.py

examples-t3:
	@$(CARGO) run --quiet --locked -p aura-storage --example t3_fees_example

nc-t3:
	@$(PYTHON) tools/nc_t3.py

evidence-t3:
	@$(PYTHON) tools/write_t3_evidence.py

.PHONY: test-t2 check-t2 examples-t2 nc-t2 evidence-t2
test-t2:
	@$(CARGO) test --locked -p aura-storage --test t2_allocation

check-t2:
	@$(PYTHON) tools/t2_allocation_check.py

examples-t2:
	@$(CARGO) run --quiet --locked -p aura-storage --example t2_allocation_example

nc-t2:
	@$(PYTHON) tools/nc_t2.py

evidence-t2:
	@$(PYTHON) tools/write_t2_evidence.py

.PHONY: test-t1 check-t1 examples-t1 nc-t1 evidence-t1
test-t1:
	@$(CARGO) test --locked -p aura-storage --test t1_issuance

check-t1:
	@$(PYTHON) tools/t1_issuance_check.py

examples-t1:
	@$(CARGO) run --quiet --locked -p aura-storage --example t1_issuance_example

nc-t1:
	@$(PYTHON) tools/nc_t1.py

evidence-t1:
	@$(PYTHON) tools/write_t1_evidence.py

.PHONY: test-t0 check-t0 examples-t0 nc-t0 evidence-t0
test-t0:
	@$(CARGO) test --locked -p aura-storage --test t0_economics

check-t0:
	@$(PYTHON) tools/t0_economics_check.py

examples-t0:
	@$(CARGO) run --quiet --locked -p aura-storage --example t0_sim

nc-t0:
	@$(PYTHON) tools/nc_t0.py

evidence-t0:
	@$(PYTHON) tools/write_t0_evidence.py

.PHONY: test-e3 check-e3 examples-e3 nc-e3 evidence-e3
test-e3:
	@$(CARGO) test --locked -p aura-storage --test e3_faucet

check-e3:
	@$(PYTHON) tools/e3_faucet_check.py

examples-e3:
	@$(PYTHON) tools/e3_examples_check.py

nc-e3:
	@$(PYTHON) tools/nc_e3.py

evidence-e3:
	@$(PYTHON) tools/write_e3_evidence.py

# Optional independent interop check; install the pinned test-only Python
# dependencies in tools/w1-reference-requirements.txt first.
.PHONY: test-w1-interop
test-w1-interop:
	@$(PYTHON) tools/test_w1_interop_guard.py
	@$(PYTHON) tools/w1_interop_reference.py
	@$(PYTHON) -m tools.w1_tx_reference
