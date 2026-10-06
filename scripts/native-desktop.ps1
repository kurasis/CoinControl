# Real display mode changes, restricted to a disposable CI desktop. No registry DPI emulation.
param([ValidateSet('prepare','restore')][string]$Action = 'prepare', [int]$Width = 0, [int]$Height = 0)
$ErrorActionPreference = 'Stop'
if (-not $env:CI -or -not $env:RUNNER_TEMP) { throw 'Display changes require the disposable CI desktop' }
Add-Type @'
using System;
using System.Runtime.InteropServices;
  public class NativeDesktop {
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct Mode {
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string DeviceName;
    public ushort SpecVersion, DriverVersion, Size, DriverExtra;
    public uint Fields;
    public int PositionX, PositionY;
    public uint DisplayOrientation, DisplayFixedOutput;
    public short Color, Duplex, YResolution, TTOption, Collate;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string FormName;
    public ushort LogPixels;
    public uint BitsPerPel, PelsWidth, PelsHeight, DisplayFlags, DisplayFrequency;
    public uint ICMMethod, ICMIntent, MediaType, DitherType, Reserved1, Reserved2, PanningWidth, PanningHeight;
  }
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct Device {
    public uint Size;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string Name;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string Description;
    public uint Flags;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string Id;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string Key;
  }
  [DllImport("user32.dll", EntryPoint="EnumDisplaySettingsExW", ExactSpelling=true, CharSet=CharSet.Unicode, SetLastError=true)] public static extern bool EnumDisplaySettings(string device, int index, ref Mode mode, uint flags);
  [DllImport("user32.dll", EntryPoint="EnumDisplayDevicesW", ExactSpelling=true, CharSet=CharSet.Unicode)] public static extern bool EnumDisplayDevices(string device, uint index, ref Device value, uint flags);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int ChangeDisplaySettingsEx(string device, ref Mode mode, IntPtr window, uint flags, IntPtr param);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
}
'@
[NativeDesktop]::SetThreadDpiAwarenessContext([IntPtr]::new(-4)) | Out-Null
$deviceName = $null
function ModeAt([int]$Index) {
  $mode = New-Object NativeDesktop+Mode
  $mode.Size = [System.Runtime.InteropServices.Marshal]::SizeOf([type][NativeDesktop+Mode])
  if ([NativeDesktop]::EnumDisplaySettings($script:deviceName, $Index, [ref]$mode, 0)) { return $mode }
  return $null
}
$original = ModeAt -1
$devices = @()
for ($i = 0; $i -lt 32; $i++) {
  $device = New-Object NativeDesktop+Device
  $device.Size = [System.Runtime.InteropServices.Marshal]::SizeOf([type][NativeDesktop+Device])
  if (-not [NativeDesktop]::EnumDisplayDevices($null, $i, [ref]$device, 0)) { break }
  if ($device.Flags -band 1) { $devices += $device }
}
if (-not $original) {
  foreach ($device in ($devices | Sort-Object { -($_.Flags -band 4) })) {
    $deviceName = $device.Name
    $original = ModeAt -1
    if ($original) { break }
  }
}
$result = @{ result = 'BLOCKED'; detail = ''; devices = @($devices | ForEach-Object { @{ name = $_.Name; description = $_.Description; flags = $_.Flags } }) }
if (-not $original) {
  # Indirect/headless adapters may only expose resolution changes through modern Settings.
  & (Join-Path $PSScriptRoot 'native-desktop-settings.ps1') -Action $Action -Width $Width -Height $Height
  exit 0
}
$result.originalWidth = $original.PelsWidth
$result.originalHeight = $original.PelsHeight
try {
  $target = $null
  if (($Action -eq 'prepare' -and $original.PelsWidth -ge 1920 -and $original.PelsHeight -ge 1080) -or ($Action -eq 'restore' -and $original.PelsWidth -eq $Width -and $original.PelsHeight -eq $Height)) {
    $target = $original
  } else {
    $modes = @()
    for ($i = 0; $i -lt 1024; $i++) {
      $mode = ModeAt $i
      if (-not $mode) { break }
      if ($Action -eq 'restore') {
        if ($mode.PelsWidth -eq $Width -and $mode.PelsHeight -eq $Height -and $mode.BitsPerPel -eq $original.BitsPerPel) { $modes += $mode }
      } elseif ($mode.PelsWidth -ge 1920 -and $mode.PelsHeight -ge 1080 -and $mode.BitsPerPel -eq $original.BitsPerPel) {
        $modes += $mode
      }
    }
    $target = $modes | Sort-Object @{ Expression = { [long]$_.PelsWidth * $_.PelsHeight } }, DisplayFrequency | Select-Object -First 1
  }
  if (-not $target) { throw 'Display adapter exposes no supported mode for the required native viewport matrix' }
  if ($target.PelsWidth -ne $original.PelsWidth -or $target.PelsHeight -ne $original.PelsHeight) {
    # Zero flags: temporary mode change, without persisting display configuration.
    $changed = [NativeDesktop]::ChangeDisplaySettingsEx($deviceName, [ref]$target, [IntPtr]::Zero, 0, [IntPtr]::Zero)
    if ($changed -ne 0) { throw "Windows rejected supported display mode (code $changed)" }
    Start-Sleep -Milliseconds 500
  }
  $actual = ModeAt -1
  if ($actual.PelsWidth -ne $target.PelsWidth -or $actual.PelsHeight -ne $target.PelsHeight) { throw 'Physical display mode readback differs from the requested mode' }
  $result.width = $actual.PelsWidth
  $result.height = $actual.PelsHeight
  $result.result = 'PASS'
  $result.detail = 'Supported Windows display mode applied and independently read back'
} catch {
  $result.detail = $_.Exception.Message
}
$result | ConvertTo-Json -Depth 4 -Compress
