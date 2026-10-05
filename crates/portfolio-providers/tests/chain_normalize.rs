//! Offline normalization fixtures for the stage D networks (TESTING.md
//! Layer A, "Chain normalization"): TRON, TON, Solana and the EVM networks
//! served by Zerion. Fixtures are small, sanitized records shaped like the
//! documented responses observed on 2026-10-05.

mod common;

use common::fast;
use num_bigint::BigInt;
use portfolio_core::network::NetworkId;
use portfolio_providers::http::Budget;
use portfolio_providers::tonapi::{self, Event};
use portfolio_providers::trongrid::{self, NativeTx, Trc20Transfer};
use portfolio_providers::zerion::{Positions, Window, Zerion};
use portfolio_store::ingest::{Decoding, Direction, FeeAttribution, TxStatus, Verification};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ------------------------------------------------------------------ TRON

const TRON: &str = "TT2T17KZhoDu47i2E4FWxfG79zdkEWkU9N";
const TRON_HEX: &str = "41bb1712d44d09feee51d071219b4c5d9792b76b29";
const OTHER_HEX: &str = "41cb41df5a3311069157fc1c5cac4bcf4b0f748b01";
const USDT: &str = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";

fn native(contract_type: &str, value: Value, ret: &str, fee: i64) -> NativeTx {
    serde_json::from_value(json!({
        "ret": [{"contractRet": ret, "fee": fee}],
        "txID": format!("{:064X}", fee + 7),
        "net_usage": 265, "net_fee": 0, "energy_usage": 0, "energy_fee": 0,
        "blockNumber": 86_700_000,
        "block_timestamp": 1_790_761_893_000_i64,
        "raw_data": {"contract": [{"parameter": {"value": value, "type_url": "x"}, "type": contract_type}]},
        "internal_transactions": [],
        "some_future_field": {"ignored": true}
    }))
    .unwrap()
}

#[test]
fn tron_trx_transfers_in_out_and_self() {
    let out = native(
        "TransferContract",
        json!({"amount": 5_000_000, "owner_address": TRON_HEX, "to_address": OTHER_HEX}),
        "SUCCESS",
        1_100_000,
    );
    let spec = trongrid::native_tx_for_account(&out, TRON).unwrap();
    assert_eq!(
        spec.hash,
        spec.hash.to_ascii_lowercase(),
        "hex ids are lowercase"
    );
    assert_eq!(spec.operation, "send");
    assert_eq!(spec.legs.len(), 1);
    assert_eq!(spec.legs[0].signed_raw, BigInt::from(-5_000_000));
    assert_eq!(spec.legs[0].asset.id(), "tron:native");
    assert_eq!(spec.legs[0].asset.decimals, 6);
    // ret.fee is the authoritative total: bandwidth plus account activation.
    let fee = spec.fee.unwrap();
    assert_eq!(fee.raw, BigInt::from(1_100_000));
    assert_eq!(fee.attribution, FeeAttribution::Exact);
    assert_eq!(spec.occurred_at, 1_790_761_893);

    let incoming = native(
        "TransferContract",
        json!({"amount": 8, "owner_address": OTHER_HEX, "to_address": TRON_HEX}),
        "SUCCESS",
        0,
    );
    let spec = trongrid::native_tx_for_account(&incoming, TRON).unwrap();
    assert_eq!(spec.operation, "receive");
    assert_eq!(spec.legs[0].signed_raw, BigInt::from(8));
    assert!(
        spec.legs[0].unresolved,
        "an external receipt needs a basis decision"
    );
    assert!(spec.fee.is_none(), "the sender pays the fee");

    let to_self = native(
        "TransferContract",
        json!({"amount": 1, "owner_address": TRON_HEX, "to_address": TRON_HEX}),
        "SUCCESS",
        0,
    );
    let spec = trongrid::native_tx_for_account(&to_self, TRON).unwrap();
    assert_eq!(spec.legs[0].direction, Direction::SelfTransfer);
    assert_eq!(spec.legs[0].signed_raw, BigInt::from(0));
}

