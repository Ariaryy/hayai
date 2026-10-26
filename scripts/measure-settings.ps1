# Compare release processes without touching existing Hayai instances or data.
param(
    [Parameter(Mandatory=$true)][string]$BaselineExecutable,
    [Parameter(Mandatory=$true)][string]$CandidateExecutable
)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class HayaiSettingsProbe {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint process);
    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr handle, uint message, UIntPtr word, IntPtr param);
    public static IntPtr MessageWindow(int process) {
        IntPtr after = IntPtr.Zero;
        while ((after = FindWindowEx(new IntPtr(-3), after, "HayaiNativeMessageWindow", null)) != IntPtr.Zero) {
            uint owner;
            GetWindowThreadProcessId(after, out owner);
            if (owner == process) return after;
        }
        return IntPtr.Zero;
    }
}
'@

function Get-MemorySamples($Process) {
    foreach ($sample in 1..3) {
        $Process.Refresh()
        [pscustomobject]@{
            RSSMiB = [math]::Round($Process.WorkingSet64 / 1MB, 2)
            PrivateMiB = [math]::Round($Process.PrivateMemorySize64 / 1MB, 2)
        }
        if ($sample -lt 3) { Start-Sleep -Milliseconds 300 }
    }
}

$scratch = Join-Path ([IO.Path]::GetTempPath()) ('hayai-settings-memory-' + [guid]::NewGuid())
$appData = Join-Path $scratch 'Hayai'
[void](New-Item -ItemType Directory -Path $appData)
$preferences = Join-Path $appData 'settings.json'
Set-Content -LiteralPath $preferences -Encoding utf8NoBOM -Value '{"version":1,"appearance":"dark","keybindings":{"toggle_launcher":"ctrl-alt-f24"}}'

$measurements = foreach ($executable in @($BaselineExecutable, $CandidateExecutable)) {
    $resolved = (Resolve-Path -LiteralPath $executable).Path
    $run = Start-Process -FilePath $resolved -WindowStyle Hidden -PassThru -Environment @{
        LOCALAPPDATA = $scratch
        APPDATA = (Join-Path $scratch 'roaming')
        HAYAI_SETTINGS_PATH = $preferences
    }
    try {
        Start-Sleep -Seconds 15
        $idle = @(Get-MemorySamples $run)
        $messageWindow = [HayaiSettingsProbe]::MessageWindow($run.Id)
        $active = @()
        if ($messageWindow -ne [IntPtr]::Zero) {
            # The native thread validates the ID. Deliver only to this process.
            [void][HayaiSettingsProbe]::PostMessage($messageWindow, 0x0312, [UIntPtr]1, [IntPtr]::Zero)
            # Let initial icon/atlas work settle before sampling the open view.
            Start-Sleep -Seconds 3
            $active = @(Get-MemorySamples $run)
        }
        [pscustomobject]@{
            Executable = $resolved
            ExecutableBytes = (Get-Item -LiteralPath $resolved).Length
            ExecutableSHA256 = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant()
            Idle = $idle
            Active = $active
            ActiveUnavailable = if ($active.Count -eq 0) { 'Owned native window unavailable; check for a global-hotkey registration conflict.' } else { $null }
        }
    } finally {
        if (!$run.HasExited) { Stop-Process -Id $run.Id }
        $run.Dispose()
    }
}
$measurements | ConvertTo-Json -Depth 5
