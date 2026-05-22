#place_order.py
import sys
import asyncio
from lighter import SignerClient, ApiClient, OrderApi

BASE_DECIMALS = 3     
PRICE_DECIMALS = 3    
PRICE_OFFSET = 10      

async def get_execution_price(market_id: int, side: str) -> int:
    client = ApiClient()
    try:
        api = OrderApi(client)
        orders_resp = await api.order_book_orders(market_id=market_id, limit=1)
        if side.upper() == "SELL":
            if not orders_resp.bids:
                raise Exception("No bids in order book")
            price_float = float(orders_resp.bids[0].price)
            adjusted = price_float * (10 ** PRICE_DECIMALS) - PRICE_OFFSET
        else:  
            if not orders_resp.asks:
                raise Exception("No asks in order book")
            price_float = float(orders_resp.asks[0].price)
            adjusted = price_float * (10 ** PRICE_DECIMALS) + PRICE_OFFSET
        return max(1, int(adjusted)) 
    finally:
        await client.close()

async def place_order(api_private_key, account_index, api_key_index, api_url,
                      market_index, side, size):
    base_amount = round(float(size) * (10 ** BASE_DECIMALS))
    avg_price = await get_execution_price(int(market_index), side)

    print(f"[Debug] side={side}, size={size} SOL -> base_amount={base_amount}, "
          f"avg_price={avg_price}", file=sys.stderr)

    client = SignerClient(
        url=api_url,
        account_index=account_index,
        api_private_keys={api_key_index: api_private_key}
    )
    is_ask = (side.upper() == "SELL")
    order = await client.create_market_order(
        market_index=int(market_index),
        base_amount=base_amount,
        is_ask=is_ask,
        avg_execution_price=avg_price,
        client_order_index=0
    )
    return order

def main():
    if len(sys.argv) != 8:
        print("Usage: place_order.py ...", file=sys.stderr)
        sys.exit(1)

    api_private_key = sys.argv[1]
    account_index = int(sys.argv[2])
    api_key_index = int(sys.argv[3])
    api_url = sys.argv[4]
    market_index = sys.argv[5]
    side = sys.argv[6]
    size = sys.argv[7]

    result = asyncio.run(place_order(api_private_key, account_index,
                                     api_key_index, api_url, market_index,
                                     side, size))
    print(result)

if __name__ == "__main__":
    main()
