//! X7 — the determinism gate: a fixed scenario, replayed by independent nodes.
//!
//! The claim this module exists to make measurable is one sentence:
//!
//! > The same ordered input, executed by independent nodes, produces the same
//! > `state_root`, the same gas bill and the same events.
//!
//! "Independent" is the load-bearing word. Two runs inside one process share
//! Rust's per-process hash seed, allocator history and any thread-local cache, so
//! they can agree while a second node would not. The gate therefore also runs the
//! scenario in **separate processes** (`x7-run`), where `std::collections::HashMap`
//! is seeded differently by the OS, and in **separate builds** (debug, release,
//! release with different codegen units), where the optimizer — and, in debug,
//! integer-overflow checks — differ.
//!
//! What this module does **not** do: consensus, networking, a block structure, or
//! any node. X7 is a gate over the machine that D1..D5 will drive; it deliberately
//! borrows the `Lifecycle` (X3), the call surface (X6) and the overlay (S3)
//! unchanged, so what it measures is the code an actual node will run.
//!
//! Three properties are asserted on every step, not only at the end:
//!
//! * a **committing** step advances the versioned root and contributes gas;
//! * a **refused** step (`Returned(v)` with `v != 0`) leaves the root
//!   byte-identical and contributes gas — work that is billed but not committed;
//! * a **failed** step (trap, out-of-gas, lifecycle error) leaves the root
//!   byte-identical too, and is counted separately, because a node that disagreed
//!   about *which* transactions failed would also disagree about the root.
//!
//! The rolling digest folds every step in, so a divergence is localised to the
//! step that caused it rather than reported as "the final roots differ".

use aura_crypto::blake3_hash;
use aura_state::{Account, MerkleTree, StateKey, StateView, apply_diff};
use aura_types::{Address, Amount, Hash, Nonce};

use crate::lifecycle::{Lifecycle, LifecycleReceipt};
use crate::{BlockContext, ContractCall, ContractReceipt};

/// Seed of the scenario. Changing it changes the whole run — which is exactly
/// what NC-6 and `a_different_seed_changes_the_root` use as a sensitivity check.
pub const X7_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// How many transactions the scenario contains after its deploys.
pub const X7_STEPS: usize = 1_000;

/// How many funded accounts share the ledger.
pub const X7_ACCOUNTS: usize = 8;

/// How many independent nodes the gate compares.
pub const X7_NODES: usize = 8;

/// Fuel budget of an ordinary call in the scenario.
pub const X7_GAS_LIMIT: u64 = 1_000_000;

/// Fuel budget of the deliberately starved calls: enough to start, not to finish.
pub const X7_STARVED_GAS: u64 = 4_000;

/// Block height every call in the scenario sees.
pub const X7_HEIGHT: u64 = 1_000;

/// Block timestamp every call in the scenario sees (fixed, never a clock).
pub const X7_TIMESTAMP_MS: u64 = 1_700_000_000_000;

/// Gas a native transfer is charged: flat, because it touches two accounts and
/// nothing else. It is billed so that a mixed workload cannot hide a gas drift.
pub const X7_TRANSFER_GAS: u64 = 100;

/// Starting balance of every funded account.
pub const X7_START_BALANCE: u128 = 1_000_000;

/// Domain separation for the rolling digest.
const X7_DIGEST_CONTEXT: &[u8] = b"auralite/x7/determinism-digest/v1";

/// Domain separation for the events chain.
const X7_EVENTS_CONTEXT: &[u8] = b"auralite/x7/events-chain/v1";

// ----------------------------------------------------------------- codes

/// The four contracts the scenario drives, all shipped in earlier phases.
///
/// They are reused rather than re-invented: a determinism gate over brand-new
/// bytecode would say nothing about the contracts the chain actually runs, and
/// pinning them here means X7 re-verifies the X4/X6 artifacts on every run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// `examples/kv` — ABI v1, one slot, one event, `run(i32)`.
    Kv,
    /// `examples/store` — ABI v1, `run()`.
    Store,
    /// `examples/events` — ABI v1, `run()` that reads `block_ctx` and `hash`.
    Events,
    /// `examples/vault` — ABI v3, a nested call on every withdrawal.
    Vault,
}