#[test]
fn tron_failed_contract_call_charges_only_the_fee() {
    let failed = native(
        "TriggerSmartContract",
        json!({"data": "a9059cbb0000", "owner_address": TRON_HEX,
               "contract_address": "41a614f803b6fd780986a42c78ec9c7f77e6ded13c", "call_value": 10}),
        "REVERT",
        6_400_000,
    );
    let spec = trongrid::native_tx_for_account(&failed, TRON).unwrap();
    assert_eq!(spec.status, TxStatus::Failed);
    assert!(spec.legs.is_empty(), "a reverted call moves nothing");
    assert_eq!(spec.fee.unwrap().raw, BigInt::from(6_400_000));
}

#[test]
fn tron_token_send_record_carries_the_energy_fee() {
    let send = native(
        "TriggerSmartContract",
        json!({"data": "a9059cbb000000", "owner_address": TRON_HEX,
               "contract_address": "41a614f803b6fd780986a42c78ec9c7f77e6ded13c"}),
        "SUCCESS",
        13_373_400,
    );
    let spec = trongrid::native_tx_for_account(&send, TRON).unwrap();
    assert_eq!(spec.operation, "send");
    assert!(
        spec.legs.is_empty(),
        "the token amount comes from the TRC-20 category"
    );
    assert_eq!(spec.fee.unwrap().raw, BigInt::from(13_373_400));
    assert!(spec.part.is_none());
}

#[test]
fn tron_staking_moves_nothing_and_rewards_are_income() {
    for kind in [
        "FreezeBalanceV2Contract",
        "UnfreezeBalanceV2Contract",
        "WithdrawExpireUnfreezeContract",
        "DelegateResourceContract",
        "UnDelegateResourceContract",
        "VoteWitnessContract",
    ] {
        let tx = native(
            kind,
            json!({"owner_address": TRON_HEX, "balance": 1000}),
            "SUCCESS",
            0,
        );
        let spec = trongrid::native_tx_for_account(&tx, TRON).unwrap();
        assert!(spec.legs.is_empty(), "{kind}");
        assert_eq!(spec.decoding, Decoding::Interpreted, "{kind}");
    }
    let mut claim = native(
        "WithdrawBalanceContract",
        json!({"owner_address": TRON_HEX}),
        "SUCCESS",
        0,
    );
    claim.withdraw_amount = Some(7_681_785);
    let spec = trongrid::native_tx_for_account(&claim, TRON).unwrap();
    assert_eq!(spec.legs[0].signed_raw, BigInt::from(7_681_785));
    assert_eq!(spec.legs[0].leg_type, "staking_reward");
}

#[test]
fn tron_internal_trx_and_out_of_scope_records() {
    let mut call = native(
        "TriggerSmartContract",
        json!({"data": "7ff36ab5", "owner_address": TRON_HEX,
               "contract_address": "41a614f803b6fd780986a42c78ec9c7f77e6ded13c", "call_value": 2_000_000}),
        "SUCCESS",
        345_000,
    );
    call.internal_transactions = serde_json::from_value(json!([
        {"from_address": "41a614f803b6fd780986a42c78ec9c7f77e6ded13c", "to_address": TRON_HEX,
         "data": {"note": "call", "rejected": false, "call_value": {"_": 1_500_000}}},
        {"from_address": "41a614f803b6fd780986a42c78ec9c7f77e6ded13c", "to_address": TRON_HEX,
         "data": {"note": "call", "rejected": true, "call_value": {"_": 9_999}}},
        {"from_address": "41a614f803b6fd780986a42c78ec9c7f77e6ded13c", "to_address": OTHER_HEX,
         "data": {"note": "call", "rejected": false}}
    ]))
    .unwrap();
    let spec = trongrid::native_tx_for_account(&call, TRON).unwrap();
    assert_eq!(spec.operation, "execute");
    let amounts: Vec<BigInt> = spec.legs.iter().map(|l| l.signed_raw.clone()).collect();
    assert_eq!(
        amounts,
        vec![BigInt::from(-2_000_000), BigInt::from(1_500_000)]
    );

    let trc10 = native(
        "TransferAssetContract",
        json!({"owner_address": OTHER_HEX, "to_address": TRON_HEX, "asset_name": "31303032303030", "amount": 5}),
        "SUCCESS",
        0,
    );
    let spec = trongrid::native_tx_for_account(&trc10, TRON).unwrap();
    assert!(spec.legs.is_empty());
    assert_eq!(
        spec.decoding,
        Decoding::Partial,
        "TRC-10 is visible but not interpreted"
    );

    let exotic = native(
        "ExchangeCreateContract",
        json!({"owner_address": TRON_HEX}),
        "SUCCESS",
        0,
    );
    let spec = trongrid::native_tx_for_account(&exotic, TRON).unwrap();
    assert_eq!(spec.decoding, Decoding::RawOnly);
    assert_eq!(spec.operation, "exchangecreate");
}

