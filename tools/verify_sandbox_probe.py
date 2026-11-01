"""Audit actual Sandbox evidence, keeping the GPU failure distinct from passing core checks."""
import argparse,hashlib,json,subprocess
from pathlib import Path
from xml.etree import ElementTree as ET
ROOT=Path(__file__).resolve().parent.parent

def sha(p):
    h=hashlib.sha256()
    with p.open('rb') as f:
        while b:=f.read(1024*1024):h.update(b)
    return h.hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def main():
    ap=argparse.ArgumentParser();ap.add_argument('evidence',type=Path);ap.add_argument('--output',type=Path,required=True);a=ap.parse_args()
    d=read(a.evidence);task=Path(d['task']).resolve();assert task.is_relative_to(ROOT/'target')
    manifest=read(task/'input/manifest.json');g=read(task/'output/result.json')
    assert d['token']==manifest['token']==g['token'] and g==d['guest']
    assert sha(Path(d['config']))==d['config_sha256']
    config=ET.parse(d['config']).getroot();assert config.findtext('Networking')=='Disable'
    assert config.findtext('ClipboardRedirection')=='Disable' and config.findtext('AudioInput')=='Disable' and config.findtext('VideoInput')=='Disable'
    mapped={m.findtext('SandboxFolder'):(m.findtext('HostFolder'),m.findtext('ReadOnly')) for m in config.findall('MappedFolders/MappedFolder')}
    assert mapped['C:\\MediaProbeInput']==(str(task/'input'),'true')
    assert mapped['C:\\MediaProbeProduct']==(d['product'],'true')
    assert mapped['C:\\MediaProbeOutput']==(str(task/'output'),'false')
    assert sha(task/'input/guest.ps1')==d['guest_script_sha256']==sha(ROOT/'tools/sandbox_probe_guest.ps1')
    assert g['account']=='WDAGUtilityAccount' and g['computer'].lower()!=manifest['host_computer'].lower()
    assert g['machine']['Model']=='Virtual Machine' and not g['default_routes'] and not g['path_development_tools'] and not g['heic_store_extensions']
    delivery=ROOT/'dist/0.1.0-local-test-20261019-pathfix';package=read(delivery/'delivery.json')
    assert g['exe_sha256']==d['exe_sha256']==manifest['exe_sha256']==package['exe_sha256']
    assert sha(task/'input/installer.exe')==manifest['installer_sha256']==sha(delivery/'media-browser-setup-local-test.exe')
    assert sha(task/'input/opengl-probe.exe')==manifest['opengl_probe_sha256']==sha(ROOT/'target/diagnostics/Release/opengl-probe.exe')
    assert g['payload_files_verified']==408 and g['installed_files_verified']==409
    for sample in manifest['samples']:assert sha(task/'input'/sample['file'])==sample['sha256']
    assert g['heic']['decoder']=='1.23.6' and (g['heic']['width'],g['heic']['height'])==(1280,854)
    for name in ['decoded-heic.png','installed-heic.png']:
        probed=json.loads(subprocess.check_output([str(ROOT/'vendor/runtime/ffprobe.exe'),'-v','error','-show_entries','stream=codec_name,width,height','-of','json',str(task/'output'/name)]))['streams'][0]
        assert probed['codec_name']=='png' and (probed['width'],probed['height'])==(1280,854)
    for name in ['mov-stdout.log','installed-mov-stdout.log']:
        row=read(task/'output'/name);assert row['duration']==65 and row['audio_codec']=='aac' and len(row['seeks'])==4 and all(s['pass'] and abs(s['actual']-s['requested'])<=.15 for s in row['seeks'])
    assert g['installed_heic_decode'] and g['installed_mov_null_seek'] and g['install_uninstall_registry_verified']
    gl=read(task/'output/opengl-capability-stdout.log');assert gl==g['opengl'] and gl['version']=='1.1.0' and gl['renderer']=='GDI Generic' and not gl['at_least_opengl_2']
    error=(task/'output/png-ui-stderr.log').read_text(encoding='utf-8-sig');assert 'egui_glow requires opengl 2.0+' in error
    assert d['state']=='guest_failed' and g['state']=='failed' and not d['checks_pass'] and not g['checks_pass']
    assert d['closure_exit_code']==0 and d['closure']['closed_owned_probe'] and not d['closure']['remaining']
    result=dict(evidence_integrity_checks_pass=True,checks_pass=False,full_offline_acceptance_pass=False,offline_core_dependencies_pass=True,installation_and_uninstall_pass=True,native_png_ui_pass=False,exe_sha256=d['exe_sha256'],guest_os=g['os'],guest_opengl=gl,evidence=str(a.evidence),guest_output=str(task/'output'),closed_owned_guest=True,
                limits=['Sandbox guest is Windows 10 build 19041, not proof of full Win10 22H2 or Win11 baseline','Native UI fails because this guest exposes GDI Generic OpenGL 1.1; main host supports NVIDIA OpenGL 4.6','MOV tests use null video/audio, not visible video or sound','No full offline workflow, physical input, HDR, or public distribution certification'])
    a.output.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2))
if __name__=='__main__':main()
