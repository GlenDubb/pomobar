<#
.SYNOPSIS
    Launches Pomobar in demo mode, checks it keeps running through a whole
    session ending (including the pop-up), and saves screenshots.

.DESCRIPTION
    Used by CI on a real Windows machine. In demo mode "minutes" are seconds,
    so the 25-minute focus session ends after 25 seconds.
#>
param(
    [Parameter(Mandatory)] [string] $Exe,
    [string] $OutDir = 'dist/screenshots'
)

$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $OutDir | Out-Null
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Win {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc f, IntPtr l);
    public static string Describe(uint pid) {
        var sb = new System.Text.StringBuilder();
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            if (p == pid) {
                var c = new System.Text.StringBuilder(256); var t = new System.Text.StringBuilder(512);
                GetClassName(h, c, 256); GetWindowText(h, t, 512);
                sb.AppendLine("  window class='" + c + "' title='" + t + "'");
                EnumChildWindows(h, (ch, l2) => {
                    var t2 = new System.Text.StringBuilder(1024); GetWindowText(ch, t2, 1024);
                    if (t2.Length > 0) sb.AppendLine("    child text: " + t2);
                    return true; }, IntPtr.Zero);
            }
            return true; }, IntPtr.Zero);
        return sb.ToString();
    }
}
'@

function Save-Screen([string] $Name) {
    try {
        $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
        $bmp.Save((Join-Path $OutDir "$Name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $bmp.Dispose()
        Write-Host "Saved $Name.png ($($b.Width)x$($b.Height))"
    } catch {
        Write-Warning "Screenshot failed: $_"
    }
}

function Get-Title([string] $Class) {
    $h = [Win]::FindWindow($Class, $null)
    if ($h -eq [IntPtr]::Zero) { return $null }
    $sb = New-Object System.Text.StringBuilder 256
    [void][Win]::GetWindowText($h, $sb, 256)
    return $sb.ToString()
}

$p = Start-Process -FilePath $Exe -ArgumentList '--demo' -PassThru
Start-Sleep -Seconds 4
if ($p.HasExited) { throw "Pomobar exited early (code $($p.ExitCode))" }

$title = Get-Title 'WilliamsAutomation.Pomobar.Main'
Write-Host "Windows owned by Pomobar (pid $($p.Id)):"
Write-Host ([Win]::Describe([uint32]$p.Id))
if (-not $title) { Save-Screen '0-no-window'; throw 'Main window not found' }
Write-Host "Main window title while running: '$title'"
if ($title -notmatch '^\d\d:\d\d') { throw "Title should start with the time left, got '$title'" }
Save-Screen '1-running'

# The demo focus session (25 s) ends; the pop-up should appear.
Start-Sleep -Seconds 25
if ($p.HasExited) { throw "Pomobar exited during the session change (code $($p.ExitCode))" }
$popup = Get-Title 'WilliamsAutomation.Pomobar.Alert'
if ($null -eq $popup) { throw 'The end-of-session pop-up did not appear' }
Write-Host "Pop-up is showing. Main title now: '$(Get-Title 'WilliamsAutomation.Pomobar.Main')'"
Save-Screen '2-session-ended'

Stop-Process -Id $p.Id -Force
Write-Host 'Smoke test passed.'
