# W1 extended local fuzz observation

**WIP local evidence, not a W1 phase receipt, a 24-hour H3 campaign, an
independent security audit, or evidence of absence of bugs.** A sequential
three-target ASan/libFuzzer run using `tools/w1_fuzz_campaign.py` completed
with status 0 and no artifacts. Each target's source-manifest check matched
before and after its run. The source manifest is not a complete environment
image; this observation applies to the recorded pre-recovery-target tree,
not automatically to subsequent changes.

Environment: `rustc 1.101.0-nightly (1c3a65069 2026-11-02)`, cargo-fuzz
0.13.2, base Git HEAD `c749fc0` plus recorded local changes, one build job.
The runner records exact commands, seeds, initial and evolved corpora and
source hashes in `/home/user/w1-campaign-extended/`. Each target requested
300 seconds; these times are libFuzzer-reported and exclude compilation.

| Target | Iterations | Time | Exit | SHA-256 of run.log |
|---|---:|---:|---:|---|
| `fuzz_keystore_parse` | 9,255,635 | 301 s | 0 | `4d72be9ac5e59d39c507ee653c34577ec9e875680aabd9ab71383d6ae08ddad7` |
| `fuzz_keystore_decrypt` | 240,211 | 301 s | 0 | `f546b3b9800c5425fb6a306a316fb414672cd1412036c14abc24cb07483439ac` |
| `fuzz_scrypt_differential` | 184,115 | 301 s | 0 | `04be80d6d9b99b0643d5477df8344a21dab6d5abc2bab1f2e5ea7ed1beca2357` |

`result.json` SHA-256:
`93af5d532ac026ee62d8f0e93d61d606f395eac5a68eaed352bdfe7872e7f2ce`;
`source-sha256.json` SHA-256:
`d87eb9ab7418a08d0d90ed7ffee47f8a45065d54258bc64f738f2e595f7c0375`.
The decrypt run used the intentionally weak test-only KDF feature; the scrypt
differential run used tiny parameters. All inputs and corpora are public test
data. ASan and these checks do not establish production-KDF performance or
security. Seeds do not make fuzz evolution deterministic. A later separate
recovery-metadata target is not included in this observation.

## Follow-up after adding the recovery marker target

A separate 60-second-per-target run on a later source snapshot included a new
`fuzz_recovery_metadata` target. It feeds bounded and oversized public UTF-8
input to the recovery-marker parser and checks accepted-marker serialization
round-trips and the 4096-byte acceptance limit. This does **not** independently
prove rejection of all invalid metadata; existing targeted regression tests
cover selected invalid cases. All four targets completed with exit 0, source
hashes unchanged during their respective runs and no artifacts:

| Target | Iterations | Time | SHA-256 of run.log |
|---|---:|---:|---|
| `fuzz_keystore_parse` | 2,570,611 | 61 s | `9b5338868ce5c00adc10287755991eeaa454f339b76f5710d44cda44bf205032` |
| `fuzz_keystore_decrypt` | 49,794 | 61 s | `279f180fb47ca559c309d83bc0ddb0aa651e8c3cf536a34a8539a314b9516b01` |
| `fuzz_scrypt_differential` | 33,816 | 61 s | `8f666f2b273ca5121efee9c8fc9e355ec6ac52a524fd36cda13aeb58f4644fa2` |
| `fuzz_recovery_metadata` | 2,846,376 | 61 s | `4ab1e86e2992ae553e7957cd8a7c14491df14cf9fa7023b65b9d83338a7fd1b4` |

Commands, seeds and corpora: `/home/user/w1-campaign-recovery/`.
`result.json` SHA-256:
`00421b3704c110aa42d51cfd65611e297254942e3b481fc67eafb1db52ed9dd1`;
`source-sha256.json` SHA-256:
`3b88fe5068b08485ba6114e597ab7ca15ac8d772a3ff2b76dea885c3c3d83406`.
The same test-only-KDF, limited-duration and no-security-proof caveats apply.

## Follow-up with metamorphic recovery marker checks

A third, separate four-target run used a later source snapshot and completed
61 seconds per target with exit 0, no artifacts and unchanged source hashes.
The recovery parser target additionally generated a duplicate member, an extra
member and an oversized document from each accepted canonical marker and
required their rejection. The target checks these transformations, not every
possible malformed JSON input. Independently of fuzzing, a hand-authored test
exercises 25 recovery marker cases and the exact 4096/4097-byte boundary.

| Target | Iterations | Time | SHA-256 of run.log |
|---|---:|---:|---|
| `fuzz_keystore_parse` | 2,509,667 | 61 s | `2c4064114d184c98ed9e9e77202421a4ad0e7627c21638a7458c19b280464f09` |
| `fuzz_keystore_decrypt` | 52,540 | 61 s | `0f85f256a61810da9d904e2949f034767e18d60c267eb5327023b04a5a896e81` |
| `fuzz_scrypt_differential` | 31,865 | 61 s | `ea343ddbfb18f1fbbf41f16978317435ec6f984a608243ead4aa071d5c672df3` |
| `fuzz_recovery_metadata` | 3,255,574 | 61 s | `ac222dce8e5cb03701b63a88d9ebd272fecb6b75fb14b3d3e8a16db1acd910e4` |

