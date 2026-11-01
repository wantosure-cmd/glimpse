$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$root=Join-Path $taskRoot 'test-media/ui-demo'
New-Item -ItemType Directory -Force -Path "$root/图集 1/图片","$root/图集 1/视频","$root/图集 2","$root/手机相册" | Out-Null
$ffmpeg=(Get-Command ffmpeg).Source
& $ffmpeg -hide_banner -loglevel error -y -f lavfi -i 'testsrc2=size=900x600:rate=1' -frames:v 1 "$root/封面.jpg"
for($n=1;$n -le 12;$n++) {
    $source=if($n%2 -eq 0){'heic-upstream.png'}else{'heic-arrow.png'}
    Copy-Item -LiteralPath "$taskRoot/artifacts/$source" -Destination "$root/图集 1/图片/$n.png"
}
Copy-Item -LiteralPath "$taskRoot/test-media/test-h264-aac.mp4" -Destination "$root/图集 1/视频/1.mp4"
Copy-Item -LiteralPath "$taskRoot/test-media/test-h263-aac.3gp" -Destination "$root/图集 1/视频/2.3gp"
Copy-Item -LiteralPath "$taskRoot/test-media/test-mpeg4-mp3.avi" -Destination "$root/图集 2/花絮.avi"
Copy-Item -LiteralPath "$taskRoot/test-media/arrow.heic" -Destination "$root/手机相册/iPhone HEIC.heic"
Copy-Item -LiteralPath "$taskRoot/test-media/RGB_10__128x128.heif" -Destination "$root/手机相册/10bit.heif"
Copy-Item -LiteralPath "$root/封面.jpg" -Destination "$root/手机相册/龘.jpg"
[IO.File]::WriteAllText("$root/图集 2/损坏图片.jpg",'Deliberately corrupt test image')
Write-Output $root
