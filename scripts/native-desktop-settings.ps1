# Modern Windows Settings fallback when the graphics driver has no legacy GDI modes.
param([ValidateSet('prepare','restore')][string]$Action = 'prepare', [int]$Width = 0, [int]$Height = 0)
$ErrorActionPreference = 'Stop'
if (-not $env:CI -or -not $env:RUNNER_TEMP) { throw 'Display changes require the disposable CI desktop' }
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class DesktopPixels {
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
}
'@
[DesktopPixels]::SetThreadDpiAwarenessContext([IntPtr]::new(-4)) | Out-Null
$prior = @(Get-Process SystemSettings -ErrorAction SilentlyContinue | ForEach-Object Id)
$result = @{ result = 'BLOCKED'; method = 'Windows Settings UI Automation'; detail = '' }
try {
  Start-Process 'ms-settings:display'
  $settings = $null
  for ($i = 0; $i -lt 40; $i++) {
    $process = Get-Process SystemSettings -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero } | Select-Object -First 1
    if ($process) { $settings = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle); break }
    Start-Sleep -Milliseconds 250
  }
  if (-not $settings) { throw 'Windows Display Settings has no accessible interactive window' }
  $comboCondition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ComboBox)
  $resolution = $null
  for ($i = 0; $i -lt 20 -and -not $resolution; $i++) {
    foreach ($combo in $settings.FindAll([System.Windows.Automation.TreeScope]::Descendants, $comboCondition)) {
      $selection = $null
      if ($combo.TryGetCurrentPattern([System.Windows.Automation.SelectionPattern]::Pattern, [ref]$selection)) {
        $selected = @($selection.Current.GetSelection())
        if ($selected.Count -eq 1 -and $selected[0].Current.Name -match '^(\d+)\s*[x×]\s*(\d+)') {
          $resolution = $combo; $result.originalWidth = [int]$Matches[1]; $result.originalHeight = [int]$Matches[2]; break
        }
      }
    }
    if (-not $resolution) { Start-Sleep -Milliseconds 250 }
  }
  if (-not $resolution) { throw 'No accessible physical resolution selector on this hosted display' }
  if ($Action -eq 'prepare') {
    if ($result.originalWidth -ge 1920 -and $result.originalHeight -ge 1080) { $Width = $result.originalWidth; $Height = $result.originalHeight }
    else { $Width = 1920; $Height = 1080 }
  }
  if ($Width -ne $result.originalWidth -or $Height -ne $result.originalHeight) {
    $resolution.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    Start-Sleep -Milliseconds 300
    $items = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ListItem))
    $item = $items | Where-Object { $_.Current.Name -match ("^$Width\s*[x×]\s*$Height(?:\s|$)") } | Select-Object -First 1
    if (-not $item) { throw "Windows Settings does not offer physical $Width x $Height on this display" }
    $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    $confirmed = $false
    for ($i = 0; $i -lt 20; $i++) {
      $buttons = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button))
      $keep = $buttons | Where-Object { $_.Current.Name -eq 'Keep changes' } | Select-Object -First 1
      if ($keep) { $keep.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); $confirmed = $true; break }
      Start-Sleep -Milliseconds 250
    }
    if (-not $confirmed) { throw 'Resolution change was not confirmed; Windows may revert it automatically' }
  }
  if ([DesktopPixels]::GetSystemMetrics(0) -ne $Width -or [DesktopPixels]::GetSystemMetrics(1) -ne $Height) { throw 'Physical screen readback differs from the selected resolution' }
  $result.width = $Width; $result.height = $Height
  $result.result = 'PASS'
  $result.detail = 'Windows Settings resolution selection verified against physical screen metrics'
} catch { $result.detail = $_.Exception.Message }
finally { Get-Process SystemSettings -ErrorAction SilentlyContinue | Where-Object { $_.Id -notin $prior } | ForEach-Object { Stop-Process -Id $_.Id -ErrorAction SilentlyContinue } }
$result | ConvertTo-Json -Depth 4 -Compress
