//! E8 example: replays the 50 standard vectors and the DAO scenario,
//! renders the governance UI page and prints the pinned output that
//! `tools/e8_nft_check.py` must reproduce byte for byte:
//!
//! ```text
//! E8-VECTORS-OK 50
//! E8-UI-OK <sha256 of governance-ui.html>
//! E8-SIM-OK {"vectors": 50, ...}
//! ```
//!
//! Run: `cargo run --quiet --locked -p aura-storage --example e8_nft_example`

use std::collections::BTreeMap;
use std::fs;

use aura_storage::dao_ui::{run_e8_scenario, scenario_digest};
use aura_storage::nft::{E8Error, NftWorld};

const FIXTURE: &str = include_str!("../tests/fixtures/e8-standards.txt");
const VECTORS_DIGEST: &str = "8cb9da945b0eed8f4432d31b71d20fffb72a6317fc90204d1890bd480c062bd3";

struct Vector {
    id: String,
    op: String,
    args: BTreeMap<String, String>,
    status: String,
    err: String,
    reads: BTreeMap<String, String>,
}

fn parse() -> (String, Vec<Vector>) {
    let mut digest = String::new();
    let mut vectors: Vec<Vector> = Vec::new();
    let mut cur: Option<Vector> = None;
    for line in FIXTURE.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line.split_once('=').expect("key=value");
        match k {
            "digest" if cur.is_none() => digest = String::from(v),
            "vector" => {
                if let Some(done) = cur.take() {
                    vectors.push(done);
                }
                cur = Some(Vector {
                    id: String::from(v),
                    op: String::new(),
                    args: BTreeMap::new(),
                    status: String::new(),
                    err: String::new(),
                    reads: BTreeMap::new(),
                });
            }
            "op" => cur.as_mut().unwrap().op = String::from(v),
            "status" => cur.as_mut().unwrap().status = String::from(v),
            "err" => cur.as_mut().unwrap().err = String::from(v),
            other => {
                let block = cur.as_mut().unwrap();
                if let Some(read) = other.strip_prefix("read.") {
                    block.reads.insert(String::from(read), String::from(v));
                } else {
                    block.args.insert(String::from(other), String::from(v));
                }
            }
        }
    }
    if let Some(done) = cur.take() {
        vectors.push(done);
    }
    (digest, vectors)
}

fn run_vector(w: &mut NftWorld, v: &Vector) -> Result<(), E8Error> {
    let a = |key: &str| -> &str { v.args.get(key).map(String::as_str).unwrap() };
    match v.op.as_str() {
        "mint721" => w.mint721(a("by"), a("to"), a("id").parse().unwrap()),
        "transfer721" => w.transfer721(a("by"), a("id").parse().unwrap(), a("to")),
        "approve721" => w.approve721(a("by"), a("id").parse().unwrap(), a("spender")),
        "transfer_from721" => w.transfer_from721(a("by"), a("id").parse().unwrap(), a("to")),
        "burn721" => w.burn721(a("by"), a("id").parse().unwrap()),
        "read_owner_of" => w.owner_of(a("id").parse().unwrap()).map(|_| ()),
        "mint1155" => w.mint1155(
            a("by"),
            a("to"),
            a("id").parse().unwrap(),
            a("amount").parse().unwrap(),
        ),
        "transfer1155" => w.transfer1155(
            a("frm"),
            a("by"),
            a("id").parse().unwrap(),
            a("to"),
            a("amount").parse().unwrap(),
        ),
        "set_operator" => w.set_operator(a("by"), a("operator"), a("approved") == "true"),
        "transfer_batch1155" => {
            let ids: Vec<u64> = a("ids").split(',').map(|s| s.parse().unwrap()).collect();
            let amounts: Vec<u128> = a("amounts")
                .split(',')
                .map(|s| s.parse().unwrap())
                .collect();
            w.transfer_batch1155(a("frm"), a("by"), a("to"), &ids, &amounts)
        }
        "register_name" => w.register_name(a("by"), a("name"), a("day").parse().unwrap()),
        "renew_name" => w.renew_name(a("name"), a("day").parse().unwrap()),
        "transfer_name" => w.transfer_name(a("by"), a("name"), a("to"), a("day").parse().unwrap()),
        "set_target" => w.set_target(a("by"), a("name"), a("target"), a("day").parse().unwrap()),
        "resolve" => w.resolve(a("name"), a("day").parse().unwrap()).map(|_| ()),
        "set_primary" => w.set_primary(a("by"), a("name"), a("day").parse().unwrap()),
        "advance_day" => {
            w.advance_day(a("day").parse().unwrap());
            Ok(())
        }
        "set_identity" => {
            w.set_identity(a("by"), a("name"), a("digest"), a("day").parse().unwrap())
        }
        "clear_identity" => w.clear_identity(a("by"), a("name")),
        "read_identity" => w.identity(a("name")).map(|_| ()),
        other => panic!("unknown op {other}"),
    }
}