fn trc20(tx: &str, from: &str, to: &str, value: &str) -> Trc20Transfer {
    serde_json::from_value(json!({
        "transaction_id": tx,
        "token_info": {"symbol": "USDT", "address": USDT, "decimals": 6, "name": "Tether USD"},
        "block_timestamp": 1_790_761_893_000_i64,
        "from": from, "to": to, "type": "Transfer", "value": value
    }))
    .unwrap()
}

#[test]
fn tron_trc20_events_are_components_with_contract_identity() {
    let other = "TN8rTPYz5AvRYGGtzRLdA96nSfoqCy7gtq";
    let tx = "FC476D111024E15F8A9E899253FB34BF5FAE713C67E50F57CBED34BABE9CB584";
    let events = vec![
        trc20(tx, TRON, other, "3123513000000"),
        // Identical event twice in one transaction stays two movements.
        trc20("aa", other, TRON, "5"),
        trc20("aa", other, TRON, "5"),
        // Address poisoning: a zero-value transfer "from" the account.
        trc20("bb", TRON, other, "0"),
        // Not this account.
        trc20("cc", other, other, "7"),
    ];
    let specs = trongrid::trc20_specs_for_account(&events, TRON);
    assert_eq!(specs.len(), 3);
    let send = &specs[0];
    assert_eq!(send.hash, tx.to_ascii_lowercase());
    assert_eq!(send.operation, "send");
    assert!(send.fee.is_none(), "the fee is on the native record");
    assert_eq!(
        send.legs[0].signed_raw,
        BigInt::from(-3_123_513_000_000_i64)
    );
    let asset = &send.legs[0].asset;
    assert_eq!(
        asset.contract.as_deref(),
        Some(USDT),
        "Base58 contract kept exactly"
    );
    assert_eq!(asset.id(), format!("tron:token:{USDT}"));
    assert_eq!(asset.verification, Verification::Unverified);
    assert_ne!(specs[1].part, specs[2].part);
    assert!(specs[1].part.as_deref().unwrap().ends_with(":0"));
    assert!(specs[2].part.as_deref().unwrap().ends_with(":1"));
    // Deterministic: the same page yields the same component keys.
    let again = trongrid::trc20_specs_for_account(&events, TRON);
    assert_eq!(
        specs.iter().map(|s| s.part.clone()).collect::<Vec<_>>(),
        again.iter().map(|s| s.part.clone()).collect::<Vec<_>>()
    );
}

// ------------------------------------------------------------------ TON

const TON_RAW: &str = "0:83dfd552e63729b472fcbcc8c45ebcc6691702558b68ec7527e1ba403a0f31a8";
const PEER: &str = "0:53bff336e1032a6e2c848678cd09f614e742f645f987f49094893c35c6992709";
const MASTER: &str = "0:b113a994b5024a16719f69139328eb759596c38a25f59028b146fecdc3621dfe";
/// The same master in bounceable user-friendly form.
const MASTER_FRIENDLY: &str = "EQCxE6mUtQJKFnGfaROTKOt1lZbDiiX1kCixRv7Nw2Id_sDs";

fn jetton(address: &str) -> Value {
    json!({"address": address, "name": "Tether USD", "symbol": "USD₮", "decimals": 6,
           "verification": "whitelist", "image": "https://example.invalid/x.png"})
}

fn event(id: &str, actions: Value, extra: i64, in_progress: bool) -> Event {
    serde_json::from_value(json!({
        "event_id": id,
        "account": {"address": TON_RAW, "is_scam": false, "is_wallet": true},
        "timestamp": 1_790_930_264,
        "actions": actions,
        "is_scam": false,
        "lt": 107_352_536_000_003_i64,
        "in_progress": in_progress,
        "extra": extra,
        "progress": 1
    }))
    .unwrap()
}

