# Аудит кода — 7 октября 2026

Исходный commit: `e196814334fdd529024abbc662771539c19ae2a7` (`main`, 0.1.6).
Рабочее дерево перед аудитом было чистым. Незавершённые изменения пользователя
не обнаружены. Проверки выполнены в Linux cloud workspace: Node 24.19.0,
Rust 1.97.0, npm 11.9.0; CI использует Node 22 из `.nvmrc`.

Изучены `AGENTS.md`, `docs/spec/README.md`, архитектура и требования спецификации,
манифесты и lockfiles, Tauri commands/capabilities, CI и команды запуска.
Ручная проверка сосредоточена на настройках/профилях, приватности, UI-форматировании,
HTTP-ошибках, кэшах SQLite, восстановлении, виртуализации и инструментах сборки.
Это ограниченный аудит, а не доказательство отсутствия остальных дефектов.

## Исходное состояние

До изменений все доступные основные проверки прошли:

| Проверка               | Результат                                               | Доказательство            |
| ---------------------- | ------------------------------------------------------- | ------------------------- |
| `npm run check`        | TypeScript, ESLint, Prettier, rustfmt, Clippy PASS      | [Лог](BASELINE_CHECK.txt) |
| `npm run test:offline` | 172 детерминированных Rust-теста, 37 Vitest-тестов PASS | [Лог](BASELINE_TESTS.txt) |
| `npm run build`        | TypeScript и production Vite build PASS                 | [Лог](BASELINE_BUILD.txt) |
| `npm audit --json`     | 0 известных уязвимостей по ответу npm                   | [JSON](NPM_AUDIT.json)    |

Ещё 18 функций live-тестов возвращаются при `RUN_LIVE_API_TESTS=0` и отображаются
Cargo как успешные. Здесь они **не считаются** проверками реальных API.
Проверка npm не охватывает Rust advisories; `cargo audit` в этой среде не установлен.

## План и внесённые исправления

1. Сначала воспроизвести проблемы в целевых тестах, не расходуя квоты API.
2. Исправить подтверждённые сбои внутри существующих интерфейсов.
3. Удалить подтверждённую заглушку и лишние прямые зависимости, обновить комментарии.
4. Повторить основные проверки, генерацию DTO и browser layout; доставить код в GitHub.

P1 — риск приватности или потери пользовательского состояния; P2 — воспроизводимый
сбой или неверный контракт; P3 — сопровождение и небольшое упрощение.

| Приоритет | Файл                                                                                 | Причина и исправление                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| --------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| P1        | `src/app/AppContext.tsx`                                                             | Несколько изменений до следующего render используют один старый `settings`; параллельные полные записи и поздние ответы перетирают изменения. Патчи объединяются с текущим Query cache, сохранения выполняются последовательно, промежуточный ответ не заменяет следующий optimistic state. Ошибка последнего сохранения перечитывает сохранённые настройки. Перед сменой профиля ожидаются незавершённые сохранения, чтобы они не попали в следующий store. |
| P1        | `src/app/AppContext.tsx`                                                             | `invalidateQueries()` оставляет баланс и inactive queries предыдущего профиля в кэше до загрузки нового. После успешной смены используется `resetQueries()`, включая отмену старых reads. При отказе смены профиля текущие данные сохраняются.                                                                                                                                                                                                               |
| P1        | `src/app/AppContext.tsx`                                                             | Пока настройки не загружены, `privacy_mode ?? false` разрешает показ сумм. Во время загрузки значения маскируются; после загрузки действует сохранённая настройка.                                                                                                                                                                                                                                                                                           |
| P1        | `crates/portfolio-providers/src/http.rs`                                             | `serde_json::Error::Display` может включать неверное значение из ответа, в том числе отражённый сервером ключ. `Body::json` сообщает только строку и колонку; тип ошибки, endpoint и успешное декодирование сохранены. Проверено синтетическим sentinel, настоящие ключи не использованы.                                                                                                                                                                    |
| P2        | `scripts/gen-bindings.mjs`                                                           | Удаляет рабочие bindings до выполнения Cargo. При сбое сборки проект остаётся без DTO. Генерация и Prettier выполняются в staging под ignored `target`; прежние файлы заменяются только после успеха. При отказе установки выполняется rollback; staging удаляется и при child failure, код выхода сохраняется.                                                                                                                                              |
| P2        | `src/lib/format.ts`                                                                  | `formatQuantity("0.1", "en-US", 0)` вызывает `repeat(-1)` и падает. Для нулевой точности минимальная единица отображения — `1`; существующие дробные режимы сохранены.                                                                                                                                                                                                                                                                                       |
| P2        | `src/ipc/client.ts`                                                                  | `isCommandError` проверял только наличие полей, поэтому объект в `message` мог попасть в React как текст ошибки. Теперь `code` и `message` должны быть строками; поддерживается существующий mock error без `detail`.                                                                                                                                                                                                                                        |
| P3        | `src/lib/format.ts`                                                                  | `signOf("-0.00")` возвращал отрицательный знак; нулю возвращается 0. Удалён избыточный внутренний union alias, сводившийся к `string`. Точная decimal-арифметика не менялась.                                                                                                                                                                                                                                                                                |
| P3        | `src/components/WindowedTableBody.tsx`                                               | Повторно извлекал тот же набор virtual items внутри каждой строки. Один набор используется в текущем render; размеры, overscan, focus и порядок строк сохранены.                                                                                                                                                                                                                                                                                             |
| P3        | `src/pages/SettingsPage.tsx`                                                         | Абзац Chainstack вложен в phrasing-only `span`. Контейнер заменён на `div` с прежним классом.                                                                                                                                                                                                                                                                                                                                                                |
| P3        | `crates/portfolio-store/Cargo.toml`, `src-tauri/Cargo.toml`, `Cargo.lock`            | В store нет использования `tracing`, в shell — `thiserror`. Проверены Rust source/build/tests/examples, feature flags, re-exports и генерация. Удалены только две прямые зависимости; они остаются в lockfile как зависимости других crates, версии не обновлены.                                                                                                                                                                                            |
| P3        | `scripts/not-yet.mjs`                                                                | Остаток раннего этапа с сообщениями, что native tests и release verifier ещё не реализованы. Реальные scripts подключены в package.json/CI/README; ссылок или loader на заглушку нет. Удалена только эта незаявленная временная заглушка.                                                                                                                                                                                                                    |
| P3        | `crates/portfolio-store/src/wallets.rs`, `src/components/TokenIcon.tsx`, `README.md` | Комментарий add_account обещал возврат существующего аккаунта вместо фактического AccountExists; описание иконок привязано к прошедшему этапу; README перечислял лишь ранних провайдеров и старую версию upgrade. Исправлены описания, поведение сохранено.                                                                                                                                                                                                  |