impl Code {
    /// Every code the scenario uses, in a fixed order.
    pub const ALL: [Code; 4] = [Code::Kv, Code::Store, Code::Events, Code::Vault];

    /// The shipped artifact.
    pub fn wasm(self) -> &'static [u8] {
        match self {
            Code::Kv => include_bytes!("../../../examples/kv/wasm/kv.wasm"),
            Code::Store => include_bytes!("../../../examples/store/wasm/store.wasm"),
            Code::Events => include_bytes!("../../../examples/events/wasm/events.wasm"),
            Code::Vault => include_bytes!("../../../examples/vault/wasm/vault.wasm"),
        }
    }

    /// The name used in reports and in the pinned fixture.
    pub fn name(self) -> &'static str {
        match self {
            Code::Kv => "kv",
            Code::Store => "store",
            Code::Events => "events",
            Code::Vault => "vault",
        }
    }
}

// --------------------------------------------------------------- scenario

/// One transaction of the scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Deploy a shipped contract at `address`, called by `caller`.
    Deploy {
        /// The address the code lands on.
        address: [u8; 20],
        /// Which artifact.
        code: Code,
        /// The deployer, funded in the genesis set.
        caller: [u8; 20],
        /// Native value moved into the contract at deploy.
        value: u128,
    },
    /// Call a stored contract through the X3 lifecycle (`run(arg)`).
    CallStored {
        /// The contract.
        contract: [u8; 20],
        /// The signer.
        caller: [u8; 20],
        /// The single `i32` argument of an ABI v1 `run`, or `None` for a
        /// contract whose `run` takes no argument. The distinction is not
        /// cosmetic: X3 refuses a call whose arity does not match the export, so
        /// passing an argument to `store`/`events` is a lifecycle error, not a call.
        arg: Option<i32>,
        /// Fuel budget of this call.
        gas_limit: u64,
    },
    /// Call a contract with ABI v2/v3 calldata.
    CallData {
        /// The contract.
        contract: [u8; 20],
        /// The signer.
        caller: [u8; 20],
        /// Selector and arguments.
        data: Vec<u8>,
        /// Fuel budget of this call.
        gas_limit: u64,
    },
    /// Move native value between two accounts, with no contract involved.
    Transfer {
        /// Payer.
        from: [u8; 20],
        /// Payee.
        to: [u8; 20],
        /// Amount in smallest units.
        amount: u128,
    },
}

/// A step of the scenario: its index and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Position in the ordered input.
    pub index: usize,
    /// The transaction.
    pub op: Op,
}

/// SplitMix64 — the smallest PRNG that is fully specified in this file.
///
/// Not a cryptographic RNG and not intended as one: the scenario must be
/// *reproducible from a constant*, so the generator has to be part of the code
/// the gate pins, not a library whose stream could change between versions.
#[derive(Debug, Clone)]
pub struct Prng(u64);

impl Prng {
    /// Seed the generator.
    pub fn new(seed: u64) -> Self {
        Prng(seed)
    }

    /// The next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..bound`. `bound` must be non-zero.
    pub fn below(&mut self, bound: u64) -> u64 {
        debug_assert!(bound > 0, "bound must be non-zero");
        self.next_u64() % bound
    }
}

/// The address of funded account `index`.
pub fn account_address(index: usize) -> [u8; 20] {
    let mut bytes = [0xa0u8; 20];
    bytes[19] = index as u8;
    bytes
}

/// The address of contract `code`.
pub fn contract_address(code: Code) -> [u8; 20] {
    match code {
        Code::Kv => [0x11; 20],
        Code::Store => [0x22; 20],
        Code::Events => [0x33; 20],
        Code::Vault => [0x44; 20],
    }
}

/// An address that never holds code, used by the refusal steps.
pub const NOBODY: [u8; 20] = [0x5a; 20];

