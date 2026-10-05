# Install, launch, upgrade and uninstall the actual production NSIS artifact.
$ErrorActionPreference = 'Stop'
$reportDir = 'target/release-report'
New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
$checks = [System.Collections.Generic.List[object]]::new()
function Record($name, $ok, $detail) {
  $checks.Add(@{ name = $name; result = $(if ($ok) { 'PASS' } else { 'FAIL' }); detail = $detail })
  if (-not $ok) { throw $name }
}
$installer = Get-ChildItem 'target/release/bundle/nsis/*.exe' | Select-Object -First 1
$installDir = Join-Path $env:RUNNER_TEMP 'CoinControl-installed'
$profile = Join-Path $env:APPDATA 'com.coincontrol.portfoliodesk/profiles/real.sqlite'
try {
  if (-not $installer) { throw 'NSIS installer is missing' }
  $install = Start-Process -FilePath $installer.FullName -ArgumentList @('/S', "/D=$installDir") -Wait -PassThru
  Record 'Silent clean installation' ($install.ExitCode -eq 0) "exit $($install.ExitCode)"
  $binary = Get-ChildItem $installDir -Filter '*.exe' | Where-Object { $_.Name -notmatch 'uninstall' } | Select-Object -First 1
  Record 'Installed production application' ([bool]$binary) 'Application executable is installed'
  $application = Start-Process $binary.FullName -PassThru
  Start-Sleep -Seconds 8
  $application.Refresh()
  Record 'Actual release first launch' (-not $application.HasExited -and (Test-Path $profile)) 'Production process remains alive and creates its SQLite profile'
  Stop-Process -Id $application.Id -Force
  Start-Sleep -Seconds 1
  $before = (Get-FileHash $profile -Algorithm SHA256).Hash
  $upgrade = Start-Process -FilePath $installer.FullName -ArgumentList @('/S', "/D=$installDir") -Wait -PassThru
  Record 'Installer upgrade retains portfolio database' ($upgrade.ExitCode -eq 0 -and (Get-FileHash $profile -Algorithm SHA256).Hash -eq $before) 'Existing SQLite file is unchanged by reinstall'
  $uninstaller = Get-ChildItem $installDir -Filter '*uninstall*.exe' | Select-Object -First 1
  Record 'Uninstaller exists' ([bool]$uninstaller) 'NSIS uninstall executable'
  $uninstall = Start-Process $uninstaller.FullName -ArgumentList '/S' -Wait -PassThru
  Record 'Normal uninstall preserves portfolio database' ($uninstall.ExitCode -eq 0 -and (Test-Path $profile) -and (Get-FileHash $profile -Algorithm SHA256).Hash -eq $before) 'Portfolio data is preserved'
} catch {
  $checks.Add(@{ name = 'Installer scenario'; result = 'FAIL'; detail = $_.Exception.Message })
} finally {
  @{ mode = 'windows-production-installer'; at = (Get-Date).ToUniversalTime().ToString('o'); checks = $checks } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $reportDir 'INSTALLER_REPORT.json')
}
$checks | Format-Table name, result, detail
if ($checks.result -contains 'FAIL') { exit 1 }
