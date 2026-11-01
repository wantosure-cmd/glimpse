"""Launch one guarded offline Sandbox probe, retaining output and never enabling features."""
import datetime,hashlib,json,os,shutil,socket,subprocess,time,uuid
from pathlib import Path
from xml.sax.saxutils import escape
ROOT=Path(__file__).resolve().parent.parent

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,d):p.write_text(json.dumps(d,ensure_ascii=False,indent=2),encoding='utf-8')
def main():
    inventory=subprocess.run(['powershell','-NoProfile','-Command',"@(Get-Process | Where-Object ProcessName -like 'WindowsSandbox*' | Select-Object Id,ProcessName) | ConvertTo-Json -Compress"],capture_output=True)
    assert inventory.returncode==0,inventory.stderr
    assert not inventory.stdout.strip(), 'Existing Sandbox process detected; never replace a user session'
    token=uuid.uuid4().hex;task=ROOT/'target'/('sandbox-probe-'+token);task.mkdir();inputs=task/'input';inputs.mkdir();outputs=task/'output';outputs.mkdir();(inputs/'samples').mkdir()
    product=ROOT/'dist/0.1.0-local-test-20261019-pathfix/media-browser-portable'
    delivery=json.loads((product.parent/'delivery.json').read_text(encoding='utf-8'))
    samples=[]
    for src,name in [('test-media/upstream-example.heic','sample.heic'),('test-media/format-matrix-v1/h264-mov/sample.mov','sample.mov'),('test-media/format-matrix-v1/png/sample.png','sample.png')]:
        source=ROOT/src;dest=inputs/'samples'/name;shutil.copyfile(source,dest);samples.append(dict(file='samples/'+name,sha256=sha(source)))
    shutil.copyfile(ROOT/'tools/sandbox_probe_guest.ps1',inputs/'guest.ps1')
    installer=product.parent/'media-browser-setup-local-test.exe'
    shutil.copyfile(installer,inputs/'installer.exe')
    gl_probe=ROOT/'target/diagnostics/Release/opengl-probe.exe'
    shutil.copyfile(gl_probe,inputs/'opengl-probe.exe')
    manifest=dict(token=token,host_computer=socket.gethostname(),exe_sha256=delivery['exe_sha256'],installer_sha256=sha(installer),opengl_probe_sha256=sha(gl_probe),samples=samples)
    write(inputs/'manifest.json',manifest)
    maps=''.join('<MappedFolder><HostFolder>'+escape(str(host))+'</HostFolder><SandboxFolder>'+guest+'</SandboxFolder><ReadOnly>'+readonly+'</ReadOnly></MappedFolder>' for host,guest,readonly in [(inputs,r'C:\MediaProbeInput','true'),(product,r'C:\MediaProbeProduct','true'),(outputs,r'C:\MediaProbeOutput','false')])
    config='<Configuration><Networking>Disable</Networking><ClipboardRedirection>Disable</ClipboardRedirection><AudioInput>Disable</AudioInput><VideoInput>Disable</VideoInput><PrinterRedirection>Disable</PrinterRedirection><vGPU>Enable</vGPU><MemoryInMB>6144</MemoryInMB><MappedFolders>'+maps+r'</MappedFolders><LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\MediaProbeInput\guest.ps1</Command></LogonCommand></Configuration>'
    wsb=task/'probe.wsb';wsb.write_text(config,encoding='utf-8')
    evidence=ROOT/'artifacts'/('sandbox-probe-'+token+'.json')
    d=dict(token=token,started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),task=str(task),config=str(wsb),config_sha256=sha(wsb),guest_script_sha256=sha(inputs/'guest.ps1'),product=str(product),exe_sha256=delivery['exe_sha256'],state='prepared',checks_pass=False,scope='Sandbox availability and guarded offline native portable probe; no feature enablement or host settings changes')
    write(evidence,d);print('LIVE_EVIDENCE '+str(evidence),flush=True)
    si=subprocess.STARTUPINFO();si.dwFlags|=subprocess.STARTF_USESHOWWINDOW;si.wShowWindow=0
    with (task/'launcher-stdout.log').open('wb') as out,(task/'launcher-stderr.log').open('wb') as err:
        proc=subprocess.Popen([str(Path(os.environ['WINDIR'])/'System32/WindowsSandbox.exe'),str(wsb)],stdout=out,stderr=err,startupinfo=si)
        d.update(state='launched',launcher_pid=proc.pid);write(evidence,d)
        start=time.monotonic()
        while time.monotonic()-start<240:
            result=outputs/'result.json'
            if result.exists():
                try:guest=json.loads(result.read_text(encoding='utf-8-sig'))
                except (ValueError,OSError):time.sleep(.5);continue
                assert guest['token']==token
                d['guest']=guest;write(evidence,d)
                if guest['state'] in ('finished','failed'):break
            code=proc.poll()
            if code is not None and code!=0:
                d.update(state='launcher_failed',launcher_exit_code=code);break
            time.sleep(.5)
        else:d['state']='observation_timeout_no_restart'
        d['launcher_exit_code']=proc.poll()
        if d.get('guest',{}).get('state') in ('finished','failed'):
            d['state']='guest_'+d['guest']['state'];d['checks_pass']=d['guest']['checks_pass']
        # Windows 10 Sandbox may not honor shutdown.exe from inside the guest.
        # Check our own launch command and child ownership, never other sessions.
        cfg=str(wsb).replace("'","''")
        cleanup="""$ErrorActionPreference='Stop'; $taskPid=%d; $taskConfig='%s'; $taskLauncher=Get-CimInstance Win32_Process -Filter ('ProcessId='+$taskPid); if($taskLauncher){if($taskLauncher.Name -ine 'WindowsSandbox.exe' -or -not $taskLauncher.CommandLine.Contains($taskConfig)){throw 'Launcher ownership mismatch'}; $taskClients=@(Get-CimInstance Win32_Process -Filter ('ParentProcessId='+$taskPid) | Where-Object Name -ieq 'WindowsSandboxClient.exe'); foreach($taskClient in $taskClients){Stop-Process -Id $taskClient.ProcessId -Force}; $taskRemaining=Get-Process -Id $taskPid -ErrorAction SilentlyContinue; if($taskRemaining){if(-not $taskRemaining.WaitForExit(10000)){Stop-Process -Id $taskPid -Force}}}; $taskRemainingAll=@(Get-Process | Where-Object ProcessName -like 'WindowsSandbox*' | Select-Object Id,ProcessName); ConvertTo-Json -InputObject @{closed_owned_probe=$true;remaining=$taskRemainingAll} -Depth 3""" % (proc.pid,cfg)
        closed=subprocess.run(['powershell','-NoProfile','-Command',cleanup],capture_output=True,timeout=30)
        (task/'closure-stderr.log').write_bytes(closed.stderr)
        d['closure_exit_code']=closed.returncode
        if closed.returncode==0:d['closure']=json.loads(closed.stdout)
        else:d['closure_failure']='Inspect closure-stderr.log; never relaunch while owned processes are live'
        d['launcher_exit_code_after_closure']=proc.poll()
        d['ended_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();write(evidence,d)
    print(json.dumps(d,ensure_ascii=False,indent=2))
    raise SystemExit(0 if d['checks_pass'] else 1)
if __name__=='__main__':main()
