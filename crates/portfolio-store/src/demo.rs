//! Synthetic demo portfolio.
//!
//! Demo data lives only in the separate demo profile database and is refused
//! anywhere else. Every value is generated deterministically with integer
//! arithmetic; none of it represents a real wallet or a live market price.

use bigdecimal::{BigDecimal, Signed};
use num_bigint::BigInt;
use portfolio_core::accounting::price_change_percent;
use portfolio_core::address::normalize_address;
use portfolio_core::decimal::{parse_dec, quantity_to_raw, to_canonical};
use portfolio_core::network::NetworkId;

use crate::accounting::{BasisLotInput, LegClassification, LegOverride};
use crate::{ProfileKind, Result, Store, StoreError};
use portfolio_core::accounting::BasisKind;

const DAY: i64 = 86_400;
const HOUR: i64 = 3_600;
const HISTORY_DAYS: i64 = 400;
/// Network fee of the single demo send (paid in ETH by the sending account).
const DEMO_FEE_ETH: &str = "0.00042";

/// Builds a chain-specific asset identifier. Never derived from a ticker alone.
pub fn asset_id(network: NetworkId, contract: Option<&str>) -> String {
    match contract {
        None => format!("{}:native", network.as_str()),
        Some(c) => format!("{}:token:{}", network.as_str(), c),
    }
}

struct DemoAsset {
    network: NetworkId,
    contract: Option<&'static str>,
    symbol: &'static str,
    name: &'static str,
    decimals: u32,
    verification: &'static str,
    /// Starting price in micro-USD; `None` means no quote source exists.
    start_price_micros: Option<u128>,
    stable: bool,
    seed: u64,
}

const ASSETS: &[DemoAsset] = &[
    DemoAsset {
        network: NetworkId::Bitcoin,
        contract: None,
        symbol: "BTC",
        name: "Bitcoin",
        decimals: 8,
        verification: "verified",
        start_price_micros: Some(41_000_000_000),
        stable: false,
        seed: 11,
    },
    DemoAsset {
        network: NetworkId::Ethereum,
        contract: None,
        symbol: "ETH",
        name: "Ether",
        decimals: 18,
        verification: "verified",
        start_price_micros: Some(2_300_000_000),
        stable: false,
        seed: 23,
    },
    DemoAsset {
        network: NetworkId::Ethereum,
        contract: Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
        symbol: "USDC",
        name: "USD Coin",
        decimals: 6,
        verification: "verified",
        start_price_micros: Some(1_000_000),
        stable: true,
        seed: 31,
    },
    DemoAsset {
        network: NetworkId::Base,
        contract: None,
        symbol: "ETH",
        name: "Ether",
        decimals: 18,
        verification: "verified",
        start_price_micros: Some(2_300_000_000),
        stable: false,
        seed: 23,
    },
    DemoAsset {
        network: NetworkId::Solana,
        contract: None,
        symbol: "SOL",
        name: "Solana",
        decimals: 9,
        verification: "verified",
        start_price_micros: Some(95_000_000),
        stable: false,
        seed: 47,
    },
    DemoAsset {
        network: NetworkId::Tron,
        contract: None,
        symbol: "TRX",
        name: "TRON",
        decimals: 6,
        verification: "verified",
        start_price_micros: Some(105_000),
        stable: false,
        seed: 53,
    },
    DemoAsset {
        network: NetworkId::Tron,
        contract: Some("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t"),
        symbol: "USDT",
        name: "Tether USD",
        decimals: 6,
        verification: "verified",
        start_price_micros: Some(1_000_000),
        stable: true,
        seed: 61,
    },
    DemoAsset {
        network: NetworkId::Ton,
        contract: None,
        symbol: "TON",
        name: "Toncoin",
        decimals: 9,
        verification: "verified",
        start_price_micros: Some(2_400_000),
        stable: false,
        seed: 71,
    },
    DemoAsset {
        network: NetworkId::Ethereum,
        contract: Some("0x00000000000000000000000000000000000d3e30"),
        symbol: "DEMO",
        name: "Unpriced demo token",
        decimals: 18,
        verification: "unverified",
        start_price_micros: None,
        stable: false,
        seed: 0,
    },
    DemoAsset {
        network: NetworkId::Ethereum,
        contract: Some("0x000000000000000000000000000000000005ba3e"),
        symbol: "FREE-AIRDROP",
        name: "Unsolicited token",
        decimals: 18,
        verification: "spam",
        start_price_micros: Some(500_000),
        stable: false,
        seed: 83,
    },
];

