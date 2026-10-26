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
        audit-closeout nc-c10 evidence-c10 devnet clean

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
	@$(CARGO) test -p aura-types -p aura-codec -p aura-crypto -p aura-tx -p aura-block -p aura-state -p aura-genesis -p aura-testkit
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
	@for phase in 0 1 2 3 4 5 6 7 8 9 10; do \
	    $(PYTHON) tools/nc_c$$phase.py > /tmp/aura-nc-c$$phase.log 2>&1 || \
	      { echo "C$$phase NEGATIVE CONTROLS FAILED"; tail -30 /tmp/aura-nc-c$$phase.log; exit 1; }; \
	    grep '^RESULT:' /tmp/aura-nc-c$$phase.log; \
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

devnet:
	@echo "ERROR: devnet is D7 (NODE_PLAN). It is not implemented at C0." >&2
	@exit 1

clean:
	@$(CARGO) clean
