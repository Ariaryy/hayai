# Measure a release executable using only windows owned by the spawned process.
# Close other Hayai instances first so this process can register Alt+Space.
# Example: ./scripts/measure-calculator.ps1 -Executable ./target/release/hayai.exe
param([Parameter(Mandatory=$true)][string]$Executable)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class HayaiProbe {
    public delegate bool WindowCallback(IntPtr handle, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(WindowCallback callback, IntPtr data);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string name);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint process);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, UIntPtr word, IntPtr param);
    public static IntPtr MessageWindow(int process) {
        IntPtr after=IntPtr.Zero;
        while ((after=FindWindowEx(new IntPtr(-3),after,"HayaiNativeMessageWindow",null)) != IntPtr.Zero) {
            uint owner; GetWindowThreadProcessId(after,out owner);
            if(owner==process) return after;
        }
        return IntPtr.Zero;
    }
    public static IntPtr AppWindow(int process) {
        IntPtr result=IntPtr.Zero;
        EnumWindows((handle,data) => { uint owner; GetWindowThreadProcessId(handle,out owner);
            if(owner==process) {result=handle; return false;} return true;},IntPtr.Zero);
        return result;
    }
}
'@
$run = Start-Process -FilePath $Executable -WindowStyle Hidden -PassThru
try {
    Start-Sleep -Seconds 15
    $messageWindow = [HayaiProbe]::MessageWindow($run.Id)
    if ($messageWindow -eq [IntPtr]::Zero) { throw 'Owned native window not found' }
    $run.Refresh()
    $coldRSS = $run.WorkingSet64
    $coldPrivate = $run.PrivateMemorySize64
    $samples = @()
    foreach ($query in @('1 Mb to MB','1 lb to kg','12pm newyork to ist','12pm london to ist','5 kg * 9.8 m/s^2 to N')) {
        [void][HayaiProbe]::PostMessage($messageWindow,0x0312,[UIntPtr]1,[IntPtr]::Zero)
        Start-Sleep -Milliseconds 700
        $appWindow = [HayaiProbe]::AppWindow($run.Id)
        if ($appWindow -eq [IntPtr]::Zero) { throw 'Owned GPUI window not found' }
        foreach ($character in $query.ToCharArray()) {
            [void][HayaiProbe]::PostMessage($appWindow,0x0102,[UIntPtr]([int]$character),[IntPtr]::Zero)
            Start-Sleep -Milliseconds 25
        }
        Start-Sleep -Milliseconds 700
        $run.Refresh()
        $samples += $run.WorkingSet64
        [void][HayaiProbe]::PostMessage($messageWindow,0x0312,[UIntPtr]1,[IntPtr]::Zero)
        Start-Sleep -Milliseconds 500
    }
    Start-Sleep -Seconds 2
    $run.Refresh()
    [pscustomobject]@{
        Executable=$Executable
        ColdIdleRSSMiB=[math]::Round($coldRSS / 1MB,2)
        ColdIdlePrivateMiB=[math]::Round($coldPrivate / 1MB,2)
        WarmIdlePrivateMiB=[math]::Round($run.PrivateMemorySize64 / 1MB,2)
        IdleRSSMiB=[math]::Round($run.WorkingSet64 / 1MB,2)
        ActiveMedianRSSMiB=[math]::Round(($samples | Sort-Object)[2] / 1MB,2)
        ActiveRSSMiB=@($samples | ForEach-Object { [math]::Round($_ / 1MB,2) })
    } | ConvertTo-Json

} finally {
    if (!$run.HasExited) { Stop-Process -Id $run.Id }
    $run.Dispose()
}
