use anyhow::{Result, anyhow};
use ethers::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use dotenv::dotenv;
#[macro_use]
extern crate lazy_static;
use std::env;

lazy_static! {
    static ref METEORA_POSITION_ADDRESS: String = env::var("METEORA_POSITION_ADDRESS")
        .expect("METEORA_POSITION_ADDRESS not set in .env");
    static ref LIGHTER_API_URL: String = env::var("LIGHTER_API_URL")
        .expect("LIGHTER_API_URL not set in .env");
    static ref LIGHTER_SIGNING_KEY: String = env::var("LIGHTER_SIGNING_KEY")
        .expect("LIGHTER_SIGNING_KEY not set in .env");
    static ref LIGHTER_ACCOUNT_INDEX: u64 = env::var("LIGHTER_ACCOUNT_INDEX")
        .expect("LIGHTER_ACCOUNT_INDEX not set in .env")
        .parse()
        .expect("LIGHTER_ACCOUNT_INDEX must be a u64");
    static ref LIGHTER_MARKET_ID: u32 = env::var("LIGHTER_MARKET_ID")
        .expect("LIGHTER_MARKET_ID not set in .env")
        .parse()
        .expect("LIGHTER_MARKET_ID must be a u32");
}

const CHECK_INTERVAL_SECS: u64 = 3;
const MIN_REBALANCE_DIFF: f64 = 0.05;
const LEVERAGE: u32 = 3;

#[derive(Deserialize, Debug)]
struct MeteoraPositionResponse {
    #[serde(rename = "amountX")]
    amount_x: String,
}

#[derive(Deserialize, Debug)]
struct LighterPositionResponse {
    size: String,
}

struct DeltaNeutralBot {
    http_client: reqwest::Client,
    wallet: LocalWallet,
}

impl DeltaNeutralBot {
    fn new() -> Result<Self> {
        let wallet = LIGHTER_SIGNING_KEY.parse::<LocalWallet>()?;
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        Ok(Self { http_client, wallet })
    }

    async fn get_meteora_sol(&self) -> Result<f64> {
        let url = format!("https://dlmm-api.meteora.ag/position/{}", *METEORA_POSITION_ADDRESS);
        let res = self.http_client.get(&url).send().await?;

        if res.status().is_success() {
            let data: MeteoraPositionResponse = res.json().await?;
            let sol_amount = data.amount_x.parse::<f64>()?;
            Ok(sol_amount)
        } else {
            Err(anyhow!("Meteora API error: {}", res.status()))
        }
    }

    async fn get_lighter_short(&self) -> Result<f64> {
        let url = format!(
            "{}/api/v1/position?account_index={}&market_id={}",
            *LIGHTER_API_URL, *LIGHTER_ACCOUNT_INDEX, *LIGHTER_MARKET_ID
        );
        let res = self.http_client.get(&url).send().await?;

        if res.status().is_success() {
            let data: LighterPositionResponse = res.json().await?;
            let size = data.size.parse::<f64>()?;
            if size < 0.0 {
                Ok(size.abs())
            } else {
                Ok(0.0)
            }
        } else {
            Err(anyhow!("Lighter API error while fetching position: {}", res.status()))
        }
    }

    async fn place_market_order(&self, side: &str, amount: f64) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();

        let message = format!(
            "{}_{}_{}_MARKET_{}_{}",
            *LIGHTER_ACCOUNT_INDEX, *LIGHTER_MARKET_ID, side, amount, timestamp
        );

        let signature = self.wallet.sign_message(message.as_bytes()).await?;
        let signature_hex = format!("0x{}", signature);

        let url = format!("{}/api/v1/order", *LIGHTER_API_URL);

        let payload = serde_json::json!({
            "account_index": *LIGHTER_ACCOUNT_INDEX,
            "market_id": *LIGHTER_MARKET_ID,
            "side": side,
            "order_type": "MARKET",
            "size": amount.to_string(),
            "timestamp": timestamp
        });
        let res = self.http_client.post(&url)
            .header("X-Lighter-Signature", signature_hex)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if res.status().is_success() || res.status() == 201 {
            println!("[Lighter] Successful {} order for {} SOL", side, amount);
            Ok(())
        } else {
            let err_text = res.text().await?;
            Err(anyhow!("Lighter order execution error: {}", err_text))
        }
    }

    async fn set_leverage(&self) -> Result<()> {
        let url = format!("{}/api/v1/leverage", *LIGHTER_API_URL);
        let payload = serde_json::json!({
            "account_index": *LIGHTER_ACCOUNT_INDEX,
            "market_id": *LIGHTER_MARKET_ID,
            "leverage": LEVERAGE
        });

        let res = self.http_client.post(&url).json(&payload).send().await?;
        if res.status().is_success() {
            println!("[Lighter] Leverage set to {}x", LEVERAGE);
            Ok(())
        } else {
            Err(anyhow!("Failed to set leverage: {}", res.status()))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    println!("=== Delta-neutral bot (Rust) started ===");

    let bot = DeltaNeutralBot::new()?;

    if let Err(e) = bot.set_leverage().await {
        println!("[Warning] Failed to set leverage: {}. Possibly already configured.", e);
    }

    loop {
        let meteora_fut = bot.get_meteora_sol();
        let lighter_fut = bot.get_lighter_short();

        match tokio::join!(meteora_fut, lighter_fut) {
            (Ok(meteora_sol), Ok(lighter_short)) => {
                let delta = meteora_sol - lighter_short;

                println!(
                    "[Monitor] Pool: {:.4} SOL | Hedge: {:.4} SOL | Delta: {:.4}",
                    meteora_sol, lighter_short, delta
                );

                if delta.abs() >= MIN_REBALANCE_DIFF {
                    if delta > 0.0 {
                        println!("-> Imbalance! Price falling, pool holds more SOL. Increasing short by {:.4} SOL", delta.abs());
                        if let Err(e) = bot.place_market_order("SELL", delta.abs()).await {
                            println!("[Critical Error] Failed to increase short: {}", e);
                        }
                    } else {
                        println!("-> Imbalance! Price rising, pool holds less SOL. Buying back short by {:.4} SOL", delta.abs());
                        if let Err(e) = bot.place_market_order("BUY", delta.abs()).await {
                            println!("[Critical Error] Failed to buy back short: {}", e);
                        }
                    }
                } else {
                    println!("-> Delta within acceptable range.");
                }
            }
            (Err(e), _) => println!("[Error] Failed to fetch Meteora data: {}", e),
            (_, Err(e)) => println!("[Error] Failed to fetch Lighter data: {}", e),
        }

        sleep(Duration::from_secs(CHECK_INTERVAL_SECS)).await;
    }
}