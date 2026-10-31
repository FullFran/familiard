# Opens a pet on a real Windows desktop, photographs it, and checks that clicks hit the pet
# only where it is solid. Run from the repository root after building.
$ErrorActionPreference = "Stop"
$exe = (Resolve-Path "target/release/familiard.exe").Path
$env:FAMILIARD_HOME = Join-Path $env:RUNNER_TEMP "familiard-smoke"

& $exe add pets/demo
& $exe summon demo --at 200,150
if ($LASTEXITCODE -ne 0) { throw "summon failed" }
Start-Sleep -Seconds 2

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class Probe {
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(POINT p);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    public static string At(int x, int y) {
        var name = new StringBuilder(64);
        GetClassName(WindowFromPoint(new POINT { X = x, Y = y }), name, 64);
        return name.ToString();
    }
}
"@
[Probe]::SetProcessDPIAware() | Out-Null

$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$picture = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
$graphics = [System.Drawing.Graphics]::FromImage($picture)
$graphics.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
$picture.Save("windows-smoke.png", [System.Drawing.Imaging.ImageFormat]::Png)

# The demo's body fills the middle of its box; the top corner of the box is transparent.
$solid = [Probe]::At(200 + 40, 150 + 40)
$transparent = [Probe]::At(200 + 1, 150 + 1)
Write-Output "solid pixel hits '$solid', transparent pixel hits '$transparent'"
if ($solid -ne "familiard") { throw "the pet does not receive clicks on its solid pixels" }
if ($transparent -eq "familiard") { throw "the pet blocks clicks on its transparent pixels" }

& $exe pets
& $exe dismiss demo
Start-Sleep -Seconds 1
if ([Probe]::At(200 + 40, 150 + 40) -eq "familiard") { throw "the pet did not leave" }
Write-Output "ok"
