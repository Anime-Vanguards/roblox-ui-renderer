# Brings the Roblox Studio window whose title contains -Title to the foreground and prints the window that
# had focus before ("previous=<hwnd>"); -Restore <hwnd> gives the focus back. Studio renders its viewport
# differently while it is not the focused window (measured 2026-09-28: a story 97.47% exact against a
# capture taken unfocused, 99.98% focused; the "stretched capture" of notes/text.md), so captures focus it.
#   powershell -File focus.ps1 -Title "MyPlace"
#   powershell -File focus.ps1 -Restore 123456
param(
	[string]$Title = "",
	[long]$Restore = 0
)

$ErrorActionPreference = "Stop"
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Text;
public static class StudioFocus {
	[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
	[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
	[DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
	[DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
	[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
	[DllImport("user32.dll")] public static extern void keybd_event(byte v, byte s, uint f, UIntPtr e);
	// Windows only lets the foreground process hand focus on; a synthetic Alt press lifts that lock
	public static bool Focus(IntPtr h) {
		if (IsIconic(h)) ShowWindow(h, 9);
		keybd_event(0x12, 0, 0, UIntPtr.Zero);
		keybd_event(0x12, 0, 2, UIntPtr.Zero);
		return SetForegroundWindow(h) || GetForegroundWindow() == h;
	}
}
'@

if ($Restore -ne 0) {
	$h = [IntPtr]$Restore
	if ([StudioFocus]::IsWindow($h)) { [StudioFocus]::Focus($h) | Out-Null }
	exit 0
}

$studio = Get-Process RobloxStudioBeta -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle.IndexOf($Title, [StringComparison]::OrdinalIgnoreCase) -ge 0 } | Select-Object -First 1
if (-not $studio) {
	Write-Error "No Roblox Studio window with '$Title' in its title"
	exit 1
}
$previous = [StudioFocus]::GetForegroundWindow()
$ok = [StudioFocus]::Focus($studio.MainWindowHandle)
Write-Output "previous=$([long]$previous)"
Write-Output "focused=$ok"
