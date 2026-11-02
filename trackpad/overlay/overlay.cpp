#include "protocol.h"
#include <d3d11.h>
#include <dxgi1_3.h>
#include <dcomp.h>
#include <d2d1_1.h>
#include <dwmapi.h>
#include <shobjidl.h>
#include <wincodec.h>
#include <windows.graphics.capture.interop.h>
#include <windows.graphics.directx.direct3d11.interop.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Graphics.Capture.h>
#include <winrt/Windows.Graphics.DirectX.h>
#include <winrt/Windows.Graphics.DirectX.Direct3D11.h>
#include <winrt/Windows.Security.Authorization.AppCapabilityAccess.h>
#include <vector>
#include <algorithm>
#include <string>
#include <cstdio>
#include <share.h>
using namespace winrt;
using namespace winrt::Windows::Graphics::Capture;
using namespace winrt::Windows::Graphics::DirectX;
using namespace winrt::Windows::Graphics::DirectX::Direct3D11;
using D2D1::RectF;
static FILE* logFile;
static std::wstring directory;
void log(const char* text){if(logFile){fprintf(logFile,"%.3f %s\n",protocol::now()/protocol::frequency(),text);fflush(logFile);}}
struct Handle {HANDLE h=nullptr;~Handle(){if(h)CloseHandle(h);} operator HANDLE()const{return h;}};
void png(const wchar_t* path,BYTE* data,UINT w,UINT h,UINT stride){
 com_ptr<IWICImagingFactory> f;check_hresult(CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(f.put())));
 com_ptr<IWICStream> stream;check_hresult(f->CreateStream(stream.put()));check_hresult(stream->InitializeFromFilename(path,GENERIC_WRITE));
 com_ptr<IWICBitmapEncoder> e;check_hresult(f->CreateEncoder(GUID_ContainerFormatPng,nullptr,e.put()));check_hresult(e->Initialize(stream.get(),WICBitmapEncoderNoCache));
 com_ptr<IWICBitmapFrameEncode> frame;check_hresult(e->CreateNewFrame(frame.put(),nullptr));check_hresult(frame->Initialize(nullptr));check_hresult(frame->SetSize(w,h));
 auto format=GUID_WICPixelFormat32bppBGRA;check_hresult(frame->SetPixelFormat(&format));check_hresult(frame->WritePixels(h,stride,stride*h,data));check_hresult(frame->Commit());check_hresult(e->Commit());
}
struct Window {
 HWND hwnd{};RECT rect{};D2D1_RECT_F grid{};
 GraphicsCaptureItem item{nullptr};Direct3D11CaptureFramePool pool{nullptr};GraphicsCaptureSession capture{nullptr};
 com_ptr<ID2D1Bitmap1> bitmap; com_ptr<ID3D11Texture2D> texture;
 Window()=default;Window(Window&&)=default;Window& operator=(Window&&)=default;
 void stop(){try{if(capture)capture.Close();if(pool)pool.Close();}catch(...) {}capture=nullptr;pool=nullptr;item=nullptr;}
 ~Window(){stop();}
};
class Overlay {
public:
 HWND hwnd{};RECT monitor{};UINT width{},height{};bool displayChanged=false,quit=false,measure=false;
 com_ptr<ID3D11Device> dev;com_ptr<ID3D11DeviceContext> context;com_ptr<IDXGISwapChain1> swap;com_ptr<IDCompositionDevice> composition;
 com_ptr<IDCompositionTarget> target;com_ptr<IDCompositionVisual> root;com_ptr<IDCompositionEffectGroup> opacity;com_ptr<ID2D1DeviceContext> draw;com_ptr<ID2D1Bitmap1> back,snapshot,wallpaper;
 com_ptr<ID2D1SolidColorBrush> shade;com_ptr<ID3D11Texture2D> backTexture;IDirect3DDevice captureDevice{nullptr};Handle frameWait;
 std::vector<Window> windows;protocol::State state{};uint32_t session=0,lastSeq=~0u,gesture=0;uint64_t lastQpc=0;
 enum Mode{Idle,Tracking,Settling,Handoff,Fading,Armed} mode=Idle;
 float progress=0,startProgress=0,goal=0;double stageTime=0,actionTime=0,readySince=0;bool actionSent=false;
 double freq=protocol::frequency();
 std::vector<Window> desktopWindows;com_ptr<ID2D1Bitmap1> desktopSnapshot;
 bool restoring=false;
 com_ptr<ID2D1Bitmap1> restoreBackground;
 winrt::Windows::Foundation::IAsyncOperation<winrt::Windows::Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus> borderAccess{nullptr};
 bool borderLogged=false;
 size_t captureIndex=0;
 std::vector<BYTE> taskViewProbe;
 struct AnimationScope {
  BOOL original=TRUE;bool changed=false;double deadline=0;
  void restore(){if(changed){BOOL ok=SystemParametersInfo(SPI_SETCLIENTAREAANIMATION,0,(PVOID)(INT_PTR)original,0);log(ok?"client animations restored":"client animation restoration failed");if(ok)changed=false;}}
  void suppress(double now){restore();if(changed)return;
   if(SystemParametersInfo(SPI_GETCLIENTAREAANIMATION,0,&original,0)&&original){changed=!!SystemParametersInfo(SPI_SETCLIENTAREAANIMATION,0,(PVOID)FALSE,0);deadline=now+.45;log(changed?"Task View client animations temporarily disabled":"client animation suppression unavailable");}}
  ~AnimationScope(){restore();}
 } animations;
 std::wstring wallpaperPath;FILETIME wallpaperStamp{};DESKTOP_WALLPAPER_POSITION wallpaperPosition=DWPOS_FILL;
 D2D1_COLOR_F desktopColor=D2D1::ColorF(.035f,.045f,.065f,1);
 static LRESULT CALLBACK proc(HWND h,UINT m,WPARAM w,LPARAM l){
  auto self=(Overlay*)GetWindowLongPtr(h,GWLP_USERDATA);
  if(m==WM_NCCREATE){self=(Overlay*)((CREATESTRUCT*)l)->lpCreateParams;SetWindowLongPtr(h,GWLP_USERDATA,(LONG_PTR)self);}
  if(m==WM_MOUSEACTIVATE)return MA_NOACTIVATE;if(m==WM_NCHITTEST)return HTTRANSPARENT;
  if(self){if(m==WM_DISPLAYCHANGE || m==WM_DPICHANGED){self->displayChanged=true;return 0;}
   if(m==WM_CLOSE){self->quit=true;return 0;}
   if(m==WM_APP+1){try{self->shot((int)w);return 1;}catch(...){return 0;}}
   if(m==WM_APP+2)return self->mode;
  }return DefWindowProc(h,m,w,l);
 }
 void init(){
  MONITORINFO mi{sizeof(mi)};GetMonitorInfo(MonitorFromPoint({0,0},MONITOR_DEFAULTTOPRIMARY),&mi);monitor=mi.rcMonitor;
  width=monitor.right-monitor.left;height=monitor.bottom-monitor.top;
  WNDCLASS wc{};wc.lpfnWndProc=proc;wc.hInstance=GetModuleHandle(nullptr);wc.lpszClassName=protocol::WindowClass;RegisterClass(&wc);
  hwnd=CreateWindowEx(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_TRANSPARENT|WS_EX_LAYERED,wc.lpszClassName,L"Trackpad overlay",WS_POPUP,monitor.left,monitor.top,width,height,nullptr,nullptr,wc.hInstance,this);
  check_bool(hwnd!=nullptr);SetLayeredWindowAttributes(hwnd,0,255,LWA_ALPHA);
  BOOL affinity=SetWindowDisplayAffinity(hwnd,WDA_EXCLUDEFROMCAPTURE);DWORD value=0;GetWindowDisplayAffinity(hwnd,&value);
  char msg[160];sprintf_s(msg,"capture affinity set=%d read=%lu",affinity,value);log(msg);
  check_hresult(D3D11CreateDevice(nullptr,D3D_DRIVER_TYPE_HARDWARE,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,dev.put(),nullptr,context.put()));
  auto dx=dev.as<IDXGIDevice>();com_ptr<IDXGIAdapter> adapter;check_hresult(dx->GetAdapter(adapter.put()));com_ptr<IDXGIFactory2> factory;check_hresult(adapter->GetParent(IID_PPV_ARGS(factory.put())));
  DXGI_SWAP_CHAIN_DESC1 desc{};desc.Width=width;desc.Height=height;desc.Format=DXGI_FORMAT_B8G8R8A8_UNORM;desc.SampleDesc.Count=1;desc.BufferUsage=DXGI_USAGE_RENDER_TARGET_OUTPUT;desc.BufferCount=2;desc.SwapEffect=DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;desc.AlphaMode=DXGI_ALPHA_MODE_PREMULTIPLIED;desc.Flags=DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT;
  check_hresult(factory->CreateSwapChainForComposition(dev.get(),&desc,nullptr,swap.put()));auto s2=swap.as<IDXGISwapChain2>();check_hresult(s2->SetMaximumFrameLatency(1));frameWait.h=s2->GetFrameLatencyWaitableObject();
  check_hresult(DCompositionCreateDevice(dx.get(),__uuidof(IDCompositionDevice),composition.put_void()));check_hresult(composition->CreateTargetForHwnd(hwnd,TRUE,target.put()));check_hresult(composition->CreateVisual(root.put()));check_hresult(root->SetContent(swap.get()));check_hresult(target->SetRoot(root.get()));check_hresult(composition->Commit());
  com_ptr<ID2D1Factory1> df;check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED,__uuidof(ID2D1Factory1),nullptr,df.put_void()));com_ptr<ID2D1Device> dd;check_hresult(df->CreateDevice(dx.get(),dd.put()));check_hresult(dd->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE,draw.put()));
  check_hresult(swap->GetBuffer(0,IID_PPV_ARGS(backTexture.put())));auto surface=backTexture.as<IDXGISurface>();
  auto props=D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_TARGET|D2D1_BITMAP_OPTIONS_CANNOT_DRAW,D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM,D2D1_ALPHA_MODE_PREMULTIPLIED));
  check_hresult(draw->CreateBitmapFromDxgiSurface(surface.get(),&props,back.put()));draw->SetTarget(back.get());check_hresult(draw->CreateSolidColorBrush(D2D1::ColorF(0,0,0),shade.put()));
  com_ptr<IInspectable> inspect;check_hresult(CreateDirect3D11DeviceFromDXGIDevice(dx.get(),inspect.put()));captureDevice=inspect.as<IDirect3DDevice>();
  check_hresult(composition->CreateEffectGroup(opacity.put()));check_hresult(root->SetEffect(opacity.get()));
  loadWallpaper();
  // Request once at process start, never on a three-finger tap. Until access
  // succeeds use frozen screen crops: a denied request must never add a border.
  try{borderAccess=GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless);}catch(...){log("borderless access unavailable; using screen crops");}
 }
 ~Overlay(){hide();if(hwnd)DestroyWindow(hwnd);}
 void hide(bool preserveAnimationScope=false){if(hwnd)ShowWindow(hwnd,SW_HIDE);if(!preserveAnimationScope)animations.restore();if(opacity&&composition){opacity->SetOpacity(1);composition->Commit();}windows.clear();snapshot=nullptr;restoreBackground=nullptr;restoring=false;mode=Idle;log("idle; overlay hidden; capture sessions released");}
 bool borderless(){
  try{if(!borderAccess || borderAccess.Status()!=winrt::Windows::Foundation::AsyncStatus::Completed)return false;
   auto status=borderAccess.GetResults();bool allowed=status==winrt::Windows::Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus::Allowed;
   if(!borderLogged){char line[100];sprintf_s(line,"borderless access status=%d; WGC enabled=%d",int(status),allowed);log(line);borderLogged=true;}return allowed;
  }catch(...){return false;}
 }
 com_ptr<ID2D1Bitmap1> screen(){
  HDC src=GetDC(nullptr),dc=CreateCompatibleDC(src);BITMAPINFO bi{};bi.bmiHeader={sizeof(BITMAPINFOHEADER),(LONG)width,-(LONG)height,1,32,BI_RGB};void* bits=nullptr;
  HBITMAP bmp=CreateDIBSection(src,&bi,DIB_RGB_COLORS,&bits,nullptr,0);if(!bmp){DeleteDC(dc);ReleaseDC(nullptr,src);throw hresult_error(E_OUTOFMEMORY);}
  auto old=SelectObject(dc,bmp);BOOL ok=BitBlt(dc,0,0,width,height,src,monitor.left,monitor.top,SRCCOPY|CAPTUREBLT);
  com_ptr<ID2D1Bitmap1> result;auto bp=D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_NONE,D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM,D2D1_ALPHA_MODE_IGNORE));
  if(ok)GdiFlush();
  HRESULT hr=ok?draw->CreateBitmap(D2D1::SizeU(width,height),bits,width*4,&bp,result.put()):E_FAIL;
  if(ok){taskViewProbe.resize(32*32*4);for(UINT y=0;y<32;y++)memcpy(taskViewProbe.data()+y*32*4,(BYTE*)bits+((height/32+y)*width+width/32)*4,32*4);}
  SelectObject(dc,old);DeleteObject(bmp);DeleteDC(dc);ReleaseDC(nullptr,src);check_hresult(hr);return result;
 }
 void loadWallpaper(){
  // Re-read on every session: public API follows the currently visible desktop.
  try{
   com_ptr<IDesktopWallpaper> desktop;check_hresult(CoCreateInstance(CLSID_DesktopWallpaper,nullptr,CLSCTX_ALL,IID_PPV_ARGS(desktop.put())));
   LPWSTR id=nullptr,path=nullptr;check_hresult(desktop->GetMonitorDevicePathAt(0,&id));HRESULT hr=desktop->GetWallpaper(id,&path);CoTaskMemFree(id);check_hresult(hr);
   std::wstring file=path?path:L"";CoTaskMemFree(path);
   DESKTOP_WALLPAPER_POSITION position=DWPOS_FILL;desktop->GetPosition(&position);COLORREF color=0;desktop->GetBackgroundColor(&color);desktopColor=D2D1::ColorF(GetRValue(color)/255.f,GetGValue(color)/255.f,GetBValue(color)/255.f,1);
   WIN32_FILE_ATTRIBUTE_DATA attributes{};GetFileAttributesEx(file.c_str(),GetFileExInfoStandard,&attributes);
   if(file==wallpaperPath && CompareFileTime(&wallpaperStamp,&attributes.ftLastWriteTime)==0 && position==wallpaperPosition)return;
   wallpaper=nullptr;wallpaperPath=file;wallpaperStamp=attributes.ftLastWriteTime;wallpaperPosition=position;if(file.empty())return;
   com_ptr<IWICImagingFactory> f;check_hresult(CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(f.put())));
   com_ptr<IWICBitmapDecoder> decoder;check_hresult(f->CreateDecoderFromFilename(file.c_str(),nullptr,GENERIC_READ,WICDecodeMetadataCacheOnLoad,decoder.put()));com_ptr<IWICBitmapFrameDecode> frame;check_hresult(decoder->GetFrame(0,frame.put()));
   com_ptr<IWICFormatConverter> converter;check_hresult(f->CreateFormatConverter(converter.put()));check_hresult(converter->Initialize(frame.get(),GUID_WICPixelFormat32bppPBGRA,WICBitmapDitherTypeNone,nullptr,0,WICBitmapPaletteTypeCustom));
   // Decode once at monitor resolution rather than retaining a 6K wallpaper.
   com_ptr<IWICBitmapScaler> scaler;check_hresult(f->CreateBitmapScaler(scaler.put()));UINT w,h;frame->GetSize(&w,&h);float sx=float(width)/w,sy=float(height)/h,scale=std::max(sx,sy);
   if(position==DWPOS_FIT)scale=std::min(sx,sy);if(position==DWPOS_CENTER||position==DWPOS_TILE)scale=1;
   UINT targetW=std::max(1u,UINT(w*scale)),targetH=std::max(1u,UINT(h*scale));if(position==DWPOS_STRETCH){targetW=width;targetH=height;}
   check_hresult(scaler->Initialize(converter.get(),targetW,targetH,WICBitmapInterpolationModeFant));check_hresult(draw->CreateBitmapFromWicBitmap(scaler.get(),nullptr,wallpaper.put()));
  }catch(...){log("wallpaper unavailable; using solid desktop background");}
 }
 static BOOL CALLBACK enumerate(HWND h,LPARAM l){
  auto self=(Overlay*)l;if(h==self->hwnd || !IsWindowVisible(h) || IsIconic(h))return TRUE;
  DWORD pid;GetWindowThreadProcessId(h,&pid);if(pid==GetCurrentProcessId())return TRUE;
  auto ex=GetWindowLongPtr(h,GWL_EXSTYLE);if(ex&WS_EX_TOOLWINDOW)return TRUE;
  if(GetWindow(h,GW_OWNER) && !(ex&WS_EX_APPWINDOW))return TRUE;
  wchar_t cls[128];GetClassName(h,cls,128);if(!wcscmp(cls,L"Progman")||!wcscmp(cls,L"WorkerW")||!wcscmp(cls,L"Shell_TrayWnd")||!wcscmp(cls,L"Shell_SecondaryTrayWnd"))return TRUE;
  DWORD cloak=0;DwmGetWindowAttribute(h,DWMWA_CLOAKED,&cloak,sizeof(cloak));if(cloak)return TRUE;
  RECT r{},intersection{};if(FAILED(DwmGetWindowAttribute(h,DWMWA_EXTENDED_FRAME_BOUNDS,&r,sizeof(r))))GetWindowRect(h,&r);
  if(r.right-r.left<30 || r.bottom-r.top<30 || !IntersectRect(&intersection,&r,&self->monitor))return TRUE;
  Window win;win.hwnd=h;win.rect=r;self->windows.push_back(std::move(win));return TRUE;
 }
 void prepare(const protocol::State& s){
  hide();session=s.session;gesture=s.gesture;lastQpc=s.qpc;state=s;progress=s.progress;
  auto began=protocol::now();snapshot=screen();auto froze=protocol::now();EnumWindows(enumerate,(LPARAM)this);auto enumerated=protocol::now();
  // DWM cloaking already excludes other virtual desktops. Avoid synchronous
  // Explorer COM round trips on every finger landing (and Explorer restarts).
  size_t n=windows.size();int cols=std::max(1,std::min(int(n),int(ceil(sqrt(double(n)*width/height))))),rows=std::max(1,int((n+cols-1)/cols));
  // Native screenshot measurements: equal image heights ~331px, gaps ~26px,
  // image centers y~504 on 1920x1200. Use aspect-preserving packed rows.
  float gap=width*.01354f,areaWidth=width*.88f;
  float ch=std::min(height*.50f,(height*.607f-(rows-1)*height*.055f)/rows);
  std::vector<float> rowHeights(rows,ch);
  for(int row=0;row<rows;row++){
   float ratios=0;int count=0;for(size_t i=size_t(row)*cols;i<n&&i<size_t(row+1)*cols;i++){auto& w=windows[i];ratios+=float(w.rect.right-w.rect.left)/(w.rect.bottom-w.rect.top);count++;}
   rowHeights[row]=std::min(ch,(areaWidth-gap*std::max(0,count-1))/std::max(.1f,ratios));
  }
  for(size_t i=0;i<n;i++){
   auto& w=windows[i];float rw=float(w.rect.right-w.rect.left),rh=float(w.rect.bottom-w.rect.top);int row=int(i)/cols;
   float scale=rowHeights[row]/rh,total=0,x=0;
   for(size_t j=size_t(row)*cols;j<n&&j<size_t(row+1)*cols;j++){auto& v=windows[j];float itemWidth=rowHeights[row]*float(v.rect.right-v.rect.left)/(v.rect.bottom-v.rect.top);if(j<i)x+=itemWidth+gap;total+=itemWidth+gap;}
   x+=(width-(total-gap))/2;float y=height*.42f+(row-(rows-1)*.5f)*(ch+height*.055f)-rh*scale/2;
   w.grid=RectF(x,y,x+rw*scale,y+rh*scale);
  }
  stageTime=protocol::now()/freq;mode=Armed;actionSent=false;readySince=0;
  // Pre-submit the zero frame while hidden; tracking only needs Present + Show.
  captureIndex=0;progress=0;render();
  char msg[240];sprintf_s(msg,"prepared session=%u windows=%zu acquisitionMs=%.2f freezeMs=%.2f enumerateMs=%.2f",session,n,(protocol::now()-began)*1000/freq,(froze-began)*1000/freq,(enumerated-froze)*1000/freq);log(msg);
 }
 void prepareCapture(){
  if(captureIndex>=windows.size()||!borderless())return;
  auto& w=windows[captureIndex++];auto began=protocol::now();
  // Incremental acquisition yields to protocol input between windows. A short
  // arm still has a complete screen-crop preview and never waits for WGC.
  try{auto interop=get_activation_factory<GraphicsCaptureItem,IGraphicsCaptureItemInterop>();check_hresult(interop->CreateForWindow(w.hwnd,guid_of<GraphicsCaptureItem>(),put_abi(w.item)));w.pool=Direct3D11CaptureFramePool::CreateFreeThreaded(captureDevice,DirectXPixelFormat::B8G8R8A8UIntNormalized,2,w.item.Size());w.capture=w.pool.CreateCaptureSession(w.item);w.capture.IsCursorCaptureEnabled(false);w.capture.IsBorderRequired(false);w.capture.StartCapture();}catch(...){w.stop();log("borderless window capture unavailable; screenshot crop fallback");}
  char line[120];sprintf_s(line,"armed WGC window=%zu startMs=%.2f",captureIndex,(protocol::now()-began)*1000/freq);log(line);
 }
 void begin(const protocol::State& s){
  if(mode!=Armed || session!=s.session)prepare(s);
  gesture=s.gesture;state=s;lastQpc=s.qpc;progress=s.progress;mode=Tracking;
  if(gesture==3){
   restoreBackground=snapshot;windows.clear();snapshot=nullptr;restoring=!desktopWindows.empty();
   // Copy only immutable images/geometry; never re-open minimized captures.
   for(auto& cached:desktopWindows){if(!IsWindow(cached.hwnd))continue;Window w;w.hwnd=cached.hwnd;w.rect=cached.rect;w.bitmap=cached.bitmap;w.texture=cached.texture;windows.push_back(std::move(w));}
   snapshot=desktopSnapshot;
   if(windows.empty()){restoring=false;log("restore has no cache; preview suppressed");return;}
  }
  render();ShowWindow(hwnd,SW_SHOWNOACTIVATE);SetWindowPos(hwnd,HWND_TOPMOST,monitor.left,monitor.top,width,height,SWP_NOACTIVATE);
  if(measure)DwmFlush();char msg[160];sprintf_s(msg,"tracking start session=%u gesture=%u writeToFirstFrameMs=%.3f",session,gesture,(protocol::now()-s.qpc)*1000/freq);log(msg);
 }
 void updateCaptures(){for(auto& w:windows){if(!w.pool)continue;try{
  Direct3D11CaptureFrame latest{nullptr};while(auto f=w.pool.TryGetNextFrame())latest=f;if(!latest)continue;
  auto access=latest.Surface().as<::Windows::Graphics::DirectX::Direct3D11::IDirect3DDxgiInterfaceAccess>();com_ptr<ID3D11Texture2D> tex;check_hresult(access->GetInterface(IID_PPV_ARGS(tex.put())));
  D3D11_TEXTURE2D_DESC desc;tex->GetDesc(&desc);D3D11_TEXTURE2D_DESC current{};if(w.texture)w.texture->GetDesc(&current);
  if(!w.texture || current.Width!=desc.Width || current.Height!=desc.Height){
   w.bitmap=nullptr;w.texture=nullptr;desc.BindFlags=D3D11_BIND_SHADER_RESOURCE|D3D11_BIND_RENDER_TARGET;desc.MiscFlags=0;desc.Usage=D3D11_USAGE_DEFAULT;desc.CPUAccessFlags=0;
   check_hresult(dev->CreateTexture2D(&desc,nullptr,w.texture.put()));auto surface=w.texture.as<IDXGISurface>();auto props=D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_NONE,D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM,D2D1_ALPHA_MODE_PREMULTIPLIED));check_hresult(draw->CreateBitmapFromDxgiSurface(surface.get(),&props,w.bitmap.put()));
  }context->CopyResource(w.texture.get(),tex.get());
 }catch(...){/* Window destruction/capture loss retains last good frame. */}}}
 static float rubber(float p){if(p<0)return -.13f*(1-expf(p/.13f));if(p>1)return 1+.13f*(1-expf(-(p-1)/.13f));return p;}
 void render(){
  if(gesture==3 && !restoring)return;
  if(mode!=Handoff)updateCaptures();float p=rubber(gesture==3?1-progress:progress),clamped=std::clamp(p,0.f,1.f);
  draw->BeginDraw();draw->Clear(desktopColor);
  if(wallpaper){auto size=wallpaper->GetSize();if(wallpaperPosition==DWPOS_TILE){for(float y=0;y<height;y+=size.height)for(float x=0;x<width;x+=size.width)draw->DrawBitmap(wallpaper.get(),RectF(x,y,x+size.width,y+size.height));}else draw->DrawBitmap(wallpaper.get(),RectF((width-size.width)/2,(height-size.height)/2,(width+size.width)/2,(height+size.height)/2));}
  if(gesture==3 && restoreBackground)draw->DrawBitmap(restoreBackground.get(),RectF(0,0,float(width),float(height)));
  if(snapshot && clamped<.16f)draw->DrawBitmap(snapshot.get(),RectF(0,0,float(width),float(height)),1-clamped/.16f);
  if(gesture==1){shade->SetOpacity(clamped*.65f);draw->FillRectangle(RectF(0,0,float(width),float(height)),shade.get());}
  if(fabsf(p)<.00001f && snapshot)draw->DrawBitmap(snapshot.get(),RectF(0,0,float(width),float(height)));
  else {
   // Windows are enumerated front to back; draw back to front.
   for(auto it=windows.rbegin();it!=windows.rend();++it){auto& w=*it;
    D2D1_RECT_F from=RectF(float(w.rect.left-monitor.left),float(w.rect.top-monitor.top),float(w.rect.right-monitor.left),float(w.rect.bottom-monitor.top)),to=w.grid;
    if(gesture==2 || gesture==3){float cx=(from.left+from.right)/2;float rw=(from.right-from.left)*.7f,rh=(from.bottom-from.top)*.7f;to=RectF(cx-rw/2,float(height)+80,cx+rw/2,float(height)+80+rh);}
    auto dest=RectF(from.left+(to.left-from.left)*p,from.top+(to.top-from.top)*p,from.right+(to.right-from.right)*p,from.bottom+(to.bottom-from.bottom)*p);
    if(w.bitmap){draw->DrawBitmap(w.bitmap.get(),dest,1,D2D1_INTERPOLATION_MODE_LINEAR);}
    else if(snapshot){draw->DrawBitmap(snapshot.get(),dest,1,D2D1_INTERPOLATION_MODE_LINEAR,&from);}
   }
  }
  check_hresult(draw->EndDraw());check_hresult(swap->Present(1,0));
 }
 void settle(bool commit){startProgress=progress;goal=commit?1.f:0.f;stageTime=protocol::now()/freq;mode=Settling;if(commit)action();}
 void input(const protocol::State& s){
  if(s.session!=session){hide();if(s.phase==4){prepare(s);return;}if(s.phase==2 && s.gesture){session=s.session;gesture=s.gesture;state=s;progress=s.progress;settle(true);return;}if(s.phase==1 && s.gesture){begin(s);}else {session=s.session;return;}}
  if(mode==Armed){
   state=s;lastQpc=s.qpc;
   if(s.phase==0||s.phase==3){hide();return;}
   if(s.phase==4)return;
   if(s.gesture&&(s.phase==1||s.phase==2))begin(s);
  }
  if(mode!=Tracking)return;
  state=s;lastQpc=s.qpc;
  if(s.phase==1)progress=s.progress;
  else if(s.phase==2 && s.gesture){progress=s.progress;settle(true);}
  else if(s.phase==3 || s.phase==0)settle(false);
 }
 bool shellReady(){
  if(gesture==2 || gesture==3){bool any=false;for(auto& w:windows){if(!IsWindow(w.hwnd))continue;any=true;DWORD cloak=0;DwmGetWindowAttribute(w.hwnd,DWMWA_CLOAKED,&cloak,sizeof(cloak));bool visible=IsWindowVisible(w.hwnd)&&!IsIconic(w.hwnd)&&!cloak;if(gesture==2&&visible)return false;if(gesture==3&&!visible)return false;}return gesture==2||any||windows.empty();}
  HWND h=GetForegroundWindow();wchar_t c[128]{},title[128]{};GetClassName(h,c,128);GetWindowText(h,title,128);
  if(!IsWindowVisible(h)||wcscmp(c,L"XamlExplorerHostIslandWindow")||wcscmp(title,L"Task View"))return false;
  // Foreground is assigned before the shell has submitted its first frame.
  // With WDA_EXCLUDEFROMCAPTURE a small screen read observes the actual shell,
  // not our preview. Wait for its wallpaper to replace the original corner.
  if(taskViewProbe.empty())return false;
  HDC src=GetDC(nullptr),dc=CreateCompatibleDC(src);BITMAPINFO bi{};bi.bmiHeader={sizeof(BITMAPINFOHEADER),32,-32,1,32,BI_RGB};void* bits=nullptr;
  auto bmp=CreateDIBSection(src,&bi,DIB_RGB_COLORS,&bits,nullptr,0);auto old=SelectObject(dc,bmp);bool painted=false;
  if(bmp&&BitBlt(dc,0,0,32,32,src,monitor.left+width/32,monitor.top+height/32,SRCCOPY|CAPTUREBLT)){
   GdiFlush();unsigned difference=0;for(size_t i=0;i<taskViewProbe.size();i++)if(i%4!=3)difference+=abs(int(((BYTE*)bits)[i])-int(taskViewProbe[i]));painted=difference>32*32*3*12;
  }
  SelectObject(dc,old);DeleteObject(bmp);DeleteDC(dc);ReleaseDC(nullptr,src);return painted;
 }
 void action(){
  if(gesture==1)animations.suppress(protocol::now()/freq);
  // Freeze images before Win+D minimizes the sources, retaining a separate
  // immutable cache across idle, armed taps, cancelled restore and Task View.
  if(gesture==2){updateCaptures();desktopWindows.clear();desktopSnapshot=snapshot;
   for(auto& w:windows){Window cached;cached.hwnd=w.hwnd;cached.rect=w.rect;cached.bitmap=w.bitmap;cached.texture=w.texture;desktopWindows.push_back(std::move(cached));}}
  INPUT keys[4]{};WORD key=gesture==1?VK_TAB:'D';for(auto& k:keys)k.type=INPUT_KEYBOARD;keys[0].ki.wVk=VK_LWIN;keys[1].ki.wVk=key;keys[2].ki.wVk=key;keys[2].ki.dwFlags=KEYEVENTF_KEYUP;keys[3].ki.wVk=VK_LWIN;keys[3].ki.dwFlags=KEYEVENTF_KEYUP;
  UINT sent=SendInput(4,keys,sizeof(INPUT));char msg[160];sprintf_s(msg,"shell action gesture=%u SendInput=%u commitToInputMs=%.3f",gesture,sent,(protocol::now()-state.qpc)*1000/freq);log(msg);
  if(sent!=4){INPUT releases[2]{};releases[0]=keys[2];releases[1]=keys[3];SendInput(2,releases,sizeof(INPUT));hide();return;}
  actionSent=true;actionTime=protocol::now()/freq;mode=Handoff;readySince=0;
 }
 void tick(){double now=protocol::now()/freq;
  if(animations.changed && now>=animations.deadline)animations.restore();
  if((mode==Tracking||mode==Armed) && now-lastQpc/freq>1.5){log("watchdog expired; cancelling");if(mode==Armed){hide();return;}settle(false);}
  if(mode==Armed){updateCaptures();prepareCapture();return;}
  if(mode==Settling || mode==Handoff){
   float duration=mode==Handoff?.16f:std::clamp(.12f+fabsf(goal-startProgress)*.10f-fabsf(state.velocity)*.008f,.09f,.22f);
   float t=float(std::clamp((now-stageTime)/duration,0.,1.));float ease=1-powf(1-t,3);progress=startProgress+(goal-startProgress)*ease;
   if(mode==Settling && t>=1){render();hide();return;}
  }
  if(mode==Handoff){bool ready=shellReady();
   // The shell renders above our band for Task View. Release immediately when
   // it takes over; Win+D releases on visibility, without a settle/hold/fade.
   if(ready || now-actionTime>(gesture==1?.45:.30)){char msg[160];sprintf_s(msg,"handoff gesture=%u ready=%d actionToHideMs=%.3f",gesture,ready,(protocol::now()/freq-actionTime)*1000);log(msg);hide(gesture==1);return;}
  }
  if(mode!=Idle && !(mode==Handoff && progress>=1))render();
 }
 void shot(int id){if(mode==Idle)return;D3D11_TEXTURE2D_DESC desc;backTexture->GetDesc(&desc);desc.Usage=D3D11_USAGE_STAGING;desc.BindFlags=0;desc.MiscFlags=0;desc.CPUAccessFlags=D3D11_CPU_ACCESS_READ;com_ptr<ID3D11Texture2D> tex;check_hresult(dev->CreateTexture2D(&desc,nullptr,tex.put()));context->CopyResource(tex.get(),backTexture.get());D3D11_MAPPED_SUBRESOURCE map;check_hresult(context->Map(tex.get(),0,D3D11_MAP_READ,0,&map));
  CreateDirectory((directory+L"\\..\\test-shots").c_str(),nullptr);wchar_t name[80];swprintf_s(name,L"\\..\\test-shots\\gesture-%u-%d.png",gesture,id);try{png((directory+name).c_str(),(BYTE*)map.pData,width,height,map.RowPitch);}catch(...){context->Unmap(tex.get(),0);throw;}context->Unmap(tex.get(),0);log("saved compositor PNG");}
 void run(HANDLE wake,const protocol::State* memory){
  // A terminal block can outlive this process. It is not a new instruction
  // after restart (especially important for the Win+D toggle).
  protocol::State initial{};if(protocol::read(memory,initial)&&(initial.phase==2||initial.phase==3)){session=initial.session;lastSeq=initial.seq;}
  while(!quit){HANDLE handles[2]={wake,frameWait};DWORD count=mode==Idle?1:2;
   if(mode==Armed || (mode==Handoff && progress>=1))count=1;
   DWORD timeout=mode==Idle?INFINITE:mode==Armed?32:8;
   if(animations.changed)timeout=std::min(timeout,DWORD(std::max(1.,(animations.deadline-protocol::now()/freq)*1000)));
   auto wait=MsgWaitForMultipleObjectsEx(count,handles,timeout,QS_ALLINPUT,MWMO_INPUTAVAILABLE);
   if(wait==WAIT_FAILED)throw hresult_error(HRESULT_FROM_WIN32(GetLastError()));
   MSG msg;while(PeekMessage(&msg,nullptr,0,0,PM_REMOVE)){if(msg.message==WM_QUIT)quit=true;TranslateMessage(&msg);DispatchMessage(&msg);}
   if(displayChanged){hide();log("display changed; exiting safely (launcher may restart)");break;}
   if(animations.changed && protocol::now()/freq>=animations.deadline)animations.restore();
   protocol::State s{};bool changed=protocol::read(memory,s)&&s.seq!=lastSeq;
   if(changed){lastSeq=s.seq;input(s);}
   if(mode!=Idle){tick();if(changed && s.phase!=4){double submitted=(protocol::now()-s.qpc)*1000/freq;
    if(measure)DwmFlush();double flushed=(protocol::now()-s.qpc)*1000/freq;
    char line[260];sprintf_s(line,"frame session=%u seq=%u p=%.3f writeToPresentReturnMs=%.3f writeToDwmFlushMs=%.3f",s.session,s.seq,s.progress,submitted,measure?flushed:-1.);log(line);}}
  }
 }
};
int WINAPI wWinMain(HINSTANCE,HINSTANCE,PWSTR args,int){
 SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
 Handle mutex;mutex.h=CreateMutex(nullptr,TRUE,protocol::Running);if(!mutex.h)return 1;if(GetLastError()==ERROR_ALREADY_EXISTS){DWORD result=WaitForSingleObject(mutex,0);if(result!=WAIT_OBJECT_0&&result!=WAIT_ABANDONED)return 0;}
 wchar_t path[MAX_PATH];GetModuleFileName(nullptr,path,MAX_PATH);directory=path;directory.resize(directory.find_last_of(L"\\"));logFile=_wfsopen((directory+L"\\overlay.log").c_str(),L"w",_SH_DENYNO);
 int result=0;try{init_apartment(apartment_type::single_threaded);Handle mapping,wake;mapping.h=CreateFileMapping(INVALID_HANDLE_VALUE,nullptr,PAGE_READWRITE,0,4096,protocol::Mapping);wake.h=CreateEvent(nullptr,FALSE,FALSE,protocol::Wake);check_bool(mapping.h&&wake.h);
  auto memory=(protocol::State*)MapViewOfFile(mapping,FILE_MAP_READ,0,0,4096);check_bool(memory!=nullptr);
  try{Overlay overlay;overlay.measure=wcsstr(args,L"--measure")!=nullptr;overlay.init();log("ready");overlay.run(wake,memory);}catch(...){UnmapViewOfFile(memory);throw;}UnmapViewOfFile(memory);
 }catch(hresult_error const& e){char msg[128];sprintf_s(msg,"fatal HRESULT=%08lx; overlay destroyed",(HRESULT)e.code());log(msg);result=1;}catch(...){log("fatal exception; overlay destroyed");result=1;}
 if(logFile)fclose(logFile);ReleaseMutex(mutex);return result;
}
