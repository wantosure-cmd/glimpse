param([string]$Output='', [switch]$CheckCargoCache, [string]$DeliveryDirectory='')
$ErrorActionPreference='Stop'
$taskRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if(-not $Output){$Output=Join-Path $taskRoot ('artifacts/handoff-preflight-'+[guid]::NewGuid().ToString('N')+'.json')}
$taskOutput=[IO.Path]::GetFullPath($Output)
if(Test-Path -LiteralPath $taskOutput){throw 'Use a new report file; existing reports are never overwritten'}
$taskParent=Split-Path $taskOutput -Parent
if(-not (Test-Path -LiteralPath $taskParent)){New-Item -ItemType Directory -Path $taskParent | Out-Null}
$taskCargo=Get-Content -LiteralPath (Join-Path $taskRoot 'Cargo.toml') -Raw -Encoding utf8
$taskVersion=[regex]::Match($taskCargo,'(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
$taskPin=[regex]::Match((Get-Content -LiteralPath (Join-Path $taskRoot 'rust-toolchain.toml') -Raw -Encoding utf8),'channel\s*=\s*"([^"]+)"').Groups[1].Value
if($DeliveryDirectory){
    $taskDelivery=@(Get-Item -LiteralPath $DeliveryDirectory)
    $taskCandidate=Join-Path $taskDelivery[0].FullName 'delivery.json'
    if(-not (Test-Path -LiteralPath $taskCandidate) -or ((Get-Content -LiteralPath $taskCandidate -Raw -Encoding utf8 | ConvertFrom-Json).application_version -ne $taskVersion)){throw 'Explicit delivery must match Cargo.toml version'}
} else {
$taskDelivery=@(Get-ChildItem -LiteralPath (Join-Path $taskRoot 'dist') -Directory -ErrorAction SilentlyContinue | Where-Object {
    $taskCandidate=Join-Path $_.FullName 'delivery.json'
    (Test-Path -LiteralPath $taskCandidate) -and ((Get-Content -LiteralPath $taskCandidate -Raw -Encoding utf8 | ConvertFrom-Json).application_version -eq $taskVersion)
})
}
if($taskDelivery.Count -ne 1){throw 'Expected exactly one delivery matching Cargo.toml; use -DeliveryDirectory for a separate checkout and delivery'}
$taskPortable=Join-Path $taskDelivery[0].FullName 'media-browser-portable'
$taskPackage=Get-Content -LiteralPath (Join-Path $taskPortable 'package-files.json') -Raw -Encoding utf8 | ConvertFrom-Json
$taskPayloadFailures=[Collections.Generic.List[string]]::new()
foreach($taskFile in $taskPackage){
    $taskFilePath=[IO.Path]::GetFullPath((Join-Path $taskPortable $taskFile.path))
    if(-not $taskFilePath.StartsWith($taskPortable+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Payload path escapes portable folder'}
    if(-not (Test-Path -LiteralPath $taskFilePath) -or (Get-FileHash -LiteralPath $taskFilePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $taskFile.sha256){$taskPayloadFailures.Add($taskFile.path)}
}
$taskOS=Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$taskTools=[ordered]@{}
foreach($taskTool in @('git','python','py','rustup','cargo','rustc','cmake','pwsh')){
    $taskCommand=Get-Command ($taskTool+'.exe') -ErrorAction SilentlyContinue | Select-Object -First 1
    $taskTools[$taskTool]=if($taskCommand){$taskCommand.Source}else{$null}
}
$taskPython=$null
if($taskTools.python -and $taskTools.python -notlike '*\WindowsApps\*'){
    $taskPython=(& $taskTools.python --version 2>&1 | Out-String).Trim()
}
$taskToolchainFound=$false
$taskRustVersion=$null
$taskCargoCache=$null
if($taskTools.rustup){
    $taskInstalled=@(& $taskTools.rustup toolchain list 2>&1)
    $taskToolchain=($taskPin+'-x86_64-pc-windows-msvc')
    $taskToolchainFound=@($taskInstalled | Where-Object {"$_" -match ('^'+[regex]::Escape($taskToolchain)+'(?:\s|$)')}).Count -gt 0
    if($taskToolchainFound){
        $taskRustVersion=(& $taskTools.rustup run $taskToolchain rustc --version 2>&1 | Out-String).Trim()
        if($CheckCargoCache){
            Push-Location -LiteralPath $taskRoot
            try{
                $taskOldPreference=$ErrorActionPreference
                $ErrorActionPreference='Continue'
                $taskMetadata=@(& $taskTools.rustup run $taskToolchain cargo metadata --locked --offline --format-version 1 2>&1)
                $taskMetadataCode=$LASTEXITCODE
                $ErrorActionPreference=$taskOldPreference
                $taskCargoCache=[ordered]@{exit_code=$taskMetadataCode;resolved_offline=($taskMetadataCode -eq 0);note='Metadata resolution only, not a compiler/linker or native-GPU test'}
                if($taskMetadataCode -ne 0){$taskCargoCache['diagnostic']=($taskMetadata | Select-Object -Last 6 | Out-String).Trim()}
            }finally{Pop-Location}
        }
    }
}
$taskVswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$taskVisualStudio=$null
if(Test-Path -LiteralPath $taskVswhere){
    $taskVisualStudio=(& $taskVswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Out-String).Trim()
}
$taskSdk=Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue
if(-not $taskSdk){$taskSdk=Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue}
$taskSdkRoot=if($taskSdk){$taskSdk.KitsRoot10}else{$null}
$taskPythonReady=$false
if($taskPython -match 'Python\s+(\d+)\.(\d+)'){$taskPythonReady=([int]$Matches[1] -eq 3 -and [int]$Matches[2] -ge 12) -or [int]$Matches[1] -gt 3}
$taskGit=[ordered]@{present=([bool]$taskTools.git);untracked_entries=$null;head=$null}
if($taskTools.git -and (Test-Path -LiteralPath (Join-Path $taskRoot '.git'))){
    $taskOldGitPreference=$ErrorActionPreference
    $ErrorActionPreference='Continue'
    $taskStatus=@(& $taskTools.git -C $taskRoot status --short 2>&1)
    $taskGit['status_exit_code']=$LASTEXITCODE
    $ErrorActionPreference=$taskOldGitPreference
    if($taskGit.status_exit_code -eq 0){$taskGit.untracked_entries=@($taskStatus | Where-Object {"$_".StartsWith('??')}).Count}
    else{$taskGit['diagnostic']=($taskStatus | Select-Object -Last 4 | Out-String).Trim()}
    $taskGit.note='Untracked working files are included by project_handoff.py; do not rely on .git alone'
}
$taskResult=[ordered]@{
    recorded_utc=[DateTime]::UtcNow.ToString('o');project_root=$taskRoot;application_version=$taskVersion
    windows=[ordered]@{product=$taskOS.ProductName;build=$taskOS.CurrentBuildNumber;ubr=$taskOS.UBR;display_version=$taskOS.DisplayVersion}
    powershell=$PSVersionTable.PSVersion.ToString();is_64bit_process=[Environment]::Is64BitProcess
    portable_executable=(Join-Path $taskPortable 'media-browser.exe');payload_files_checked=$taskPackage.Count;payload_failures=@($taskPayloadFailures)
    runtime_files_ready=($taskPayloadFailures.Count -eq 0);tools=$taskTools;python_version=$taskPython
    pinned_rust_toolchain=$taskPin;toolchain_installed=$taskToolchainFound;rust_version=$taskRustVersion
    visual_studio_cpp=$taskVisualStudio;windows_sdk_root=$taskSdkRoot;cargo_cache=$taskCargoCache;git=$taskGit
    build_prerequisites_detected=($taskToolchainFound -and [bool]$taskVisualStudio -and [bool]$taskSdkRoot -and $taskPythonReady)
    build_executed=$false;application_launched=$false;target_os_acceptance_pass=$false
    scope='Read-only environment and package-file checks. No software installation, downloads, registry writes, media edits, or application launch.'
}
$taskResult | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $taskOutput -Encoding utf8
[pscustomobject]@{report=$taskOutput;runtime_files_ready=$taskResult.runtime_files_ready;build_prerequisites_detected=$taskResult.build_prerequisites_detected;cargo_cache=$taskCargoCache} | ConvertTo-Json -Depth 5
if($taskPayloadFailures.Count){exit 1}
