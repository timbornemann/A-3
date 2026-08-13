param(
    [Parameter(Mandatory = $true)]
    [int]$ProcessId,

    [Parameter(Mandatory = $true)]
    [string]$OutputPath,

    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = 'Stop'
$deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
$desktopProcess = $null

do {
    $desktopProcess = Get-Process -Id $ProcessId -ErrorAction Stop
    $desktopProcess.Refresh()
    if ($desktopProcess.MainWindowHandle -ne [IntPtr]::Zero -and $desktopProcess.MainWindowTitle -like '*A^3*') {
        break
    }
    Start-Sleep -Milliseconds 200
} while ([DateTimeOffset]::UtcNow -lt $deadline)

if ($desktopProcess.MainWindowHandle -eq [IntPtr]::Zero -or $desktopProcess.MainWindowTitle -notlike '*A^3*') {
    throw 'The A^3 native window did not become visible before the smoke timeout.'
}

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class A3NativeWindow {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rectangle {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr handle, out Rectangle rectangle);

    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr handle, IntPtr deviceContext, uint flags);
}
'@

$rectangle = New-Object A3NativeWindow+Rectangle
if (-not [A3NativeWindow]::GetWindowRect($desktopProcess.MainWindowHandle, [ref]$rectangle)) {
    throw 'Windows did not return the A^3 native window bounds.'
}

$width = $rectangle.Right - $rectangle.Left
$height = $rectangle.Bottom - $rectangle.Top
if ($width -lt 720 -or $height -lt 520) {
    throw "The A^3 native window is smaller than its minimum product viewport: ${width}x${height}."
}

Start-Sleep -Milliseconds 1500
$bitmap = New-Object System.Drawing.Bitmap $width, $height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
try {
    $deviceContext = $graphics.GetHdc()
    try {
        if (-not [A3NativeWindow]::PrintWindow($desktopProcess.MainWindowHandle, $deviceContext, 2)) {
            throw 'Windows could not render the A^3 native window into the smoke screenshot.'
        }
    } finally {
        $graphics.ReleaseHdc($deviceContext)
    }
    $bitmap.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)

    $sampleColors = [System.Collections.Generic.HashSet[int]]::new()
    $xStep = [Math]::Max(1, [Math]::Floor($width / 32))
    $yStep = [Math]::Max(1, [Math]::Floor($height / 24))
    for ($x = 0; $x -lt $width; $x += $xStep) {
        for ($y = 0; $y -lt $height; $y += $yStep) {
            $null = $sampleColors.Add($bitmap.GetPixel($x, $y).ToArgb())
        }
    }
} finally {
    $graphics.Dispose()
    $bitmap.Dispose()
}

if ($sampleColors.Count -lt 8) {
    throw 'The A^3 native window screenshot is visually empty.'
}

[ordered]@{
    processId = $ProcessId
    title = $desktopProcess.MainWindowTitle
    width = $width
    height = $height
    distinctSampleColors = $sampleColors.Count
} | ConvertTo-Json -Compress
