# Posts a batch of mouse messages to the Studio viewport, then captures a timed series of a region.
#   powershell -File series.ps1 -Title "MyPlace" -Width 3090 -Height 1727 -ActionsFile a.json
#     -Count 40 -IntervalMs 50 -Crop "x,y,w,h" -OutPrefix C:\out\frame-
param(
	[Parameter(Mandatory = $true)][string]$Title,
	[Parameter(Mandatory = $true)][int]$Width,
	[Parameter(Mandatory = $true)][int]$Height,
	[Parameter(Mandatory = $true)][string]$ActionsFile,
	[Parameter(Mandatory = $true)][int]$Count,
	[Parameter(Mandatory = $true)][int]$IntervalMs,
	[Parameter(Mandatory = $true)][string]$Crop,
	[Parameter(Mandatory = $true)][string]$OutPrefix
)

$ErrorActionPreference = "Stop"
Add-Type -TypeDefinition (Get-Content -Raw (Join-Path $PSScriptRoot "WinInput.cs")) -Language CSharp
Add-Type -TypeDefinition (Get-Content -Raw (Join-Path $PSScriptRoot "WinSeries.cs")) -ReferencedAssemblies System.Drawing -Language CSharp

$hwnd = [WinInput]::FindViewport($Title, $Width, $Height)
if ($hwnd -eq 0) {
	Write-Error "No Studio viewport of ${Width}x${Height} found for title '$Title'"
	exit 1
}
$data = New-Object System.Collections.Generic.List[int]
foreach ($item in (ConvertFrom-Json (Get-Content -Raw $ActionsFile))) {
	$code = switch ($item.op) { "move" { 1 } "down" { 2 } "up" { 3 } }
	$data.Add([int]$code); $data.Add([int]$item.button); $data.Add([int]$item.x); $data.Add([int]$item.y); $data.Add([int]$item.keys); $data.Add(0)
}
if ($data.Count -gt 0) {
	[void][WinInput]::PostBatch($hwnd, $data.ToArray())
}
$parts = $Crop.Split(",")
[WinSeries]::Capture($hwnd, $Count, $IntervalMs, [int]$parts[0], [int]$parts[1], [int]$parts[2], [int]$parts[3], $OutPrefix)