/// (wallet label, network, address)
const ACCOUNTS: &[(&str, NetworkId, &str)] = &[
    (
        "Cold storage",
        NetworkId::Bitcoin,
        "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4",
    ),
    (
        "Cold storage",
        NetworkId::Ethereum,
        "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
    ),
    (
        "Daily",
        NetworkId::Base,
        "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
    ),
    (
        "Daily",
        NetworkId::Solana,
        "11111111111111111111111111111112",
    ),
    (
        "Daily",
        NetworkId::Tron,
        "410000000000000000000000000000000000000001",
    ),
    (
        "Daily",
        NetworkId::Ton,
        "0:0000000000000000000000000000000000000000000000000000000000000001",
    ),
];

/// (account index, asset index, days before now, quantity after change)
const HOLDING_CHANGES: &[(usize, usize, i64, &str)] = &[
    (0, 0, 365, "0.5"),
    (0, 0, 120, "0.75"),
    (1, 1, 380, "4.2"),
    (1, 1, 5, "4.18958"),
    (1, 2, 200, "2500"),
    (1, 8, 30, "1000000"),
    (1, 9, 10, "999999"),
    (2, 3, 60, "0.8"),
    (3, 4, 300, "35"),
    (4, 5, 250, "12000"),
    (4, 6, 90, "1800"),
    (5, 7, 150, "420"),
];

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

/// Deterministic price path in micro-USD, oldest first, one entry per step.
fn price_path(asset: &DemoAsset, steps: usize, max_move_permille: u64) -> Vec<u128> {
    let Some(mut price) = asset.start_price_micros else {
        return Vec::new();
    };
    let mut rng = Lcg(asset.seed.wrapping_mul(1_000_003));
    let mut out = Vec::with_capacity(steps);
    for _ in 0..steps {
        if asset.stable {
            // Stablecoins drift within +/-0.2%; never forced to exactly $1.
            let offset = u128::from(rng.next() % 41);
            price = 998_000 + offset * 100;
        } else {
            let span = 2 * max_move_permille + 1;
            let step = u128::from(rng.next() % span);
            price = price * (1_000 - u128::from(max_move_permille) + step) / 1_000;
            price = price.max(1);
        }
        out.push(price);
    }
    out
}

fn micros_to_text(micros: u128) -> String {
    to_canonical(&BigDecimal::new(BigInt::from(micros), 6))
}

impl Store {
    /// Seeds the demo profile. Refused for any non-demo profile. Idempotent.
    pub async fn seed_demo(&self) -> Result<()> {
        if self.profile != ProfileKind::Demo {
            return Err(StoreError::Invalid(
                "demo data can only be written to the demo profile".into(),
            ));
        }
        let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wallets")
            .fetch_one(&self.pool)
            .await?;
        if existing > 0 {
            return Ok(());
        }

        let now = self.now();
        let today = now - now.rem_euclid(DAY);
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;

        for asset in ASSETS {
            sqlx::query(
                "INSERT INTO assets (id, network_id, asset_kind, canonical_identifier, decimals, symbol, name, verification, metadata_provider, metadata_updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'demo', ?)",
            )
            .bind(asset_id(asset.network, asset.contract))
            .bind(asset.network.as_str())
            .bind(if asset.contract.is_some() { "token" } else { "native" })
            .bind(asset.contract.unwrap_or(""))
            .bind(i64::from(asset.decimals))
            .bind(asset.symbol)
            .bind(asset.name)
            .bind(asset.verification)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            let daily = price_path(asset, HISTORY_DAYS as usize, 30);
            for (i, micros) in daily.iter().enumerate() {
                let at = today - (HISTORY_DAYS - 1 - i as i64) * DAY;
                insert_price(&mut tx, asset, *micros, now, at, "day", "estimated", None).await?;
            }
            let hourly = price_path(
                &DemoAsset {
                    seed: asset.seed + 7,
                    ..clone_asset(asset, daily.last().copied())
                },
                8 * 24,
                6,
            );
            for (i, micros) in hourly.iter().enumerate() {
                let is_current = i + 1 == hourly.len();
                let at = if is_current {
                    now - 60
                } else {
                    now - now.rem_euclid(HOUR) - (8 * 24 - 2 - i as i64) * HOUR
                };
                let quality = if is_current { "current" } else { "estimated" };
                let change = (i + 1 == hourly.len() && hourly.len() > 24).then(|| {
                    let then = hourly[hourly.len() - 25];
                    // (now/then - 1) * 100 in exact decimal arithmetic.
                    let now_price = BigDecimal::new(BigInt::from(*micros), 6);
                    let then_price = BigDecimal::new(BigInt::from(then), 6);
                    price_change_percent(&now_price, &then_price)
                        .map(|p| {
                            to_canonical(&p.with_scale_round(4, bigdecimal::RoundingMode::HalfEven))
                        })
                        .unwrap_or_default()
                });
                let granularity = if quality == "current" { "tick" } else { "hour" };
                insert_price(
                    &mut tx,
                    asset,
                    *micros,
                    now,
                    at,
                    granularity,
                    quality,
                    change,
                )
                .await?;
            }
        }

        let mut wallet_ids: Vec<(&str, String)> = Vec::new();
        for (label, _, _) in ACCOUNTS {
            if wallet_ids.iter().any(|(l, _)| l == label) {
                continue;
            }
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO wallets (id, label, archived, created_at) VALUES (?, ?, 0, ?)",
            )
            .bind(&id)
            .bind(label)
            .bind(now - 400 * DAY)
            .execute(&mut *tx)
            .await?;
            wallet_ids.push((label, id));
        }