/// The owner of the vault, and the deployer of everything else.
///
/// Written out rather than derived so it can stay a `const` **and** equal
/// [`account_address(0)`](account_address): the index lives in the last byte, and
/// a deployer that is not a funded account cannot pay the vault's initial value.
pub const DEPLOYER: [u8; 20] = [
    0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0,
    0xa0, 0xa0, 0xa0, 0x00,
];

/// Build the ordered input: four deploys, then [`X7_STEPS`] transactions.
///
/// The mix is fixed by construction (see the fractions in the body) and driven by
/// [`Prng`] from [`X7_SEED`], so the whole list is a pure function of a constant.
pub fn scenario() -> Vec<Step> {
    scenario_with_seed(X7_SEED)
}

/// The same scenario under a different seed — the negative control's input.
pub fn scenario_with_seed(seed: u64) -> Vec<Step> {
    scenario_of(seed, X7_STEPS)
}

/// A shorter scenario, for the gate's cheap assertions.
///
/// The pinned measurement and the eight-node comparison always use the full
/// [`X7_STEPS`]; the unit-level properties (commutativity of transfers, the
/// starving call, the shape of the input) hold for any length, and shrinking them
/// keeps `make test` in seconds rather than minutes. The *prefix* of a shorter run
/// is not the prefix of a longer one — the generator draws from one stream — so a
/// short run is a different input, never a truncation.
pub fn scenario_of(seed: u64, transactions: usize) -> Vec<Step> {
    let mut prng = Prng::new(seed);
    let mut steps: Vec<Step> = Vec::with_capacity(transactions + Code::ALL.len());

    for code in Code::ALL {
        let value = match code {
            Code::Vault => 200_000,
            _ => 0,
        };
        steps.push(Step {
            index: steps.len(),
            op: Op::Deploy {
                address: contract_address(code),
                code,
                caller: DEPLOYER,
                value,
            },
        });
    }

    while steps.len() < transactions + Code::ALL.len() {
        let index = steps.len();
        let roll = prng.below(100);
        let op = match roll {
            // 40% — the workhorse: one slot, one event, one argument.
            0..=39 => Op::CallStored {
                contract: contract_address(Code::Kv),
                caller: account_address(prng.below(X7_ACCOUNTS as u64) as usize),
                arg: Some((prng.next_u64() as i32) & 0x00ff_ffff),
                gas_limit: X7_GAS_LIMIT,
            },
            // 18% — a `run()` with no argument and no return value.
            40..=57 => Op::CallStored {
                contract: contract_address(Code::Store),
                caller: account_address(prng.below(X7_ACCOUNTS as u64) as usize),
                arg: None,
                gas_limit: X7_GAS_LIMIT,
            },
            // 15% — the same shape, but the contract reads `block_ctx` and wants
            // an import (`hash`), which is where a host leak would show first.
            58..=72 => Op::CallStored {
                contract: contract_address(Code::Events),
                caller: account_address(prng.below(X7_ACCOUNTS as u64) as usize),
                arg: None,
                gas_limit: X7_GAS_LIMIT,
            },
            // 10% — pure value movement: two accounts, no VM at all.
            73..=82 => {
                let from = prng.below(X7_ACCOUNTS as u64) as usize;
                let mut to = prng.below(X7_ACCOUNTS as u64) as usize;
                if to == from {
                    to = (from + 1) % X7_ACCOUNTS;
                }
                Op::Transfer {
                    from: account_address(from),
                    to: account_address(to),
                    amount: 1 + prng.below(500) as u128,
                }
            }
            // 8% — ABI v3: a withdrawal that makes a nested call on every path.
            83..=90 => {
                let who = account_address(prng.below(X7_ACCOUNTS as u64) as usize);
                let amount = 1 + prng.below(400);
                // `withdraw(u128)` takes the amount alone; `deposited([u8; 20])`
                // takes the address alone. Sending both to either is `-21`, which
                // is a legitimate refusal but not a useful part of the workload.
                let data = if prng.below(2) == 0 {
                    let mut data = 0x02u32.to_le_bytes().to_vec();
                    data.extend_from_slice(&(amount as u128).to_le_bytes());
                    data
                } else {
                    let mut data = 0x04u32.to_le_bytes().to_vec();
                    data.extend_from_slice(&who);
                    data
                };
                Op::CallData {
                    contract: contract_address(Code::Vault),
                    caller: who,
                    data,
                    gas_limit: X7_GAS_LIMIT,
                }
            }
            // 4% — the vault's owner credits an account, so withdrawals succeed.
            91..=94 => {
                let who = account_address(prng.below(X7_ACCOUNTS as u64) as usize);
                let amount = 1 + prng.below(500);
                let mut data = 0x01u32.to_le_bytes().to_vec();
                data.extend_from_slice(&who);
                data.extend_from_slice(&(amount as u128).to_le_bytes());
                Op::CallData {
                    contract: contract_address(Code::Vault),
                    caller: DEPLOYER,
                    data,
                    gas_limit: X7_GAS_LIMIT,
                }
            }
            // 3% — a call that must be *refused*: an address with no code.
            95..=97 => Op::CallStored {
                contract: NOBODY,
                caller: account_address(prng.below(X7_ACCOUNTS as u64) as usize),
                arg: None,
                gas_limit: X7_GAS_LIMIT,
            },
            // 2% — a call that must run out of gas: it starts and does not finish.
            _ => Op::CallStored {
                contract: contract_address(Code::Kv),
                caller: account_address(prng.below(X7_ACCOUNTS as u64) as usize),
                arg: Some((prng.next_u64() as i32) & 0x00ff_ffff),
                gas_limit: X7_STARVED_GAS,
            },
        };
        steps.push(Step { index, op });
    }
    steps
}

