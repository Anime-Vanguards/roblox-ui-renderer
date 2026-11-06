# Grows the Roblox Studio window whose title contains -Title by -DeltaWidth / -DeltaHeight physical pixels
# (restoring it first if it is maximized, at the same rectangle) without activating it. Used to give the
# viewport a whole-number logical size: a fractional width or height stretches Studio's projection and
# blurs captures (measured 2026-09-28/30; cli/studio.mjs ensureWholeViewport).
#   powershell -File resize.ps1 -Title "MyPlace" -DeltaWidth 1 -DeltaHeight 0
param(
	[Parameter(Mandatory = $true)][string]$Title,
	[int]$DeltaWidth = 0,
	[int]$DeltaHeight = 0
)

$ErrorActionPreference = "Stop"
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public static class StudioResize {
	[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
	[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
	[DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr h);
	[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
	[DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
	[StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
'@
[StudioResize]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null

$studio = Get-Process RobloxStudioBeta -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle.IndexOf($Title, [StringComparison]::OrdinalIgnoreCase) -ge 0 } | Select-Object -First 1
if (-not $studio) {
	Write-Error "No Roblox Studio window with '$Title' in its title"
	exit 1
}
$h = $studio.MainWindowHandle
$r = New-Object StudioResize+RECT
[StudioResize]::GetWindowRect($h, [ref]$r) | Out-Null
if ([StudioResize]::IsZoomed($h)) {
	# SW_SHOWNOACTIVATE (4): restore without taking focus, then put it back on the maximized rectangle
	[StudioResize]::ShowWindow($h, 4) | Out-Null
	Start-Sleep -Milliseconds 200
}
# SWP_NOZORDER | SWP_NOACTIVATE
[StudioResize]::SetWindowPos($h, [IntPtr]::Zero, $r.L, $r.T, ($r.R - $r.L) + $DeltaWidth, ($r.B - $r.T) + $DeltaHeight, 0x0014) | Out-Null
Write-Output ("size={0}x{1}" -f (($r.R - $r.L) + $DeltaWidth), (($r.B - $r.T) + $DeltaHeight))
