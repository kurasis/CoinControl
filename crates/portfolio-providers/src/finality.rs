//! Bounded authoritative confirmation checks. Missing indexes/errors never prove a reorg.
use crate::{ProviderError, http::HttpClient, rpc};
use num_traits::ToPrimitive;
use portfolio_core::network::NetworkId;
use serde_json::{Value, json};
use url::Url;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validation {
    Final,
    Confirmed,
    Pending,
    Failed,
    Reorged,
    Unavailable,
}
pub struct FinalityBatch {
    pub results: Vec<(String, Validation)>,
    pub boundary: String,
}
fn height(v: &Value, p: &'static str, m: &'static str) -> Result<i64, ProviderError> {
    rpc::raw_hex(
        v.as_str()
            .ok_or_else(|| rpc::invalid(p, m, "missing height"))?,
        p,
        m,
    )?
    .to_i64()
    .ok_or_else(|| rpc::invalid(p, m, "height out of range"))
}
fn valid_evm_hash(hash: &str) -> bool {
    hash.len() == 66 && hash.starts_with("0x") && hash[2..].bytes().all(|b| b.is_ascii_hexdigit())
}
pub async fn evm(
    http: &HttpClient,
    url: Url,
    network: NetworkId,
    candidates: &[(String, i64)],
) -> Result<FinalityBatch, ProviderError> {
    let p = http.provider();
    let chain = rpc::call(http, url.clone(), "eth_chainId", json!([]), 0).await?;
    if height(&chain, p, "eth_chainId")? as u64 != network.evm_chain_id().unwrap_or(0) {
        return Err(rpc::invalid(p, "eth_chainId", "wrong mainnet"));
    }
    // Native chain finality tags reflect that chain's consensus/L1 settlement; no universal confirmation count.
    let finalized = rpc::call(
        http,
        url.clone(),
        "eth_getBlockByNumber",
        json!(["finalized", false]),
        0,
    )
    .await?;
    let final_height = height(&finalized["number"], p, "eth_getBlockByNumber")?;
    let final_hash = finalized["hash"]
        .as_str()
        .ok_or_else(|| rpc::invalid(p, "eth_getBlockByNumber", "missing finalized hash"))?;
    if !valid_evm_hash(final_hash) {
        return Err(rpc::invalid(
            p,
            "eth_getBlockByNumber",
            "invalid finalized hash",
        ));
    }
    let mut results = Vec::new();
    for (hash, stored_height) in candidates {
        let receipt = rpc::call(
            http,
            url.clone(),
            "eth_getTransactionReceipt",
            json!([hash]),
            0,
        )
        .await?;
        let validation = if receipt.is_null() {
            let tx = rpc::call(
                http,
                url.clone(),
                "eth_getTransactionByHash",
                json!([hash]),
                0,
            )
            .await?;
            if !tx.is_null() && tx.get("blockNumber").is_some_and(Value::is_null) {
                Validation::Pending
            } else {
                let block = rpc::call(
                    http,
                    url.clone(),
                    "eth_getBlockByNumber",
                    json!([format!("0x{stored_height:x}"), false]),
                    0,
                )
                .await?;
                if block.is_null() {
                    Validation::Unavailable
                } else {
                    if height(&block["number"], p, "eth_getBlockByNumber")? != *stored_height {
                        return Err(rpc::invalid(
                            p,
                            "eth_getBlockByNumber",
                            "wrong canonical height",
                        ));
                    }
                    let list = block["transactions"].as_array().ok_or_else(|| {
                        rpc::invalid(p, "eth_getBlockByNumber", "missing transaction membership")
                    })?;
                    if list.iter().any(|v| v.as_str().is_none()) {
                        return Err(rpc::invalid(
                            p,
                            "eth_getBlockByNumber",
                            "invalid transaction membership",
                        ));
                    }
                    if list.iter().any(|v| v.as_str() == Some(hash.as_str())) {
                        Validation::Unavailable
                    } else {
                        Validation::Reorged
                    }
                }
            }
        } else {
            if receipt["transactionHash"].as_str() != Some(hash.as_str()) {
                return Err(rpc::invalid(
                    p,
                    "eth_getTransactionReceipt",
                    "wrong transaction identity",
                ));
            }
            let receipt_height = height(&receipt["blockNumber"], p, "eth_getTransactionReceipt")?;
            let block = rpc::call(
                http,
                url.clone(),
                "eth_getBlockByNumber",
                json!([format!("0x{receipt_height:x}"), false]),
                0,
            )
            .await?;
            if !block.is_null()
                && height(&block["number"], p, "eth_getBlockByNumber")? != receipt_height
            {
                return Err(rpc::invalid(
                    p,
                    "eth_getBlockByNumber",
                    "wrong canonical height",
                ));
            }
            let canonical = block["hash"].as_str().filter(|h| valid_evm_hash(h));
            let reported = receipt["blockHash"].as_str().filter(|h| valid_evm_hash(h));
            if canonical.is_none() || reported.is_none() {
                Validation::Unavailable
            } else if canonical != reported || receipt_height != *stored_height {
                Validation::Reorged
            } else if receipt["status"] == "0x0" {
                Validation::Failed
            } else if receipt["status"] != "0x1" {
                return Err(rpc::invalid(
                    p,
                    "eth_getTransactionReceipt",
                    "missing execution status",
                ));
            } else if receipt_height <= final_height {
                Validation::Final
            } else {
                Validation::Confirmed
            }
        };
        results.push((hash.clone(), validation));
    }
    Ok(FinalityBatch {
        results,
        boundary: format!("finalized:{final_height}:{final_hash}"),
    })
}
pub async fn solana(
    http: &HttpClient,
    url: Url,
    candidates: &[(String, i64)],
) -> Result<FinalityBatch, ProviderError> {
    let p = http.provider();
    let slot = rpc::call(
        http,
        url.clone(),
        "getSlot",
        json!([{"commitment":"finalized"}]),
        0,
    )
    .await?
    .as_i64()
    .ok_or_else(|| rpc::invalid(p, "getSlot", "missing finalized slot"))?;
    let signatures: Vec<_> = candidates.iter().map(|(hash, _)| hash).collect();
    let response = rpc::call(
        http,
        url.clone(),
        "getSignatureStatuses",
        json!([signatures,{"searchTransactionHistory":true}]),
        0,
    )
    .await?;
    let states = response["value"]
        .as_array()
        .filter(|v| v.len() == candidates.len())
        .ok_or_else(|| rpc::invalid(p, "getSignatureStatuses", "incomplete status batch"))?;
    let mut results = Vec::new();
    for ((hash, stored_slot), state) in candidates.iter().zip(states) {
        let validation = if state.is_null() {
            if *stored_slot > slot {
                Validation::Unavailable
            } else {
                let block = rpc::call(http,url.clone(),"getBlock",json!([stored_slot,{"commitment":"finalized","transactionDetails":"signatures","rewards":false,"maxSupportedTransactionVersion":0}]),0).await?;
                if block.is_null() {
                    Validation::Unavailable
                } else {
                    let signatures = block["signatures"].as_array().ok_or_else(|| {
                        rpc::invalid(p, "getBlock", "missing signature membership")
                    })?;
                    if signatures.iter().any(|v| v.as_str().is_none()) {
                        return Err(rpc::invalid(p, "getBlock", "invalid signature membership"));
                    }
                    if signatures.iter().any(|v| v.as_str() == Some(hash.as_str())) {
                        Validation::Unavailable
                    } else {
                        Validation::Reorged
                    }
                }
            }
        } else if state["slot"].as_i64().is_none() {
            return Err(rpc::invalid(
                p,
                "getSignatureStatuses",
                "missing signature slot",
            ));
        } else if state.get("err").is_none() {
            return Err(rpc::invalid(
                p,
                "getSignatureStatuses",
                "missing execution status",
            ));
        } else if state["slot"].as_i64() != Some(*stored_slot) {
            Validation::Reorged
        } else if state.get("err").is_some_and(|v| !v.is_null()) {
            Validation::Failed
        } else {
            match state["confirmationStatus"].as_str() {
                Some("finalized") if *stored_slot <= slot => Validation::Final,
                Some("finalized") => Validation::Unavailable,
                Some("confirmed") => Validation::Confirmed,
                Some("processed") => Validation::Pending,
                _ => Validation::Unavailable,
            }
        };
        results.push((hash.clone(), validation));
    }
    Ok(FinalityBatch {
        results,
        boundary: format!("finalized-slot:{slot}"),
    })
}
