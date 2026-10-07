# Live provider test report

Run at 2026-10-07T10:22:24.418Z. Requests are counted locally, retries included.
Credentials, full URLs and response bodies are never recorded.

| Provider                       | Result | Requests | Duration (ms) | Endpoints / check                                                                                                                                           |
| ------------------------------ | ------ | -------- | ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| alchemy                        | PASS   | 20       | 19303         | alchemy_getTokenBalances/alchemy_getTokenMetadata, eth_chainId/eth_getBalance/alchemy_getAssetTransfers, eth_getTransactionReceipt/eth_getTransactionByHash |
|                                | PASS   |          |               | known native payment and exact fee: independent known 79 ETH principal; exact sender receipt fee                                                            |
|                                | PASS   |          |               | exact contract USDC balance: ERC-20 raw hexadecimal balance; official USDC contract and six decimals                                                        |
|                                | PASS   |          |               | ethereum mainnet balance/history: correct chain ID; 5 bounded transfer-index rows                                                                           |
|                                | PASS   |          |               | Ethereum index pagination: 5 second-page rows                                                                                                               |
|                                | PASS   |          |               | base mainnet balance/history: correct chain ID; 5 bounded transfer-index rows                                                                               |
|                                | PASS   |          |               | arbitrum mainnet balance/history: correct chain ID; 5 bounded transfer-index rows                                                                           |
|                                | PASS   |          |               | optimism mainnet balance/history: correct chain ID; 5 bounded transfer-index rows                                                                           |
|                                | PASS   |          |               | polygon mainnet balance/history: correct chain ID; 5 bounded transfer-index rows                                                                            |
|                                | PASS   |          |               | bounded CU: 6750 conservative estimated CU; ceiling 10000                                                                                                   |
| chainstack                     | PASS   | 3        | 2090          | balance reserve                                                                                                                                             |
|                                | PASS   |          |               | solana token discovery limitation: native balance retained; token access partial: chainstack getTokenAccountsByOwner: credential rejected (HTTP 403)        |
|                                | PASS   |          |               | solana native balance: documented mainnet balance response; history not claimed                                                                             |
|                                | PASS   |          |               | solana integer precision and provenance: integer raw quantities and independent provider attribution                                                        |
| alchemy total across suites    | PASS   | 20       | -             | shared budget ceiling 50                                                                                                                                    |
| chainstack total across suites | PASS   | 3        | -             | shared budget ceiling 50                                                                                                                                    |
