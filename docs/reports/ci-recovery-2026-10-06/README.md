# CI recovery baseline and local regression

Baseline: main source `5a9f8f62db894b6f269d099bc48518dd80fd2830`, [CI 37373466945 attempt 2](https://github.com/kurasis/CoinControl/actions/runs/37373466945/attempts/2), 2026-10-06 UTC. Files with `BASELINE_` names are downloaded reports with normalized formatting, not results of the fixes.

- Five successful jobs: Linux checks/offline/browser; live providers; Windows installer; cached load on Linux and Windows.
- Native Windows: 19 PASS, one BLOCKED invalid UI Automation method, one installer check skipped in this separate job.
- Production load: EXE preflight mismatch; cleanup before firewall creation failed. Startup and CSV/native load measurements did not run.
- Eight live suites PASS, shared request counts: DefiLlama 44, Esplora 13, Live Coin Watch 5, TonAPI 11, TronGrid 33, Zerion 35 (ceiling 50 each).
- Installer 10 PASS; release inspection 61 PASS; browser 24 combinations / 264 page / 48 panel checks and 10,000-row variable-height keyboard/focus scenario PASS.

Actual NSIS extraction with 7-Zip 26.00 on Linux yielded `portfolio-desk.exe` hash `1029fdfc4fe01cefbff71309a122a7214fe7efa1a7f03b5e8ea5fb95dfd45193`, identical to the installed native report. Installer hash: `e07a5554ae15513129231010580e05c8bc82645fa8084065b4558d234a125d08`. The separate compiler-output EXE recorded by the previous verifier has hash `ff2fa62c32d9b6c2f831ebf237d73ff8887e3eb4911831e1aee6eb88a2a580d9`.

The fixed release verifier was exercised against this real installer in an isolated directory with a deliberately different raw-output EXE. It extracted/scanned the production payload and recorded the exact paired installer/payload hashes. This is Linux artifact inspection, not execution of a Windows application. Local code checks, 133 deterministic Rust and 27 frontend tests, build and PowerShell syntax parsing passed. New Windows fixes and physical Windows 11 coverage remain unverified until their own native evidence is available.