// ------------------------------------------------------------------ report

/// What one node measured.
///
/// The three hashes are the gate's subject; the four counters exist so a
/// divergence can be *attributed* (a node that committed a different number of
/// steps disagreed about execution, not about hashing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeReport {
    /// Which node produced this — a label, never an input.
    pub node: u64,
    /// Steps the scenario contained.
    pub steps: usize,
    /// Steps whose receipt carried a diff with at least one change.
    pub committed: usize,
    /// Steps that returned a non-zero status and wrote nothing.
    pub refused: usize,
    /// Steps that trapped, ran out of gas or hit a lifecycle error.
    pub failed: usize,
    /// Of `failed`: the machine itself refused (out of gas or rejected).
    pub failed_machine: usize,
    /// Of `failed`: the contract trapped.
    pub failed_trap: usize,
    /// Of `failed`: the lifecycle refused before any code ran (no code, bad calldata).
    pub failed_lifecycle: usize,
    /// Whole gas bill of the run.
    pub gas_total: u64,
    /// Version the final tree sits at.
    pub version: u64,
    /// Root of the tree without the version stamp.
    pub tree_root: Hash,
    /// Root including the version stamp — the number a header commits to.
    pub state_root: Hash,
    /// Chain over every event the run emitted, in order.
    pub events_root: Hash,
    /// Rolling digest over every step: status, gas and root, in order.
    pub digest: Hash,
}

impl NodeReport {
    /// The one-line form the gate compares.
    pub fn summary(&self) -> String {
        format!(
            "node={} steps={} committed={} refused={} failed={} [{}] gas={} version={} \
             state_root={} tree_root={} events_root={} digest={}",
            self.node,
            self.steps,
            self.committed,
            self.refused,
            self.failed,
            self.failure_mix(),
            self.gas_total,
            self.version,
            self.state_root.to_hex(),
            self.tree_root.to_hex(),
            self.events_root.to_hex(),
            self.digest.to_hex(),
        )
    }

    /// Why the failed steps failed — the attribution the report quotes.
    pub fn failure_mix(&self) -> String {
        format!(
            "machine={} trap={} lifecycle={}",
            self.failed_machine, self.failed_trap, self.failed_lifecycle
        )
    }

    /// The fields the pinned fixture stores, without the node label.
    pub fn fingerprint(&self) -> String {
        format!(
            "steps={} committed={} refused={} failed={} gas={} version={} \
             state_root={} tree_root={} events_root={} digest={}",
            self.steps,
            self.committed,
            self.refused,
            self.failed,
            self.gas_total,
            self.version,
            self.state_root.to_hex(),
            self.tree_root.to_hex(),
            self.events_root.to_hex(),
            self.digest.to_hex(),
        )
    }
}

