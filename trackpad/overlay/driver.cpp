#include "protocol.h"
#include <cstdio>
#include <string>
#include <algorithm>
#include <vector>
#include <dwmapi.h>
#include <wincodec.h>
#include <wrl/client.h>
#include <mmsystem.h>
#include <uiautomation.h>
using Microsoft::WRL::ComPtr;
void key(WORD k){INPUT a[2]{};a[0].type=a[1].type=INPUT_KEYBOARD;a[0].ki.wVk=a[1].ki.wVk=k;a[1].ki.dwFlags=KEYEVENTF_KEYUP;SendInput(2,a,sizeof(INPUT));}
void desktop(){INPUT a[4]{};for(auto& i:a)i.type=INPUT_KEYBOARD;a[0].ki.wVk=a[3].ki.wVk=VK_LWIN;a[1].ki.wVk=a[2].ki.wVk='D';a[2].ki.dwFlags=a[3].ki.dwFlags=KEYEVENTF_KEYUP;SendInput(4,a,sizeof(INPUT));}
struct Driver {
 HANDLE mapping{},wake{};protocol::State* memory{};protocol::State s{};
 Driver(){mapping=CreateFileMapping(INVALID_HANDLE_VALUE,nullptr,PAGE_READWRITE,0,4096,protocol::Mapping);wake=CreateEvent(nullptr,FALSE,FALSE,protocol::Wake);memory=(protocol::State*)MapViewOfFile(mapping,FILE_MAP_ALL_ACCESS,0,0,4096);if(!memory||!wake)throw 1;protocol::read(memory,s);}
 ~Driver(){if(memory)UnmapViewOfFile(memory);if(mapping)CloseHandle(mapping);if(wake)CloseHandle(wake);}
 void write(unsigned phase,float p,float velocity=0){s.magic=protocol::Magic;s.phase=phase;s.progress=p;s.velocity=velocity;s.qpc=protocol::now();InterlockedIncrement((volatile LONG*)&memory->seq);MemoryBarrier();
  memory->magic=s.magic;memory->gesture=s.gesture;memory->phase=s.phase;memory->progress=p;memory->velocity=velocity;memory->qpc=s.qpc;memory->session=s.session;MemoryBarrier();InterlockedIncrement((volatile LONG*)&memory->seq);SetEvent(wake);}
 void arm(int ms=220){s.gesture=0;s.session++;if(!s.session)s.session=1;auto end=GetTickCount64()+ms;do{write(4,0);Sleep(8);}while(GetTickCount64()<end);}
 void start(unsigned gesture){arm();s.gesture=gesture;write(1,0);Sleep(16);}
 void hold(float p,int ms){auto end=GetTickCount64()+ms;do{write(1,p);Sleep(8);}while(GetTickCount64()<end);}
 void ramp(float from,float to,int ms){auto begin=GetTickCount64();for(;;){float t=std::min(1.f,float(GetTickCount64()-begin)/ms);write(1,from+(to-from)*t,(to-from)*1000/ms);if(t>=1)break;Sleep(8);}}
 void shot(int id){auto h=FindWindow(protocol::WindowClass,nullptr);DWORD_PTR result=0;if(!h || !SendMessageTimeout(h,WM_APP+1,id,0,SMTO_ABORTIFHUNG,2000,&result)||!result)printf("shot failed\n");else printf("saved gesture-%u-%d.png\n",s.gesture,id);}
};
std::vector<HWND> apps(){std::vector<HWND> result;EnumWindows([](HWND h,LPARAM p)->BOOL{wchar_t c[128];GetClassName(h,c,128);DWORD cloak=0;DwmGetWindowAttribute(h,DWMWA_CLOAKED,&cloak,sizeof(cloak));if(IsWindowVisible(h)&&!IsIconic(h)&&!cloak&&!(GetWindowLongPtr(h,GWL_EXSTYLE)&WS_EX_TOOLWINDOW)&&!GetWindow(h,GW_OWNER)&&wcscmp(c,L"Progman")&&wcscmp(c,L"WorkerW")&&wcscmp(c,L"Shell_TrayWnd")){((std::vector<HWND>*)p)->push_back(h);}return TRUE;},(LPARAM)&result);return result;}
void inspect(const std::vector<HWND>& original){int hidden=0;for(auto h:original)hidden+=!IsWindowVisible(h)||IsIconic(h);printf("original apps=%zu hidden/minimized=%d\n",original.size(),hidden);int counts[2]{};EnumWindows([](HWND h,LPARAM p)->BOOL{wchar_t c[128];GetClassName(h,c,128);if(!wcscmp(c,protocol::WindowClass)){auto n=(int*)p;n[1]++;n[0]+=!!IsWindowVisible(h);printf("overlay hwnd=%p visible=%d mode=%lld\n",h,IsWindowVisible(h),(long long)SendMessage(h,WM_APP+2,0,0));}if(IsWindowVisible(h)&&(!wcscmp(c,L"MultitaskingViewFrame")||!wcscmp(c,L"XamlExplorerHostIslandWindow")))printf("shell view class=%ls\n",c);return TRUE;},(LPARAM)counts);printf("overlay windows=%d visible=%d\n",counts[1],counts[0]);}
LRESULT CALLBACK controlProc(HWND h,UINT m,WPARAM w,LPARAM l){if(m==WM_PAINT){PAINTSTRUCT ps;auto dc=BeginPaint(h,&ps);RECT r;GetClientRect(h,&r);auto brush=CreateSolidBrush(GetWindowLongPtr(h,GWLP_USERDATA)?RGB(30,120,160):RGB(150,65,45));FillRect(dc,&r,brush);DeleteObject(brush);SetBkMode(dc,TRANSPARENT);SetTextColor(dc,RGB(255,255,255));auto text=L"Trackpad overlay capture test";TextOut(dc,30,45,text,lstrlen(text));EndPaint(h,&ps);return 0;}return DefWindowProc(h,m,w,l);}
// Independent GDI monitor capture: with WDA exclusion this should omit the overlay.
void capture(const wchar_t* path){CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);int w=GetSystemMetrics(SM_CXSCREEN),h=GetSystemMetrics(SM_CYSCREEN);HDC src=GetDC(nullptr),dc=CreateCompatibleDC(src);BITMAPINFO bi{};bi.bmiHeader={sizeof(BITMAPINFOHEADER),w,-h,1,32,BI_RGB};void* data;auto bmp=CreateDIBSection(src,&bi,DIB_RGB_COLORS,&data,nullptr,0);auto old=SelectObject(dc,bmp);BitBlt(dc,0,0,w,h,src,0,0,SRCCOPY|CAPTUREBLT);
 ComPtr<IWICImagingFactory> f;CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&f));ComPtr<IWICStream> stream;f->CreateStream(&stream);stream->InitializeFromFilename(path,GENERIC_WRITE);ComPtr<IWICBitmapEncoder> e;f->CreateEncoder(GUID_ContainerFormatPng,nullptr,&e);e->Initialize(stream.Get(),WICBitmapEncoderNoCache);ComPtr<IWICBitmapFrameEncode> frame;e->CreateNewFrame(&frame,nullptr);frame->Initialize(nullptr);frame->SetSize(w,h);auto fmt=GUID_WICPixelFormat32bppBGRA;frame->SetPixelFormat(&fmt);frame->WritePixels(h,w*4,w*h*4,(BYTE*)data);frame->Commit();e->Commit();SelectObject(dc,old);DeleteObject(bmp);DeleteDC(dc);ReleaseDC(nullptr,src);}
