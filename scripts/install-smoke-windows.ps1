# Production 0.1.0 -> current version upgrade on a populated, app-created schema 6 database.
param([Parameter(Mandatory=$true)][string]$PreviousInstaller)
$ErrorActionPreference = 'Stop'
$reportDir = 'target/release-report'
New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
$targetVersion = (Get-Content 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json).version
$checks = [System.Collections.Generic.List[object]]::new()
$application = $null
function Record($name, $ok, $detail) {
  $checks.Add(@{ name = $name; result = $(if ($ok) { 'PASS' } else { 'FAIL' }); detail = $detail })
  if (-not $ok) { throw $name }
}
function NodeCommand([string[]]$Arguments) {
  & node @Arguments
  if ($LASTEXITCODE -ne 0) { throw 'External offline fixture/snapshot utility failed' }
}
function Install($path) {
  $result = Start-Process -FilePath $path -ArgumentList @('/S', "/D=$installDir") -Wait -PassThru
  if ($result.ExitCode -ne 0) { throw "Installer failed (exit $($result.ExitCode))" }
}
function StopApplication {
  if ($script:application) {
    Stop-Process -Id $script:application.Id -Force -ErrorAction SilentlyContinue
    $script:application = $null
    Start-Sleep -Seconds 1
  }
}
function LaunchSnapshot($label) {
  $script:application = Start-Process $binary.FullName -PassThru
  Start-Sleep -Seconds 8
  $script:application.Refresh()
  if ($script:application.HasExited -or -not (Test-Path $profile)) { throw 'Installed production app failed to open its profile' }
  StopApplication
  $path = Join-Path $reportDir "$label.json"
  NodeCommand @('scripts/acceptance-fixture.mjs', 'snapshot', $profile, $path)
  return Get-Content $path -Raw | ConvertFrom-Json
}
function SamePortfolio($left, $right) {
  return (($left.fingerprints | ConvertTo-Json -Depth 5 -Compress) -ceq ($right.fingerprints | ConvertTo-Json -Depth 5 -Compress)) -and
    (($left.balances | ConvertTo-Json -Depth 5 -Compress) -ceq ($right.balances | ConvertTo-Json -Depth 5 -Compress))
}
$installer = Get-ChildItem 'target/release/bundle/nsis/*.exe' | Select-Object -First 1
$installDir = Join-Path $env:RUNNER_TEMP 'CoinControl-installed'
$profile = Join-Path $env:APPDATA 'com.coincontrol.portfoliodesk/profiles/real.sqlite'
$firewallName = "CoinControl-upgrade-offline-$env:GITHUB_RUN_ID"
$firewallInstalled = $false
$baselineHash = '00b4dc04679d3de435a6c91d494ef1960361a999a01a63b80978915bd621b7c5'
try {
  if (-not $env:CI -or -not $env:RUNNER_TEMP) { throw 'Run only in the disposable Windows CI user profile' }
  if ((Test-Path $profile) -or (Test-Path $installDir)) { throw 'Refusing to overwrite an existing installation or portfolio' }
  if (-not $installer) { throw 'NSIS installer is missing' }
  Record 'Pinned previous production artifact' ((Get-FileHash $PreviousInstaller -Algorithm SHA256).Hash.ToLowerInvariant() -eq $baselineHash) 'main f0603fd; CI 37345501830; SHA-256 verified before execution'
  Install (Resolve-Path $PreviousInstaller).Path
  $binary = Get-ChildItem $installDir -Filter '*.exe' | Where-Object { $_.Name -notmatch 'uninstall' } | Select-Object -First 1
  # The synthetic migration fixture must never be refreshed by public chain data.
  New-NetFirewallRule -Name $firewallName -DisplayName 'CoinControl isolated production upgrade fixture' -Direction Outbound -Program $binary.FullName -Action Block -Profile Any -ErrorAction Stop | Out-Null
  $firewallInstalled = $true
  Record 'Installer fixture network isolated' $true 'Program-specific outbound firewall rule; removed in finally; live API acceptance is separate'
  Record 'Install previous production 0.1.0'  ($binary.VersionInfo.ProductVersion -match '^0\.1\.0(?:\.|$)') "version $($binary.VersionInfo.ProductVersion)"
  $empty = LaunchSnapshot 'upgrade-empty-baseline'
  Record 'Previous app creates an empty schema 6 portfolio' ($empty.schema -eq 6 -and $empty.integrity -eq 'ok' -and $empty.fingerprints.wallets.rows -eq 0) 'Production app creates and opens SQLite; no native-e2e feature'
  NodeCommand @('scripts/acceptance-fixture.mjs', 'seed', $profile)
  $before = LaunchSnapshot 'upgrade-before'
  Record 'Previous app replays populated synthetic portfolio' ($before.integrity -eq 'ok' -and $before.accountingDirty -eq '0' -and $before.ownTransferLegs -eq 2 -and $before.feeCharges -eq 1 -and $before.fingerprints.accounting_overrides.rows -eq 2 -and $before.fingerprints.group_wallets.rows -eq 3) '2 accounts; 2 owned-transfer legs; 1 fee; 2 audit versions; overlapping groups'
  Install $installer.FullName
  $binary = Get-ChildItem $installDir -Filter '*.exe' | Where-Object { $_.Name -notmatch 'uninstall' } | Select-Object -First 1
  Record "Upgrade installs production $targetVersion" ($binary.VersionInfo.ProductVersion -match ('^' + [regex]::Escape($targetVersion) + '(?:\.|$)')) "version $($binary.VersionInfo.ProductVersion)"
  $after = LaunchSnapshot 'upgrade-after'
  Record 'Production launch migrates schema 6 to 7' ($after.schema -eq 7 -and $after.integrity -eq 'ok' -and $after.accountingDirty -eq '0') 'Migration and accounting readiness checked after actual installed app launch'
  Record 'Populated upgrade preserves exact portfolio and audit' (SamePortfolio $before $after) "$(@($after.fingerprints.PSObject.Properties).Count) source/derived table fingerprints plus exact balance quantities, including groups, lots, history, settings and audit versions"
  $reopened = LaunchSnapshot 'upgrade-reopened'
  Record 'Upgraded portfolio survives another process restart' (SamePortfolio $after $reopened) 'No duplicated balance, movement, fee or accounting record'
  $beforeUninstall = (Get-FileHash $profile -Algorithm SHA256).Hash
  $uninstaller = Get-ChildItem $installDir -Filter '*uninstall*.exe' | Select-Object -First 1
  Record 'Uninstaller exists' ([bool]$uninstaller) 'NSIS uninstall executable'
  $uninstall = Start-Process $uninstaller.FullName -ArgumentList '/S' -Wait -PassThru
  NodeCommand @('scripts/acceptance-fixture.mjs', 'snapshot', $profile, (Join-Path $reportDir 'upgrade-uninstalled.json'))
  $uninstalled = Get-Content (Join-Path $reportDir 'upgrade-uninstalled.json') -Raw | ConvertFrom-Json
  Record 'Normal uninstall preserves populated portfolio' ($uninstall.ExitCode -eq 0 -and (Test-Path $profile) -and (Get-FileHash $profile -Algorithm SHA256).Hash -eq $beforeUninstall -and $uninstalled.integrity -eq 'ok' -and (SamePortfolio $reopened $uninstalled)) 'Data and WAL-backed evidence retained without an opt-in deletion request; exact fingerprints and quantities verified after uninstall'
} catch {
  $checks.Add(@{ name = 'Installer scenario'; result = 'FAIL'; detail = $_.Exception.Message })
} finally {
  StopApplication
  if ($firewallInstalled) { Remove-NetFirewallRule -Name $firewallName -ErrorAction Stop }
  @{ mode = 'windows-production-installer'; at = (Get-Date).ToUniversalTime().ToString('o'); sourceSha = $env:ACCEPTANCE_SOURCE_SHA; ciSha = $env:GITHUB_SHA; baseline = @{ version = '0.1.0'; schema = 6; sourceSha = 'f0603fde8a7f4e13d60509ee2c188ffe2486d136'; runId = '37345501830'; sha256 = $baselineHash }; targetVersion = $targetVersion; checks = $checks } | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $reportDir 'INSTALLER_REPORT.json')
}
$checks | Format-Table name, result, detail
if ($checks.result -contains 'FAIL') { exit 1 }
