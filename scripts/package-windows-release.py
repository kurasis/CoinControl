"""Package only an inspected installer that passed installed production checks."""
import argparse
import hashlib
import json
import re
import zipfile
from pathlib import Path


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def package(args):
    if not re.fullmatch(r"[0-9]+", args.run_id):
        raise ValueError("Invalid CI run ID")
    if not re.fullmatch(r"[0-9a-f]{40}", args.build_sha):
        raise ValueError("Invalid build source")
    reports = {}
    for name, directory in [
        ("RELEASE_REPORT", args.release_report),
        ("INSTALLER_REPORT", args.release_report),
        ("NATIVE_LOAD_REPORT", args.load_report),
    ]:
        report = json.loads((Path(directory) / (name + ".json")).read_text())
        checks = report.get("checks", [])
        if not checks or any(c.get("result") != "PASS" for c in checks):
            raise ValueError("Required checks did not pass: " + name)
        reports[name] = report
    upgrade = reports["INSTALLER_REPORT"]
    load = reports["NATIVE_LOAD_REPORT"]
    for report in [upgrade, load]:
        if report.get("sourceSha") != args.build_sha or report.get("ciSha") != args.build_sha:
            raise ValueError("Reports do not match the selected build source")
    version = upgrade["targetVersion"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError("Invalid application version")
    installers = list(Path(args.installer).glob("*.exe"))
    if len(installers) != 1:
        raise ValueError("Expected one production installer")
    installer = installers[0]
    if installer.name != f"Portfolio Desk_{version}_x64-setup.exe":
        raise ValueError("Installer version/architecture mismatch")
    data = installer.read_bytes()
    if not data.startswith(b"MZ"):
        raise ValueError("Installer is not a Windows executable")
    inspected = reports["RELEASE_REPORT"]["packagedApplications"]
    if len(inspected) != 1 or sha256(data) != inspected[0]["installerSha256"]:
        raise ValueError("Installer checksum differs from inspected payload")
    if inspected[0]["applicationSha256"] != load["applicationSha256"]:
        raise ValueError("Installed startup checks used another application payload")
    manifest = {
        "product": "Portfolio Desk / CoinControl",
        "version": version,
        "platform": "windows-x64",
        "channel": "prerelease",
        "codeSigned": reports["RELEASE_REPORT"]["signed"],
        "buildSourceSha": args.build_sha,
        "workflowRunId": int(args.run_id),
        "workflowUrl": f"https://github.com/kurasis/CoinControl/actions/runs/{args.run_id}",
        "installer": {"filename": installer.name, "sha256": sha256(data)},
        "checks": {name: len(r["checks"]) for name, r in reports.items()},
        "firstNormalUsefulScreenMs": load["initialNormalNetworkStartup"]["processToUsefulMs"],
        "fullReleaseAcceptance": False,
    }
    readme = f"""Portfolio Desk / CoinControl {version} — Windows x64
Тестовая версия. Распакуйте ZIP и запустите {installer.name}.
Microsoft WebView2 загружается при необходимости; потребуется интернет.
Сохраните свои API-ключи в Настройки → Источники данных.
Ключей и пользовательских данных в архиве нет. Установщик не подписан.

Статус синхронизации виден в боковой панели и разделе Кошельки.
Настройки → Сетевая консоль → Включить сетевую консоль показывает
запросы приложения и HTTP/RPC-статусы без ключей и адресов кошельков.

Доступ Alchemy к каждой выбранной сети включается в Alchemy.
В проверках 0.1.4 Helius и Alchemy Ethereum прошли; Base, Arbitrum,
Optimism и Polygon вернули 403, Zerion — 429. Покрытие новых источников
частичное; ограничения показаны в приложении. Приёмка на физическом
Windows 11 ещё не завершена.

Отчёт: https://github.com/kurasis/CoinControl/blob/main/TEST_REPORT.md
Сведения о сборке: BUILD_INFO.json. Контрольные суммы: SHA256SUMS.txt.

Extract this ZIP and run the setup EXE. This is an unsigned prerelease
installer, not a portable executable. Save your own keys in Settings →
Data sources. No keys or user profiles are included. Full release acceptance
is still pending; see the verification report for current API limitations.
"""
    entries = {
        installer.name: data,
        "README.txt": readme.encode("utf-8"),
        "BUILD_INFO.json": (json.dumps(manifest, indent=2) + "\n").encode("utf-8"),
    }
    entries["SHA256SUMS.txt"] = "".join(
        sha256(content) + "  " + name + "\n" for name, content in entries.items()
    ).encode("utf-8")
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f"CoinControl-{version}-windows-x64.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name, content in entries.items():
            z.writestr(name, content)
    with zipfile.ZipFile(archive) as z:
        if z.testzip() is not None or set(z.namelist()) != set(entries):
            raise ValueError("Archive verification failed")
        for name, content in entries.items():
            if z.read(name) != content:
                raise ValueError("Archive contents changed")
    (output / (archive.name + ".sha256")).write_text(sha256(archive.read_bytes()) + "  " + archive.name + "\n")
    (output / "BUILD_INFO.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Verified {archive.name}: {archive.stat().st_size} bytes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ["run-id", "build-sha", "installer", "release-report", "load-report", "output"]:
        parser.add_argument("--" + name, required=True)
    package(parser.parse_args())