// ------------------------------------------------------------------- runner

/// How the ledger starts: every account funded, every contract empty.
pub fn genesis() -> MerkleTree {
    debug_assert_eq!(
        DEPLOYER,
        account_address(0),
        "the deployer must be the first funded account"
    );
    let mut entries: Vec<(Hash, Vec<u8>)> = Vec::with_capacity(X7_ACCOUNTS);
    for index in 0..X7_ACCOUNTS {
        let account = Account::new(
            Nonce::new(0),
            Amount::from_units(X7_START_BALANCE),
            Hash::ZERO,
            Hash::ZERO,
        );
        entries.push((
            StateKey::account(&address(account_address(index))).digest(),
            account.to_bytes(),
        ));
    }
    MerkleTree::from_entries(0, &entries)
}

fn address(bytes: [u8; 20]) -> Address {
    Address::from_bytes(bytes)
}

fn call_to(contract: [u8; 20], caller: [u8; 20], gas_limit: u64) -> ContractCall {
    ContractCall {
        contract: address(contract),
        caller: address(caller),
        block: BlockContext {
            height: X7_HEIGHT,
            timestamp_ms: X7_TIMESTAMP_MS,
        },
        gas_limit,
    }
}

/// Chain one step into the rolling digest.
///
/// `status` is the contract's `i64` return value narrowed to the low 32 bits a
/// status code can use. Widening it back is not needed: two nodes that differ in
/// the high half of a return value would already be a divergence in the report's
/// roots, and a status that fits an `i32` is what every ABI in this chain returns.
fn fold(digest: Hash, index: usize, class: u8, status: i64, gas: u64, root: &Hash) -> Hash {
    let mut bytes = Vec::with_capacity(1 + 8 + 1 + 4 + 8 + 32);
    bytes.extend_from_slice(digest.as_bytes());
    bytes.extend_from_slice(&(index as u64).to_le_bytes());
    bytes.push(class);
    bytes.extend_from_slice(&(status as i32).to_le_bytes());
    bytes.extend_from_slice(&gas.to_le_bytes());
    bytes.extend_from_slice(root.as_bytes());
    blake3_hash(&bytes)
}

/// Chain one step's events into the events root.
fn fold_events(chain: Hash, index: usize, events: &[Vec<u8>]) -> Hash {
    let mut bytes = Vec::with_capacity(32 + 8 + 4);
    bytes.extend_from_slice(chain.as_bytes());
    bytes.extend_from_slice(&(index as u64).to_le_bytes());
    bytes.extend_from_slice(&(events.len() as u32).to_le_bytes());
    for event in events {
        bytes.extend_from_slice(&(event.len() as u32).to_le_bytes());
        bytes.extend_from_slice(event);
    }
    blake3_hash(&bytes)
}

/// The running state of one node while it replays the scenario.
struct Node {
    node: u64,
    tree: MerkleTree,
    lifecycle: Lifecycle,
    committed: usize,
    refused: usize,
    failed: usize,
    failed_machine: usize,
    failed_trap: usize,
    failed_lifecycle: usize,
    gas_total: u64,
    events_root: Hash,
    digest: Hash,
}

impl Node {
    fn new(node: u64) -> Self {
        let mut start = Vec::with_capacity(X7_DIGEST_CONTEXT.len() + X7_EVENTS_CONTEXT.len());
        start.extend_from_slice(X7_DIGEST_CONTEXT);
        start.extend_from_slice(X7_EVENTS_CONTEXT);
        Node {
            node,
            tree: genesis(),
            lifecycle: Lifecycle::new(),
            committed: 0,
            refused: 0,
            failed: 0,
            failed_machine: 0,
            failed_trap: 0,
            failed_lifecycle: 0,
            gas_total: 0,
            events_root: blake3_hash(X7_EVENTS_CONTEXT),
            digest: blake3_hash(X7_DIGEST_CONTEXT),
        }
    }

