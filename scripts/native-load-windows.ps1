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
$payload = @($release.packagedApplications | Where-Object { $_.installerSha256 -eq $hash -and $_.filename -eq 'portfolio-desk.exe' })
if ($payload.Count -ne 1 -or $release.checks.result -contains 'FAIL') { throw 'Production installer has no unique inspected application payload' }
$setup = Start-Process $installer.FullName -ArgumentList @('/S', "/D=$installDir") -Wait -PassThru
if ($setup.ExitCode -ne 0) { throw "Production installation failed ($($setup.ExitCode))" }
$binary = Get-Item (Join-Path $installDir $payload[0].filename) -ErrorAction SilentlyContinue
if (-not $binary) { throw 'Installed production executable is missing' }
if ((Get-FileHash $binary.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -ne $payload[0].applicationSha256) { throw 'Installed production executable differs from the inspected NSIS payload' }
$webviewPolicies = [System.Collections.Generic.List[object]]::new()
function ConfigureWebviewPolicy([string]$Option, [string]$Value) {
  $path = "HKCU:\Software\Policies\Microsoft\Edge\WebView2\$Option"
  $existed = Test-Path $path
  if (-not $existed) { New-Item $path -Force | Out-Null }
  $key = Get-Item $path
  try {
    foreach ($appId in @('com.coincontrol.portfoliodesk', 'portfolio-desk.exe')) {
      $hadValue = $key.GetValueNames() -contains $appId
      $webviewPolicies.Add(@{ path = $path; keyExisted = $existed; name = $appId; hadValue = $hadValue; value = $(if ($hadValue) { $key.GetValue($appId, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } else { $null }); kind = $(if ($hadValue) { $key.GetValueKind($appId) } else { [Microsoft.Win32.RegistryValueKind]::String }) })
      $key.SetValue($appId, $Value, [Microsoft.Win32.RegistryValueKind]::String)
    }
  } finally { $key.Close() }
}
try {
  # Microsoft's per-application WebView2 overrides; original values stay private in memory.
  # Environment overrides were ignored by the installed runtime in the preceding CI attempt.
  $driverPort = $(if ($env:E2E_DRIVER_PORT) { [int]$env:E2E_DRIVER_PORT } else { 4444 })
  ConfigureWebviewPolicy 'AdditionalBrowserArguments' "--remote-debugging-port=$($driverPort + 2) --remote-debugging-address=127.0.0.1"
  ConfigureWebviewPolicy 'UserDataFolder' (Join-Path $env:RUNNER_TEMP 'coincontrol-load-webview')
  $env:E2E_WEBVIEW2_POLICY = '1'
  & cargo run -p portfolio-store --example performance --release -- --native-fixture $profile 'target/native-load-report'
  if ($LASTEXITCODE -ne 0) { throw 'External native load fixture preparation failed' }
  $env:E2E_APP_PATH = $binary.FullName
  $env:E2E_INSTALLER_SHA256 = $hash
  & node scripts/test-native-load-windows.mjs
  if ($LASTEXITCODE -ne 0) { throw 'Production native load acceptance failed; see NATIVE_LOAD_REPORT.json' }
} finally {
  foreach ($saved in $webviewPolicies) {
    $key = Get-Item $saved.path
    try {
      if ($saved.hadValue) { $key.SetValue($saved.name, $saved.value, $saved.kind) }
      else { $key.DeleteValue($saved.name, $false) }
    } finally { $key.Close() }
  }
  foreach ($path in @($webviewPolicies | Where-Object { -not $_.keyExisted } | ForEach-Object path | Select-Object -Unique)) {
    if ((Get-Item $path).GetValueNames().Count -eq 0) { Remove-Item $path }
  }
  $uninstaller = Get-ChildItem $installDir -Filter '*uninstall*.exe' | Select-Object -First 1
  if ($uninstaller) { Start-Process $uninstaller.FullName -ArgumentList @('/S') -Wait | Out-Null }
}
