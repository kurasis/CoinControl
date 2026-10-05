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
$filename = $null
foreach ($id in @('1001', '1148')) {
  $idCondition = [System.Windows.Automation.PropertyCondition]::new($automation::AutomationIdProperty, $id)
  $elements = $dialog.FindAll($scope::Descendants, $idCondition)
  foreach ($element in $elements) {
    $valuePattern = $null
    if ($element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$valuePattern)) {
      $filename = $valuePattern
      break
    }
  }
  if ($filename) { break }
}
if (-not $filename) { throw 'Native Save filename control with ValuePattern is unavailable' }
$filename.SetValue($Destination)
$saveCondition = [System.Windows.Automation.PropertyCondition]::new($automation::AutomationIdProperty, '1')
$save = $dialog.FindFirst($scope::Descendants, $saveCondition)
if (-not $save) { throw 'Native Save action is unavailable' }
$save.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
Write-Output 'Native Save dialog accepted the destination'
