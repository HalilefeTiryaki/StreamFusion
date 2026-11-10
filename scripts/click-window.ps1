param(
    [string]$ProcessName = "streamfusion",
    [int]$FromRight = 90,
    [int]$FromTop = 60
)

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public struct WinRect2 { public int Left; public int Top; public int Right; public int Bottom; }

public static class ClickApi {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out WinRect2 rect);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
}
"@

$proc = Get-Process $ProcessName -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
[ClickApi]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 500

$rect = New-Object WinRect2
[ClickApi]::GetWindowRect($proc.MainWindowHandle, [ref]$rect) | Out-Null
$x = $rect.Right - $FromRight
$y = $rect.Top + $FromTop

[ClickApi]::SetCursorPos($x, $y) | Out-Null
Start-Sleep -Milliseconds 150
[ClickApi]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)  # left down
[ClickApi]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)  # left up
Write-Output "clicked at $x,$y (window $($rect.Left),$($rect.Top) - $($rect.Right),$($rect.Bottom))"
