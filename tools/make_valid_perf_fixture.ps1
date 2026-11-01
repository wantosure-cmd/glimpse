$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$fixture=Join-Path $taskRoot 'target/perf-media-5000'
if((Test-Path -LiteralPath $fixture) -and !(Test-Path -LiteralPath "$fixture/.media-browser-fixture")){throw 'Refusing to overwrite an unmarked directory'}
[IO.Directory]::CreateDirectory($fixture) | Out-Null
[IO.File]::WriteAllText("$fixture/.media-browser-fixture",'Locally duplicated valid decoder fixtures; not private user media.')
for($group=1;$group -le 100;$group++){
    $folder=Join-Path $fixture ('图集 '+$group)
    [IO.Directory]::CreateDirectory($folder) | Out-Null
    for($n=1;$n -le 50;$n++){
        if($n -le 45){$source="$taskRoot/test-media/ui-demo/封面.jpg";$extension='.jpg'}
        elseif($n -le 48){$source="$taskRoot/artifacts/heic-10bit.png";$extension='.png'}
        elseif($n -eq 49){$source="$taskRoot/test-media/upstream-example.heic";$extension='.heic'}
        elseif($group%10 -eq 0){$source="$taskRoot/test-media/test-h264-aac.mp4";$extension='.mp4'}
        else{$source="$taskRoot/test-media/ui-demo/封面.jpg";$extension='.jpg'}
        [IO.File]::Copy($source,(Join-Path $folder ($n.ToString()+$extension)),$true)
    }
}
[pscustomobject]@{path=$fixture;images=4990;videos=10;scope='Valid JPEG/PNG/HEIC/MP4 duplicates for browsing performance; not independent camera samples'} | ConvertTo-Json | Set-Content "$taskRoot/artifacts/p2-valid-fixture.json" -Encoding utf8
Get-Content "$taskRoot/artifacts/p2-valid-fixture.json"