fn main() {
    // 1. The 50 standard vectors, exactly as the suite pins them.
    let (fixture_digest, vectors) = parse();
    assert_eq!(fixture_digest, VECTORS_DIGEST);
    let mut world = NftWorld::new();
    for v in &vectors {
        let result = run_vector(&mut world, v);
        match v.status.as_str() {
            "ok" => assert!(result.is_ok(), "{}: {:?}", v.id, result.err()),
            "err" => assert_eq!(result.unwrap_err().to_string(), v.err, "{}", v.id),
            other => panic!("bad status {other}"),
        }
        for (key, want) in &v.reads {
            assert_eq!(&world.query(key), want, "{}: {key}", v.id);
        }
    }
    println!("E8-VECTORS-OK {}", vectors.len());

    // 2. The DAO scenario: the first vote through the UI.
    let run = run_e8_scenario();
    assert_eq!(
        scenario_digest(&run.steps),
        "17fd44850375c54c9cf9826b8ed55da0faf1855da72514184c83501f07409d71"
    );
    assert_eq!(run.ui_votes, 4);
    assert_eq!(run.state, "executed");

    // 3. The governance UI page, written into the evidence tier.
    let mut ui = aura_storage::dao_ui::DaoUi::new(
        vec![
            250_000 * aura_storage::issuance::ALCRY,
            150_000 * aura_storage::issuance::ALCRY,
            100_000 * aura_storage::issuance::ALCRY,
            75_000 * aura_storage::issuance::ALCRY,
            25_000 * aura_storage::issuance::ALCRY,
        ],
        ["alice", "bob", "carol", "dave", "eve"]
            .iter()
            .map(|s| String::from(*s))
            .collect(),
    );
    let id = ui.propose(0, 1000).unwrap();
    ui.vote(id, 1, aura_storage::governance::Support::Yes, 2000)
        .unwrap();
    ui.vote(id, 2, aura_storage::governance::Support::Yes, 3000)
        .unwrap();
    ui.vote(id, 3, aura_storage::governance::Support::No, 4000)
        .unwrap();
    ui.vote(id, 4, aura_storage::governance::Support::Abstain, 5000)
        .unwrap();
    ui.finalize(id, 121_960).unwrap();
    ui.execute(id, 156_520).unwrap();
    let html = ui.render_html();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/evidence/e8/governance-ui.html");
    fs::create_dir_all(out.parent().expect("parent")).expect("mkdir");
    fs::write(&out, html.as_bytes()).expect("write html");
    let sha = aura_state::sha256_bytes(html.as_bytes()).to_hex();
    println!("E8-UI-OK {sha}");

    // 4. The pinned JSON (mirrored by tools/e8_nft_check.py).
    println!(
        "E8-SIM-OK {{\"vectors\": {}, \"vectors_ok\": true, \
         \"first_vote\": [1, 1, \"yes\"], \"ui_votes\": {}, \
         \"tally_yes\": {}, \"tally_no\": {}, \"tally_abstain\": {}, \
         \"state\": \"{}\"}}",
        vectors.len(),
        run.ui_votes,
        run.tally.0 / aura_storage::issuance::ALCRY,
        run.tally.1 / aura_storage::issuance::ALCRY,
        run.tally.2 / aura_storage::issuance::ALCRY,
        run.state
    );
}
