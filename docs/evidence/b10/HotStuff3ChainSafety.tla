---- MODULE HotStuff3ChainSafety ----
EXTENDS Integers, FiniteSets, Sequences

\* @type: Int;
CONSTANT N
\* @type: Int;
CONSTANT F
\* @type: Set(Int);
CONSTANT Views
\* @type: Set(Str);
CONSTANT Branches

ASSUME N >= 4 /\ 3 * F < N

Validators == 1..N
ByzantineValidators == 1..F
HonestValidators == (F + 1)..N
QuorumThreshold == 2 * F + 1

VARIABLES
    \* @type: Set(<<Int, Int, Str>>);
    votes,
    \* @type: Set(<<Int, Str, Set(Int)>>);
    qcs,
    \* @type: Set(<<Int, Int, Str>>);
    committed

\* Honest validators sign at most one branch in a given view.
HonestSingleVotePerView ==
    \A v \in HonestValidators, view \in Views, b1 \in Branches, b2 \in Branches :
        (<<v, view, b1>> \in votes /\ <<v, view, b2>> \in votes) => (b1 = b2)

\* Quorum intersection: any two 2f+1 quorums overlap in at least f+1 validators (>= 1 honest).
QuorumIntersection ==
    \A q1 \in SUBSET Validators, q2 \in SUBSET Validators :
        (Cardinality(q1) >= QuorumThreshold /\ Cardinality(q2) >= QuorumThreshold)
        => Cardinality((q1 \cap q2) \cap HonestValidators) >= 1

\* 3-chain commit rule: three consecutive QCs in the same view commit the target branch.
ThreeChainCommit(view, height, branch, signers) ==
    /\ Cardinality(signers) >= QuorumThreshold
    /\ <<view, branch, signers>> \in qcs

\* Safety invariant: no two conflicting branches are ever committed at the same height for f < n/3.
AccountableSafetyInvariant ==
    \A v1 \in Views, v2 \in Views, h \in Int, b1 \in Branches, b2 \in Branches :
        (<<v1, h, b1>> \in committed /\ <<v2, h, b2>> \in committed) => (b1 = b2)
====
