# Drive the real Windows common Save dialog owned by the tested application.
# No app command, file-path override, key, or test-only IPC is involved.
param([Parameter(Mandatory=$true)][int]$ApplicationPid,
      [Parameter(Mandatory=$true)][string]$Destination)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$automation = [System.Windows.Automation.AutomationElement]
$scope = [System.Windows.Automation.TreeScope]
$pidCondition = [System.Windows.Automation.PropertyCondition]::new($automation::ProcessIdProperty, $ApplicationPid)
$classCondition = [System.Windows.Automation.PropertyCondition]::new($automation::ClassNameProperty, '#32770')
$condition = [System.Windows.Automation.AndCondition]::new($pidCondition, $classCondition)
$deadline = (Get-Date).AddSeconds(30)
do {
  $dialog = $automation::RootElement.FindFirst($scope::Children, $condition)
  if ($dialog) { break }
  Start-Sleep -Milliseconds 100
} while ((Get-Date) -lt $deadline)
if (-not $dialog) { throw 'Application-owned native Save dialog did not open' }
# Windows exposes the common dialog before its Shell controls are populated.
# Its filename edit's AutomationId varies across Windows/WebView2 images.
$filename = $null
$deadline = (Get-Date).AddSeconds(20)
do {
  $elements = $dialog.FindAll($scope::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  foreach ($element in $elements) {
    if ($element.Current.ControlType -ne [System.Windows.Automation.ControlType]::Edit) { continue }
    if ($element.Current.AutomationId -notin @('1001', '1148') -and $element.Current.Name -notmatch '^File name') { continue }
    $valuePattern = $null
    if ($element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$valuePattern)) {
      $filename = $valuePattern
      break
    }
  }
  if ($filename) { break }
  Start-Sleep -Milliseconds 100
} while ((Get-Date) -lt $deadline)
if (-not $filename) {
  # Only names/IDs/types in the application-owned dialog; never control values.
  $elements | ForEach-Object { @{ id=$_.Current.AutomationId; name=$_.Current.Name; type=$_.Current.ControlType.ProgrammaticName } } |
    ConvertTo-Json -Depth 3 | Set-Content 'target/native-report/save-dialog-controls.json'
  # Some hosted desktop images omit ValuePattern on the Shell filename edit.
  # Use the common dialog's documented keyboard accelerator, scoped to its HWND.
  Add-Type -AssemblyName System.Windows.Forms
  Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class CoinControlNativeDialog {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
'@
  $handle = [IntPtr]$dialog.Current.NativeWindowHandle
  [CoinControlNativeDialog]::SetForegroundWindow($handle) | Out-Null
  Start-Sleep -Milliseconds 200
  if ([CoinControlNativeDialog]::GetForegroundWindow() -ne $handle) { throw 'Native Save dialog could not acquire keyboard focus' }
  [System.Windows.Forms.Clipboard]::SetText($Destination)
  [System.Windows.Forms.SendKeys]::SendWait('%n')
  [System.Windows.Forms.SendKeys]::SendWait('^a')
  [System.Windows.Forms.SendKeys]::SendWait('^v')
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
  Write-Output 'Native Save dialog accepted the destination through scoped keyboard input'
  exit 0
}
$filename.SetValue($Destination)
$saveCondition = [System.Windows.Automation.PropertyCondition]::new($automation::AutomationIdProperty, '1')
$save = $dialog.FindFirst($scope::Descendants, $saveCondition)
if (-not $save) { throw 'Native Save action is unavailable' }
$save.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
Write-Output 'Native Save dialog accepted the destination'
