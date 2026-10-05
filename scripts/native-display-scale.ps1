# Exercise the Windows Display Settings UI. Never substitute browser zoom for OS DPI.
param([int]$ApplicationPid, [int]$Percent = 0)
$ErrorActionPreference = 'Stop'
if (-not $env:CI -or -not $env:RUNNER_TEMP) { throw 'Display changes are allowed only on the disposable CI desktop' }
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class DisplayMeasurement {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct MonitorInfo { public int Size; public Rect Monitor, Work; public uint Flags; }
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect r);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr h, uint flags);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr h, ref MonitorInfo info);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
}
'@
[DisplayMeasurement]::SetThreadDpiAwarenessContext([IntPtr]::new(-4)) | Out-Null
$priorSettings = @(Get-Process SystemSettings -ErrorAction SilentlyContinue | ForEach-Object Id)
$result = @{ requestedPercent = $Percent; result = 'BLOCKED'; originalPercent = $null; selectedPercent = $null; detail = '' }
try {
  Start-Process 'ms-settings:display'
  $settings = $null
  for ($attempt = 0; $attempt -lt 40; $attempt++) {
    $process = Get-Process SystemSettings -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero } | Select-Object -First 1
    if ($process) { $settings = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle); break }
    Start-Sleep -Milliseconds 250
  }
  if (-not $settings) { throw 'Windows Display Settings has no accessible interactive window' }
  $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ComboBox)
  $scale = $null
  for ($attempt = 0; $attempt -lt 20 -and -not $scale; $attempt++) {
    foreach ($combo in $settings.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)) {
      $selection = $null
      if ($combo.TryGetCurrentPattern([System.Windows.Automation.SelectionPattern]::Pattern, [ref]$selection)) {
        $selected = @($selection.GetCurrentSelection())
        if ($selected.Count -eq 1 -and $selected[0].Current.Name -match '^(\d+)%') {
          $scale = $combo; $result.originalPercent = [int]$Matches[1]; break
        }
      }
    }
    if (-not $scale) { Start-Sleep -Milliseconds 250 }
  }
  if (-not $scale) { throw 'No accessible percentage scale selector; hosted display may not support scale changes' }
  if ($Percent -gt 0 -and $Percent -ne $result.originalPercent) {
    $expand = $scale.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
    $expand.Expand()
    Start-Sleep -Milliseconds 300
    $items = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ListItem))
    $item = $items | Where-Object { $_.Current.Name -match ("^$Percent%\b|^$Percent%(?:\s|$)") } | Select-Object -First 1
    if (-not $item) { throw "Windows does not offer $Percent% on this display" }
    $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
  }
  $target = $(if ($Percent -gt 0) { $Percent } else { $result.originalPercent })
  for ($attempt = 0; $attempt -lt 40; $attempt++) {
    $handle = (Get-Process -Id $ApplicationPid).MainWindowHandle
    $dpi = [DisplayMeasurement]::GetDpiForWindow($handle)
    if ($dpi -eq [int](96 * $target / 100)) { break }
    Start-Sleep -Milliseconds 250
  }
  $result.selectedPercent = $target
  if ($dpi -ne [int](96 * $target / 100)) { throw 'Scale selection did not change application HWND DPI; logoff or a physical display may be required' }
  $result.result = 'PASS'
  $result.detail = 'Windows Settings selection and application HWND DPI agree'
} catch {
  $result.detail = $_.Exception.Message
} finally {
  $handle = (Get-Process -Id $ApplicationPid).MainWindowHandle
  $rect = New-Object DisplayMeasurement+Rect
  $info = New-Object DisplayMeasurement+MonitorInfo
  $info.Size = [System.Runtime.InteropServices.Marshal]::SizeOf($info)
  if (-not [DisplayMeasurement]::GetWindowRect($handle, [ref]$rect) -or -not [DisplayMeasurement]::GetMonitorInfo([DisplayMeasurement]::MonitorFromWindow($handle, 2), [ref]$info)) { throw 'Cannot measure application physical bounds and work area' }
  $result.dpi = [DisplayMeasurement]::GetDpiForWindow($handle)
  $result.outerPhysical = @($rect.Left, $rect.Top, $rect.Right, $rect.Bottom)
  $result.workAreaPhysical = @($info.Work.Left, $info.Work.Top, $info.Work.Right, $info.Work.Bottom)
  Get-Process SystemSettings -ErrorAction SilentlyContinue | Where-Object { $_.Id -notin $priorSettings } | ForEach-Object { Stop-Process -Id $_.Id -ErrorAction SilentlyContinue }
}
$result | ConvertTo-Json -Depth 4 -Compress
