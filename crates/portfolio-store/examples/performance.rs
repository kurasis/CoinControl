//! Opt-in, network-free load measurement. Never compiled into the application.
//! The fixture writes normalized evidence directly; ingestion is measured separately.
use std::sync::Arc;
use std::time::Instant;

use portfolio_core::clock::{Clock, FixedClock, SystemClock, utc_day};
use portfolio_core::network::NetworkId;
use portfolio_store::ingest::{
    AssetSpec, Decoding, Direction, LegSpec, TxSpec, TxStatus, Verification,
};
use portfolio_store::{ActivityFilter, ChartRange, ProfileKind, Scope, Store};
use serde_json::{Value, json};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

const NOW: i64 = 1_791_158_400;
const LEGS: i64 = 100_000;

#[allow(
    clippy::float_arithmetic,
    reason = "Latency measurements are not monetary calculations"
)]
fn elapsed(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

fn distribution(mut samples: Vec<f64>) -> Value {
    let first = samples[0];
    samples.sort_by(f64::total_cmp);
    let percentile = |p: usize| samples[(samples.len() * p).div_ceil(100) - 1];
    json!({"samples": samples.len(), "first_ms":first,"max_ms":samples[samples.len()-1],
        "p50_ms": percentile(50), "p95_ms": percentile(95)})
}

fn peak_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/status")
            .ok()?
            .lines()
            .find(|line| line.starts_with("VmHWM:"))?
            .split_whitespace()
            .nth(1)?
            .parse::<u64>()
            .ok()
            .map(|kb| kb * 1024)
    }
    #[cfg(target_os = "windows")]
    {
        let command = format!("(Get-Process -Id {}).PeakWorkingSet64", std::process::id());
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &command])
            .output()
            .ok()?;
        String::from_utf8(output.stdout).ok()?.trim().parse().ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

