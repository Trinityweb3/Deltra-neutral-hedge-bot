#set_leverage.py
import sys
import asyncio
from lighter import SignerClient

async def set_leverage(api_private_key, account_index, api_key_index, api_url, market_index, leverage):
    client = SignerClient(
        url=api_url,
        account_index=account_index,
        api_private_keys={api_key_index: api_private_key}
    )
    # margin_mode: 0 = isolated, 1 = cross
    result = await client.update_leverage(
        market_index=int(market_index),
        leverage=int(leverage),
        margin_mode=0
    )
    return result

def main():
    if len(sys.argv) != 7:
        print("Usage: set_leverage.py <api_private_key> <account_index> <api_key_index> <api_url> <market_index> <leverage>", file=sys.stderr)
        sys.exit(1)

    api_private_key = sys.argv[1]
    account_index = int(sys.argv[2])
    api_key_index = int(sys.argv[3])
    api_url = sys.argv[4]
    market_index = sys.argv[5]
    leverage = sys.argv[6]

    result = asyncio.run(set_leverage(api_private_key, account_index, api_key_index, api_url, market_index, leverage))
    print(result)

if __name__ == "__main__":
    main()
