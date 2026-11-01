$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
& "$PSScriptRoot/package_preview.ps1"
$taskData='target/input-smoke-'+[DateTime]::Now.Ticks
$taskStarted=[DateTime]::UtcNow
[pscustomobject]@{checks_pass=$false;state='running'} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-input-verification.json" -Encoding utf8
Get-ChildItem "$taskRoot/test-media/input-demo" -Recurse -File | Where-Object Extension -in '.jpg','.mp4','.avi' | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-input-hashes-before.json" -Encoding utf8
$taskApp=Start-Process -FilePath "$taskRoot/dist/browser-preview/media-browser.exe" -ArgumentList '--input-smoke','--open','test-media/input-demo','--data-dir',$taskData -WorkingDirectory $taskRoot -RedirectStandardOutput "$taskRoot/artifacts/p3-input-stdout.log" -RedirectStandardError "$taskRoot/artifacts/p3-input-stderr.log" -PassThru
$taskMeter=Start-Process -FilePath "$taskRoot/target/diagnostics/Release/audio-meter.exe" -ArgumentList $taskApp.Id -WindowStyle Hidden -RedirectStandardOutput "$taskRoot/artifacts/ui-input-audio-peaks.json" -RedirectStandardError "$taskRoot/artifacts/ui-input-audio-error.log" -PassThru
$taskApp.WaitForExit()
$taskMeter.WaitForExit()
foreach($taskName in @('p3-input-smoke.json','p3-input-preview.png','p3-input-invalid-video.png','p3-input-seek-notice.png')){
    if((Get-Item -LiteralPath "$taskRoot/artifacts/$taskName").LastWriteTimeUtc -lt $taskStarted){throw "Missing fresh evidence: $taskName"}
}
Get-ChildItem "$taskRoot/test-media/input-demo" -Recurse -File | Where-Object Extension -in '.jpg','.mp4','.avi' | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-input-hashes-after.json" -Encoding utf8
[pscustomobject]@{exit_code=$taskApp.ExitCode;started_utc=$taskStarted;ended_utc=[DateTime]::UtcNow;exe_sha256=(Get-FileHash -LiteralPath "$taskRoot/dist/browser-preview/media-browser.exe" -Algorithm SHA256).Hash;data_dir=$taskData} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-input-run.json" -Encoding utf8
python -X utf8 "$PSScriptRoot/verify_input_smoke.py"
if($LASTEXITCODE -ne 0){throw 'Raw-input native verifier failed'}
