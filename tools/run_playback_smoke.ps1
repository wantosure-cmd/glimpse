$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
& "$PSScriptRoot/package_preview.ps1"
$taskData='target/playback-smoke-'+[DateTime]::Now.Ticks
$taskStarted=[DateTime]::UtcNow
[pscustomobject]@{checks_pass=$false;state='running';started_utc=$taskStarted} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-playback-verification.json" -Encoding utf8
Get-ChildItem "$taskRoot/test-media/ui-demo" -Recurse -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-source-hashes-before.json" -Encoding utf8
$taskApp=Start-Process -FilePath "$taskRoot/dist/browser-preview/media-browser.exe" -ArgumentList '--playback-smoke','--open','test-media/ui-demo','--data-dir',$taskData -WorkingDirectory $taskRoot -RedirectStandardOutput "$taskRoot/artifacts/p3-stdout.log" -RedirectStandardError "$taskRoot/artifacts/p3-stderr.log" -PassThru
$taskApp.WaitForExit()
$taskEvidence=Get-Item -LiteralPath "$taskRoot/artifacts/p3-playback-smoke.json"
if($taskEvidence.LastWriteTimeUtc -lt $taskStarted){throw 'Missing fresh playback evidence'}
foreach($taskName in @('p3-paused-seek.png','p3-detail-preferences.png','p3-reloaded-video.png')){
    if((Get-Item -LiteralPath "$taskRoot/artifacts/$taskName").LastWriteTimeUtc -lt $taskStarted){throw "Missing fresh screenshot: $taskName"}
}
Get-ChildItem "$taskRoot/test-media/ui-demo" -Recurse -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-source-hashes-after.json" -Encoding utf8
[pscustomobject]@{exit_code=$taskApp.ExitCode;started_utc=$taskStarted;ended_utc=[DateTime]::UtcNow;exe_sha256=(Get-FileHash -LiteralPath "$taskRoot/dist/browser-preview/media-browser.exe" -Algorithm SHA256).Hash;data_dir=$taskData} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-playback-run.json" -Encoding utf8
python -X utf8 "$PSScriptRoot/verify_playback_smoke.py"
if($LASTEXITCODE -ne 0){throw 'Playback verifier failed; inspect original evidence'}
