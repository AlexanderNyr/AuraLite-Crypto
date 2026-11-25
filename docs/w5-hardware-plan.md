# W5 hardware keys — design of record (D-W5-1..D-W5-4)

**Phase:** W5 (WALLET_PLAN Stage 2). **Deliverable:** `patches/0100-W5-…`.
**Model:** `docs/w4-multisig-plan.md` (W4). The device is a deterministic
simulation of a HID/CCID-like secure element: no real hardware, no RNG, fixed
frames, byte-exact vectors from the independent oracle `tools/gen_w5_vectors.py`.
PUBLIC TEST DATA ONLY — never use for funds.

## Problem

A hardware key must never sign anything the human did not see on its own
screen. The host (a possibly infected laptop) relays frames; the device screen
is the only trustworthy display. Two failure modes dominate real incidents:

1. **blind signing** — the host asks the device to sign opaque bytes without a
   display/confirm round trip;
2. **display substitution** — the host preview shows one recipient/amount while
   the signed payload carries another.

## Decisions

- **D-W5-1 (display source).** The device screen is derived exclusively from
  the canonical `UnsignedTx` bytes it is asked to sign — never from
  host-supplied strings. The screen renders the nine unsigned protocol fields
  plus the decoded intent (`to`/`amount`/`validator`).
- **D-W5-2 (payloads that can be shown).** The device only signs payloads it
  can fully decode: `transfer` payload is exactly
  `recipient (20) || amount (LE u64)` (28 bytes); `delegate`/`undelegate` use
  the canonical `AuraLite-Crypto/wallet-stake-v1` stake payload. Anything else
  (`deploy`, `call`, truncated payloads) is refused as `undecodable payload` —
  a device never signs bytes it cannot display. A host preview
  (`HostClaim`) that disagrees with the derived display is refused as
  `display mismatch` before the human is asked.
- **D-W5-3 (confirmation binding).** Signing happens only after the human's
  explicit `Confirm` on the derived display (`Phase::Armed -> Confirm`). Every
  other path — `BlindSign` frames, `Decide` without a displayed round, replayed
  frames — returns `blind confirmation rejected`. `Deny` always refuses and
  never latches: the human can open a fresh round (retry works).
- **D-W5-4 (signature).** The device signs `UnsignedTx::signing_bytes()` with
  the fixed AuraLite `tx` domain (Ed25519), so a hardware-produced `Tx` is
  indistinguishable from and verifiable by the W3 offline path
  (`Tx::verify_signature`).

## Protocol (HID/CCID-like)

`HardwareDevice::exchange(DeviceCommand) -> DeviceResponse` with a fixed
command set: `GetVersion`, `BeginSign { unsigned, claim }`,
`Decide { confirm | deny }`, `BlindSign { body }`. Responses: `Version`,
`Display(frame)`, `Signed(tx)`, `Refused(error)`. State machine:
`Idle -> Armed(displayed) -> Idle` per round; `Deny` returns to `Idle`.

## Gate (WALLET_PLAN row W5)

`10 "human confirmed/denied" scenarios; NC: blind confirmation -> red`:

| # | Scenario | Expect |
| --- | --- | --- |
| 1 | `confirm_transfer_basic` | exit 0, screen + `sig` pinned |
| 2 | `deny_transfer_basic` | exit 65 `error: hardware: denied by user` |
| 3 | `confirm_transfer_high_value` | exit 0, `amount=123456789` on screen |
| 4 | `deny_transfer_high_value` | exit 65 denied |
| 5 | `blind_confirmation_rejected` | exit 65 blind rejected — **named control** |
| 6 | `display_recipient_substitution` | exit 65 `display mismatch` |
| 7 | `deny_then_retry_confirm` | exit 0 after a denied round (retry works) |
| 8 | `confirm_delegate` | exit 0, validator+amount on screen |
| 9 | `undecodable_payload_refused` | exit 65 `undecodable payload` |
| 10 | `foreign_chain_refused` | exit 65 `foreign chain-id` |

Mutation controls (`tools/nc_w5.py`, 6): blind acceptance, deny ignored,
display binding dropped, chain check dropped, amount render altered, payload
decode check dropped — each must turn its named scenario red.

Cumulative NC after W5: **656** (650 through P6 + 6).