bool taskView(){HWND h=GetForegroundWindow();wchar_t c[128]{},t[128]{};GetClassName(h,c,128);GetWindowText(h,t,128);return !wcscmp(c,L"XamlExplorerHostIslandWindow")&&!wcscmp(t,L"Task View");}
struct CpuFrame {
 int width=GetSystemMetrics(SM_CXSCREEN),height=GetSystemMetrics(SM_CYSCREEN);std::vector<BYTE> pixels;
 void grab(){
  HDC src=GetDC(nullptr),dc=CreateCompatibleDC(src);BITMAPINFO bi{};bi.bmiHeader={sizeof(BITMAPINFOHEADER),width,-height,1,32,BI_RGB};void* bits=nullptr;
  auto bmp=CreateDIBSection(src,&bi,DIB_RGB_COLORS,&bits,nullptr,0);auto old=SelectObject(dc,bmp);
  if(bmp&&BitBlt(dc,0,0,width,height,src,0,0,SRCCOPY|CAPTUREBLT)){GdiFlush();pixels.assign((BYTE*)bits,(BYTE*)bits+size_t(width)*height*4);}
  SelectObject(dc,old);DeleteObject(bmp);DeleteDC(dc);ReleaseDC(nullptr,src);
 }
 void save(const wchar_t* path){if(pixels.empty())return;
  ComPtr<IWICImagingFactory> f;CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&f));ComPtr<IWICStream> stream;f->CreateStream(&stream);stream->InitializeFromFilename(path,GENERIC_WRITE);ComPtr<IWICBitmapEncoder> e;f->CreateEncoder(GUID_ContainerFormatPng,nullptr,&e);e->Initialize(stream.Get(),WICBitmapEncoderNoCache);ComPtr<IWICBitmapFrameEncode> frame;e->CreateNewFrame(&frame,nullptr);frame->Initialize(nullptr);frame->SetSize(width,height);auto fmt=GUID_WICPixelFormat32bppBGRA;frame->SetPixelFormat(&fmt);frame->WritePixels(height,width*4,UINT(pixels.size()),pixels.data());frame->Commit();e->Commit();
 }
};
void winTab(){INPUT a[4]{};for(auto& i:a)i.type=INPUT_KEYBOARD;a[0].ki.wVk=a[3].ki.wVk=VK_LWIN;a[1].ki.wVk=a[2].ki.wVk=VK_TAB;a[2].ki.dwFlags=a[3].ki.dwFlags=KEYEVENTF_KEYUP;SendInput(4,a,sizeof(INPUT));}
void nativeProbe(){
 if(taskView()){puts("Task View already open; probe skipped");return;}
 // Scope restoration also covers failures during capture/UI Automation.
 struct Restore {BOOL original=TRUE;bool got=false;Restore(){got=!!SystemParametersInfo(SPI_GETCLIENTAREAANIMATION,0,&original,0);}~Restore(){if(taskView())key(VK_ESCAPE);if(got)SystemParametersInfo(SPI_SETCLIENTAREAANIMATION,0,(PVOID)(INT_PTR)original,0);}} restore;
 CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);
 CreateDirectory(L"test-shots",nullptr);
 for(int disabled=0;disabled<2;disabled++){
  if(disabled && !restore.got)break;
  if(restore.got)printf("SPI set disabled=%d success=%d original=%d\n",disabled,SystemParametersInfo(SPI_SETCLIENTAREAANIMATION,0,(PVOID)(INT_PTR)(disabled?FALSE:restore.original),0),restore.original);
  auto start=protocol::now();winTab();bool observed=false;std::vector<CpuFrame> frames;
  for(int target:{50,100,160,240,400}){
   while((protocol::now()-start)*1000/protocol::frequency()<target){if(taskView()&&!observed){printf("native foreground disabled=%d at=%.3fms\n",disabled,(protocol::now()-start)*1000/protocol::frequency());observed=true;}Sleep(1);}
   if(taskView()&&!observed){printf("native foreground disabled=%d at=%.3fms\n",disabled,(protocol::now()-start)*1000/protocol::frequency());observed=true;}
   printf("native capture start disabled=%d target=%d at=%.3fms\n",disabled,target,(protocol::now()-start)*1000/protocol::frequency());
   frames.emplace_back();frames.back().grab();
   printf("native capture end disabled=%d target=%d at=%.3fms\n",disabled,target,(protocol::now()-start)*1000/protocol::frequency());
  }
  if(!disabled && taskView()){
   ComPtr<IUIAutomation> automation;ComPtr<IUIAutomationElement> root;ComPtr<IUIAutomationCondition> all;ComPtr<IUIAutomationElementArray> children;
   if(SUCCEEDED(CoCreateInstance(CLSID_CUIAutomation,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&automation)))&&SUCCEEDED(automation->ElementFromHandle(GetForegroundWindow(),&root))&&SUCCEEDED(automation->CreateTrueCondition(&all))&&SUCCEEDED(root->FindAll(TreeScope_Descendants,all.Get(),&children))){
    int n=0;children->get_Length(&n);printf("UIA children=%d\n",n);for(int i=0;i<n;i++){ComPtr<IUIAutomationElement> element;children->GetElement(i,&element);RECT r{};CONTROLTYPEID type=0;element->get_CurrentBoundingRectangle(&r);element->get_CurrentControlType(&type);if(r.right>r.left&&r.bottom>r.top)printf("UIA type=%d rect=%ld,%ld,%ld,%ld\n",type,r.left,r.top,r.right,r.bottom);}
   }
  }
  if(taskView())key(VK_ESCAPE);
  if(restore.got)SystemParametersInfo(SPI_SETCLIENTAREAANIMATION,0,(PVOID)(INT_PTR)restore.original,0);
  int index=0;for(int target:{50,100,160,240,400}){wchar_t file[128];swprintf_s(file,L"test-shots\\native-%d-%d.png",disabled,target);frames[index++].save(file);}Sleep(150);
 }
}
int wmain(int argc,wchar_t** argv){SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);if(argc<2){puts("OverlayDriver up|down|back [0.5 hold [milliseconds]|commit|cancel|scrub|shots]\nOverlayDriver arm [milliseconds] | verify | safety | status | escape | restore | stop | native-probe | capture path.png");return 0;}
 struct TimerResolution{TimerResolution(){timeBeginPeriod(1);}~TimerResolution(){timeEndPeriod(1);}} timerResolution;
 if(!wcscmp(argv[1],L"classes")){EnumWindows([](HWND h,LPARAM)->BOOL{if(IsWindowVisible(h)){wchar_t c[128],t[256];GetClassName(h,c,128);GetWindowText(h,t,256);printf("%p class=%ls title=%ls\n",h,c,t);}return TRUE;},0);return 0;}
 std::wstring cmd=argv[1];if(cmd==L"status"){inspect(apps());BOOL animations=TRUE;SystemParametersInfo(SPI_GETCLIENTAREAANIMATION,0,&animations,0);printf("client area animations=%d\n",animations);return 0;}if(cmd==L"escape"){key(VK_ESCAPE);return 0;}if(cmd==L"restore"){desktop();return 0;}if(cmd==L"stop"){PostMessage(FindWindow(protocol::WindowClass,nullptr),WM_CLOSE,0,0);return 0;}if(cmd==L"capture"&&argc>2){capture(argv[2]);return 0;}if(cmd==L"native-probe"){nativeProbe();return 0;}
 try{Driver d;
 if(cmd==L"arm"){d.arm(argc>2?_wtoi(argv[2]):400);d.write(0,0);Sleep(40);inspect(apps());return 0;}
 if(cmd==L"armed-evidence"){d.arm(300);inspect(apps());CreateDirectory(L"test-shots",nullptr);capture(L"test-shots\\armed-borderless.png");d.write(0,0);return 0;}
 if(cmd==L"controls"){
  WNDCLASS wc{};wc.lpfnWndProc=controlProc;wc.hInstance=GetModuleHandle(nullptr);wc.lpszClassName=L"TrackpadOverlay.TestControl";RegisterClass(&wc);
  HWND controls[2]{};for(int i=0;i<2;i++){controls[i]=CreateWindowEx(WS_EX_APPWINDOW,wc.lpszClassName,i?L"Overlay test blue":L"Overlay test orange",WS_OVERLAPPEDWINDOW,160+i*650,170+i*230,610,390,nullptr,nullptr,wc.hInstance,nullptr);SetWindowLongPtr(controls[i],GWLP_USERDATA,i);ShowWindow(controls[i],SW_SHOWNOACTIVATE);UpdateWindow(controls[i]);}
  // The render helper uses WGC callbacks; pump this source process while the
  // scripted writer runs so our own test windows remain responsive.
  HANDLE worker=CreateThread(nullptr,0,[](LPVOID p)->DWORD{auto d=(Driver*)p;for(unsigned g:{1u,2u}){d->start(g);for(float f:{.3f,.6f,1.f}){d->hold(f,200);d->shot(int(f*100));}d->write(3,1);Sleep(400);}return 0;},&d,0,nullptr);
  if(worker){while(MsgWaitForMultipleObjects(1,&worker,FALSE,INFINITE,QS_ALLINPUT)==WAIT_OBJECT_0+1){MSG m;while(PeekMessage(&m,nullptr,0,0,PM_REMOVE)){TranslateMessage(&m);DispatchMessage(&m);}}CloseHandle(worker);}
  for(auto h:controls)DestroyWindow(h);inspect(apps());return 0;
 }
 if(cmd==L"safety"){
  auto original=apps();d.start(1);d.hold(.6f,100);puts("abandoning tracking for watchdog");Sleep(1900);inspect(original);
  d.arm(100);puts("abandoning armed state for watchdog");Sleep(1700);inspect(original);
  d.start(1);d.write(3,.7f);Sleep(40);d.start(2);d.hold(.4f,80);d.write(3,.4f);Sleep(300);puts("replaced cancelling session");inspect(original);
  d.start(1);d.hold(-.3f,150);d.hold(1.4f,150);d.write(3,1.4f);Sleep(500);puts("rubber-band and cancel");inspect(original);return 0;
 }
 if(cmd==L"verify"){
  if(taskView()){puts("Task View already open; verification skipped");return 2;}
  struct Cleanup{bool shown=false,opened=false;std::vector<HWND> original;~Cleanup(){if(opened&&taskView())key(VK_ESCAPE);if(shown){bool stillShown=false;for(auto h:original)stillShown|=IsWindow(h)&&IsIconic(h);if(stillShown)desktop();}}} cleanup;
  cleanup.original=apps();
  auto original=apps();for(unsigned g:{1u,2u}){d.start(g);for(float p:{.3f,.6f,1.f}){d.hold(p,220);d.shot(int(p*100));}d.ramp(1,0,350);d.write(3,0);Sleep(450);puts("cancel finished");inspect(original);
   d.start(g);d.ramp(0,.7f,350);d.write(2,.7f);if(g==1)cleanup.opened=true;Sleep(650);puts("commit finished");inspect(original);
   if(g==1){if(taskView())key(VK_ESCAPE);}else{
    bool shown=false;for(auto h:original)shown|=IsWindow(h)&&IsIconic(h);
    if(shown){cleanup.shown=true;d.start(3);for(float p:{.3f,.6f,1.f}){d.hold(p,120);d.shot(int(p*100));}d.write(3,1);Sleep(300);puts("restore cancelled");inspect(original);
     d.start(3);d.ramp(0,.7f,220);d.write(2,.7f);Sleep(500);puts("restore committed");inspect(original);bool stillShown=false;for(auto h:original)stillShown|=IsWindow(h)&&IsIconic(h);cleanup.shown=stillShown;}
   }Sleep(350);puts("restored");inspect(original);
  }return 0;
 }
 unsigned g=cmd==L"down"?2:cmd==L"up"?1:cmd==L"back"?3:0;if(!g)return 2;std::wstring op=argc>2?argv[2]:L"scrub";
 bool resume=d.s.phase==1 && d.s.gesture==g && (protocol::now()-d.s.qpc)/protocol::frequency()<1.4;
 if(!resume)d.start(g);
 if(op==L"commit"){d.ramp(d.s.progress,.7f,280);d.write(2,.7f);}
 else if(op==L"low-commit"){d.hold(.01f,80);d.write(2,.01f,-4);}
 else if(op==L"cancel"){d.hold(.6f,200);d.write(3,.6f);}
 else if(op==L"scrub"){d.ramp(0,.6f,650);d.ramp(.6f,0,650);d.write(3,0);}
 else if(op==L"shots"){for(float p:{.3f,.6f,1.f}){d.hold(p,250);d.shot(int(p*100));}d.write(3,1);}
 else {float p=(float)_wtof(op.c_str());int duration=argc>4?_wtoi(argv[4]):3000;d.hold(p,duration);d.write(3,p);}
 }catch(...){puts("driver failed");return 1;}return 0;
}
