use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use dotenv::dotenv;
#[macro_use]
extern crate lazy_static;
use std::env;

lazy_static! {
    static ref METEORA_POOL_ADDRESS: String = env::var("METEORA_POOL_ADDRESS")
        .expect("METEORA_POOL_ADDRESS not set in .env");
    static ref METEORA_WALLET_ADDRESS: String = env::var("METEORA_WALLET_ADDRESS")
        .expect("METEORA_WALLET_ADDRESS not set in .env");

    static ref LIGHTER_API_URL: String = env::var("LIGHTER_API_URL")
        .expect("LIGHTER_API_URL not set in .env");
    static ref LIGHTER_API_KEY: String = env::var("LIGHTER_API_KEY")
        .expect("LIGHTER_API_KEY not set in .env");
    static ref LIGHTER_API_SECRET: String = env::var("LIGHTER_API_SECRET")
        .expect("LIGHTER_API_SECRET not set in .env");
    static ref LIGHTER_API_KEY_INDEX: u32 = env::var("LIGHTER_API_KEY_INDEX")
        .expect("LIGHTER_API_KEY_INDEX not set")
        .parse()
        .expect("LIGHTER_API_KEY_INDEX must be u32");
    static ref LIGHTER_ACCOUNT_INDEX: u64 = env::var("LIGHTER_ACCOUNT_INDEX")
        .expect("LIGHTER_ACCOUNT_INDEX not set")
        .parse()
        .expect("LIGHTER_ACCOUNT_INDEX must be u64");
    static ref LIGHTER_MARKET_ID: u32 = env::var("LIGHTER_MARKET_ID")
        .expect("LIGHTER_MARKET_ID not set")
        .parse()
        .expect("LIGHTER_MARKET_ID must be u32");
    static ref LEVERAGE: u32 = env::var("LEVERAGE")
        .unwrap_or_else(|_| "3".to_string())
        .parse()
        .expect("LEVERAGE must be a u32");
}

const CHECK_INTERVAL_SECS: u64 = 10;
const MIN_REBALANCE_DIFF: f64 = 0.05;
const LIGHTER_BASE_DECIMALS: u32 = 3;

#[derive(Deserialize, Debug)]
struct MeteoraApiResponse {
    positions: Vec<MeteoraPosition>,
}
#[derive(Deserialize, Debug)]
struct MeteoraPosition {
    #[serde(rename = "unrealizedPnl")]
    unrealized_pnl: UnrealizedPnl,
}
#[derive(Deserialize, Debug)]
struct UnrealizedPnl {
    #[serde(rename = "balanceTokenX")]
    balance_token_x: TokenBalance,
}
#[derive(Deserialize, Debug)]
struct TokenBalance {
    amount: String,
}

#[derive(Deserialize, Debug)]
struct LighterAccountResponse {
    accounts: Vec<LighterAccount>,
}
#[derive(Deserialize, Debug)]
struct LighterAccount {
    positions: Vec<LighterAccountPosition>,
}
#[derive(Deserialize, Debug)]
struct LighterAccountPosition {
    market_id: u32,
    position: String,
}

struct DeltaNeutralBot {
    http_client: reqwest::Client,
}