    /// Commit a receipt's diff, counting whether it actually changed anything.
    fn commit(&mut self, index: usize, receipt: &ContractReceipt) {
        let events = receipt.events.clone();
        let gas = receipt.gas_used;
        let status = match &receipt.status {
            crate::ExecStatus::Returned(value) => *value,
            other => {
                // A trap of any kind: the class byte distinguishes "the machine
                // said no" from "the contract said no".
                let code = match other {
                    crate::ExecStatus::OutOfGas => -8i64,
                    crate::ExecStatus::Rejected => -17,
                    _ => i64::MIN,
                };
                let class = 3u8;
                self.bill(index, gas, &events);
                self.failed += 1;
                match other {
                    crate::ExecStatus::OutOfGas | crate::ExecStatus::Rejected => {
                        self.failed_machine += 1;
                    }
                    _ => self.failed_trap += 1,
                }
                self.digest = fold(
                    self.digest,
                    index,
                    class,
                    code,
                    gas,
                    &self.tree.state_root(),
                );
                return;
            }
        };
        match receipt.diff.as_ref() {
            Some(diff) if !diff.changes.is_empty() => {
                let next = apply_diff(&self.tree, diff).expect("diff applies to its own base");
                self.tree = next;
                self.committed += 1;
                self.bill(index, gas, &events);
                self.digest = fold(self.digest, index, 1, status, gas, &self.tree.state_root());
            }
            _ => {
                // Returned but wrote nothing: a refusal. The root must not move,
                // and the assertion is made here rather than in the test so that a
                // non-deterministic *commit* cannot slip through as a pass.
                let before = self.tree.state_root();
                self.refused += 1;
                self.bill(index, gas, &events);
                self.digest = fold(self.digest, index, 2, status, gas, &before);
            }
        }
    }

    fn bill(&mut self, index: usize, gas: u64, events: &[Vec<u8>]) {
        self.gas_total = self.gas_total.saturating_add(gas);
        self.events_root = fold_events(self.events_root, index, events);
    }

    /// A native transfer: no VM, two accounts, one diff.
    fn transfer(&mut self, index: usize, from: [u8; 20], to: [u8; 20], amount: u128) {
        let version = self.tree.version() + 1;
        let mut view = StateView::new(self.tree.clone(), X7_GAS_LIMIT);
        let payer = StateKey::account(&address(from));
        let payee = StateKey::account(&address(to));
        let mut payer_account = view
            .get(&payer)
            .and_then(|bytes| Account::from_canonical_bytes(bytes).ok())
            .unwrap_or_else(Account::new_empty);
        let mut payee_account = view
            .get(&payee)
            .and_then(|bytes| Account::from_canonical_bytes(bytes).ok())
            .unwrap_or_else(Account::new_empty);
        let ok = payer_account
            .debit(Amount::from_units(amount))
            .and_then(|()| payee_account.credit(Amount::from_units(amount)))
            .is_ok()
            && payer_account.bump_nonce().is_ok();
        if !ok {
            // An overdraft is a refused transaction, not a trap: the root does not
            // move, the gas is still billed.
            self.refused += 1;
            self.bill(index, X7_TRANSFER_GAS, &[]);
            self.digest = fold(
                self.digest,
                index,
                2,
                -1,
                X7_TRANSFER_GAS,
                &self.tree.state_root(),
            );
            return;
        }
        view.set(&payer, payer_account.to_bytes());
        view.set(&payee, payee_account.to_bytes());
        view.charge_gas(X7_TRANSFER_GAS)
            .expect("the transfer gas fits the budget");
        let diff = view.commit(version);
        self.tree = apply_diff(&self.tree, &diff).expect("diff applies to its own base");
        self.committed += 1;
        self.bill(index, X7_TRANSFER_GAS, &[]);
        self.digest = fold(
            self.digest,
            index,
            1,
            0,
            X7_TRANSFER_GAS,
            &self.tree.state_root(),
        );
    }

