# Grows the Roblox Studio window whose title contains -Title by -DeltaWidth / -DeltaHeight physical pixels
# (restoring it first if it is maximized, at the same rectangle) without activating it. Used to give the
# viewport a whole-number logical size: a fractional width or height stretches Studio's projection and
# blurs captures (measured 2026-09-28/30; cli/studio.mjs ensureWholeViewport). The window is looked for on the
# user's desktop, then on the desktop tools/studio/launch.mjs starts Studio on (WinCapture.cs).
#   powershell -File resize.ps1 -Title "MyPlace" -DeltaWidth 1 -DeltaHeight 0
param(
	[Parameter(Mandatory = $true)][string]$Title,
	[int]$DeltaWidth = 0,
	[int]$DeltaHeight = 0
)

$ErrorActionPreference = "Stop"
$source = Get-Content -Raw (Join-Path $PSScriptRoot "WinCapture.cs")
Add-Type -TypeDefinition $source -ReferencedAssemblies System.Drawing -Language CSharp
$result = [WinCapture]::Grow($Title, $DeltaWidth, $DeltaHeight)
if (-not $result) {
	Write-Error "No Roblox Studio window with '$Title' in its title"
	exit 1
}
Write-Output $result
