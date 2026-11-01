param(
    [ValidateSet('app','lib-tests','app-tests','setup')][string]$Stage='app',
    [string]$VisualStudioPath,
    [switch]$CheckOnly
)
$ErrorActionPreference='Stop'
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$taskArtifacts=Join-Path $taskRoot 'artifacts'
New-Item -ItemType Directory -Path $taskArtifacts -Force | Out-Null
$taskStamp=[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff')
$taskOldPath=$env:PATH
$taskOldToolchain=$env:RUSTUP_TOOLCHAIN
$taskCargoBin=Join-Path ([Environment]::GetFolderPath('UserProfile')) '.cargo/bin'
if(Test-Path -LiteralPath $taskCargoBin){$env:PATH=$taskCargoBin+';'+$env:PATH}
try {
    if(-not $VisualStudioPath){
        $taskVswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        if(Test-Path -LiteralPath $taskVswhere){
            $VisualStudioPath=(& $taskVswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1)
        }
        # The local Microsoft installer can provision Build Tools without the
        # optional vswhere utility. Accept its standard location as well.
        if(-not $VisualStudioPath){
            $taskStandardVs=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/2022/BuildTools'
            if(Test-Path -LiteralPath (Join-Path $taskStandardVs 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll')){$VisualStudioPath=$taskStandardVs}
        }
    }
    if(-not $VisualStudioPath){throw 'No Visual Studio installation with x64 C++ tools found'}
    $taskDevModule=Join-Path $VisualStudioPath 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll'
    if(-not (Test-Path -LiteralPath $taskDevModule)){throw 'Visual Studio developer environment is not ready'}
    Import-Module $taskDevModule
    Enter-VsDevShell -VsInstallPath $VisualStudioPath -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64'
    $taskChannel=Select-String -LiteralPath (Join-Path $taskRoot 'rust-toolchain.toml') -Pattern '^channel\s*=\s*"([^"]+)"' | Select-Object -First 1
    if(-not $taskChannel){throw 'Missing pinned Rust channel'}
    $taskToolchain=$taskChannel.Matches[0].Groups[1].Value+'-x86_64-pc-windows-msvc'
    # `which` refuses a missing toolchain instead of downloading it. All build
    # stages below use the installed pinned compiler and Cargo's offline mode.
    $taskCargo=(& rustup.exe which --toolchain $taskToolchain cargo)
    if($LASTEXITCODE -ne 0){throw 'Pinned Rust toolchain is not installed; install prerequisites explicitly before building'}
    $env:RUSTUP_TOOLCHAIN=$taskToolchain
    $taskCompiler=(Get-Command cl.exe -ErrorAction Stop).Source
    if(-not $env:WindowsSdkDir -or -not (Test-Path -LiteralPath $env:WindowsSdkDir)){throw 'Windows SDK is not ready'}
    Push-Location -LiteralPath $taskRoot
    try {
        if($CheckOnly){
            $taskRustVersion=(& rustup.exe run $taskToolchain rustc -Vv) -join "`n"
            if($LASTEXITCODE -ne 0){throw 'Pinned rustc did not execute'}
            $taskCargoVersion=(& $taskCargo --version) -join "`n"
            if($LASTEXITCODE -ne 0){throw 'Pinned cargo did not execute'}
            [pscustomobject]@{checks_pass=$true;toolchain=$taskToolchain;rustc=$taskRustVersion;cargo=$taskCargoVersion;visual_studio=$VisualStudioPath;cl=$taskCompiler;sdk=$env:WindowsSdkDir;sdk_version=$env:WindowsSDKVersion;scope='Readiness check only; no build, installation, network access or acceptance claim'} | ConvertTo-Json | Tee-Object -FilePath (Join-Path $taskArtifacts "windows-build-environment-$taskStamp.json")
        } else {
            $taskArguments=switch($Stage){
                'app' {@('build','--release','--locked','--offline','--bin','media-browser')}
                'lib-tests' {@('test','--release','--locked','--offline','--lib','--','--test-threads=1')}
                'app-tests' {@('test','--release','--locked','--offline','--bin','media-browser','--','--test-threads=1')}
                'setup' {@()}
            }
            $taskLog=Join-Path $taskArtifacts "windows-$Stage-$taskStamp.log"
            $taskCode=0
            try {
                if($Stage -eq 'setup'){
                    & (Join-Path $PSScriptRoot 'build_setup.ps1') 2>&1 | Tee-Object -FilePath $taskLog
                } else {
                    & $taskCargo @taskArguments 2>&1 | Tee-Object -FilePath $taskLog
                }
                $taskCode=$LASTEXITCODE
            } catch {
                $_ | Out-String | Tee-Object -FilePath $taskLog -Append
                $taskCode=1
            }
            [pscustomobject]@{stage=$Stage;exit_code=$taskCode;toolchain=$taskToolchain;log=$taskLog;ended_utc=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Tee-Object -FilePath "$taskLog.result.json"
            if($taskCode -ne 0){throw "Windows build stage failed: $Stage; see $taskLog"}
        }
    } finally {Pop-Location}
} finally {
    $env:PATH=$taskOldPath
    $env:RUSTUP_TOOLCHAIN=$taskOldToolchain
}