// Optional backend acceptance on the same large fixture, runnable without Windows.
// LocalSet pins the measured async timer and preview caller to one executor thread.
async fn verify_native_import(
    store: &Store,
    pool: &sqlx::SqlitePool,
    csv: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let gaps = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = gaps.clone();
    let local = tokio::task::LocalSet::new();
    let measured = local.run_until(async {
        let timer = tokio::task::spawn_local(async move {
            let mut last = Instant::now();
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                observed.borrow_mut().push(elapsed(last));
                last = Instant::now();
            }
        });
        let result: Result<Value, Box<dyn std::error::Error>> = async {
            let start = Instant::now();
            let cancelled = store.preview_basis_import("native-basis.csv", csv, None).await?;
            assert!(cancelled.can_commit, "{cancelled:?}");
            store.discard_basis_import(&cancelled.batch_id).await?;
            let preview = store.preview_basis_import("native-basis.csv", csv, None).await?;
            let preview_ms = elapsed(start);
            assert_eq!((preview.ok_count, preview.error_count), (1, 0));
            assert!(preview.can_commit);
            let start = Instant::now();
            let committed = store.commit_basis_import(&preview.batch_id).await?;
            let commit_replay_ms = elapsed(start);
            assert_eq!(committed.applied_rows, 1);
            assert_eq!(committed.replay.events, LEGS as u32);
            assert_eq!(store.portfolio_summary(&Scope::All).await?.total_value_usd.as_deref(), Some("50000"));
            let duplicate = store.preview_basis_import("native-basis.csv", csv, None).await?;
            assert!(duplicate.duplicate_file && !duplicate.can_commit);
            store.discard_basis_import(&duplicate.batch_id).await?;
            assert!(store.integrity_ok().await?);
            let decisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounting_overrides").fetch_one(pool).await?;
            let movements: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_legs").fetch_one(pool).await?;
            assert_eq!((decisions, movements), (1, LEGS));
            Ok(json!({"mode":"offline-large-store-csv", "result":"PASS",
                "dataset":{"accounts":50,"assets":500,"activity_legs":LEGS},
                "applied_rows":1, "decisions":decisions, "movements":movements,
                "duplicate_file_rejected":true,"cancelled_preview_discarded":true,
                "two_previews_and_discard_ms":preview_ms,"commit_replay_ms":commit_replay_ms,
                "limits":["Production Store APIs; this is not native renderer/startup evidence.",
                    "One CSV decision over 100000 existing normalized legs; not 100000 provider imports."]}))
        }.await;
        timer.abort();
        result
    }).await?;
    let samples = gaps.borrow();
    let worst = samples.iter().copied().fold(0.0, f64::max);
    assert!(
        !samples.is_empty() && worst < 1000.0,
        "async executor blocked for {worst} ms"
    );
    let mut measured = measured;
    measured["async_loop_timer"] =
        json!({"samples":samples.len(),"max_gap_ms":worst,"target_ms":1000});
    println!(
        "Verified large Store CSV import, discard and duplicate rejection; async gap {worst:.0} ms"
    );
    Ok(measured)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !args.is_empty()
        && !((args.len() == 3 || (args.len() == 4 && args[3] == "--verify-import"))
            && args[0] == "--native-fixture")
    {
        return Err(
            "usage: performance [--native-fixture NEW_PROFILE OUTPUT_DIRECTORY [--verify-import]]"
                .into(),
        );
    }
    let native_fixture = !args.is_empty();
    let now = if native_fixture {
        SystemClock.now()
    } else {
        NOW
    };
    let output = std::path::PathBuf::from(if native_fixture {
        &args[2]
    } else {
        "target/performance-report"
    });
    std::fs::create_dir_all(&output)?;
    let temp = tempfile::tempdir()?;
    let path = if native_fixture {
        let path = std::path::PathBuf::from(&args[1]);
        if path.exists() {
            return Err("refusing to overwrite an existing native fixture profile".into());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        path
    } else {
        temp.path().join("load.sqlite")
    };
    let start = Instant::now();
    let store = Store::open(
        &path,
        if native_fixture {
            ProfileKind::Real
        } else {
            ProfileKind::Test
        },
        Arc::new(FixedClock(now)),
    )
    .await?;
    if native_fixture {
        let mut settings = store.get_settings().await?;
        settings.language = Some("en".into());
        settings.timezone = Some("UTC".into());
        settings.theme = portfolio_store::ThemePreference::Dark;
        store.update_settings(&settings).await?;
    }
    let wallet = store.create_wallet("Synthetic load (no network)").await?;
    let mut accounts = Vec::new();
    for i in 0..50 {
        accounts.push(
            store
                .add_account(
                    &wallet.id,
                    NetworkId::Ethereum,
                    &format!("0x{:040x}", i + 1),
                    None,
                )
                .await?,
        );
    }
    let mut assets = Vec::new();
    for i in 0..500 {
        let spec = AssetSpec {
            network: NetworkId::Ethereum,
            contract: Some(format!("0x{:040x}", i + 10_000)),
            decimals: 6,
            symbol: Some(format!("LOAD{i}")),
            name: Some(format!("Synthetic asset {i}")),
            verification: Verification::Verified,
            provider: "performance",
        };
        store.upsert_asset(&spec).await?;
        store
            .record_balance(
                &accounts[i % 50].id,
                &spec,
                &100_000_000.into(),
                None,
                "fresh",
            )
            .await?;
        assets.push(spec);
    }
    // One transaction per leg, 150 receipts and 50 disposals per asset.
    // Net quantity: 100 units for each of 500 assets; current price: $1.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .foreign_keys(true),
        )
        .await?;
    let mut tx = pool.begin().await?;
    sqlx::query("CREATE TEMP TABLE load_sequence(n INTEGER PRIMARY KEY, account TEXT, asset TEXT)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("WITH RECURSIVE seq(n) AS (SELECT 0 UNION ALL SELECT n+1 FROM seq WHERE n+1 < ?),
        a AS (SELECT id, row_number() OVER(ORDER BY canonical_address)-1 AS n FROM accounts),
        s AS (SELECT id, row_number() OVER(ORDER BY canonical_identifier)-1 AS n FROM assets)
        INSERT INTO load_sequence SELECT seq.n,a.id,s.id FROM seq JOIN a ON a.n=seq.n%50 JOIN s ON s.n=seq.n%500")
        .bind(LEGS).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO chain_transactions(id,network_id,canonical_tx_id,block_height,position,occurred_at,status,source_provider)
        SELECT printf('ethereum:load-%06d',n),'ethereum',printf('load-%06d',n),n+1,'0',?+n,'confirmed','performance' FROM load_sequence")
        .bind(now-LEGS).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO account_transactions(account_id,transaction_id,operation,provider,decoding,first_seen_at)
        SELECT account,printf('ethereum:load-%06d',n),CASE WHEN (n/500)%4=3 THEN 'send' ELSE 'receive' END,
        'performance','interpreted',?+n FROM load_sequence")
        .bind(now-LEGS).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO activity_legs(id,transaction_id,account_id,asset_id,signed_raw_quantity,direction,leg_type,decoding,unresolved)
        SELECT printf('ethereum:load-%06d',n)||':'||account||':0',printf('ethereum:load-%06d',n),account,asset,
        CASE WHEN (n/500)%4=3 THEN '-1000000' ELSE '1000000' END,
        CASE WHEN (n/500)%4=3 THEN 'out' ELSE 'in' END,
        CASE WHEN (n/500)%4=3 THEN 'send' ELSE 'receive' END,'interpreted',1 FROM load_sequence")
        .execute(&mut *tx).await?;
    sqlx::query("INSERT INTO movement_slots(prefix,next_index) SELECT printf('ethereum:load-%06d',n)||':'||account,1 FROM load_sequence")
        .execute(&mut *tx).await?;
    sqlx::query("INSERT INTO prices(asset_id,provider,price_usd,requested_at,observed_at,granularity,quality)
        SELECT id,'performance','1',?,?, 'tick','current' FROM assets")
        .bind(now).bind(now).execute(&mut *tx).await?;
    tx.commit().await?;
    let fixture_load_ms = elapsed(start);
    println!("Loaded 50 accounts / 500 assets / {LEGS} legs in {fixture_load_ms:.0} ms");
    let start = Instant::now();
    let replay = store.replay_accounting().await?;
    let replay_ms = elapsed(start);
    println!(
        "Replayed {} events / {} lots in {replay_ms:.0} ms",
        replay.events, replay.lots
    );
    assert_eq!(replay.events, LEGS as u32);
    assert_eq!(
        store
            .portfolio_summary(&Scope::All)
            .await?
            .total_value_usd
            .as_deref(),
        Some("50000")
    );
    assert!(store.integrity_ok().await?);
    if native_fixture {
        // One real receipt is sufficient to trigger preview/replay over all 100000 legs.
        // This CSV attaches a decision to existing history; it creates no new balance.
        let receipt_at = now - LEGS;
        let seconds = receipt_at.rem_euclid(86_400);
        let acquired = format!(
            "{}T{:02}:{:02}:{:02}Z",
            utc_day(receipt_at),
            seconds / 3600,
            (seconds / 60) % 60,
            seconds % 60
        );
        let csv = format!(
            "external_row_id,network_id,account_address,transaction_id,asset_identifier,quantity,acquired_at_utc,total_basis_usd,basis_kind,note\nnative-load-1,ethereum,0x{:040x},load-000000,0x{:040x},1,{acquired},1,known,native-load-acceptance\n",
            1, 10_000,
        );
        std::fs::write(output.join("native-basis.csv"), &csv)?;
        if args.len() == 4 {
            let verified = verify_native_import(&store, &pool, &csv).await?;
            std::fs::write(
                output.join("LOCAL_IMPORT_REPORT.json"),
                serde_json::to_string_pretty(&verified)? + "\n",
            )?;
        }
        let report = json!({
            "mode":"offline-native-fixture", "profile_kind":"real",
            "dataset":{"accounts":50,"assets":500,"activity_legs":LEGS},
            "expected_total_usd":"50000", "now":now,
            "fixture_load_ms":fixture_load_ms, "replay_ms":replay_ms,
            "replay_events":replay.events, "first_account_id":accounts[0].id,
            "build_mode":if cfg!(debug_assertions){"debug"}else{"release"},
            "limits":["Synthetic normalized SQL evidence, never queried on chain.",
                "Fixture preparation is external and is not a production provider import."]
        });
        std::fs::write(
            output.join("NATIVE_FIXTURE.json"),
            serde_json::to_string_pretty(&report)? + "\n",
        )?;
        store.close().await;
        pool.close().await;
        return Ok(());
    }
    // Check the actual normalized ingestion path with a bounded, idempotent overlap.
    let start = Instant::now();
    for i in 0..1_000_usize {
        let outgoing = (i / 500) % 4 == 3;
        let inserted = store
            .ingest_transaction(
                &accounts[i % 50].id,
                &TxSpec {
                    network: NetworkId::Ethereum,
                    part: None,
                    hash: format!("load-{i:06}"),
                    block_height: Some(i as i64 + 1),
                    position: Some("0".into()),
                    occurred_at: now - LEGS + i as i64,
                    status: TxStatus::Confirmed,
                    provider: "performance",
                    operation: "receive".into(),
                    legs: vec![LegSpec {
                        asset: assets[i % 500].clone(),
                        signed_raw: if outgoing {
                            (-1_000_000).into()
                        } else {
                            1_000_000.into()
                        },
                        direction: if outgoing {
                            Direction::Out
                        } else {
                            Direction::In
                        },
                        leg_type: "receive".into(),
                        counterparty: None,
                        decoding: Decoding::Interpreted,
                        unresolved: true,
                    }],
                    fee: None,
                    decoding: Decoding::Interpreted,
                    evidence: json!({}),
                },
            )
            .await?;
        assert!(
            !inserted,
            "overlap duplicated an existing account transaction"
        );
    }
    let overlap_ingest_ms = elapsed(start);
    store.replay_if_dirty().await?;
    store.close().await;
    let start = Instant::now();
    let store = Store::open(&path, ProfileKind::Test, Arc::new(FixedClock(now))).await?;
    assert!(store.replay_if_dirty().await?.is_none());
    assert_eq!(
        store
            .portfolio_summary(&Scope::All)
            .await?
            .total_value_usd
            .as_deref(),
        Some("50000")
    );
    let cached_open_and_summary_ms = elapsed(start);
    let account_scope = Scope::Accounts {
        ids: vec![accounts[0].id.clone()],
    };
    let first_page = store
        .list_activity(&Scope::All, &ActivityFilter::default(), None, 50)
        .await?;
    let next_page = store
        .list_activity(
            &Scope::All,
            &ActivityFilter::default(),
            first_page.next_cursor.as_deref(),
            50,
        )
        .await?;
    for (offset, page) in [(0, &first_page), (50, &next_page)] {
        assert_eq!(page.rows.len(), 50);
        for (i, row) in page.rows.iter().enumerate() {
            assert_eq!(
                row.transaction_id,
                format!("ethereum:load-{:06}", 99_999 - offset - i)
            );
        }
    }
    let mut timings = serde_json::Map::new();
    for name in ["summary", "holdings", "account", "activity", "chart"] {
        let mut samples = Vec::new();
        for _ in 0..25 {
            let start = Instant::now();
            match name {
                "summary" => {
                    store.portfolio_summary(&Scope::All).await?;
                }
                "holdings" => {
                    assert_eq!(store.list_holdings(&Scope::All).await?.len(), 500);
                }
                "account" => {
                    assert_eq!(store.list_holdings(&account_scope).await?.len(), 10);
                }
                "activity" => {
                    let page = store
                        .list_activity(&Scope::All, &ActivityFilter::default(), None, 50)
                        .await?;
                    assert_eq!(page.rows.len(), 50);
                    assert!(page.next_cursor.is_some());
                }
                "chart" => {
                    store.get_chart(&Scope::All, ChartRange::Month).await?;
                }
                _ => unreachable!(),
            }
            samples.push(elapsed(start));
        }
        let measured = distribution(samples);
        println!("{name}: {measured}");
        timings.insert(name.into(), measured);
    }
    let pass = timings
        .values()
        .all(|v| v["p95_ms"].as_f64().is_some_and(|ms| ms < 300.0));
    let report = json!({"mode":"offline-store-load", "result": if pass {"PASS"} else {"FAIL"},
        "os": std::env::consts::OS, "arch":std::env::consts::ARCH,
        "logical_cpus":std::thread::available_parallelism()?.get(),
        "build_mode":if cfg!(debug_assertions){"debug"}else{"release"},
        "dataset":{"accounts":50,"assets":500,"activity_legs":LEGS},
        "fixture_load_ms":fixture_load_ms,"replay_ms":replay_ms,
        "overlap_ingest":{"transactions":1000,"ms":overlap_ingest_ms},
        "cached_open_and_summary_ms":cached_open_and_summary_ms,
        "peak_process_memory_bytes":peak_memory_bytes(),"queries":timings,
        "query_p95_target_ms":300,
        "limits":["Fixture load is direct normalized SQL, not 100000 provider requests or CSV rows.",
            "Startup measures store reopen plus first summary; native process/render startup is separate.",
            "Cancellation is measured by native synchronization scenarios, not this store harness.",
            "No network requests; synthetic account addresses are never queried."]});
    std::fs::write(
        output.join("PERFORMANCE_REPORT.json"),
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    store.close().await;
    pool.close().await;
    if !pass {
        return Err("Cached query p95 exceeds the 300 ms acceptance target".into());
    }
    Ok(())
}