Новые тесты: `src/app/AppContext.test.tsx`, `src/ipc/client.test.ts`,
`scripts/gen-bindings.test.mjs`; дополнены тесты форматирования и HTTP.
Отрицательные контроли до соответствующих исправлений:
[кэш/настройки](CONTEXT_NEGATIVE_CONTROL.txt), [приватность](PRIVACY_NEGATIVE_CONTROL.txt),
[HTTP JSON](JSON_NEGATIVE_CONTROL.txt), [форматирование/guard](FORMAT_NEGATIVE_CONTROL.txt),
[генератор](BINDINGS_NEGATIVE_CONTROL.txt), [сохранение при смене профиля](PROFILE_SAVE_NEGATIVE_CONTROL.txt).

Динамические Tauri commands, сгенерированные permissions/DTO, i18n-ключи, CSS/font
imports, lazy imports, CLI/build-time dependencies и исторические отчёты сохранены.
Отсутствие прямого import не использовалось как единственное основание удаления.
Пользовательские базы, ключи, backups и исторические отчёты не удалялись.
В среде удалён только генерируемый `target/debug/incremental`: при повторной Rust
сборке закончилось место. Первый промежуточный прогон также выявил нарушение
React render purity в новом test probe; probe исправлен через effect.

## Вопросы, отложенные при первоначальном аудите

| Приоритет | Файл                                                                | Причина и решение                                                                                                                                                                                                                                                       |
| --------- | ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P2        | `crates/portfolio-store/src/portfolio.rs`                           | Дневная UTC-сетка может начинаться после первой операции внутри дня. Добавление точки первого события меняет исторический ряд и period performance. Запрошено согласование; до ответа бизнес-логика не меняется.                                                        |
| P2        | `Cargo.toml`                                                        | `rust-version = "1.90"` ниже MSRV закреплённого `sqlx 0.9.0` (`1.94.0` в его manifest). Pin/CI используют проверенный Rust 1.97. Объявленный публичный MSRV не повышен без согласования; сборка на 1.90 не заявляется поддерживаемой.                                   |
| P2        | `crates/portfolio-store/src/settings.rs`, `src/lib/format.ts`       | Сохраняемый timezone не проверяется как IANA; некорректное значение извне/backup может вызвать RangeError в Intl. UI предлагает допустимые зоны. Изменение валидации IPC или политики восстановления требует согласования; здесь контракт принятия настроек не менялся. |
| P3        | `.github/workflows/ci.yml`, `.github/workflows/publish-windows.yml` | GitHub предупреждает об устаревшем Node 20 runtime в actions/checkout@v4, setup-node@v4 и upload-artifact@v4; runner принудительно использует Node 24. Проверки выполняются; major-обновления Actions в этом аудите не применены без согласования.                      |

Все четыре изменения согласованы пользователем и выполнены в
[следующей задаче](../chart-timezone-actions-2026-10-07/README.md). Таблица выше
фиксирует решения первоначального аудита, до этого согласования.

Схема SQLite, accounting engine, маршрутизация/лимиты провайдеров, публичные IPC
команды и DTO сохранены. Release 0.1.6 не перепубликуется: аудит доставляет исходный
код, новый ZIP этой задачей не запрошен.

## Итоговые проверки

