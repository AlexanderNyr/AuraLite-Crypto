# H4 evidence report (property / differential)

Recorded campaign `full`: 1000000 generated txs, each signed, verified and
canonical-round-tripped; `sum(balances)` checked after every one
(937224 checks, final total 16000000, digest ff506f8fa8770d6e071bf2badd1d15e93bcfde441f23df8abfa89e39186c9e82).

Cross-language differential both ways: Rust dump checked by the
Python reference (C4 tx encoder + S1 tree), Python cases checked by
the Rust binary (`campaign/logs/` holds the four differential logs).

Deviation (plan §3): the gate is `--txs 1000000` (`make
property-h4-full`); this workspace records the schedule named above
with exact counts — no extrapolation.

Mutation controls: 6/6 (`tools/nc_h4.py`, log in `nc.log`); the
named control injects a conservation bug and the property finds it.
