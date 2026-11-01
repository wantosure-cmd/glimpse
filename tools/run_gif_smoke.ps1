$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
& "$PSScriptRoot/package_preview.ps1"
$taskData='target/gif-smoke-'+[DateTime]::Now.Ticks
$taskStarted=[DateTime]::UtcNow
[pscustomobject]@{checks_pass=$false;state='running';started_utc=$taskStarted} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-gif-verification.json" -Encoding utf8
Get-ChildItem "$taskRoot/test-media/gif-demo" -Filter '*.gif' -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-gif-hashes-before.json" -Encoding utf8
$taskApp=Start-Process -FilePath "$taskRoot/dist/browser-preview/media-browser.exe" -ArgumentList '--gif-smoke','--open','test-media/gif-demo','--data-dir',$taskData -WorkingDirectory $taskRoot -RedirectStandardOutput "$taskRoot/artifacts/p3-gif-stdout.log" -RedirectStandardError "$taskRoot/artifacts/p3-gif-stderr.log" -PassThru
$taskApp.WaitForExit()
foreach($taskName in @('p3-gif-smoke.json','p3-gif-frame-0.png','p3-gif-frame-1.png','p3-gif-frame-2.png')){
    if((Get-Item -LiteralPath "$taskRoot/artifacts/$taskName").LastWriteTimeUtc -lt $taskStarted){throw "Missing fresh evidence: $taskName"}
}
Get-ChildItem "$taskRoot/test-media/gif-demo" -Filter '*.gif' -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-gif-hashes-after.json" -Encoding utf8
[pscustomobject]@{exit_code=$taskApp.ExitCode;started_utc=$taskStarted;ended_utc=[DateTime]::UtcNow;exe_sha256=(Get-FileHash -LiteralPath "$taskRoot/dist/browser-preview/media-browser.exe" -Algorithm SHA256).Hash;data_dir=$taskData} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p3-gif-run.json" -Encoding utf8
python -X utf8 "$PSScriptRoot/verify_gif_smoke.py"
if($LASTEXITCODE -ne 0){throw 'GIF native verifier failed'}