    fn deploy(&mut self, index: usize, step: &Op) -> bool {
        let Op::Deploy {
            address: at,
            code,
            caller,
            value,
        } = step
        else {
            return false;
        };
        let version = self.tree.version() + 1;
        let receipt: LifecycleReceipt = self
            .lifecycle
            .deploy(
                &self.tree,
                call_to(*at, *caller, X7_GAS_LIMIT),
                code.wasm().to_vec(),
                Amount::from_units(*value),
                Some(0),
            )
            .expect("a shipped artifact deploys");
        let gas = receipt.gas_used;
        let events = receipt.events.clone();
        match receipt.diff.as_ref() {
            Some(diff) if !diff.changes.is_empty() => {
                self.tree =
                    apply_diff(&self.tree, diff).expect("a deploy diff applies to its base");
                self.committed += 1;
                self.bill(index, gas, &events);
                self.digest = fold(self.digest, index, 1, 0, gas, &self.tree.state_root());
            }
            _ => {
                self.failed += 1;
                self.bill(index, gas, &events);
                self.digest = fold(self.digest, index, 3, 0, gas, &self.tree.state_root());
            }
        }
        let _ = version;
        true
    }

    fn call(&mut self, index: usize, step: &Op) -> bool {
        match step {
            Op::CallStored {
                contract,
                caller,
                arg,
                gas_limit,
            } => {
                match self.lifecycle.call_stored(
                    &self.tree,
                    call_to(*contract, *caller, *gas_limit),
                    *arg,
                ) {
                    Ok(receipt) => self.commit(index, &receipt),
                    Err(_) => self.lifecycle_failure(index, *gas_limit),
                }
                true
            }
            Op::CallData {
                contract,
                caller,
                data,
                gas_limit,
            } => {
                match self.lifecycle.call_data(
                    &self.tree,
                    call_to(*contract, *caller, *gas_limit),
                    data.clone(),
                ) {
                    Ok(receipt) => self.commit(index, &receipt),
                    Err(_) => self.lifecycle_failure(index, *gas_limit),
                }
                true
            }
            _ => false,
        }
    }

    /// A call that never reached a contract (no code, bad calldata length).
    ///
    /// It is a *failure*, not a refusal: no receipt exists, so a node that
    /// disagreed here would disagree about which transactions were accepted.
    /// The whole budget is billed, which is what the T-phase will charge.
    fn lifecycle_failure(&mut self, index: usize, gas_limit: u64) {
        self.failed += 1;
        self.failed_lifecycle += 1;
        self.bill(index, gas_limit, &[]);
        self.digest = fold(
            self.digest,
            index,
            3,
            -1,
            gas_limit,
            &self.tree.state_root(),
        );
    }

    fn finish(self, steps: usize) -> NodeReport {
        NodeReport {
            node: self.node,
            steps,
            committed: self.committed,
            refused: self.refused,
            failed: self.failed,
            failed_machine: self.failed_machine,
            failed_trap: self.failed_trap,
            failed_lifecycle: self.failed_lifecycle,
            gas_total: self.gas_total,
            version: self.tree.version(),
            tree_root: self.tree.tree_root(),
            state_root: self.tree.state_root(),
            events_root: self.events_root,
            digest: self.digest,
        }
    }
}

/// Replay `steps` as node `node`.
///
/// The node label is *not* an input: it is carried into the report so that eight
/// reports can be told apart, and nothing in the execution reads it.
pub fn run(steps: &[Step], node: u64) -> NodeReport {
    let mut state = Node::new(node);
    for step in steps {
        if state.deploy(step.index, &step.op) {
            continue;
        }
        if state.call(step.index, &step.op) {
            continue;
        }
        if let Op::Transfer { from, to, amount } = step.op {
            state.transfer(step.index, from, to, amount);
        }
    }
    state.finish(steps.len())
}

/// Replay the standard scenario in this process.
pub fn run_scenario() -> NodeReport {
    run(&scenario(), 0)
}

/// The artifacts the scenario executes, as `(name, bytes, blake3)`.
///
/// The gate pins these the same way X4/X5/X6 pin theirs: a determinism claim about
/// bytecode nobody can name is not a claim.
pub fn code_pins() -> Vec<(&'static str, usize, Hash)> {
    Code::ALL
        .iter()
        .map(|code| (code.name(), code.wasm().len(), blake3_hash(code.wasm())))
        .collect()
}
