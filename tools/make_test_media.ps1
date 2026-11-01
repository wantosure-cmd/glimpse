$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path $PSScriptRoot -Parent
$media = Join-Path $taskRoot 'test-media'
New-Item -ItemType Directory -Force -Path $media | Out-Null
$ffmpeg = (Get-Command ffmpeg).Source
& $ffmpeg -hide_banner -loglevel error -y -f lavfi -i 'testsrc2=size=640x360:rate=24:duration=65' -f lavfi -i 'sine=frequency=440:sample_rate=44100:duration=65' -c:v libx264 -preset ultrafast -crf 25 -pix_fmt yuv420p -c:a aac -shortest "$media/test-h264-aac.mp4"
if ($LASTEXITCODE) { throw 'MP4 generation failed' }
& $ffmpeg -hide_banner -loglevel error -y -i "$media/test-h264-aac.mp4" -c copy "$media/test-h264-aac.mov"
& $ffmpeg -hide_banner -loglevel error -y -i "$media/test-h264-aac.mp4" -c:v mpeg4 -q:v 5 -c:a libmp3lame "$media/test-mpeg4-mp3.avi"
& $ffmpeg -hide_banner -loglevel error -y -i "$media/test-h264-aac.mp4" -vf scale=176:144 -r 15 -c:v h263 -b:v 160k -c:a aac -ar 16000 -ac 1 -b:a 32k "$media/test-h263-aac.3gp"
Copy-Item -LiteralPath "$taskRoot/vendor/extracted/sources/libheif-1.23.6/examples/example.heic" -Destination "$media/upstream-example.heic"
Get-ChildItem -LiteralPath $media -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash | ConvertTo-Json | Set-Content "$media/generated-hashes.json" -Encoding utf8
