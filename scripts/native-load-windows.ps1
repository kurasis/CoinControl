# Install and exercise the current production artifact in a disposable CI user.
$ErrorActionPreference = 'Stop'
if (-not $env:CI -or -not $env:RUNNER_TEMP) { throw 'Run only in disposable Windows CI' }
$installDir = Join-Path $env:RUNNER_TEMP 'CoinControl-native-load-installed'
$profile = Join-Path $env:APPDATA 'com.coincontrol.portfoliodesk/profiles/real.sqlite'
if ((Test-Path $installDir) -or (Test-Path $profile)) { throw 'Refusing to replace an existing installation or portfolio' }
$installer = Get-ChildItem 'target/native-load-installer/*.exe' | Select-Object -First 1
if (-not $installer) { throw 'Current production installer artifact is missing' }
$release = Get-Content 'target/native-load-production/RELEASE_REPORT.json' -Raw | ConvertFrom-Json
$hash = (Get-FileHash $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
if (-not ($release.checks | Where-Object { $_.name -like 'PE artifact:*' -and $_.detail -eq "sha256 $hash" -and $_.result -eq 'PASS' })) { throw 'Production installer hash does not match release inspection' }
$setup = Start-Process $installer.FullName -ArgumentList @('/S', "/D=$installDir") -Wait -PassThru
if ($setup.ExitCode -ne 0) { throw "Production installation failed ($($setup.ExitCode))" }
$binary = Get-ChildItem $installDir -Filter '*.exe' | Where-Object { $_.Name -notmatch 'uninstall' } | Select-Object -First 1
if (-not $binary) { throw 'Installed production executable is missing' }
try {
  & cargo run -p portfolio-store --example performance --release -- --native-fixture $profile 'target/native-load-report'
  if ($LASTEXITCODE -ne 0) { throw 'External native load fixture preparation failed' }
  $env:E2E_APP_PATH = $binary.FullName
  & node scripts/test-native-load-windows.mjs
  if ($LASTEXITCODE -ne 0) { throw 'Production native load acceptance failed; see NATIVE_LOAD_REPORT.json' }
} finally {
  $uninstaller = Get-ChildItem $installDir -Filter '*uninstall*.exe' | Select-Object -First 1
  if ($uninstaller) { Start-Process $uninstaller.FullName -ArgumentList @('/S') -Wait | Out-Null }
}