fn ton_transfer(sender: &str, recipient: &str, amount: i64, status: &str) -> Value {
    json!({"type": "TonTransfer", "status": status,
           "TonTransfer": {"sender": {"address": sender}, "recipient": {"address": recipient},
                           "amount": amount, "comment": "hi"},
           "base_transactions": ["1fa5"]})
}

#[test]
fn ton_transfer_and_fee_from_extra() {
    let e = event(
        "AB01",
        json!([ton_transfer(PEER, TON_RAW, 10_000, "ok")]),
        -123,
        false,
    );
    let spec = tonapi::event_for_account(&e, TON_RAW);
    assert_eq!(spec.hash, "ab01");
    assert_eq!(spec.position.as_deref(), Some("107352536000003"));
    assert_eq!(spec.operation, "receive");
    assert_eq!(spec.legs.len(), 1);
    assert_eq!(spec.legs[0].signed_raw, BigInt::from(10_000));
    assert_eq!(spec.legs[0].asset.id(), "ton:native");
    // The receiving contract's compute/storage fee is the account's cost.
    assert_eq!(spec.fee.unwrap().raw, BigInt::from(123));
    assert_eq!(spec.evidence["base_transactions"], json!(["1fa5"]));

    let e = event(
        "ab02",
        json!([ton_transfer(TON_RAW, PEER, 2_000_000_000, "ok")]),
        -2_500_000,
        false,
    );
    let spec = tonapi::event_for_account(&e, TON_RAW);
    assert_eq!(spec.operation, "send");
    assert_eq!(spec.legs[0].signed_raw, BigInt::from(-2_000_000_000_i64));
    assert_eq!(spec.fee.unwrap().raw, BigInt::from(2_500_000));
}

#[test]
fn ton_jetton_is_identified_by_master_not_wallet() {
    let action = json!({"type": "JettonTransfer", "status": "ok", "JettonTransfer": {
        "sender": {"address": PEER}, "recipient": {"address": TON_RAW},
        "senders_wallet": "0:8e46d41d00023ac87e16ccf58ddc1a33f893a6d3c5a218c248dd143e7e993726",
        "recipients_wallet": "0:da4ecce819f637e2fd7fb8ee26c406088db2df84205f215eec6b88611cbb91fd",
        "amount": "1000000000", "jetton": jetton(MASTER_FRIENDLY)}});
    let spec = tonapi::event_for_account(&event("ab03", json!([action]), -3472, false), TON_RAW);
    let leg = &spec.legs[0];
    assert_eq!(
        leg.asset.contract.as_deref(),
        Some(MASTER),
        "friendly master normalized to raw"
    );
    assert_eq!(leg.asset.decimals, 6);
    assert_eq!(leg.asset.verification, Verification::Verified);
    assert_eq!(leg.signed_raw, BigInt::from(1_000_000_000));
    assert_eq!(spec.fee.unwrap().raw, BigInt::from(3472));
}

#[test]
fn ton_event_with_several_messages_and_a_swap() {
    let swap = json!({"type": "JettonSwap", "status": "ok", "JettonSwap": {
        "dex": "stonfi", "amount_in": "", "amount_out": "2500000", "ton_in": 1_000_000_000,
        "user_wallet": {"address": TON_RAW}, "router": {"address": PEER},
        "jetton_master_out": jetton(MASTER)}});
    let tip = ton_transfer(TON_RAW, PEER, 5_000_000, "ok");
    let spec = tonapi::event_for_account(
        &event("ab04", json!([swap, tip]), -61_000_000, false),
        TON_RAW,
    );
    assert_eq!(spec.operation, "trade");
    let legs: Vec<(String, BigInt)> = spec
        .legs
        .iter()
        .map(|l| (l.asset.id(), l.signed_raw.clone()))
        .collect();
    assert_eq!(
        legs,
        vec![
            ("ton:native".into(), BigInt::from(-1_000_000_000_i64)),
            (format!("ton:token:{MASTER}"), BigInt::from(2_500_000)),
            ("ton:native".into(), BigInt::from(-5_000_000)),
        ]
    );
    assert_eq!(spec.fee.unwrap().raw, BigInt::from(61_000_000));
}

