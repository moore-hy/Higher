Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Collections.Generic;

public class Win32 {
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc enumProc, IntPtr lParam);
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    
    [DllImport("user32.dll")]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
    
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
    
    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);
}
"@

$windows = New-Object 'System.Collections.Generic.List[object]'

$proc = [Win32+EnumWindowsProc]{
    param($hWnd, $lParam)
    if ([Win32]::IsWindowVisible($hWnd)) {
        $sb = New-Object System.Text.StringBuilder(256)
        [Win32]::GetWindowText($hWnd, $sb, 256) | Out-Null
        $title = $sb.ToString()
        if ($title -ne '') {
            $pid = 0
            [Win32]::GetWindowThreadProcessId($hWnd, [ref]$pid) | Out-Null
            $windows.Add([PSCustomObject]@{
                Title = $title
                HWnd  = $hWnd
                PID   = $pid
            })
        }
    }
    return $true
}

[Win32]::EnumWindows($proc, [IntPtr]::Zero) | Out-Null
$windows | Format-Table -AutoSize
