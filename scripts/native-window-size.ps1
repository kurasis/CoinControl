# Resize the actual Tauri HWND client area; no browser viewport emulation.
param([int]$ApplicationPid, [int]$Width, [int]$Height, [double]$PixelRatio = 1)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class NativeWindowSize {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out Rect r);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int z, uint flags);
}
'@
[NativeWindowSize]::SetThreadDpiAwarenessContext([IntPtr]::new(-4)) | Out-Null
$process = Get-Process -Id $ApplicationPid
$handle = $process.MainWindowHandle
if ($handle -eq [IntPtr]::Zero) { throw 'Application HWND is missing' }
$outer = New-Object NativeWindowSize+Rect
$client = New-Object NativeWindowSize+Rect
if (-not [NativeWindowSize]::GetWindowRect($handle, [ref]$outer) -or -not [NativeWindowSize]::GetClientRect($handle, [ref]$client)) { throw 'Cannot measure native HWND' }
$w = [int][Math]::Round($Width * $PixelRatio) + ($outer.Right - $outer.Left) - ($client.Right - $client.Left)
$h = [int][Math]::Round($Height * $PixelRatio) + ($outer.Bottom - $outer.Top) - ($client.Bottom - $client.Top)
if (-not [NativeWindowSize]::SetWindowPos($handle, [IntPtr]::Zero, 0, 0, $w, $h, 0x0044)) { throw 'Cannot resize native HWND' }
@{ requestedClient = @($Width, $Height); dpi = [NativeWindowSize]::GetDpiForWindow($handle); outerPhysical = @($w, $h) } | ConvertTo-Json -Compress