#[test]
fn ton_bounced_transfer_moves_nothing_but_costs_fees() {
    // The transfer bounced back: the action failed and only the fees net of
    // the refund remain in `extra`.
    let e = event(
        "ab05",
        json!([ton_transfer(TON_RAW, PEER, 3_000_000_000, "failed")]),
        -1_200_000,
        false,
    );
    let spec = tonapi::event_for_account(&e, TON_RAW);
    assert!(spec.legs.is_empty());
    assert_eq!(spec.fee.unwrap().raw, BigInt::from(1_200_000));
    assert_eq!(spec.evidence["failed_actions"], json!(1));

    // A positive `extra` is TON returned to the account beyond the actions.
    let e = event("ab06", json!([]), 40_000, false);
    let spec = tonapi::event_for_account(&e, TON_RAW);
    assert!(spec.fee.is_none());
    assert_eq!(spec.legs[0].leg_type, "refund");
    assert_eq!(spec.legs[0].signed_raw, BigInt::from(40_000));
}

#[test]
fn ton_unfinished_trace_is_pending_and_unknown_actions_partial() {
    let e = event(
        "ab07",
        json!([ton_transfer(PEER, TON_RAW, 1, "ok")]),
        0,
        true,
    );
    let spec = tonapi::event_for_account(&e, TON_RAW);
    assert_eq!(spec.status, TxStatus::Pending);
    assert_eq!(spec.decoding, Decoding::Partial);

    let nft = json!({"type": "NftItemTransfer", "status": "ok",
                     "NftItemTransfer": {"sender": {"address": PEER}, "recipient": {"address": TON_RAW}, "nft": PEER}});
    let spec = tonapi::event_for_account(&event("ab08", json!([nft]), -10, false), TON_RAW);
    assert!(spec.legs.is_empty(), "NFTs are deferred");
    assert_eq!(spec.decoding, Decoding::Partial);
    assert_eq!(spec.operation, "nft");
}

// ------------------------------------------------------------------ Zerion: Solana and EVM

fn position(
    chain: &str,
    symbol: &str,
    address: Option<&str>,
    int: &str,
    decimals: u32,
    trash: bool,
) -> Value {
    json!({
        "type": "positions", "id": format!("{symbol}-{chain}"),
        "attributes": {
            "position_type": "wallet",
            "quantity": {"int": int, "decimals": decimals, "float": 0.0, "numeric": "0"},
            "fungible_info": {"name": symbol, "symbol": symbol, "flags": {"verified": !trash},
                "implementations": [{"chain_id": chain, "address": address, "decimals": decimals}]},
            "flags": {"displayable": true, "is_trash": trash},
            "updated_at_block": 453_468_127
        },
        "relationships": {"chain": {"data": {"type": "chains", "id": chain}}}
    })
}

fn zerion(server: &MockServer) -> Zerion {
    Zerion::with_config(&server.uri(), "zk_test", Budget::unlimited(), fast()).unwrap()
}

const SOL: &str = "86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY";
const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

#[tokio::test]
async fn solana_positions_keep_case_and_merge_token_accounts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/wallets/{SOL}/positions/")))
        .and(query_param("filter[chain_ids]", "solana"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            position("solana", "SOL", None, "1328377689846", 9, false),
            position("solana", "USDC", Some(USDC_MINT), "1000000", 6, false),
            // A second token account of the same mint is the same holding.
            position("solana", "USDC", Some(USDC_MINT), "250000", 6, false),
            position("ethereum", "ETH", None, "1", 18, false)
        ]})))
        .mount(&server)
        .await;
    let Positions::Ready(p) = zerion(&server)
        .positions(NetworkId::Solana, SOL)
        .await
        .unwrap()
    else {
        panic!("indexing")
    };
    assert_eq!(p.len(), 2, "other networks never leak in");
    assert_eq!(p[0].asset.id(), "solana:native");
    assert_eq!(p[0].asset.decimals, 9);
    assert_eq!(p[1].asset.contract.as_deref(), Some(USDC_MINT));
    assert_eq!(p[1].raw, BigInt::from(1_250_000));
}

