# Delta-Neutral Bot (Rust)

A high-frequency delta-neutral bot written in Rust that maintains a SOL hedge between a Meteora DLMM position and a perpetual short on Lighter DEX.

## How it works

- Periodically fetches the SOL amount from a Meteora DLMM position (`amount_x`) and the size of an existing short position on Lighter.
- Calculates the **delta** = `Meteora SOL balance - Lighter short size`.
- If the absolute delta exceeds `MIN_REBALANCE_DIFF` (default 0.05 SOL), the bot places a market order on Lighter to restore neutrality:
  - **delta > 0** → pool holds more SOL → price is falling → increase short (`SELL`).
  - **delta < 0** → pool holds less SOL → price is rising → buy back part of the short (`BUY`).
- Leverage is set to 3× on startup (configurable)

## Prerequisites

- Rust toolchain (stable)
- A Meteora DLMM position address (Solana)
- A Lighter account with an API signing key (Ethereum‑compatible private key in hex)
- Access to Lighter's mainnet API

## Configuration

Create a `.env` file in the project root with the following variables:

```env
METEORA_POSITION_ADDRESS=<your_dlmm_position_address>
LIGHTER_API_URL=https://mainnet.zklighter.elliot.ai
LIGHTER_SIGNING_KEY=<your_api_key_hexed>
LIGHTER_ACCOUNT_INDEX=123456
LIGHTER_MARKET_ID=1