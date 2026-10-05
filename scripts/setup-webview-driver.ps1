# Match the external driver to the actual installed WebView2 runtime.
$ErrorActionPreference = 'Stop'
function Find-WebViewRuntime {
  @("${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application\*\msedgewebview2.exe", "$env:ProgramFiles\Microsoft\EdgeWebView\Application\*\msedgewebview2.exe", "$env:LOCALAPPDATA\Microsoft\EdgeWebView\Application\*\msedgewebview2.exe") | ForEach-Object { Get-Item $_ -ErrorAction SilentlyContinue } | Sort-Object { [version]$_.VersionInfo.ProductVersion } -Descending | Select-Object -First 1
}
$runtime = Find-WebViewRuntime
if (-not $runtime) {
  $bootstrap = Join-Path $env:RUNNER_TEMP 'WebView2Setup.exe'
  Invoke-WebRequest 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $bootstrap
  $setup = Start-Process $bootstrap -ArgumentList @('/silent', '/install') -Wait -PassThru
  if ($setup.ExitCode -ne 0) { throw "WebView2 setup failed: $($setup.ExitCode)" }
  $runtime = Find-WebViewRuntime
}
if (-not $runtime) { throw 'Microsoft WebView2 runtime is unavailable' }
$version = $runtime.VersionInfo.ProductVersion
Write-Host "Matching Edge WebDriver to WebView2 runtime $version"
$zip = Join-Path $env:RUNNER_TEMP 'edgedriver.zip'
$dir = Join-Path $env:RUNNER_TEMP 'edgedriver'
Invoke-WebRequest "https://msedgedriver.microsoft.com/$version/edgedriver_win64.zip" -OutFile $zip
Expand-Archive $zip -DestinationPath $dir
"EDGE_WEBDRIVER_PATH=$dir\msedgedriver.exe" | Out-File -FilePath $env:GITHUB_ENV -Append