        let mut account_ids = Vec::new();
        for (label, network, address) in ACCOUNTS {
            let wallet = &wallet_ids
                .iter()
                .find(|(l, _)| l == label)
                .expect("wallet")
                .1;
            let normalized = normalize_address(*network, address)?;
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO accounts (id, wallet_id, network_id, canonical_address, display_address, archived, created_at)
                 VALUES (?, ?, ?, ?, ?, 0, ?)",
            )
            .bind(&id)
            .bind(wallet)
            .bind(network.as_str())
            .bind(&normalized.canonical)
            .bind(&normalized.display)
            .bind(now - 400 * DAY)
            .execute(&mut *tx)
            .await?;
            account_ids.push(id);
        }

        for (label, members) in [
            ("Long-term", vec!["Cold storage"]),
            ("Everything", vec!["Cold storage", "Daily"]),
        ] {
            let gid = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO groups (id, label, created_at) VALUES (?, ?, ?)")
                .bind(&gid)
                .bind(label)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            for member in members {
                let wid = &wallet_ids
                    .iter()
                    .find(|(l, _)| *l == member)
                    .expect("wallet")
                    .1;
                sqlx::query("INSERT INTO group_wallets (group_id, wallet_id) VALUES (?, ?)")
                    .bind(&gid)
                    .bind(wid)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        // Quantity history, activity legs, and the current balance observation.
        let mut previous: Vec<((usize, usize), String)> = Vec::new();
        for (n, (acct, asset_idx, days_ago, qty)) in HOLDING_CHANGES.iter().enumerate() {
            let asset = &ASSETS[*asset_idx];
            let raw = quantity_to_raw(&parse_dec(qty)?, asset.decimals)?;
            let at = now - days_ago * DAY;
            sqlx::query(
                "INSERT INTO portfolio_snapshots (account_id, asset_id, at, raw_quantity, quality, valuation_version)
                 VALUES (?, ?, ?, ?, 'observed', 1)",
            )
            .bind(&account_ids[*acct])
            .bind(asset_id(asset.network, asset.contract))
            .bind(at)
            .bind(raw.to_string())
            .execute(&mut *tx)
            .await?;

            let before = previous
                .iter()
                .rev()
                .find(|(k, _)| *k == (*acct, *asset_idx))
                .map(|(_, v)| parse_dec(v))
                .transpose()?
                .unwrap_or_default();
            let mut delta = parse_dec(qty)? - before;
            let sending = delta.is_negative();
            if sending {
                // The balance drop includes the separately recorded network fee.
                delta += parse_dec(DEMO_FEE_ETH)?;
            }
            let delta_raw = if delta.is_negative() {
                -quantity_to_raw(&-delta.clone(), asset.decimals)?
            } else {
                quantity_to_raw(&delta, asset.decimals)?
            };
            let tx_id = format!("demo-tx-{n:03}");
            let operation = if sending { "send" } else { "receive" };
            sqlx::query(
                "INSERT INTO chain_transactions (id, network_id, canonical_tx_id, occurred_at, status, source_provider)
                 VALUES (?, ?, ?, ?, 'final', 'demo')",
            )
            .bind(&tx_id)
            .bind(asset.network.as_str())
            .bind(format!("demo{n:060}"))
            .bind(at)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO activity_legs (id, transaction_id, account_id, asset_id, signed_raw_quantity, direction, leg_type, decoding, unresolved)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 'interpreted', ?)",
            )
            .bind(format!("{tx_id}:0"))
            .bind(&tx_id)
            .bind(&account_ids[*acct])
            .bind(asset_id(asset.network, asset.contract))
            .bind(delta_raw.to_string())
            .bind(if operation == "send" { "out" } else { "in" })
            .bind(operation)
            // Receipts from unknown senders have unknown basis until reviewed.
            .bind(i64::from(operation == "receive"))
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO account_transactions (account_id, transaction_id, operation, provider, decoding, first_seen_at)
                 VALUES (?, ?, ?, 'demo', 'interpreted', ?)",
            )
            .bind(&account_ids[*acct])
            .bind(&tx_id)
            .bind(operation)
            .bind(at)
            .execute(&mut *tx)
            .await?;
            if operation == "send" {
                let fee_asset = asset_id(asset.network, None);
                sqlx::query(
                    "INSERT INTO transaction_fees (id, transaction_id, payer_account_id, asset_id, raw_quantity, attribution)
                     VALUES (?, ?, ?, ?, ?, 'exact')",
                )
                .bind(format!("{tx_id}:fee"))
                .bind(&tx_id)
                .bind(&account_ids[*acct])
                .bind(fee_asset)
                .bind(quantity_to_raw(&parse_dec(DEMO_FEE_ETH)?, 18)?.to_string())
                .execute(&mut *tx)
                .await?;
            }
            previous.push(((*acct, *asset_idx), (*qty).to_owned()));
        }

        let mut latest: Vec<((usize, usize), &str)> = Vec::new();
        for (acct, asset_idx, _, qty) in HOLDING_CHANGES {
            latest.retain(|(k, _)| *k != (*acct, *asset_idx));
            latest.push(((*acct, *asset_idx), qty));
        }
        for ((acct, asset_idx), qty) in latest {
            let asset = &ASSETS[asset_idx];
            // One account shows the stale state: last balance fetch failed 3 hours ago.
            let (observed_at, status) = if asset.network == NetworkId::Ton {
                (now - 3 * HOUR, "stale")
            } else {
                (now - 120, "fresh")
            };
            let raw = quantity_to_raw(&parse_dec(qty)?, asset.decimals)?;
            sqlx::query(
                "INSERT INTO balance_observations (account_id, asset_id, raw_quantity, observed_at, provider, status)
                 VALUES (?, ?, ?, ?, 'demo', ?)",
            )
            .bind(&account_ids[acct])
            .bind(asset_id(asset.network, asset.contract))
            .bind(raw.to_string())
            .bind(observed_at)
            .bind(status)
            .execute(&mut *tx)
            .await?;
        }

        // Example decisions, so the demo shows known, estimated and missing
        // cost basis side by side. They go through the normal override path.
        for (n, decision) in demo_decisions(now) {
            crate::review::insert_override(
                &mut tx,
                "leg",
                &format!("demo-tx-{n:03}:0"),
                &decision,
                "demo",
                now,
            )
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

/// Decisions keyed by `HOLDING_CHANGES` index. Receipt 4 (2,500 tokens) and
/// the unverified token stay unreviewed on purpose.
fn demo_decisions(now: i64) -> Vec<(usize, LegOverride)> {
    let lot = |quantity: &str, basis: &str, days_ago: i64| LegOverride {
        basis_lots: Some(vec![BasisLotInput {
            quantity: quantity.into(),
            basis_usd: Some(basis.into()),
            basis_kind: BasisKind::Known,
            acquired_at: now - days_ago * DAY,
        }]),
        note: Some("Demo: bought on an exchange".into()),
        ..LegOverride::default()
    };
    let market = || LegOverride {
        basis_from_market: true,
        ..LegOverride::default()
    };
    vec![
        (0, lot("0.5", "14250", 400)),
        (1, market()),
        (2, lot("4.2", "7350", 380)),
        (
            3,
            LegOverride {
                classification: Some(LegClassification::Sale),
                proceeds_from_market: true,
                ..LegOverride::default()
            },
        ),
        (7, market()),
        (8, market()),
        (9, market()),
        (10, market()),
        (11, market()),
    ]
}

fn clone_asset(asset: &DemoAsset, start: Option<u128>) -> DemoAsset {
    DemoAsset {
        network: asset.network,
        contract: asset.contract,
        symbol: asset.symbol,
        name: asset.name,
        decimals: asset.decimals,
        verification: asset.verification,
        start_price_micros: start,
        stable: asset.stable,
        seed: asset.seed,
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_price(
    tx: &mut sqlx::SqliteConnection,
    asset: &DemoAsset,
    micros: u128,
    requested_at: i64,
    observed_at: i64,
    granularity: &str,
    quality: &str,
    change: Option<String>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO prices (asset_id, provider, price_usd, requested_at, observed_at, granularity, quality, change_24h_percent)
         VALUES (?, 'demo', ?, ?, ?, ?, ?, ?)",
    )
    .bind(asset_id(asset.network, asset.contract))
    .bind(micros_to_text(micros))
    .bind(requested_at)
    .bind(observed_at)
    .bind(granularity)
    .bind(quality)
    .bind(change)
    .execute(tx)
    .await?;
    Ok(())
}
