# Live provider test report

Run at 2026-10-07T11:43:01.849Z. Requests are counted locally, retries included.
Credentials, full URLs and response bodies are never recorded.

| Provider                   | Result | Requests | Duration (ms) | Endpoints / check                                                                                      |
| -------------------------- | ------ | -------- | ------------- | ------------------------------------------------------------------------------------------------------ |
| zerion                     | FAIL   | 3        | 2106          | positions                                                                                              |
|                            | FAIL   |          |               | Suite completion: Provider request aborted; consult sanitized test output for status                   |
| drpc                       | PASS   | 18       | 17205         | balance reserve                                                                                        |
|                            | PASS   |          |               | ethereum native balance: documented mainnet balance response; history not claimed                      |
|                            | PASS   |          |               | ethereum integer precision and provenance: integer raw quantities and independent provider attribution |
|                            | PASS   |          |               | base native balance: documented mainnet balance response; history not claimed                          |
|                            | PASS   |          |               | base integer precision and provenance: integer raw quantities and independent provider attribution     |
|                            | PASS   |          |               | arbitrum native balance: documented mainnet balance response; history not claimed                      |
|                            | PASS   |          |               | arbitrum integer precision and provenance: integer raw quantities and independent provider attribution |
|                            | PASS   |          |               | optimism native balance: documented mainnet balance response; history not claimed                      |
|                            | PASS   |          |               | optimism integer precision and provenance: integer raw quantities and independent provider attribution |
|                            | PASS   |          |               | polygon native balance: documented mainnet balance response; history not claimed                       |
|                            | PASS   |          |               | polygon integer precision and provenance: integer raw quantities and independent provider attribution  |
|                            | PASS   |          |               | bsc native balance: documented mainnet balance response; history not claimed                           |
|                            | PASS   |          |               | bsc integer precision and provenance: integer raw quantities and independent provider attribution      |
| drpc total across suites   | PASS   | 18       | -             | shared budget ceiling 50                                                                               |
| zerion total across suites | PASS   | 3        | -             | shared budget ceiling 50                                                                               |
