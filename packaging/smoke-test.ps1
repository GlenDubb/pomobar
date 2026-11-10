<#
.SYNOPSIS
    Launches WillPomo in demo mode, checks it keeps running through a whole
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
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    public static IntPtr Find(uint pid, string cls) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p);
            var c = new System.Text.StringBuilder(256); GetClassName(h, c, 256);
            if (p == pid && c.ToString() == cls) { found = h; return false; } return true; }, IntPtr.Zero);
        return found;
    }
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc f, IntPtr l);
    // Title of the first top-level window with this class in the process, or null.
    public static string TitleOf(uint pid, string cls) {
        string found = null;
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            var c = new System.Text.StringBuilder(256); GetClassName(h, c, 256);
            if (p == pid && c.ToString() == cls) {
                var t = new System.Text.StringBuilder(512); GetWindowText(h, t, 512);
                found = t.ToString(); return false;
            }
            return true; }, IntPtr.Zero);
        return found;
    }
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
    return [Win]::TitleOf([uint32]$p.Id, $Class)
}

$p = Start-Process -FilePath $Exe -ArgumentList '--demo' -PassThru
Start-Sleep -Seconds 4
if ($p.HasExited) { throw "WillPomo exited early (code $($p.ExitCode))" }

$title = Get-Title 'WilliamsAutomation.WillPomo.Main'
Write-Host "Windows owned by WillPomo (pid $($p.Id)):"
Write-Host ([Win]::Describe([uint32]$p.Id))
if (-not $title) { Save-Screen '0-no-window'; throw 'Main window not found' }
Write-Host "Main window title while running: '$title'"
if ($title -notmatch '^\d\d:\d\d') { throw "Title should start with the time left, got '$title'" }
Save-Screen '1-running'

# The demo focus session (25 s) ends; the pop-up should appear.
Start-Sleep -Seconds 25
if ($p.HasExited) { throw "WillPomo exited during the session change (code $($p.ExitCode))" }
$popup = Get-Title 'WilliamsAutomation.WillPomo.Alert'
if ($null -eq $popup) { throw 'The end-of-session pop-up did not appear' }
Write-Host "Pop-up is showing. Main title now: '$(Get-Title 'WilliamsAutomation.WillPomo.Main')'"
Save-Screen '2-session-ended'

# Minimising should bring up the mini timer above the taskbar.
$main = [Win]::Find([uint32]$p.Id, 'WilliamsAutomation.WillPomo.Main')
[void][Win]::ShowWindow($main, 6)  # SW_MINIMIZE
Start-Sleep -Seconds 2
if ($p.HasExited) { throw "WillPomo exited when minimised (code $($p.ExitCode))" }
if ($null -eq (Get-Title 'WilliamsAutomation.WillPomo.Mini')) { throw 'Mini timer did not appear when minimised' }
Write-Host 'Mini timer is showing.'
Save-Screen '3-mini-mode'
[void][Win]::ShowWindow($main, 9)  # SW_RESTORE
Start-Sleep -Seconds 1
if ($null -ne (Get-Title 'WilliamsAutomation.WillPomo.Mini')) { throw 'Mini timer should close when the window is restored' }
Write-Host 'Mini timer closed on restore.'

Stop-Process -Id $p.Id -Force
Write-Host 'Smoke test passed.'
