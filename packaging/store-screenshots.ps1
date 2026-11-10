<#
.SYNOPSIS
    Captures Microsoft Store screenshots of WillPomo on a clean Windows desktop
    (used by the "Store screenshots" GitHub workflow).
#>
param(
    [Parameter(Mandatory)] [string] $Exe,
    [string] $OutDir = 'store/screenshots'
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $OutDir | Out-Null
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class W {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool SystemParametersInfo(int a, int b, string c, int d);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, int x, int y, uint d, UIntPtr e);
    public static void ClickAt(int x, int y) { SetCursorPos(x, y); mouse_event(2, 0, 0, 0, UIntPtr.Zero); mouse_event(4, 0, 0, 0, UIntPtr.Zero); }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    // Close stray system message boxes (class #32770) that aren't ours.
    public static void CloseDialogs(uint keepPid) {
        EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p);
            var c = new System.Text.StringBuilder(256); GetClassName(h, c, 256);
            if (p != keepPid && IsWindowVisible(h) && c.ToString() == "#32770") PostMessage(h, 0x0010, IntPtr.Zero, IntPtr.Zero);
            return true; }, IntPtr.Zero);
    }
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
    public static IntPtr Find(uint pid, string cls) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p);
            var c = new System.Text.StringBuilder(256); GetClassName(h, c, 256);
            if (p == pid && c.ToString() == cls) { found = h; return false; } return true; }, IntPtr.Zero);
        return found;
    }
}
'@

# Bigger screen: the Store needs at least 1366x768.
try { Set-DisplayResolution -Width 1920 -Height 1080 -Force -ErrorAction Stop; Write-Host 'Resolution set to 1920x1080' }
catch { Write-Warning "Could not change resolution: $_" }
Start-Sleep -Seconds 2

# A calm gradient wallpaper in WillPomo's colours.
$b = [System.Windows.Forms.SystemInformation]::VirtualScreen
$wall = Join-Path $env:TEMP 'willpomo-wall.png'
$bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$rect = New-Object System.Drawing.Rectangle 0, 0, $b.Width, $b.Height
$brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect, ([System.Drawing.Color]::FromArgb(255, 250, 236, 228)), ([System.Drawing.Color]::FromArgb(255, 226, 238, 232)), 35.0
$g.FillRectangle($brush, $rect); $bmp.Save($wall, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
[void][W]::SystemParametersInfo(20, 0, $wall, 3)   # SPI_SETDESKWALLPAPER
# Hide desktop icons for a clean shot.
Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' HideIcons 1 -ErrorAction SilentlyContinue
Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue   # restarts automatically
Start-Sleep -Seconds 6
# Minimise anything else that's open.
(New-Object -ComObject Shell.Application).MinimizeAll(); Start-Sleep -Seconds 2

# Captures a 1600x900 region that includes the taskbar but not the bottom-right
# corner, where Windows Server prints its edition watermark.
function Shot([string] $Name, [int] $Left = 120, [switch] $ClickDesktop) {
    $keep = if ($p) { [uint32]$p.Id } else { [uint32]0 }
    [W]::CloseDialogs($keep)
    Get-Process SystemProperties*, SystemSettingsAdminFlows -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    # Clicking empty desktop moves keyboard focus off the taskbar (no Start tooltip).
    if ($ClickDesktop) { [W]::ClickAt(1700, 150); Start-Sleep -Milliseconds 800 }
    [void][W]::SetCursorPos(960, 120)   # keep the pointer out of the shot
    Start-Sleep -Milliseconds 800
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
    $w = [Math]::Min(1600, $b.Width); $h = [Math]::Min(900, $b.Height)
    $crop = $bmp.Clone((New-Object System.Drawing.Rectangle $Left, ($b.Height - $h), $w, $h), $bmp.PixelFormat)
    $crop.Save((Join-Path $OutDir "$Name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $crop.Dispose(); $g.Dispose(); $bmp.Dispose()
    Write-Host "Saved $Name.png (${w}x$h)"
}

Get-Process SystemProperties* -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 20   # let the machine's own start-up pop-ups appear and get closed first
[W]::CloseDialogs(0); Get-Process SystemProperties* -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

# 1. A focus session under way (taskbar shows progress + minutes badge).
$p = Start-Process -FilePath $Exe -ArgumentList '--start' -PassThru
Start-Sleep -Seconds 8
$main = [W]::Find([uint32]$p.Id, 'WilliamsAutomation.WillPomo.Main')
[void][W]::SetForegroundWindow($main); Start-Sleep -Seconds 1
Shot '1-focus-session'

# 2. Minimised: the mini timer above the taskbar.
[void][W]::ShowWindow($main, 6); Start-Sleep -Seconds 3
# Give focus to the desktop so the taskbar's Start button doesn't show a tooltip.
[void][W]::SetForegroundWindow([W]::FindWindow('Progman', 'Program Manager')); Start-Sleep -Seconds 2
Shot '2-mini-timer' -Left 0 -ClickDesktop
Stop-Process -Id $p.Id -Force; Start-Sleep -Seconds 2

# 3. A session ending with the silent pop-up: a real 1-minute focus session.
$dir = Join-Path $env:LOCALAPPDATA 'WillPomo'
New-Item -ItemType Directory -Force $dir | Out-Null
"focus_minutes=1`nalert=silent_popup`n" | Set-Content (Join-Path $dir 'settings.ini')
$p = Start-Process -FilePath $Exe -ArgumentList '--start' -PassThru
Start-Sleep -Seconds 64
Shot '3-session-complete-popup' -ClickDesktop
Stop-Process -Id $p.Id -Force