impl DeltaNeutralBot {
    fn new() -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;
        Ok(Self { http_client })
    }

    fn lighter_request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url)
            .header("X-API-KEY", LIGHTER_API_KEY.as_str())
            .header("X-API-KEY-INDEX", LIGHTER_API_KEY_INDEX.to_string())
            .header("accept", "application/json")
    }

    async fn get_meteora_sol(&self) -> Result<f64> {
        let url = format!(
            "https://dlmm.datapi.meteora.ag/positions/{}/pnl?user={}&status=open&pageSize=10&page=1",
            *METEORA_POOL_ADDRESS, *METEORA_WALLET_ADDRESS
        );
        let res = self.http_client.get(&url).send().await?;
        if !res.status().is_success() {
            return Err(anyhow!("Meteora API error: {}", res.status()));
        }
        let data: MeteoraApiResponse = res.json().await?;
        if let Some(pos) = data.positions.first() {
            Ok(pos.unrealized_pnl.balance_token_x.amount.parse::<f64>()?)
        } else {
            Ok(0.0)
        }
    }

    async fn get_lighter_short(&self) -> Result<f64> {
        let url = format!(
            "{}/api/v1/account?by=index&value={}",
            *LIGHTER_API_URL, *LIGHTER_ACCOUNT_INDEX
        );
        let res = self.lighter_request(reqwest::Method::GET, &url).send().await?;
        let status = res.status();
        if !status.is_success() {
            let err_text = res.text().await?;
            return Err(anyhow!("Lighter account error: {} - {}", status, err_text));
        }
        let text = res.text().await?;
        let data: LighterAccountResponse = serde_json::from_str(&text)?;
        let account = data.accounts.first()
            .ok_or_else(|| anyhow!("No account found"))?;
        let pos = account.positions.iter()
            .find(|p| p.market_id == *LIGHTER_MARKET_ID)
            .ok_or_else(|| anyhow!("Market {} not in positions", *LIGHTER_MARKET_ID))?;
        let raw_size = pos.position.parse::<f64>()?;
        let size_sol = raw_size / 10_f64.powi(LIGHTER_BASE_DECIMALS as i32);
        println!("[Debug] Raw position size: {}, scaled: {:.4} SOL", raw_size, size_sol);
        Ok(if size_sol < 0.0 { size_sol.abs() } else { 0.0 })
    }

    async fn set_leverage(&self) -> Result<()> {
        let output = Command::new("python3")
            .args([
                "set_leverage.py",
                &LIGHTER_API_SECRET,
                &LIGHTER_ACCOUNT_INDEX.to_string(),
                &LIGHTER_API_KEY_INDEX.to_string(),
                &LIGHTER_API_URL,
                &LIGHTER_MARKET_ID.to_string(),
                &LEVERAGE.to_string(),
            ])
            .output()?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Set leverage failed: {}", err));
        }
        println!("[Lighter] Leverage set to {}x", *LEVERAGE);
        Ok(())
    }

    async fn place_market_order(&self, side: &str, amount: f64) -> Result<()> {
        let output = Command::new("python3")
            .args([
                "place_order.py",
                &LIGHTER_API_SECRET,
                &LIGHTER_ACCOUNT_INDEX.to_string(),
                &LIGHTER_API_KEY_INDEX.to_string(),
                &LIGHTER_API_URL,
                &LIGHTER_MARKET_ID.to_string(),
                side,
                &amount.to_string(),
            ])
            .output()?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Order failed: {}", err));
        }
        println!("[Lighter] Order placed: {}", String::from_utf8_lossy(&output.stdout).trim());
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    println!("=== Delta-neutral bot (Rust) started ===");

    let bot = DeltaNeutralBot::new()?;
    if let Err(e) = bot.set_leverage().await {
        println!("[Warning] Failed to set leverage: {}. Continuing...", e);
    }

    loop {
        match tokio::join!(bot.get_meteora_sol(), bot.get_lighter_short()) {
            (Ok(meteora_sol), Ok(lighter_short)) => {
                let delta = meteora_sol - lighter_short;
                println!("[Monitor] Pool: {:.4} SOL | Hedge: {:.4} SOL | Delta: {:.4}",
                    meteora_sol, lighter_short, delta);

                if delta.abs() >= MIN_REBALANCE_DIFF {
                    let side = if delta > 0.0 { "SELL" } else { "BUY" };
                    let abs_delta = delta.abs();
                    println!("-> Imbalance! {} {:.4} SOL", side, abs_delta);

                    if let Err(e) = bot.place_market_order(side, abs_delta).await {
                        println!("[Critical Error] {}", e);
                    } else {
                        let mut found = false;
                        for _ in 0..12 {
                            sleep(Duration::from_secs(5)).await;
                            if let Ok(hedge) = bot.get_lighter_short().await {
                                if hedge > 0.0 {
                                    let new_delta = meteora_sol - hedge;
                                    println!("[Wait] Hedge now: {:.4} SOL, new delta: {:.4}", hedge, new_delta);
                                    found = true;
                                    break;
                                }
                            }
                        }
                        if !found {
                            println!("[Warning] Position not confirmed yet, will re-check in next cycle.");
                        }
                    }
                } else {
                    println!("-> Delta within acceptable range.");
                }
            }
            (Err(e), _) => println!("[Error] Meteora: {}", e),
            (_, Err(e)) => println!("[Error] Lighter: {}", e),
        }
        sleep(Duration::from_secs(CHECK_INTERVAL_SECS)).await;
    }
}
