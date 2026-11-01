param(
    [string]$Label='cold',
    [string]$DataDir='target/perf-ui-5000',
    [int]$Seconds=60,
    [string]$Fixture='target/perf-media-5000',
    [string]$ExecutablePath='dist/browser-preview/media-browser.exe',
    [string]$MachineEvidence='',
    [switch]$Controlled4k,
    [switch]$Fling,
    [switch]$RequireNeutralInput,
    [switch]$HoverVideo,
    [switch]$LegacySizingPriority,
    [switch]$RequireProbeMetrics,
    [switch]$JumpToBottom,
    [switch]$LegacyPreparation,
    [switch]$LegacyGeometryPriority,
    [switch]$LegacyJpegPreview,
    [switch]$LegacyNearGeometryPriority,
    [switch]$ScreenshotWorker,
    [switch]$ForegroundTestWindow,
    [switch]$ExperimentalJpegDirect
)
$ErrorActionPreference='Stop'
if($HoverVideo -and $RequireNeutralInput){throw 'Hover and neutral input are distinct protocols'}
if($JumpToBottom -and $Fling){throw 'One bottom jump and repeated flings are separate protocols'}
if($Label -notmatch '^[a-zA-Z0-9_-]+$'){throw 'Label must be a simple filename component'}
$taskRoot=Split-Path $PSScriptRoot -Parent
if(-not ('GlimpseBenchmarkWindowProbe' -as [type])){
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class GlimpseBenchmarkWindowProbe {
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    public static uint ForegroundPid() {uint pid;GetWindowThreadProcessId(GetForegroundWindow(),out pid);return pid;}
    public static bool ActivateOwnWindow(IntPtr window, uint expectedPid) {
        uint pid; GetWindowThreadProcessId(window,out pid);
        if(window==IntPtr.Zero || pid!=expectedPid) return false;
        ShowWindow(window,5); return SetForegroundWindow(window);
    }
}
'@
}
$taskExe=(Resolve-Path -LiteralPath (Join-Path $taskRoot $ExecutablePath)).Path
$taskOutput="artifacts/p2-scroll-$Label.json"
if(Test-Path -LiteralPath (Join-Path $taskRoot $taskOutput)){throw 'Keep previous evidence; choose a new label'}
$taskStarted=[DateTime]::UtcNow
$taskArguments=@('--open',$Fixture,'--data-dir',$DataDir,'--benchmark-scroll',$Seconds,'--benchmark-output',$taskOutput,'--load-metrics',('artifacts/p2-scroll-'+$Label+'-load.json'))
if($ScreenshotWorker){$taskArguments+='--benchmark-screenshot-worker'}
if($Controlled4k){$taskArguments+='--benchmark-surface-4k'}else{$taskArguments+='--fullscreen'}
if($Fling){$taskArguments+='--benchmark-scroll-fling'}
if($HoverVideo){$taskArguments+='--benchmark-hover-video'}
if($LegacySizingPriority){$taskArguments+='--benchmark-legacy-sizing-priority'}
if($JumpToBottom){$taskArguments+='--benchmark-jump-to-bottom'}
if($LegacyPreparation){$taskArguments+='--benchmark-legacy-preparation'}
if($LegacyGeometryPriority){$taskArguments+='--benchmark-legacy-geometry-priority'}
if($LegacyJpegPreview){$taskArguments+='--benchmark-legacy-jpeg-preview'}
if($LegacyNearGeometryPriority){$taskArguments+='--benchmark-legacy-near-geometry-priority'}
if($ExperimentalJpegDirect){$taskArguments+='--benchmark-experimental-jpeg-direct'}
$taskApp=Start-Process -FilePath $taskExe -ArgumentList $taskArguments -WorkingDirectory $taskRoot -RedirectStandardOutput "$taskRoot/artifacts/p2-scroll-$Label-stdout.log" -RedirectStandardError "$taskRoot/artifacts/p2-scroll-$Label-stderr.log" -PassThru
$taskProcessHandle=$taskApp.Handle # Retain the process handle before Refresh/exit (Windows PowerShell 5.1).
$taskTimer=[Diagnostics.Stopwatch]::StartNew()
$taskSamples=[Collections.Generic.List[object]]::new()
$taskActivationAttempts=0
$taskActivationSucceeded=$false
while(-not $taskApp.HasExited){
    $taskApp.Refresh()
    if(-not $taskApp.HasExited){
        if($ForegroundTestWindow -and -not $taskActivationSucceeded -and $taskActivationAttempts -lt 3 -and $taskTimer.Elapsed.TotalSeconds -lt 5 -and $taskApp.MainWindowHandle -ne [IntPtr]::Zero){
            $taskActivationAttempts++
            $taskActivationSucceeded=[GlimpseBenchmarkWindowProbe]::ActivateOwnWindow($taskApp.MainWindowHandle,[uint32]$taskApp.Id)
        }
        $taskSamples.Add([pscustomobject]@{elapsed_ms=$taskTimer.Elapsed.TotalMilliseconds;working_set_bytes=$taskApp.WorkingSet64;private_bytes=$taskApp.PrivateMemorySize64;cpu_seconds=$taskApp.TotalProcessorTime.TotalSeconds;foreground_pid=[GlimpseBenchmarkWindowProbe]::ForegroundPid();app_foreground=([GlimpseBenchmarkWindowProbe]::ForegroundPid() -eq $taskApp.Id);app_minimized=([GlimpseBenchmarkWindowProbe]::IsIconic($taskApp.MainWindowHandle))})
    }
    Start-Sleep -Milliseconds 250
    if($taskTimer.Elapsed.TotalSeconds -gt ($Seconds+120)){
        # Keep interrupted observations separate from completed benchmark evidence.
        # Do not activate/terminate the process or synthesize readiness/focus scores.
        $taskInterruptedPath=Join-Path $taskRoot "artifacts/p2-scroll-$Label-interrupted-process.json"
        if(Test-Path -LiteralPath $taskInterruptedPath){throw 'Keep previous interrupted evidence; choose a new label'}
        $taskInterrupted=[pscustomobject]@{
            status='timeout-process-still-live';benchmark_complete=$false;acceptance_pass=$false
            started_utc=$taskStarted;observed_utc=[DateTime]::UtcNow
            exe_sha256=(Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash
            exe_path=$taskExe;pid=$taskApp.Id;label=$Label;data_dir=$DataDir;fixture=$Fixture
            requested_seconds=$Seconds;observation_elapsed_ms=$taskTimer.Elapsed.TotalMilliseconds
            foreground_activation_requested=[bool]$ForegroundTestWindow
            foreground_activation_attempts=$taskActivationAttempts;foreground_activation_succeeded=$taskActivationSucceeded
            peak_working_set_bytes=($taskSamples | Measure-Object working_set_bytes -Maximum).Maximum
            samples=$taskSamples
            scope='Interrupted process observations only; no complete benchmark interval or valid readiness/focus score. Process remains live; inspect exact identity before further action.'
        }
        $taskInterrupted | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $taskInterruptedPath -Encoding utf8
        throw "Diagnostic process still live: PID $($taskApp.Id); samples saved to $taskInterruptedPath; inspect before restarting"
    }
}
$taskApp.WaitForExit()
$taskEvidence=Get-Item -LiteralPath (Join-Path $taskRoot $taskOutput)
if($taskEvidence.LastWriteTimeUtc -lt $taskStarted){throw 'Stale benchmark evidence'}
$taskCpuName=Get-ItemPropertyValue -LiteralPath 'HKLM:\HARDWARE\DESCRIPTION\System\CentralProcessor\0' -Name ProcessorNameString
$taskWindows=Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$taskResult=[pscustomobject]@{
    exit_code=$taskApp.ExitCode;started_utc=$taskStarted;ended_utc=[DateTime]::UtcNow
    exe_sha256=(Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash
    exe_path=$taskExe
    cpu=$taskCpuName;logical_processors=[Environment]::ProcessorCount
    windows_product=$taskWindows.ProductName;windows_build="$($taskWindows.CurrentBuildNumber).$($taskWindows.UBR)"
    hardware_evidence=$(if($MachineEvidence){$MachineEvidence}else{'artifacts/p2-machine-hardware.json'})
    fixture_storage_evidence=$(if($MachineEvidence){$MachineEvidence}else{'artifacts/p2-fixture-storage.json'})
    label=$Label;data_dir=$DataDir;fixture=$Fixture
    controlled_4k_requested=[bool]$Controlled4k;controlled_4k_verified=$false
    neutral_input_required=[bool]$RequireNeutralInput;neutral_input_verified=$false
    hover_video_required=[bool]$HoverVideo;hover_video_verified=$false
    foreground_activation_requested=[bool]$ForegroundTestWindow;foreground_activation_attempts=$taskActivationAttempts;foreground_activation_succeeded=$taskActivationSucceeded
    viewport_scope=$(if($Controlled4k){'Requested native3840x2160 client/FBO; verify actual samples. Physical monitor may be smaller; no display settings changed'}else{'Current desktop fullscreen; dimensions must be read from native samples'})
    peak_working_set_bytes=($taskSamples | Measure-Object working_set_bytes -Maximum).Maximum
    samples=$taskSamples
}
$taskResult | ConvertTo-Json -Depth 5 | Set-Content "$taskRoot/artifacts/p2-scroll-$Label-process.json" -Encoding utf8
if($taskApp.ExitCode -ne 0){throw "Benchmark exited $($taskApp.ExitCode)"}
$taskUi=Get-Content -LiteralPath (Join-Path $taskRoot $taskOutput) -Raw | ConvertFrom-Json
$taskFocusSamples=@($taskSamples | Where-Object {$_.elapsed_ms -ge ($taskUi.samples[0].app_elapsed_ms+500) -and $_.elapsed_ms -le ($taskUi.samples[-1].app_elapsed_ms-500)})
$taskResult | Add-Member -NotePropertyName benchmark_foreground_sample_fraction -NotePropertyValue $(if($taskFocusSamples.Count){@($taskFocusSamples | Where-Object app_foreground).Count/[double]$taskFocusSamples.Count}else{$null})
$taskResult | Add-Member -NotePropertyName foreground_scope -NotePropertyValue $(if($ForegroundTestWindow){'At most3 initial activation attempts on this owned test-process window only; no cursor/input/system-setting changes. Subsequent250ms foreground samples are read-only, excluding500ms edges.'}else{'Read-only Win32 foreground PID samples250ms; no activation performed; approximate app/process clock alignment excluding500ms edges'})
$taskResult | ConvertTo-Json -Depth 5 | Set-Content "$taskRoot/artifacts/p2-scroll-$Label-process.json" -Encoding utf8
if($taskUi.error){throw "Native benchmark failed: $($taskUi.error)"}
if($taskUi.samples.Count -eq 0 -or $taskUi.samples[-1].elapsed_ms -lt $Seconds*1000){throw 'Native benchmark did not complete its requested duration'}
if($JumpToBottom -and ($taskUi.pattern -ne 'jump_to_current_bottom' -or $null -eq $taskUi.jump_target_scroll_y)){
    throw 'Native executable did not execute the requested one-time bottom jump'
}
if($RequireProbeMetrics -and $taskUi.view_preparation_mode){
    $taskExpectedPreparation=if($LegacyPreparation){'rebuild-view'}else{'reuse-validated-view'}
    if($taskUi.view_preparation_mode -ne $taskExpectedPreparation){throw 'Native preparation mode differs from requested protocol'}
}
if($LegacyPreparation -and $taskUi.view_preparation_mode -ne 'rebuild-view'){throw 'Native executable lacks requested legacy preparation control'}
if($LegacyNearGeometryPriority -or $null -ne $taskUi.near_geometry_priority){
    $taskExpectedNearGeometry=if($LegacyNearGeometryPriority){4}else{3}
    if($taskUi.near_geometry_priority -ne $taskExpectedNearGeometry){throw 'Native near geometry priority differs from requested protocol'}
    $taskResult | Add-Member -NotePropertyName near_geometry_priority -NotePropertyValue $taskUi.near_geometry_priority
    $taskResult | ConvertTo-Json -Depth 5 | Set-Content "$taskRoot/artifacts/p2-scroll-$Label-process.json" -Encoding utf8
}
if($LegacyJpegPreview -or $null -ne $taskUi.list_jpeg_preview_mode){
    $taskExpectedJpegPreview=if($ExperimentalJpegDirect -and -not $LegacyJpegPreview){'direct-clear'}else{'progressive'}
    if($taskUi.list_jpeg_preview_mode -ne $taskExpectedJpegPreview){throw 'Native JPEG preview mode differs from requested protocol'}
    $taskResult | Add-Member -NotePropertyName list_jpeg_preview_mode -NotePropertyValue $taskUi.list_jpeg_preview_mode
    $taskResult | ConvertTo-Json -Depth 5 | Set-Content "$taskRoot/artifacts/p2-scroll-$Label-process.json" -Encoding utf8
}
if($LegacyGeometryPriority -or $null -ne $taskUi.visible_geometry_priority){
    $taskExpectedGeometryPriority=if($LegacyGeometryPriority){2}else{1}
    if($taskUi.visible_geometry_priority -ne $taskExpectedGeometryPriority){throw 'Native visible geometry lane priority differs from requested protocol'}
    $taskResult | Add-Member -NotePropertyName visible_geometry_priority -NotePropertyValue $taskUi.visible_geometry_priority
    $taskResult | ConvertTo-Json -Depth 5 | Set-Content "$taskRoot/artifacts/p2-scroll-$Label-process.json" -Encoding utf8
}
if($RequireProbeMetrics){
    $taskLoad=Get-Content -LiteralPath "$taskRoot/artifacts/p2-scroll-$Label-load.json" -Raw | ConvertFrom-Json
    if($taskLoad.pending_geometry_scope -ne 'possible-next-card-column-span'){throw 'Expected potential-column geometry readiness, not just the label position'}
    $taskExpectedScope=if($LegacySizingPriority){'label-only'}else{'possible-next-card-column-span'}
    if($taskUi.sizing_priority_scope -ne $taskExpectedScope){throw 'Native sizing priority differs from requested A/B protocol'}
    $taskResult | Add-Member -NotePropertyName sizing_priority_scope -NotePropertyValue $taskUi.sizing_priority_scope
    $taskResult | Add-Member -NotePropertyName pending_geometry_scope -NotePropertyValue $taskLoad.pending_geometry_scope
    $taskResult | ConvertTo-Json -Depth 5 | Set-Content "$taskRoot/artifacts/p2-scroll-$Label-process.json" -Encoding utf8
}
if($RequireNeutralInput){
    if($taskUi.input_mode -ne 'pointer-neutral-unfocused'){throw 'Native benchmark lacks controlled neutral input; not a comparable load-only run'}
    $taskLoad=Get-Content -LiteralPath "$taskRoot/artifacts/p2-scroll-$Label-load.json" -Raw | ConvertFrom-Json
    if(@($taskLoad.rows | Where-Object {$null -ne $_.video_first_ready_render_ms}).Count){throw 'Hover playback occurred during requested load-only run; keep as mixed workload evidence'}
    $taskResult.neutral_input_verified=$true
    $taskResult | ConvertTo-Json -Depth 5 | Set-Content "$taskRoot/artifacts/p2-scroll-$Label-process.json" -Encoding utf8
}
if($HoverVideo){
    if($taskUi.input_mode -ne 'controlled-visible-video-hover'){throw 'Expected explicit controlled hover input'}
    $taskLoad=Get-Content -LiteralPath "$taskRoot/artifacts/p2-scroll-$Label-load.json" -Raw | ConvertFrom-Json
    if(@($taskLoad.rows | Where-Object {$null -ne $_.video_first_ready_render_ms}).Count -lt 20){throw 'Insufficient actual ready video rendering in hover run'}
    $taskResult.hover_video_verified=$true
    $taskResult | ConvertTo-Json -Depth 5 | Set-Content "$taskRoot/artifacts/p2-scroll-$Label-process.json" -Encoding utf8
}
if($Controlled4k){
    if($taskUi.samples.Count -eq 0){throw 'No native samples;4K rendering is unverified'}
    foreach($taskSample in $taskUi.samples){
        if([Math]::Abs($taskSample.viewport_points[0]*$taskSample.pixels_per_point-3840) -gt 1 -or [Math]::Abs($taskSample.viewport_points[1]*$taskSample.pixels_per_point-2160) -gt 1){throw 'Native sample framebuffer was not3840x2160; do not count this as4K evidence'}
    }
    $taskResult.controlled_4k_verified=$true
    $taskResult | ConvertTo-Json -Depth 5 | Set-Content "$taskRoot/artifacts/p2-scroll-$Label-process.json" -Encoding utf8
}
[pscustomobject]@{label=$Label;media=$taskUi.media;p95_ms=$taskUi.interval_p95_ms;peak_working_set_mib=$taskResult.peak_working_set_bytes/1MB;frames=$taskUi.samples.Count;viewport=$taskUi.samples[0].viewport_points;scale=$taskUi.samples[0].pixels_per_point} | ConvertTo-Json