#[tokio::test]
async fn solana_history_keeps_signatures_and_charges_fees_to_the_signer() {
    let server = MockServer::start().await;
    let sig =
        "41UWUMm9wAf81STUNBoEguJgdiViSuyPcaTPQwQxyF3J668RrquvH5XCLGCoxSVC8yjzo2YVhn68DhP6s6YaeEjx";
    let sol_info = json!({"name": "Solana", "symbol": "SOL", "flags": {"verified": true},
        "implementations": [{"chain_id": "solana", "address": "", "decimals": 9}]});
    let usdc_info = json!({"name": "USDC", "symbol": "USDC", "flags": {"verified": true},
        "implementations": [{"chain_id": "solana", "address": USDC_MINT, "decimals": 6}]});
    let tx = |hash: &str, op: &str, status: &str, sent_from: &str, transfers: Value| {
        json!({"type": "transactions", "id": hash, "attributes": {
            "operation_type": op, "hash": hash, "mined_at_block": 453_404_108,
            "mined_at": "2026-10-04T23:38:29Z", "sent_from": sent_from, "sent_to": SOL,
            "status": status, "nonce": 0,
            "fee": {"fungible_info": sol_info.clone(), "quantity": {"int": "9000", "decimals": 9}},
            "transfers": transfers, "approvals": [], "flags": {"is_trash": false}},
            "relationships": {"chain": {"data": {"type": "chains", "id": "solana"}}}})
    };
    Mock::given(method("GET"))
        .and(path(format!("/wallets/{SOL}/transactions/")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "links": {"self": "x"},
        "data": [
            tx(sig, "receive", "confirmed", "HckQ93Xqjjo8mwt5pNPWvyCTZXQZ858rvzmm7ZRrZg9t", json!([
                {"fungible_info": sol_info.clone(), "direction": "in",
                 "quantity": {"int": "1303716", "decimals": 9}}])),
            tx("5sendSig", "send", "confirmed", SOL, json!([
                {"fungible_info": usdc_info.clone(), "direction": "out",
                 "quantity": {"int": "250000", "decimals": 6}}])),
            tx("5failSig", "execute", "failed", SOL, json!([
                {"fungible_info": usdc_info, "direction": "out",
                 "quantity": {"int": "1", "decimals": 6}}]))
        ]})))
        .mount(&server)
        .await;
    let page = zerion(&server)
        .transactions(NetworkId::Solana, SOL, None, 20, Window::default())
        .await
        .unwrap();
    assert_eq!(page.next_cursor, None);
    let [receive, send, failed] = &page.txs[..] else {
        panic!()
    };
    assert_eq!(receive.hash, sig, "base58 signatures are case-sensitive");
    assert!(receive.fee.is_none(), "the sender signed and paid");
    assert_eq!(receive.legs[0].asset.id(), "solana:native");
    assert_eq!(send.legs[0].asset.contract.as_deref(), Some(USDC_MINT));
    assert_eq!(send.fee.as_ref().unwrap().raw, BigInt::from(9000));
    assert_eq!(send.fee.as_ref().unwrap().asset.id(), "solana:native");
    assert_eq!(failed.status, TxStatus::Failed);
    assert!(
        failed.legs.is_empty(),
        "a failed transaction only costs its fee"
    );
    assert_eq!(failed.fee.as_ref().unwrap().raw, BigInt::from(9000));
}

#[tokio::test]
async fn polygon_native_pol_system_contract_is_the_native_asset() {
    let server = MockServer::start().await;
    let evm = "0x1db3439a222c519ab44bb1144fc28167b4fa6ee6";
    Mock::given(method("GET"))
        .and(path(format!("/wallets/{evm}/positions/")))
        .and(query_param("filter[chain_ids]", "polygon"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            position("polygon", "POL", Some("0x0000000000000000000000000000000000001010"), "2784819257329674746", 18, false),
            position("polygon", "USDC.e", Some("0x2791BCA1F2DE4661ED88A30C99A7A9449AA84174"), "1100", 6, false),
            position("polygon", "BUDS", Some("0x6cea21103cfc8bb2516db3027c2bc5b2a28e770e"), "1", 18, true)
        ]})))
        .mount(&server)
        .await;
    let Positions::Ready(p) = zerion(&server)
        .positions(NetworkId::Polygon, evm)
        .await
        .unwrap()
    else {
        panic!("indexing")
    };
    assert_eq!(p[0].asset.id(), "polygon:native");
    assert_eq!(p[0].asset.symbol.as_deref(), Some("POL"));
    assert_eq!(
        p[1].asset.contract.as_deref(),
        Some("0x2791bca1f2de4661ed88a30c99a7a9449aa84174"),
        "EVM contracts are lowercase"
    );
    assert_eq!(p[2].asset.verification, Verification::Spam);
}
