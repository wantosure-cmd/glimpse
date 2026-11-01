// Diagnostic only: per-process audio-session peak levels, never microphone capture.
#include <windows.h>
#include <mmdeviceapi.h>
#include <audiopolicy.h>
#include <endpointvolume.h>
#include <stdio.h>
#include <stdlib.h>

int main(int argc,char** argv) {
    if(argc<2){fprintf(stderr,"usage: audio-meter <PID>\n");return 2;}
    DWORD pid=(DWORD)strtoul(argv[1],nullptr,10);
    HRESULT hr=CoInitializeEx(nullptr,COINIT_MULTITHREADED);
    if(FAILED(hr))return 3;
    IMMDeviceEnumerator* devices=nullptr;IMMDevice* device=nullptr;
    IAudioSessionManager2* manager=nullptr;
    hr=CoCreateInstance(__uuidof(MMDeviceEnumerator),nullptr,CLSCTX_ALL,IID_PPV_ARGS(&devices));
    if(SUCCEEDED(hr))hr=devices->GetDefaultAudioEndpoint(eRender,eConsole,&device);
    if(SUCCEEDED(hr))hr=device->Activate(__uuidof(IAudioSessionManager2),CLSCTX_ALL,nullptr,(void**)&manager);
    if(FAILED(hr)){fprintf(stderr,"audio init HRESULT=%08lx\n",(unsigned long)hr);return 4;}
    ULONGLONG began=GetTickCount64();
    printf("[\n");
    for(int n=0;n<180;n++) {
        float peak=0;bool found=false;IAudioSessionEnumerator* sessions=nullptr;
        if(SUCCEEDED(manager->GetSessionEnumerator(&sessions))) {
            int count=0;sessions->GetCount(&count);
            for(int i=0;i<count;i++) {
                IAudioSessionControl* control=nullptr;IAudioSessionControl2* control2=nullptr;
                if(SUCCEEDED(sessions->GetSession(i,&control))) {
                    if(SUCCEEDED(control->QueryInterface(IID_PPV_ARGS(&control2)))) {
                        DWORD owner=0;control2->GetProcessId(&owner);
                        if(owner==pid) {
                            IAudioMeterInformation* meter=nullptr;
                            if(SUCCEEDED(control->QueryInterface(IID_PPV_ARGS(&meter)))) {
                                float value=0;if(SUCCEEDED(meter->GetPeakValue(&value)) && value>peak)peak=value;
                                found=true;meter->Release();
                            }
                        }
                        control2->Release();
                    }
                    control->Release();
                }
            }
            sessions->Release();
        }
        printf("%s{\"ms\":%llu,\"pid\":%lu,\"session_found\":%s,\"peak\":%.8f}",n?",\n":"",GetTickCount64()-began,pid,found?"true":"false",peak);
        fflush(stdout);Sleep(100);
    }
    printf("\n]\n");
    manager->Release();device->Release();devices->Release();CoUninitialize();return 0;
}
