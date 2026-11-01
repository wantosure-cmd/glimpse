$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$taskFixture=Join-Path $taskRoot 'test-media/input-demo'
if((Test-Path -LiteralPath $taskFixture) -and !(Test-Path -LiteralPath "$taskFixture/.media-browser-fixture")){throw 'Refusing unmarked fixture directory'}
New-Item -ItemType Directory -Force -Path "$taskFixture/子目录" | Out-Null
Set-Content -LiteralPath "$taskFixture/.media-browser-fixture" -Value 'Dedicated copies for native raw-input regression'
Copy-Item -LiteralPath "$taskRoot/test-media/test-h264-aac.mp4" -Destination "$taskFixture/01.mp4"
Copy-Item -LiteralPath "$taskRoot/test-media/test-mpeg4-mp3.avi" -Destination "$taskFixture/02.avi"
foreach($taskIndex in 10..21){Copy-Item -LiteralPath "$taskRoot/test-media/ui-demo/封面.jpg" -Destination "$taskFixture/$taskIndex.jpg"}
Copy-Item -LiteralPath "$taskRoot/test-media/ui-demo/封面.jpg" -Destination "$taskFixture/子目录/图片.jpg"
[IO.File]::WriteAllBytes("$taskFixture/99损坏.mp4",[Text.Encoding]::UTF8.GetBytes('Not a video; dedicated malformed test sample.'))
