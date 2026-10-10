# Match the external driver to the actual installed WebView2 runtime.
param([switch]$RefreshEvergreen, [string]$ReportDirectory = '')
$ErrorActionPreference = 'Stop'
$arm64 = (Get-CimInstance Win32_Processor | Select-Object -First 1).Architecture -eq 12
function Find-WebViewRuntime {
  @("${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application\*\msedgewebview2.exe", "$env:ProgramFiles\Microsoft\EdgeWebView\Application\*\msedgewebview2.exe", "$env:LOCALAPPDATA\Microsoft\EdgeWebView\Application\*\msedgewebview2.exe") | ForEach-Object { Get-Item $_ -ErrorAction SilentlyContinue } | Sort-Object { [version]$_.VersionInfo.ProductVersion } -Descending | Select-Object -First 1
}
if ($RefreshEvergreen -and (-not $env:CI -or -not $env:RUNNER_TEMP)) { throw 'Evergreen refresh requires the disposable CI desktop' }
$runtime = Find-WebViewRuntime
$runtimeReport = @{ mode = 'webview2-runtime-setup'; at = [DateTime]::UtcNow.ToString('o'); refreshRequested = [bool]$RefreshEvergreen; beforeVersion = $(if ($runtime) { $runtime.VersionInfo.ProductVersion } else { $null }); result = 'PASS' }
try {
  if ($RefreshEvergreen -or -not $runtime) {
    # Use the architecture-aware bootstrapper on ARM64, including refreshes;
    # the x64 standalone installer is reserved for x64 CI hosts.
    $runtimeReport.installerUrl = $(if ($RefreshEvergreen -and -not $arm64) { 'https://go.microsoft.com/fwlink/p/?LinkId=2124701' } else { 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' })
    $installer = Join-Path $env:RUNNER_TEMP 'CoinControl-WebView2Setup.exe'
    Invoke-WebRequest $runtimeReport.installerUrl -OutFile $installer
    $signature = Get-AuthenticodeSignature $installer
    $runtimeReport.signatureStatus = $signature.Status.ToString()
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch '(?:^|,\s*)O=Microsoft Corporation(?:,|$)') { throw 'WebView2 installer does not have a valid Microsoft Authenticode signature' }
    $runtimeReport.installerSha256 = (Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    $setup = Start-Process $installer -ArgumentList @('/silent', '/install') -Wait -PassThru
    $runtimeReport.installerExitCode = $setup.ExitCode
    if ($setup.ExitCode -ne 0) { throw "WebView2 setup failed: $($setup.ExitCode)" }
    $runtime = Find-WebViewRuntime
  }
  if (-not $runtime) { throw 'Microsoft WebView2 runtime is unavailable' }
  $runtimeReport.afterVersion = $runtime.VersionInfo.ProductVersion
  $runtimeReport.hostArchitecture = $(if ($arm64) { 'arm64' } else { 'x64' })
  Write-Host "WebView2 runtime before $($runtimeReport.beforeVersion); after $($runtimeReport.afterVersion)"
} catch {
  $runtimeReport.result = 'FAIL'; $runtimeReport.detail = $_.Exception.Message
  throw
} finally {
  if ($ReportDirectory) {
    New-Item -ItemType Directory -Path $ReportDirectory -Force | Out-Null
    $runtimeReport | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $ReportDirectory 'WEBVIEW_RUNTIME_REPORT.json') -Encoding utf8
  }
}
if (-not $runtime) { throw 'Microsoft WebView2 runtime is unavailable' }
$version = $runtime.VersionInfo.ProductVersion
Write-Host "Matching Edge WebDriver to WebView2 runtime $version"
$zip = Join-Path $env:RUNNER_TEMP 'edgedriver.zip'
$dir = Join-Path $env:RUNNER_TEMP 'edgedriver'
$driverArchive = $(if ($arm64) { 'edgedriver_arm64.zip' } else { 'edgedriver_win64.zip' })
Invoke-WebRequest "https://msedgedriver.microsoft.com/$version/$driverArchive" -OutFile $zip
Expand-Archive $zip -DestinationPath $dir
"EDGE_WEBDRIVER_PATH=$dir\msedgedriver.exe" | Out-File -FilePath $env:GITHUB_ENV -Append
