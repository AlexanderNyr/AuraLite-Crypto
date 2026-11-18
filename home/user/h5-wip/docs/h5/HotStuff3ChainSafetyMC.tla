---- MODULE HotStuff3ChainSafetyMC ----
\* H5 model-checkable behaviour spec for the 3-chain accountable-safety
\* essence of HotStuff3ChainSafety (docs/h5-consensus-plan.md D-H5-1).
\* WeakenQuorum = FALSE models the 2f+1 quorum threshold; TRUE removes it
\* (threshold F + 1) and the model must then produce a counterexample.
EXTENDS Integers, FiniteSets

CONSTANTS
    N,              \* number of validators
    F,              \* byzantine bound
    WeakenQuorum,   \* FALSE: Threshold = 2 * F + 1; TRUE: Threshold = F + 1
    Views,          \* set of views explored
    Branches        \* set of branch identifiers

ASSUME N >= 4 /\ 3 * F < N /\ WeakenQuorum \in BOOLEAN

Validators == 1..N
ByzantineValidators == 1..F
HonestValidators == (F + 1)..N
Threshold == IF WeakenQuorum THEN F + 1 ELSE 2 * F + 1

VARIABLE votes

TypeOK ==
    \A v \in votes :
        v[1] \in Validators /\ v[2] \in Views /\ v[3] \in Branches

Init == votes = {}

\* Byzantine validators may vote every branch; honest validators at most one.
Next ==
    \E i \in Validators, view \in Views, b \in Branches :
        /\ <<i, view, b>> \notin votes
        /\ \/ i \in ByzantineValidators
           \/ \A b2 \in Branches : <<i, view, b2>> \notin votes
        /\ votes' = votes \cup {<<i, view, b>>}

\* A QC exists for (view, branch) once Threshold validators signed it.
QC(view, b) ==
    \E S \subseteq Validators :
        /\ Cardinality(S) >= Threshold
        /\ \A i \in S : <<i, view, b>> \in votes

Committed(view, b) == QC(view, b)

\* Accountable-safety essence: no two conflicting commits at the same view.
AccountableSafety ==
    \A v \in Views, b1 \in Branches, b2 \in Branches :
        (Committed(v, b1) /\ Committed(v, b2)) => b1 = b2

\* Quorum intersection: any two Threshold-quorums share an honest validator.
QuorumIntersection ==
    \A S1 \subseteq Validators, S2 \subseteq Validators :
        (Cardinality(S1) >= Threshold /\ Cardinality(S2) >= Threshold)
        => \E i \in (S1 \cap S2) \cap HonestValidators : TRUE
====