Commands, seeds and corpora: `/home/user/w1-campaign-metadata-metamorphic/`.
`result.json` SHA-256:
`55bf253e29e5b4836eb8b3519e7838500e09061efa2184d15fb58feebad19400`;
`source-sha256.json` SHA-256:
`6171dd2ae758d30dc124e493070d8de879396814e76196c9b4083a8def63d838`.
The same test-only-KDF, limited-duration and no-security-proof caveats apply.

## Version-2 evidence bundle with corpus manifests and checkout check

A fourth sequential four-target run (`format_version: 2`, seed base `20261140`,
60 seconds requested per target) recorded unkeyed SHA-256 manifests for
tracked sources, initial seed corpora and evolved corpora in
`/home/user/w1-campaign-v2-verified/`. `tools/verify_w1_fuzz_evidence.py
--check-checkout /home/user/AuraLite-Crypto /home/user/w1-campaign-v2-verified`
verified internal manifest consistency, zero artifacts, completion lines and
exact agreement between `source-sha256.json` and the tracked checkout files.
All hashes are unkeyed self-consistency checks, not cryptographic signatures,
wall-clock proof, H3's 24-hour campaign or an independent audit.

| Target | Iterations | Time | SHA-256 of run.log |
|---|---:|---:|---|
| `fuzz_keystore_parse` | 2,509,637 | 61 s | `77e1109d47558b1914baeae297adf0553055d60ae9a378b991104f00e8c4c9aa` |
| `fuzz_keystore_decrypt` | 51,301 | 61 s | `a79f7bd9675d1a5de2e8bcf1c2adcedc5350190ec10af77f178d65db4bd10521` |
| `fuzz_scrypt_differential` | 48,545 | 61 s | `a8b19393e89853fec505ee7f246e5d9aa2b6faf94497f6827eb3269842dd7a93` |
| `fuzz_recovery_metadata` | 3,547,323 | 61 s | `a195c7be697f526f30d58d6bab31a3bfafc41e18941a4406da1a5e5d7fe15123` |

`result.json` SHA-256:
`3a2723b47f063f5f74bf78175c65fe42fdf789bfa7b2d4db7b9a970d3ac1af19`;
`source-sha256.json` SHA-256:
`5839f2dddde477bbbf45da0d2d6f63c4ab033ee317d9bf636edc99dcdea08e52`.
The same test-only-KDF, limited-duration and no-security-proof caveats apply.
Note: the evolved `fuzz_keystore_parse` corpus of this historical run was later
lost to a workspace snapshot file limit; its `corpus-sha256.json` manifest is
retained, so `verify_w1_fuzz_evidence.py` can no longer fully re-check that
bundle's corpus tree.

## Version-2 bundle after the hex-case AAD and profile-marker fuzz modes

A fifth sequential four-target run (`format_version: 2`, seed base `20261150`,
60 seconds requested per target) covers the fourteen-mode decrypt oracle that
now includes `seed_derivation.normalization_profile` mutation and case-only hex
spelling mutations in AAD fields whose decoded id/salt/nonce bytes stay
identical. The parser target additionally derives extra-field, escaped-duplicate
(`\u0066ormat`) and `MAX_KEYSTORE_BYTES + 1` rejects from each accepted
document and preserves `normalization_profile()` across reserialization.
Evidence is recorded in `/home/user/w1-campaign-v2-reauth/`;
`tools/verify_w1_fuzz_evidence.py --check-checkout /home/user/AuraLite-Crypto
/home/user/w1-campaign-v2-reauth` verified internal manifest consistency, zero
artifacts, completion lines and exact agreement with the tracked checkout.
All hashes are unkeyed self-consistency checks, not cryptographic signatures,
wall-clock proof, H3's 24-hour campaign or an independent audit.

| Target | Iterations | Time | SHA-256 of run.log |
|---|---:|---:|---|
| `fuzz_keystore_parse` | 2,594,217 | 61 s | `50b9c543f7a5de5ea540959dc220653216dfb619a3b8d7514ffb19fe7aa1765c` |
| `fuzz_keystore_decrypt` | 55,602 | 61 s | `330fd7214c1b275562a1d6fa567e6f17664da8e380a477f4bc3f7a3c29e302ae` |
| `fuzz_scrypt_differential` | 35,679 | 61 s | `32c5828a02eb814bd0e79d29488d1000c0edf5b1ae98f3c13622727f88f338de` |
| `fuzz_recovery_metadata` | 3,055,255 | 61 s | `b403fe079cf00f5a52faebb5354ee4e7fbd003666672e4cb2fa594e46420a677` |

`result.json` SHA-256:
`5c0d1af620a71503d44ec3d10dede089ba33061f1c727d8be9e863f954f33b5d`;
`source-sha256.json` SHA-256:
`37b3c30c9de22dddef2219e0fca5df9e2bbd66d8c906608ea44646c43b31c989`.
The same test-only-KDF, limited-duration and no-security-proof caveats apply.
