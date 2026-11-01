$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$runtime=Join-Path $taskRoot 'vendor/runtime'
New-Item -ItemType Directory -Force -Path $runtime | Out-Null
$records=@()
foreach($name in @('ffmpeg','ffprobe')) {
    $source=(Get-Command $name -ErrorAction Stop).Source
    $destination=Join-Path $runtime ($name+'.exe')
    Copy-Item -LiteralPath $source -Destination $destination
    $records+=[pscustomobject]@{component=$name;version=((& $source -version | Select-Object -First 1) -join '');acquisition='Existing local WinGet Gyan.FFmpeg installation';upstream='https://www.gyan.dev/ffmpeg/builds/';sha256=(Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash;bytes=(Get-Item -LiteralPath $destination).Length;release_license_audit='pending P5; GPL build'}
    $license=Join-Path (Split-Path (Split-Path $source)) 'LICENSE'
    if(Test-Path -LiteralPath $license){Copy-Item -LiteralPath $license -Destination (Join-Path $runtime ($name+'-LICENSE.txt'))}
}
$records | ConvertTo-Json | Set-Content "$taskRoot/vendor/media-tools.json" -Encoding utf8
Copy-Item -LiteralPath "$taskRoot/vendor/downloads/NotoSansSC-Regular.ttf" -Destination "$runtime/Chinese-full.ttf"
Copy-Item -LiteralPath "$taskRoot/assets/NotoSansSC-OFL.txt" -Destination "$runtime/NotoSansSC-OFL.txt"
