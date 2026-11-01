$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
& "$PSScriptRoot/package_preview.ps1"
Get-ChildItem "$taskRoot/test-media/ui-demo" -Recurse -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p2-source-hashes-before.json" -Encoding utf8
$taskData='target/ui-smoke-'+[DateTime]::Now.Ticks
$started=[DateTime]::UtcNow
$taskApp=Start-Process -FilePath "$taskRoot/dist/browser-preview/media-browser.exe" -ArgumentList '--browser-smoke','--open','test-media/ui-demo','--data-dir',$taskData -WorkingDirectory $taskRoot -RedirectStandardOutput "$taskRoot/artifacts/p2-browser-stdout.log" -RedirectStandardError "$taskRoot/artifacts/p2-browser-stderr.log" -PassThru
$taskApp.WaitForExit()
foreach($name in @('p2-browser-smoke.json','p2-list.png','p2-image-detail.png','p2-videos.png','p2-error-and-sidebar.png')){
    $evidence=Get-Item -LiteralPath "$taskRoot/artifacts/$name"
    if($evidence.LastWriteTimeUtc -lt $started){throw "Missing or stale evidence: $name"}
}
[pscustomobject]@{exit_code=$taskApp.ExitCode;started_utc=$started;ended_utc=[DateTime]::UtcNow;exe_sha256=(Get-FileHash -LiteralPath "$taskRoot/dist/browser-preview/media-browser.exe" -Algorithm SHA256).Hash;data_dir=$taskData} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p2-browser-run.json" -Encoding utf8
Get-ChildItem "$taskRoot/test-media/ui-demo" -Recurse -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p2-source-hashes-after.json" -Encoding utf8
if($taskApp.ExitCode -ne 0){throw "Browser exited $($taskApp.ExitCode)"}
Get-Content "$taskRoot/artifacts/p2-browser-run.json"
python -X utf8 "$PSScriptRoot/verify_browser_smoke.py"
if($LASTEXITCODE -ne 0){throw 'Browser UI verifier failed'}
