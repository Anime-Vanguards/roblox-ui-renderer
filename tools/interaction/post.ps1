# Posts a sequence of mouse window messages to the Studio viewport (no cursor movement).
#   powershell -File post.ps1 -Title "MyPlace" -Width 3090 -Height 1727 -Actions '<json>'
# (or -ActionsFile path.json for long sequences)
# Actions: [{"op":"move","x":..,"y":..,"keys":0}, {"op":"down"|"up","button":1,"x":..,"y":..},
#           {"op":"wheel","delta":120,"x":..,"y":..}, {"op":"leave"}, {"op":"sleep","ms":..}] in physical px.
param(
	[Parameter(Mandatory = $true)][string]$Title,
	[Parameter(Mandatory = $true)][int]$Width,
	[Parameter(Mandatory = $true)][int]$Height,
	[string]$Actions = "",
	[string]$ActionsFile = ""
)

$ErrorActionPreference = "Stop"
if ($ActionsFile -ne "") {
	$Actions = Get-Content -Raw $ActionsFile
}
$source = Get-Content -Raw (Join-Path $PSScriptRoot "WinInput.cs")
Add-Type -TypeDefinition $source -Language CSharp

$hwnd = [WinInput]::FindViewport($Title, $Width, $Height)
if ($hwnd -eq 0) {
	Write-Error "No Studio viewport of ${Width}x${Height} found for title '$Title'"
	exit 1
}
$log = @("hwnd=$hwnd origin=$([WinInput]::Origin($hwnd)) cursor=$([WinInput]::Cursor())")
foreach ($action in (ConvertFrom-Json $Actions)) {
	$keys = if ($action.keys) { [int]$action.keys } else { 0 }
	switch ($action.op) {
		"move" { $ok = [WinInput]::Move($hwnd, [int]$action.x, [int]$action.y, $keys) }
		"down" { $ok = [WinInput]::Button($hwnd, [int]$action.button, $true, [int]$action.x, [int]$action.y, $keys) }
		"up" { $ok = [WinInput]::Button($hwnd, [int]$action.button, $false, [int]$action.x, [int]$action.y, $keys) }
		"wheel" { $ok = [WinInput]::Wheel($hwnd, [int]$action.delta, [int]$action.x, [int]$action.y, $keys, [bool]$action.horizontal) }
		"leave" { $ok = [WinInput]::Leave($hwnd) }
		"sleep" { Start-Sleep -Milliseconds ([int]$action.ms); $ok = $true }
		"batch" {
			$data = New-Object System.Collections.Generic.List[int]
			foreach ($item in $action.items) {
				$code = switch ($item.op) { "move" { 1 } "down" { 2 } "up" { 3 } "wheel" { if ($item.horizontal) { 5 } else { 4 } } }
				$data.Add([int]$code); $data.Add([int]$item.button); $data.Add([int]$item.x); $data.Add([int]$item.y); $data.Add([int]$item.keys); $data.Add([int]$item.delta)
			}
			$ok = [WinInput]::PostBatch($hwnd, $data.ToArray())
		}
	}
	$log += "$($action.op) $($action.x),$($action.y) -> $ok"
}
$log += "cursor=$([WinInput]::Cursor())"
$log -join "`n"
