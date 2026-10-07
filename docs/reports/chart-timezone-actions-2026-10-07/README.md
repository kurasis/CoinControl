# Согласованные улучшения после аудита — 7 октября 2026

Исходный код приложения: `153c2463f8f2f032e260c916ee25b6d21d1018b5` (PR #24).
Рабочее дерево перед изменениями было чистым. Пользователь согласовал все четыре
ранее отложенных изменения. Публичные команды и DTO, схема SQLite, провайдеры,
лимиты запросов и пороги CI сохранены. Release 0.1.6 не перепубликуется.

| Приоритет | Файл                                                                                        | Исправление и причина                                                                                                                                                                                                                                                                                                                                                                                    |
| --------- | ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P2        | `crates/portfolio-store/src/portfolio.rs`                                                   | UTC-сетка пропускала первое поступление внутри дня. В ряд стоимости портфеля и позиции добавлено точное время первого события внутри выбранного диапазона. Дубликаты не создаются; до первой операции и без цены остаются gaps. Дневные котировки по-прежнему estimated. Performance начинает расчёт с этой точки, если она оценена. Для длинных рядов зарезервирован бюджет точки: не более 1001 точки. |
| P2        | `crates/portfolio-store/src/settings.rs`, `src/lib/format.ts`, `src/pages/SettingsPage.tsx` | Сохранение timezone проверяет IANA-имя. Неверное значение из старой базы/backup читается как system, остальные настройки и исходные данные сохраняются. Intl защищён от неподдерживаемого override. UTC и сохранённые aliases остаются доступны в select.                                                                                                                                                |
| P2        | `Cargo.toml`                                                                                | Минимум Rust 1.97 соответствует уже закреплённому и используемому toolchain 1.97.0. Заявленный ранее 1.90 был ниже MSRV sqlx из lockfile.                                                                                                                                                                                                                                                                |
| P3        | `.github/workflows/ci.yml`, `.github/workflows/publish-windows.yml`                         | Официальные Actions переведены на актуальные Node 24 версии, закреплённые полными SHA с version comments. Версия Node приложения по-прежнему берётся из `.nvmrc` (22).                                                                                                                                                                                                                                   |

Timezone lookup использует `jiff-tzdb 0.1.8`, уже присутствовавший в Cargo.lock
как Windows transitive dependency (IANA 2026c): добавлена только прямая связь store с базой
IANA. Нет обновлений других Rust/npm dependencies. Проверка не зависит от
наличия системного tzdata на Windows и не выполняет сетевых запросов.

## Проверки

До изменений `npm run check`, `npm run test:offline`, `npm run build` прошли:
173 deterministic Rust tests, 48 frontend tests; 18 live test functions opt-out.
Новые целевые тесты на старой реализации подтвердили пропуск intraday first
point, RangeError при форматировании invalid timezone, принятие invalid timezone
при save и отсутствие fallback после восстановления реального file backup.
Первый вариант backup test использовал неподдерживаемую in-memory VACUUM snapshot;
fixture исправлен на файловую базу, после чего тест упал на ожидаемом timezone.

Целевые тесты после исправления прошли: 23 accounting tests, 4 settings tests,
10 formatting tests. Они проверяют точное начало/performance, сохранение estimated,
custom endpoints, отсутствие дубликатов у aligned/now events, ограничение размера,
отсутствие выдуманной стоимости без цены, границы диапазонов, валидные IANA names/
aliases, отказ save без изменения прежних настроек, backup fallback с сохранением
языка/theme/privacy/интервалов/console и форматирование на границе суток.

| Проверка                                                        | Результат                                                          | Доказательство                                                |
| --------------------------------------------------------------- | ------------------------------------------------------------------ | ------------------------------------------------------------- |
| `npm run check`                                                 | TypeScript, ESLint, Prettier, rustfmt, Clippy PASS                 | [Лог](FINAL_CHECK.txt)                                        |
| `npm run test:offline`                                          | 178 deterministic Rust и 52 Vitest PASS; 18 live functions opt-out | [Лог](FINAL_TESTS.txt)                                        |
| `npm run build`                                                 | Production frontend PASS                                           | [Лог](FINAL_BUILD.txt)                                        |
| `npm run gen:bindings`                                          | 67 export tests PASS; generated DTO неизменны                      | [Лог](GENERATED_BINDINGS.txt)                                 |
| `cargo check --workspace --all-targets --all-features --locked` | Linux compile PASS                                                 | [Лог](ALL_FEATURES.txt)                                       |
| `npm run verify:release -- --source-only`                       | Portable source/frontend checks PASS                               | [JSON](SOURCE_RELEASE_REPORT.json), [лог](SOURCE_RELEASE.txt) |

Браузерные проверки прошли: 24 сценария, 288 page layouts, 48 panel layouts,
virtualized 10000-row table и chart privacy ([JSON](BROWSER_LAYOUT_REPORT.json),
[лог](BROWSER_LAYOUT.txt)). Это Chromium/mock IPC, не native Windows. Report
содержит исходный HEAD с `sourceDirty: true`: проверялось рабочее дерево задачи.

Изменения доставлены через [PR #25](https://github.com/kurasis/CoinControl/pull/25),
точный код приложения `cc2215edb3ef13899dc2e6a230efceae96decfc5`.
[CI 37583312836](https://github.com/kurasis/CoinControl/actions/runs/37583312836)
запущен полным workflow_dispatch на main: squash унаследовал `[skip ci]` из
прежнего evidence commit, поэтому push run не появился. Дублирующий PR CI
37583215609 остановлен; до отмены он успел сделать Alchemy 12 и DefiLlama 4
запроса (по partial usage artifact). Повторных полных live прогонов не запускали.
Это не publishing-eligible push
run; новый release этой задачей не публикуется. Итоги CI записаны ниже. Windows
installer/native проверки выполняются в GitHub Actions, не в Linux workspace.
Publish workflow обновлён, но публикация release в этой задаче не запускается.

## Совместимость Actions

Официальные releases/action metadata проверены через GitHub API перед обновлением.
[Версии и SHA](ACTIONS_VERSIONS.json): checkout 7.0.1, setup-node 7.0.0,
upload-artifact 7.0.1, download-artifact 8.0.1, github-script 9.0.0,
rust-cache 2.9.2. Все используют `runs.using: node24`; требуются hosted runners
с runtime support (минимум runner 2.327.1).

- Checkout 7 ограничивает unsafe fork code при privileged triggers; здесь используются push/pull_request/workflow_dispatch.
- Setup-node читает прежний node-version-file и npm cache.
- Upload сохраняет стандартный ZIP (`archive: true` по умолчанию), имена и пути artifacts.
- Download поддерживает ZIP artifacts v4, включая старый installer baseline; проверка digest теперь по умолчанию error и не ослабляется.
- Github-script 9 меняет доступ к `require('@actions/github')`; workflow использует injected `github` и builtin `fs`, а не этот несовместимый import.
- Порог первого native экрана 2000 ms, сценарии и budgets не изменены.

## Ранее завершившийся CI

[Аудит CI](../code-audit-2026-10-07/README.md#github-ci-и-восстановленный-доступ)
теперь дополнен итоговыми Windows native artifacts после восстановления доступа.
На исходном commit normal-network startup 3499.30 ms нарушил gate 2000 ms;
остальные 11 production-load checks и cached launches прошли. Отдельный native
recovery/BTC/DPI job прошёл. Live Alchemy 403 / Chainstack 401 / dRPC BNB 429
остаются подтверждёнными внешними ограничениями старого прогона. Эти failures
не объявлены успешными и не скрыты отключением assertions.

## Итог CI на объединённом коде

[CI 37583312836](https://github.com/kurasis/CoinControl/actions/runs/37583312836)
проверил точный `cc2215edb3ef13899dc2e6a230efceae96decfc5`. Шесть jobs
приложения/сборки прошли; live job завершилась failure на прежних внешних
ограничениях. [Provenance/status](ci/CI_STATUS.json) фиксирует SHA, event и jobs.

| Проверка                                                | Результат                                                                                    | Доказательство                                                                          |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Linux checks/offline/build/browser/bindings             | PASS                                                                                         | [Job](https://github.com/kurasis/CoinControl/actions/runs/37583312836/job/112667697581) |
| Linux store load 50 accounts / 500 assets / 100000 legs | PASS; cached reopen/summary 191.40 ms; query p95 <300 ms                                     | [JSON](ci/PERFORMANCE_LINUX.json)                                                       |
| Windows store load, тот же объём                        | PASS; cached reopen/summary 462.74 ms; query p95 <300 ms                                     | [JSON](ci/PERFORMANCE_WINDOWS.json)                                                     |
| Windows NSIS build, upgrade/uninstall                   | PASS; 11 checks                                                                              | [JSON](ci/INSTALLER_REPORT.json)                                                        |
| Production EXE/resource inspection                      | PASS; 64 checks                                                                              | [JSON](ci/RELEASE_REPORT.json)                                                          |
| Windows native recovery, BTC, console, DPI              | PASS; 26 checks; installer test отдельной job                                                | [JSON](ci/NATIVE_REPORT.json)                                                           |
| Installed production startup / large UI / CSV           | PASS; 12 checks; first normal 1456.70 ms, three offline launches 738.20 / 747.20 / 745.70 ms | [JSON](ci/NATIVE_LOAD_REPORT.json)                                                      |
| Live API                                                | 15 suites PASS, 3 FAIL                                                                       | [Отчёт](ci/live/LIVE_REPORT.md), [usage](ci/live/usage.json)                            |

Порог первого полезного native экрана остался 2000 ms. Production application
hash совпадает с извлечённым NSIS payload; native-e2e feature в installer не
включён. Старый normal-network startup failure 3499.30 ms сохранён в отчёте
первого аудита; успешный новый прогон не доказывает отсутствия вариативности
Windows/WebView startup. Данные cached store harness не подменяют process/render
startup measurements.

Live failures: Alchemy Base/Arbitrum/Optimism/Polygon возвращают 403, Chainstack
Solana возвращает 401, dRPC BNB — 429. Alchemy Ethereum, Zerion, Helius, публичные
резервы, источники цен, vertical slice и network suite прошли. В полном live
прогоне каждый provider usage <=50. Отменённый PR успел потратить дополнительно
Alchemy 12 и DefiLlama 4 ([partial usage](ci/CANCELLED_PR_LIVE_USAGE.json)); native
public reads учитываются отдельными native reports. Неуспешные live suites не
перезапускались без изменений доступа/квоты; assertions и бюджеты не ослаблялись.

Actions checkout/setup-node/rust-cache/upload/download исполнены в CI успешно,
включая cross-run download старого installer baseline и новый ZIP digest gate.
Publish-only github-script 9 проверен по official release/API compatibility,
но publishing workflow не запускался. Физический Windows 11 и macOS/iOS здесь
не проверялись; native automation использует hosted Windows Server.
