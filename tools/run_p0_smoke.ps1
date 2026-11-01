$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$exe=Join-Path $taskRoot 'dist/p0-portable/media-browser.exe'
$meter=Join-Path $taskRoot 'target/diagnostics/Release/audio-meter.exe'
$media=Join-Path $taskRoot 'test-media'
$records=@()
foreach($name in @('test-h264-aac.mp4','test-h264-aac.mov','test-mpeg4-mp3.avi','test-h263-aac.3gp')) {
    $out=Join-Path $taskRoot ("artifacts/p0-gpu/"+$name)
    New-Item -ItemType Directory -Force -Path $out | Out-Null
    $taskStarted=[DateTime]::UtcNow
    $taskApp=Start-Process -FilePath $exe -ArgumentList '--smoke','--video',('"'+(Join-Path $media $name)+'"'),'--heic',('"'+(Join-Path $media 'arrow.heic')+'"') -WorkingDirectory $taskRoot -RedirectStandardOutput "$out/stdout.log" -RedirectStandardError "$out/stderr.log" -PassThru
    $taskMeter=Start-Process -FilePath $meter -ArgumentList $taskApp.Id -WindowStyle Hidden -RedirectStandardOutput "$out/audio-peaks.json" -RedirectStandardError "$out/audio-meter-error.log" -PassThru
    $taskApp.WaitForExit()
    $taskMeter.WaitForExit()
    foreach($file in @('p0-gpu-smoke.json','p0-preview.png','p0-detail.png','p0-preview-return.png','p0-first-ui.json')) {
        $evidenceFile=Get-Item -LiteralPath "$taskRoot/artifacts/$file" -ErrorAction Stop
        if($evidenceFile.LastWriteTimeUtc -lt $taskStarted){throw "Stale evidence: $file for $name"}
        Copy-Item -LiteralPath $evidenceFile.FullName -Destination $out
    }
    $records+=[pscustomobject]@{file=$name;exit_code=$taskApp.ExitCode;meter_exit_code=$taskMeter.ExitCode;evidence=$out}
    $records | ConvertTo-Json -AsArray | Set-Content "$taskRoot/artifacts/p0-gpu-runs.json" -Encoding utf8
    Write-Output "Completed $name (exit $($taskApp.ExitCode))"
}
