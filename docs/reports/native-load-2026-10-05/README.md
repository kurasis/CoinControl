# Native startup/load implementation and local CSV verification — 2026-10-05

Source `9e1a2f2ccb301e63ad74864591b624bbc979afe4`, [PR #4](https://github.com/kurasis/CoinControl/pull/4). Subsequent report commits preserve its application and automation code. Local verification is complete; new Windows startup/rendering acceptance is **NOT EXECUTED** at capture. [CI run 37366717592](https://github.com/kurasis/CoinControl/actions/runs/37366717592) is queued. The preceding [main run 37362067121](https://github.com/kurasis/CoinControl/actions/runs/37362067121) was cancelled without runner steps; GitHub annotated both Linux and Windows jobs: “The job was not acquired by Runner of type hosted even after multiple attempts”. Read the linked run for any later result; a queued job is not a pass.

## Changes and validation

Closing a CSV import during preview calculation, or navigating away, now discards the result even if it finishes after the dialog unmounts. A pending request disables replacement files, and an atomic commit remains visible until its backend operation finishes. Three frontend regressions cover late completion after close/navigation and the pending commit. Both CPU-intensive before/after preview calculations now run on blocking workers instead of occupying the async executor. Accounting formulas are unchanged.

`npm run check`, `npm run test:offline`, `npm run build` and `npm run verify:release -- --source-only` passed locally. Offline counts are **131 deterministic Rust tests / 23 Vitest tests**. Eight opt-out live Rust functions do no network work and are not counted as executed live validation. Local host: Debian 13/Linux `6.18.44`, Intel Xeon Platinum 8573C, 5 logical CPUs, 35,743,784,960 bytes RAM, Rust 1.97.0, Node 24.19.0. The full [identity](IDENTITY.json) and machine reports retain their scope.

## Executed large Store acceptance

The external release-mode `performance` example prepares **50 accounts / 500 assets / 100,000 normalized legs**, then `--verify-import` calls actual production Store APIs: valid preview, discard, another preview, one-row commit/replay and duplicate-file rejection. Preparation uses synthetic normalized SQL and never calls a provider. The verifier does not launch the application or measure native UI painting.

[LOCAL_IMPORT_REPORT.json](LOCAL_IMPORT_REPORT.json) records:

- One applied row and exactly **1 decision / 100,000 movements**; the combined portfolio remains **$50,000**.
- The first preview is discarded; the identical committed file cannot commit again. Independent read-only inspection confirms **1 committed / 2 rolled-back / 0 pending** batches and SQLite integrity `ok`.
- Two previews plus discard: **4292.83 ms**; commit/full replay: **13,538.06 ms**.
- A timer pinned to the preview caller's async executor thread recorded **209 samples**, maximum interval **215.33 ms**, under the 1000 ms limit. This is async-executor responsiveness, not renderer or UI cancellation latency.

[NATIVE_FIXTURE.json](NATIVE_FIXTURE.json) describes the same synthetic dataset: preparation **1363.28 ms**, initial accounting replay **14,532.40 ms**, 100,000 replayed events, real profile kind. Its local `--verify-import` invocation leaves a committed decision in that scratch profile. Windows CI omits that optional flag and starts its native fixture with zero decisions. The helper refuses to overwrite an existing profile.

## Implemented Windows checks awaiting execution

The separate `windows-native-load` job depends on a successful production installer job, downloads its NSIS artifact and release report, checks hashes, installs it in a disposable Windows CI user and prepares a new large real profile. It does not use `native-e2e` or an app data-directory override. Only external documented WebView2 runtime environment options attach the matching WebDriver. Application outbound traffic is blocked by an app-scoped Windows Firewall rule; private API keys are excluded from the child environment. Cleanup removes that rule and terminates only owned app/driver process trees.

The application contains one amount-free `portfolio-first-useful` Performance mark, after two animation frames with cached balance/navigation data ready. The runner compares its epoch timestamp with Windows `Get-Process.StartTime` in **three fresh application processes**, target ≤2000 ms. WebDriver attachment time is excluded; OS page/disk caches are not flushed; the first browser folder is fresh and later launches retain it. This definition does not require every chart/table row to finish rendering.

Additional actual UI checks upload the synthetic CSV through its file input, close a pending preview with feedback ≤1000 ms, verify late staging was rolled back without decisions, navigate two activity pages, then apply one decision and replay 100,000 existing legs. Frame intervals, preview/commit time, application-process memory and exact restart/integrity counts are recorded, along with five native screenshots. Every executed failed assertion makes the job fail. No Windows numbers or new screenshots are fabricated while the runner is unavailable.

Remaining release gates include completed production-native startup/cancellation/rendering evidence, Windows 11 page/overlay and 125/150/200% scaling acceptance, long-table virtualization/full import coverage and available Zerion quota (previous live runs return HTTP 429). A one-row basis import over SQL-seeded history does not prove 100,000-record provider ingestion. Historical Windows passes and screenshots in the [preceding report](../upgrade-and-native-2026-10-05/README.md) belong to that earlier code.
