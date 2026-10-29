//! Deterministic round-robin proposer (B1).
//!
//! Given a validator set, the proposer for round `r` is
//! `validators[r % validators.len()]`. This is PoA: no voting, no stakes,
//! no VRF — just a predictable rotation so every validator eventually
//! proposes. B2–B5 replace this with real BFT.

use aura_types::Address;

/// A deterministic round-robin proposer over a fixed validator set.
///
/// The validator set is immutable after construction: B4 adds enter/exit
/// queues and epoch changes, but at B1 the set is fixed for the lifetime
/// of the `Proposer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposer {
    /// The validators, in a fixed order. Round-robin index wraps here.
    validators: Vec<Address>,
}

impl Proposer {
    /// Create a new proposer over `validators`.
    ///
    /// # Panics
    ///
    /// Panics if `validators` is empty — consensus with no validators is
    /// meaningless, and the error belongs at construction time.
    pub fn new(validators: Vec<Address>) -> Self {
        assert!(
            !validators.is_empty(),
            "proposer requires at least one validator"
        );
        Self { validators }
    }

    /// The validator selected to propose at `round`.
    ///
    /// Round `r` maps to `validators[r % validator_count()]`, so the
    /// selection is fully deterministic and every validator gets an equal
    /// share of rounds over any contiguous range of `validator_count()`
    /// rounds.
    pub fn propose(&self, round: u64) -> Address {
        self.validators[round as usize % self.validators.len()]
    }

    /// Is `address` the proposer for `round`?
    pub fn is_proposer(&self, address: &Address, round: u64) -> bool {
        &self.propose(round) == address
    }

    /// Number of validators in the set.
    pub fn validator_count(&self) -> usize {
        self.validators.len()
    }

    /// The full validator set, in order.
    pub fn validators(&self) -> &[Address] {
        &self.validators
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_types::Address;

    fn validator(bytes: [u8; 20]) -> Address {
        Address::from_bytes(bytes)
    }

    #[test]
    #[should_panic(expected = "proposer requires at least one validator")]
    fn empty_validator_set_panics() {
        let _ = Proposer::new(Vec::new());
    }

    #[test]
    fn round_robin_rotation() {
        let v = [
            validator([1u8; 20]),
            validator([2u8; 20]),
            validator([3u8; 20]),
        ];
        let p = Proposer::new(v.to_vec());

        // Rounds 0..3 hit each validator once, round 3 wraps to the first.
        assert_eq!(p.propose(0), v[0]);
        assert_eq!(p.propose(1), v[1]);
        assert_eq!(p.propose(2), v[2]);
        assert_eq!(p.propose(3), v[0]);
        assert_eq!(p.propose(4), v[1]);
        assert_eq!(p.propose(5), v[2]);
        assert_eq!(p.propose(100), v[1]); // 100 % 3 == 1
    }

    #[test]
    fn is_proposer_matches() {
        let v = [validator([1u8; 20]), validator([2u8; 20])];
        let p = Proposer::new(v.to_vec());
        assert!(p.is_proposer(&v[0], 0));
        assert!(!p.is_proposer(&v[1], 0));
        assert!(p.is_proposer(&v[1], 1));
        assert!(!p.is_proposer(&v[0], 1));
    }

    #[test]
    fn validator_count_and_list() {
        let v = [
            validator([1u8; 20]),
            validator([2u8; 20]),
            validator([3u8; 20]),
            validator([4u8; 20]),
        ];
        let p = Proposer::new(v.to_vec());
        assert_eq!(p.validator_count(), 4);
        assert_eq!(p.validators(), &v);
    }

    #[test]
    fn single_validator_proposes_every_round() {
        let v = [validator([7u8; 20])];
        let p = Proposer::new(v.to_vec());
        for round in 0..1000 {
            assert_eq!(p.propose(round), v[0]);
            assert!(p.is_proposer(&v[0], round));
        }
    }
}
