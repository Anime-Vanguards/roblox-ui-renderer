# Lossless capture of a Roblox Studio viewport (PrintWindow, physical pixels).
#   powershell -File capture.ps1 -Title "MyPlace" -Width 2202 -Height 1727 -Out C:\path\shot.png
# With -Scales "1.5,1,1.25" the size is the viewport's logical size: the first scale whose physical size
# matches a Studio viewport is used and printed ("scale=1.25"), so the monitor's display scale is detected.
param(
	[Parameter(Mandatory = $true)][string]$Title,
	[Parameter(Mandatory = $true)][int]$Width,
	[Parameter(Mandatory = $true)][int]$Height,
	[Parameter(Mandatory = $true)][string]$Out,
	[string]$Scales = ""
)

$ErrorActionPreference = "Stop"
$source = Get-Content -Raw (Join-Path $PSScriptRoot "WinCapture.cs")
Add-Type -TypeDefinition $source -ReferencedAssemblies System.Drawing -Language CSharp

$hwnd = 0
if ($Scales -ne "") {
	$tried = @()
	foreach ($scale in $Scales.Split(",")) {
		$factor = [double]::Parse($scale, [Globalization.CultureInfo]::InvariantCulture)
		$w = [int][Math]::Round($Width * $factor)
		$h = [int][Math]::Round($Height * $factor)
		$tried += "${w}x${h} (${scale}x)"
		$hwnd = [WinCapture]::FindViewport($Title, $w, $h)
		if ($hwnd -ne 0) {
			Write-Output "scale=$scale"
			break
		}
	}
	$sizes = $tried -join ", "
} else {
	$hwnd = [WinCapture]::FindViewport($Title, $Width, $Height)
	$sizes = "${Width}x${Height}"
}
if ($hwnd -eq 0) {
	$seen = [WinCapture]::Describe($Title)
	Write-Error "No Studio viewport of $sizes found for title '$Title'. Studio windows (visible children >= 200 px):`n$seen"
	exit 1
}

[WinCapture]::Capture($hwnd, $Out)
