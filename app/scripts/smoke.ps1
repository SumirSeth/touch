param([ValidateSet('light','dark')]$Theme='light')
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class CaptureNative {
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags);
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
}
"@
[CaptureNative]::SetProcessDPIAware() | Out-Null
Set-Location (Join-Path $PSScriptRoot '../..')
$env:TRACKPAD_GESTURES_THEME=$Theme
$p=Start-Process (Resolve-Path target/release/trackpad-gestures.exe) -PassThru -WindowStyle Normal -RedirectStandardError app/runtime.log
try {
 for($i=0;$i -lt 30;$i++){Start-Sleep -Milliseconds 400;$p.Refresh();if($p.HasExited){throw 'Application exited before showing settings'};if($p.MainWindowHandle -ne 0){break}}
 if($p.MainWindowHandle -eq 0){throw 'No settings window'}
 [CaptureNative]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
 Start-Sleep -Seconds 2
 $rect=New-Object CaptureNative+Rect
 [CaptureNative]::GetWindowRect($p.MainWindowHandle,[ref]$rect) | Out-Null
 $bitmap=New-Object Drawing.Bitmap ($rect.Right-$rect.Left),($rect.Bottom-$rect.Top)
 $g=[Drawing.Graphics]::FromImage($bitmap)
 $dc=$g.GetHdc()
 try {if(-not [CaptureNative]::PrintWindow($p.MainWindowHandle,$dc,2)){throw "Window capture failed"}} finally {$g.ReleaseHdc($dc)}
 New-Item -ItemType Directory -Force app/screenshots | Out-Null
 $bitmap.Save((Join-Path (Get-Location) "app/screenshots/settings-$Theme.png"))
 $g.Dispose();$bitmap.Dispose()
 [CaptureNative]::SendMessage($p.MainWindowHandle,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Seconds 2
 $p.Refresh()
 if($p.HasExited){throw 'Closing settings exited the tray process'}
 "Tray-only working set bytes: $($p.WorkingSet64)"
 $second=Start-Process (Resolve-Path target/release/trackpad-gestures.exe) -PassThru -WindowStyle Hidden
 $second.WaitForExit(5000) | Out-Null
 Start-Sleep -Seconds 1
 $p.Refresh()
 if($p.MainWindowHandle -eq 0){throw 'Second launch did not reopen settings'}
 'Second launch reopened settings; tray-only lifetime passed.'
} finally {
 Start-Process (Resolve-Path target/release/trackpad-gestures.exe) -ArgumentList '--quit' -Wait -WindowStyle Hidden
 Remove-Item Env:TRACKPAD_GESTURES_THEME
}
