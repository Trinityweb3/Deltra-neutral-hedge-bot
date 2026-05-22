# Delta-Neutral MM Bot

A high-frequency delta-neutral bot written in Rust that maintains a SOL hedge between a Meteora DLMM position and a perpetual short on Lighter DEX.I

## Configuration

Create a `.env` file in the project root with the following variables:

```env
METEORA_POOL_ADDRESS=your PDA address
METEORA_WALLET_ADDRESS=Your sol address

LIGHTER_API_URL=https://mainnet.zklighter.elliot.ai
LIGHTER_ACCOUNT_INDEX=your account id
LIGHTER_MARKET_ID=2 (SOL)

LIGHTER_API_KEY_INDEX=your API's index
LIGHTER_API_KEY=your Lighter API
LIGHTER_API_SECRET=your Lighter secret api
```