| Проверка                                                        | Результат                                                                                 | Доказательство                                                |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| `npm run check`                                                 | TypeScript, ESLint, Prettier, rustfmt, Clippy PASS                                        | [Лог](FINAL_CHECK.txt)                                        |
| `npm run test:offline`                                          | 173 детерминированных Rust-теста и 48 Vitest-тестов PASS; 18 live функций opt-out         | [Лог](FINAL_TESTS.txt)                                        |
| `npm run build`                                                 | TypeScript и production Vite build PASS                                                   | [Лог](FINAL_BUILD.txt)                                        |
| `cargo check --workspace --all-targets --all-features --locked` | Linux compile PASS, включая optional ts/native-e2e flags                                  | [Лог](ALL_FEATURES.txt)                                       |
| `npm run gen:bindings`                                          | 67 export tests PASS; generated DTO files совпадают с main                                | [Лог](GENERATED_BINDINGS.txt)                                 |
| Cargo/Prettier failure fixture                                  | Оба отказа сохраняют прежние bindings, код выхода и удаляют staging; путь с пробелами     | [Лог](BINDINGS_FAILURE_RECOVERY.txt)                          |
| `npm run test:layout:browser`                                   | 24 сценария, 288 page layouts, 48 panel layouts; chart privacy и таблица 10000 строк PASS | [JSON](BROWSER_LAYOUT_REPORT.json), [лог](BROWSER_LAYOUT.txt) |
| `npm run verify:release -- --source-only`                       | Portable source/frontend subset PASS                                                      | [JSON](SOURCE_RELEASE_REPORT.json), [лог](SOURCE_RELEASE.txt) |

После последнего изменения shell-free вызова Prettier повторены targeted lint,
failure fixtures и реальная генерация DTO. Приложение проверено после ожидания
сохранений при переключении профиля. Browser report записан на базовом commit
с `sourceDirty: true`: он отражает рабочее дерево аудита, а не неизменённый release.
Исправления объединены в `main` через [PR #24](https://github.com/kurasis/CoinControl/pull/24). Код приложения: `153c2463f8f2f032e260c916ee25b6d21d1018b5`.
Локальная среда — Linux: native Windows, NSIS upgrade и DPI здесь не запускаются.
Live API не выполняются локально; отдельная CI job имеет свой bounded opt-in бюджет.
Успешные browser/mock проверки не считаются native/реальными API проверками.

## GitHub CI и восстановленный доступ

[CI 37575008756](https://github.com/kurasis/CoinControl/actions/runs/37575008756)
проверяет точный commit приложения `153c2463f8f2f032e260c916ee25b6d21d1018b5`.
Дублирующий PR run 37574997976 остановлен после merge, чтобы повторно не расходовать
квоты API. Доступ восстановлен при следующей задаче. Все jobs завершены; получены также
итоговые Windows native artifacts.

| CI job / проверка                                          | Полученный результат                                                              | Доказательство                                                                         |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Linux checks/offline/build/bindings/browser                | PASS                                                                              | [CI](https://github.com/kurasis/CoinControl/actions/runs/37575008756/job/112641819902) |
| Cached load, Linux: 50 accounts / 500 assets / 100000 legs | PASS                                                                              | [JSON](ci/PERFORMANCE_LINUX.json)                                                      |
| Cached load, Windows: тот же профиль                       | PASS                                                                              | [JSON](ci/PERFORMANCE_WINDOWS.json)                                                    |
| Windows NSIS build + upgrade/uninstall                     | PASS, 11 installer checks                                                         | [JSON](ci/INSTALLER_REPORT.json)                                                       |
| Production payload inspection                              | PASS, 64 checks                                                                   | [JSON](ci/RELEASE_REPORT.json)                                                         |
| Live API                                                   | 15 suites PASS, 3 FAIL; все provider usage <= 50                                  | [Отчёт](ci/live/LIVE_REPORT.md), [usage](ci/live/usage.json)                           |
| Windows native recovery, live BTC и DPI                    | PASS                                                                              | [JSON](ci/NATIVE_REPORT.json)                                                          |
| Production startup / 100000-leg UI/import                  | FAIL: первый normal-network запуск 3499,30 ms > 2000 ms; остальные 11 checks PASS | [JSON](ci/NATIVE_LOAD_REPORT.json)                                                     |

Live failure причины: Alchemy HTTP 403 на Base/Arbitrum/Optimism/Polygon,
Chainstack HTTP 401, dRPC BNB HTTP 429. Zerion, Helius, публичные резервы,
источники цен, vertical slice и network suite прошли. Эти ошибки не объявлены
успешными и не ретраились без изменения доступа/квоты. Отключение проверок,
ослабление assertions или платные планы не применялись.

Во время предыдущей задачи GitHub API временно возвращал `401 Bad credentials`.
Подключение восстановлено; два оставшихся Windows результата теперь подтверждены.
Startup failure остаётся отдельным дефектом производительности: cached process
launches 605,90–647,50 ms прошли, но они не заменяют неудачный первый запуск.
Порог 2000 ms сохранён. Native WebView был готов через 2196 ms по launch log,
полезный экран появился через 3499,30 ms; этот прогон не доказывает,
что задержка вызвана API-провайдерами или новой логикой настроек.
