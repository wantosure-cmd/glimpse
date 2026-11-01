$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$fixture=Join-Path $taskRoot 'target/perf-fixture-5000'
if((Test-Path -LiteralPath $fixture) -and !(Test-Path -LiteralPath "$fixture/.media-browser-fixture")) {throw 'Refusing to overwrite unmarked directory'}
[IO.Directory]::CreateDirectory($fixture) | Out-Null
[IO.File]::WriteAllText("$fixture/.media-browser-fixture",'Synthetic scanner fixture; not decoder test media.')
for($group=1;$group -le 100;$group++) {
    $folder=Join-Path $fixture ('图集 '+$group)
    [IO.Directory]::CreateDirectory($folder) | Out-Null
    for($n=1;$n -le 50;$n++) {
        $extension=if($n%4 -eq 0){'.mov'}else{'.jpg'}
        [IO.File]::WriteAllBytes((Join-Path $folder ($n.ToString()+$extension)),[byte[]](1,2,3))
    }
}
Write-Output $fixture
