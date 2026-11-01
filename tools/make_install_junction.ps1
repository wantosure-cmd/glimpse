param([Parameter(Mandatory)][string]$FixtureRoot,[Parameter(Mandatory)][string]$Link,[Parameter(Mandatory)][string]$Target)
$ErrorActionPreference='Stop'
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$taskFixture=(Resolve-Path -LiteralPath $FixtureRoot).Path
if(-not $taskFixture.StartsWith((Join-Path $taskRoot 'target/installation-smoke-'),[StringComparison]::OrdinalIgnoreCase)){throw 'Not an installer smoke fixture'}
if(-not (Test-Path -LiteralPath (Join-Path $taskFixture '.installer-smoke-owned'))){throw 'Missing owned marker'}
$taskLink=[IO.Path]::GetFullPath($Link)
$taskTarget=(Resolve-Path -LiteralPath $Target).Path
foreach($taskPath in @($taskLink,$taskTarget)){
    if(-not $taskPath.StartsWith($taskFixture+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Path escaped owned fixture'}
}
if(Test-Path -LiteralPath $taskLink){throw 'Refusing existing link path'}
New-Item -ItemType Junction -Path $taskLink -Target $taskTarget | Out-Null
