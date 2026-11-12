# Starts Roblox Studio without taking keyboard focus, or guards an existing one (StudioLaunch.cs). Run through
# tools/studio/launch.mjs.
#   launch.ps1 -Exe <RobloxStudioBeta.exe> -Arguments "<args>" [-GuardMs 45000] [-OnScreen] [-Scale1] [-Width W -Height H]
#   launch.ps1 -GuardPid <pid> [-GuardMs 20000]     watch an existing Studio (around a play session)
#   launch.ps1 -ParkPid <pid>                       move Studio's drawn cursor to the viewport's corner
param(
	[string]$Exe = "",
	[string]$Arguments = "",
	[int]$GuardMs = 45000,
	[switch]$OnScreen,
	[switch]$Scale1,
	[int]$Width = 3200,
	[int]$Height = 1900,
	[int]$GuardPid = 0,
	[int]$ParkPid = 0
)
$ErrorActionPreference = "Stop"
Add-Type -Path (Join-Path $PSScriptRoot "StudioLaunch.cs")
[StudioLaunch]::MakeDpiAware()
if ($ParkPid -gt 0) {
	$main = [StudioLaunch]::MainWindow($ParkPid)
	if ($main -eq [IntPtr]::Zero) { Write-Error "no Studio main window for pid $ParkPid"; exit 1 }
	"parked=" + [StudioLaunch]::ParkCursor($main)
	exit 0
}
$before = [StudioLaunch]::GetForegroundWindow()
if ($GuardPid -gt 0) {
	[StudioLaunch]::Guard($GuardPid, $before, $GuardMs, $false, 0, 0)
	exit 0
}
if ($Scale1) {
	# Qt's per-process display scale: Studio draws at 1x on any monitor
	$env:QT_ENABLE_HIGHDPI_SCALING = "0"
	$env:QT_AUTO_SCREEN_SCALE_FACTOR = "0"
	$env:QT_SCALE_FACTOR = "1"
}
$id = [StudioLaunch]::Start($Exe, $Arguments)
"pid=$id"
[StudioLaunch]::Guard($id, $before, $GuardMs, -not $OnScreen, $Width, $Height)
$main = [StudioLaunch]::MainWindow($id)
if ($main -ne [IntPtr]::Zero) { "parked=" + [StudioLaunch]::ParkCursor($main) }
"foreground=" + [StudioLaunch]::Describe([StudioLaunch]::GetForegroundWindow())
