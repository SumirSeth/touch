# Overlay compositor investigation

Investigated 2026-10-29 on this machine. Scope: visuals, rendering, and committing actions; raw touchpad input is assumed solved. No files in `trackpad/gestures/` were changed.

Evidence labels: **Verified** means exercised on this machine; **Source** means supported by the linked API documentation or upstream project; **Inference** means an engineering proposal or expectation, not a completed implementation.

## Verdict

**Build an overlay prototype for desktop switching and show desktop. Treat native Task View handoff as an approximation, or implement a custom overview instead.** Importantly, shell-cloaked windows are not automatically uncapturable on this build: both DWM thumbnails and Windows Graphics Capture (WGC) produced real content from a control window on an inactive virtual desktop.

| Target | Feasibility | Expected limitation |
| --- | --- | --- |
| Four fingers left/right | **Promising.** Finger-controlled translation of two desktop images is straightforward; non-animated COM switching works on this build. | Cached destinations can be stale; reconstructing a live destination from windows is imperfect. Focus, taskbar, and wallpaper changes can spoil the final handoff. |
| Three fingers down: show desktop | **Promising.** Animate captured windows away, cancel without touching real windows, then invoke the shell action underneath an opaque overlay. | Need desktop icons and wallpaper underneath, plus confirmation that the shell has completed hiding windows before removal. |
| Three fingers up: native Task View | **Partial.** A custom thumbnail grid can track fingers and cancel cleanly. | No documented API discovered for setting native Task View's animation progress or providing our final grid to it. Native layout/focus/animation will differ. |
| Three fingers up: our own persistent overview | **Technically feasible, substantially more work.** | Must implement selection, keyboard navigation, desktop strip, window lifetime handling, and accessibility ourselves. This changes the requested destination from native Task View. |

These verdicts concern the overlay approach, not proof of a finished seamless product.

## Machine and experiment record

**Verified:** ASUS `Vivobook_ASUSLaptop K3605ZU_K3605ZU`; Windows build **26300.9711**, registry DisplayVersion `26H2`; active Intel UHD-driven display **1920×1200 at 144 Hz**. NVIDIA RTX 4050 Laptop GPU is also present. `EnumDisplaySettings` agreed with WMI about the active mode. A per-monitor-DPI-aware process read **96 DPI / 100% scaling**, including `GetDpiForWindow` on the control window. One refresh interval is about **6.94 ms**.

**Verified:** three existing virtual desktops. Desktop 1 was initially visible and was visible at the end. The private desktop COM service accepted the 24H2 interface definitions from [MScholtes/VirtualDesktop](https://github.com/MScholtes/VirtualDesktop/blob/a725cbd3cdb9e977678eeaf034a7cc96d2e74bc6/VirtualDesktop11-24H2.cs). The source was inspected and compiled locally with .NET Framework `csc`; no installation was needed. MSVC and Windows SDK 10.0.22000.0 were also already installed, enabling native C++/WinRT experiments despite the absence of a .NET SDK.

Artifacts, sources, compiler scripts, logs, and PNGs are in [scratch-02](scratch-02/). Principal experiments:

- [probe.cpp](scratch-02/probe.cpp), [build.cmd](scratch-02/build.cmd), [probe-results.txt](scratch-02/probe-results.txt): capture controls. Creates temporary windows, moves only its own application windows to the adjacent desktop, captures them, and destroys them. The final version does **not** switch the user's visible desktop.
- [probe-switch.cpp](scratch-02/probe-switch.cpp), [probe-switch-results.txt](scratch-02/probe-switch-results.txt): earlier capture experiment plus a short raw desktop-switch round trip. Uses CPU readback for tracing; not a latency benchmark. Do not confuse its screenshots with the final probe run, which overwrote the common PNG names.
- [compositor.cpp](scratch-02/compositor.cpp), [build-compositor.cmd](scratch-02/build-compositor.cmd), [compositor-results.txt](scratch-02/compositor-results.txt): transparent full-display DirectComposition/flip-chain setup and hit-testing experiment.
- `VirtualDesktop11-24H2.cs`, its MIT license, and locally compiled `VirtualDesktop.exe`: upstream desktop inspection helper.

**State handling:** no registry writes, installs, Explorer/DWM restarts, or animation setting changes. `SPI_GETCLIENTAREAANIMATION` was read and returned true; no SPI setter was called. `/Animation:Off` on the inspection helper only selects its own non-animated COM method for that process; it does not turn off a system setting. Temporary capture sessions, thumbnails, and windows were closed. The raw switch test reported returning to its saved desktop, but an independent subsequent check showed Desktop 2. The cause was not established; asynchronous focus/activation or concurrent investigation is possible. I explicitly returned to the initially observed Desktop 1 after cleanup and independently checked `/List` again. The final capture run also ended on Desktop 1. Do not rely on a successful COM return alone for restoration or handoff correctness.

## What capture actually did

The source control was a normal `WS_OVERLAPPEDWINDOW | WS_EX_APPWINDOW` window painting a blue client area and an increasing frame number on a 40 ms timer. It was shown without activation. Public `IVirtualDesktopManager::MoveWindowToDesktop` moved that owned window to an existing adjacent desktop while the user stayed on the original desktop.

**Verified:** `GetWindowDesktopId` matched the destination, `IsWindowOnCurrentVirtualDesktop` was false, `IsWindowVisible` remained true, and `DwmGetWindowAttribute(DWMWA_CLOAKED)` returned **2 (`DWM_CLOAKED_SHELL`)**. An earlier no-activate popup control remained uncloaked; that initial run was rejected as evidence about cloaked windows.

| Capture path | Verified result on shell-cloaked control | What it does not establish |
| --- | --- | --- |
| `DwmRegisterThumbnail` / `DwmUpdateThumbnailProperties` | Both returned `S_OK`; a relationship established before cloaking and a newly registered relationship after cloaking both displayed the blue content and advancing counter. | Not proof every minimized, GPU, protected, or suspended app has an available/live surface. |
| WGC `CreateForWindow`, session begun while visible | Continued delivering content after cloaking; final run drained 16 frames during its roughly 700 ms observation. | No universal cross-app or lifetime guarantee. |
| WGC `CreateForWindow`, session begun after cloaking | Worked; final run drained 16 frames in roughly 700 ms. | Existing DWM thumbnail/WGC relationships were active in this first cold test. |
| WGC independent cold control | A second window had no prior thumbnail or WGC session. After confirmed cloak=2, a new WGC session delivered 15 frames in roughly 700 ms. Saved image visibly contained its counter. | `PrintWindow` was called once before this session; this was not a test of an app which has never painted. |
| `PrintWindow(hwnd, dc, 2 /* PW_RENDERFULLCONTENT */)` | Returned 1 with correct blue content and text, including the independent cloaked control. | Our GDI app cooperates with printing. GPU applications can behave differently. |
| WGC `CreateForMonitor` | Produced 1920×1200 image of the visible monitor. The cloaked source was absent at its original position; its explicitly displayed thumbnail was present. | No ability to select an inactive virtual desktop by desktop GUID. |
| DXGI `DuplicateOutput` / `AcquireNextFrame` | Both returned `S_OK`, producing the visible 1920×1200 output, likewise containing the thumbnail rather than the hidden source at its old location. | Does not expose an off-screen virtual desktop's composite image. |

Representative evidence: [independent cloaked WGC image](scratch-02/wgc-isolated-cloaked.png), [cloaked PrintWindow image](scratch-02/print-isolated-cloaked.png), [new cloaked DWM thumbnail](scratch-02/thumbnail-cold-cloaked.png). Images and logs contain pixel counts and hashes; the counter text was also visually inspected. PNG/CPU readback exists only for investigation and should not be in the production render loop.

**Source:** [WGC desktop interop](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nn-windows-graphics-capture-interop-igraphicscaptureiteminterop) targets an HWND or HMONITOR; neither operation accepts a virtual-desktop GUID. [Desktop duplication](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api) supplies the composed output image. A Windows virtual desktop is a shell grouping of windows, not a separate display output or necessarily a separate Win32 input desktop. `OpenDesktop`/`SetThreadDesktop` is therefore not a route to another shell workspace's full image.

**Source:** [DWM thumbnails](https://learn.microsoft.com/en-us/windows/win32/dwm/thumbnail-ovw) render into our destination HWND as 2D rectangles. They are not an API returning `ID3D11Texture2D`. [Registration](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmregisterthumbnail) requires top-level HWNDs and an owned destination. They suit a simple thumbnail grid or rectangular translation prototype; WGC textures are preferable for a unified shader scene, arbitrary transforms, clipping, and snapshots. Do not assume mixing DWM thumbnail placement with a separate DirectComposition visual tree gives arbitrary shared z-order.

**Source/inference:** [PrintWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-printwindow) is synchronous and depends on the source application. It belongs in an isolated, time-bounded fallback worker, never the input/render thread. A nonzero return alone does not prove useful content. A hung target could block a worker indefinitely, so a disposable helper process gives a stronger timeout boundary than a cancellable managed task.

### Current desktop, inactive desktops, and cache policy

**Proposal:** take the current desktop from a continuously warm monitor capture, copy its latest texture before showing the overlay, and freeze that image for the gesture. This gives exact initial composition, including translucent windows and taskbar. Keep a texture per desktop GUID/monitor/configuration from the last frame before it stops being visible. Also capture the new desktop once it has settled. Never switch to other desktops during gesture acquisition just to obtain screenshots.

**Inference:** cache-only horizontal transitions will look coherent and track perfectly, but destination video, clocks, notifications, window layout, and wallpaper can be stale. First visits have no snapshot. Window creation/movement/closure must invalidate affected caches; topology, scale, resolution, theme, or wallpaper changes invalidate all dependent images. A single uncompressed 1920×1200 BGRA image is about 8.8 MiB; three desktop snapshots about 26.4 MiB, before staging, double buffering, and per-window resources.

**Proposal for higher quality:** enumerate eligible top-level app windows, assign them by public `GetWindowDesktopId`, preserve z-order and physical bounds, and composite WGC textures over each desktop's wallpaper. Explicitly handle pinned apps/windows and owned popups. Use the exact whole-screen snapshot initially, and use reconstructed neighboring desktops or a cached whole image when a window capture is unavailable.

**Inference:** reconstruction will differ from the shell around shadows, invisible resize borders, rounded corners, backdrop blur/Mica/Acrylic, menus, desktop icons, taskbar, and suspended content. Screen-coordinate `GetWindowRect` and WGC `ContentSize` are not interchangeable: our 500×240 outer control produced a 486×233 WGC image. Use DWM extended frame bounds and measured source crop offsets. A zero-progress pixel comparison is a useful acceptance criterion.

### Wallpaper

**Verified:** the private `IVirtualDesktop::GetWallpaperPath` returned the same existing PNG path for all three desktops (`26-tahoe-dark-6k.png` in Downloads). The file exists, about 52 MB. No wallpaper was changed.

**Source:** [IDesktopWallpaper](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-idesktopwallpaper) exposes monitor rectangles, wallpaper, position mode, and background color. It does not provide arbitrary virtual-desktop selection. Use its read methods for current monitor/layout semantics and the versioned private desktop interface for per-desktop file paths. Read-only registry caches can be a fallback diagnostic, not a stable interface.

**Inference:** decode wallpaper once, honor fill/fit/center/tile/span and background color, and use per-desktop cached desktop imagery for icon layers. A wallpaper file alone omits icons and dynamic wallpaper applications; a screenshot/desktop-layer capture is needed for those. `Progman` thumbnails are documented examples, but the modern shell's `WorkerW`/icon/backdrop ownership was not tested here, and should not be assumed to yield a complete inactive desktop.

## Concrete rendering architecture

**Verified baseline:** created a 1920×1200 popup with `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED`, `SetLayeredWindowAttributes(...,255,LWA_ALPHA)`, and `SetWindowPos(HWND_TOPMOST,...,SWP_NOACTIVATE)`. A BGRA-capable D3D11 device, `CreateSwapChainForComposition`, premultiplied BGRA, two buffers, `FLIP_SEQUENTIAL`, DirectComposition HWND target and visual all initialized successfully. Frame-latency waitable object and `SetMaximumFrameLatency(1)` succeeded. The full-screen scene was transparent so it did not obscure the user's work.

**Verified:** `WindowFromPoint` over the taskbar returned the same underlying HWND before/after overlay creation and did not return the overlay. This is a hit-testing check, not synthetic clicks or proof against every input path. 144 `Present(1,0)` calls with frame-latency waiting took **991.4 ms**. This is consistent with 144 Hz submission pacing; unchanged transparent frames do not prove visible animation smoothness or input-to-photon latency.

**Proposal:** keep the existing C# raw-input/recognition code; put GPU composition and capture in a small native C++ DLL or helper process. The already available native compiler can build this without adding a .NET runtime. Publish only latest gesture state plus timestamp through a small ABI or shared memory. Do not forward every contact packet into a queued UI animation.

1. Render thread owns the D3D11 immediate context and textures; shell COM work lives on a separate STA/message-pumped thread. Capture callbacks copy/update resources with explicit synchronization.
2. Use the adapter driving the monitor (Intel here) to avoid unnecessary RTX-to-Intel transfers. Handle device loss, resize, DPI/display changes, sleep, lock, and duplication access loss.
3. On gesture start, latch screenshots and window metadata, draw the exact initial image, present, then show the nonactivating overlay. Set background erase handling so an uninitialized black/white frame never appears.
4. On each frame-latency signal, sample the latest centroid displacement and render `x = progress × monitorWidth`. Place the two desktops at `x` and `x ± monitorWidth`. Clamp/rubber-band at desktop boundaries. Finger-held motion uses no easing; only release completion/cancellation uses a short settling curve.
5. Coalesce input and drop obsolete capture frames. Geometry keeps updating at display cadence even when content capture arrives less often. Maintain a last-good texture for every capture; never wait for a new source frame to move an existing image.
6. Cancellation animates to exactly zero, then hides the overlay. No desktop switch, show-desktop action, or Task View invocation occurs on cancellation.

**Source:** [composition swap chains](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgifactory2-createswapchainforcomposition) support the flip presentation model; use `FLIP_SEQUENTIAL` for this composition path, rather than assuming the HWND-chain `FLIP_DISCARD` recipe is interchangeable. [Flip-model guidance](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/for-best-performance--use-dxgi-flip-model) describes waitable frame latency and hybrid-GPU costs. A direct HWND flip-chain is an alternative for an opaque rectangular overlay; DirectComposition is more flexible for layered visual transforms.

**Alternative, source-backed but not tested:** Windows.UI.Composition with a dispatcher queue and `ICompositorDesktopInterop::CreateDesktopWindowTarget`, as in Microsoft's [WinForms visual-layer sample](https://learn.microsoft.com/en-us/windows/uwp/composition/using-the-visual-layer-with-windows-forms). Its `isTopmost` composition target parameter does not by itself make the HWND globally topmost or keep it on every virtual desktop. The native D3D11/DirectComposition route has fewer managed projection dependencies in this environment.

**Click-through and z-order:** [extended style semantics](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles) distinguish no activation, topmost, and transparent painting. `WS_EX_TRANSPARENT` alone is not a universal cross-process click-through guarantee; the layered/transparent combination is relevant, as described under [layered-window hit testing](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features). Avoid treating `HTTRANSPARENT` alone as a cross-process solution. `WS_EX_NOREDIRECTIONBITMAP` is an optional optimized composition-only HWND path, but its input behavior must be validated separately rather than assuming it inherits layered-window behavior. The tested layered configuration is the safer prototype baseline.

**Inference:** topmost plus a raise immediately before showing should cover the ordinary taskbar, but other topmost windows may reorder themselves. Shell UI in privileged z-order bands, secure desktop/UAC, exclusive fullscreen, and lock screen are not reliably coverable by a normal user-mode window. Do not inject into DWM or use private z-band APIs for this design. Cover the full monitor bounds, not just the work area. For multiple monitors use one overlay per monitor and synchronize the scene.

**Capture feedback:** request `WDA_EXCLUDEFROMCAPTURE` on our own overlay and validate it with the chosen backend. [The supported affinity API](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity) describes this use. This was not tested here. Until verified, freeze whole-monitor capture while the overlay is visible to prevent recursive screenshots. Per-window source capture is less susceptible if the overlay itself is excluded from enumeration. WGC's capture borders/permission policy also need a product decision; this experiment used the default border, did not request borderless consent, and closed sessions on exit.

## Commit and handoff

### Desktop switch

**Verified:** the private `IVirtualDesktopManagerInternal` IID `53F5CA0B-158F-4124-900C-057158060B27` and `IVirtualDesktop` IID `3F07F4BE-B107-441A-AF0F-39D82529072C` worked for this build. Calling raw `SwitchDesktop` to the adjacent desktop and back returned `S_OK`; measured synchronous calls took **67.4 ms** and **18.7 ms** in that single pair. Sparse duplication samples showed the control appearing at its final x-coordinate, without sampled intermediate slide positions. This supports non-animated switching, but the trace misses the interval inside the blocking COM call and is not proof of zero intermediate animation or a seamless reveal.

**Source:** the inspected [MIT implementation](https://github.com/MScholtes/VirtualDesktop/blob/a725cbd3cdb9e977678eeaf034a7cc96d2e74bc6/VirtualDesktop11-24H2.cs) explicitly chooses `SwitchDesktop` when animations are disabled in its process, and `SwitchDesktopWithAnimation` otherwise. It also exposes `WaitForAnimationToComplete`. These are private shell methods with versioned layouts, not the public Win32 `SwitchDesktop(HDESK)` function. Their availability and signatures must be checked by build and `QueryService`; do not guess vtable slots on a newer Insider version.

**Proposal for commit:**

1. Finish our translation at the destination while keeping it fully covered and opaque. Keep processing/rendering independently while the shell call blocks.
2. Ensure the overlay survives a virtual-desktop switch. Test `IVirtualDesktopPinnedApps::PinView` for an owned overlay and undo it on exit, or prepare owned destination overlays in advance. The no-activate popup thumbnail remained visible at the same x-coordinate in our trace, but that does not prove all overlay styles/lifetimes survive. Publicly moving an overlay only after the switch can leave a gap.
3. Call the private non-animated switch once. Confirm the current GUID, expected source cloak changes, and a post-switch destination frame. Do not use a fixed 16 ms sleep as the completion criterion.
4. Reconcile our final snapshot to the newly visible live image, optionally through a very short crossfade if they differ. Remove the visual/hide the overlay only after the underlying desktop is ready. On failure restore the original scene and cancel or report the failure.

**Inference:** holding a frozen final scene for tens of milliseconds is much less conspicuous than exposing Explorer's intermediate state. It is still noticeable for video/cursors or badly stale destinations. [DwmFlush](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmflush) can help wait for our outstanding presentation, but explicitly does not flush every other process's work; it is not a shell-switch-completed fence. Final z-order/focus can change after COM returns, as the restoration discrepancy above demonstrates. Never force the previous desktop's foreground HWND active after committing: that can switch back.

### Animation settings

**Source:** [SystemParametersInfo](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfoa) distinguishes `SPI_SETCLIENTAREAANIMATION` (client-area animation policy), `SPI_SETANIMATION` with `ANIMATIONINFO` (minimize/maximize effects), and other animation settings. They are system-wide preferences, not a documented per-action desktop transition bypass. `UserPreferencesMask` is persisted user preference storage, not a stable compositor timing API.

**Verified limit:** only the getter was tested. Whether disabling client-area animations suppresses native desktop switching or Task View on **26300.9711** was **not verified**, and should not be asserted from the getter or the Settings UI label. Even temporary changes can race Explorer's policy caching or other applications. Use the proven private non-animated desktop method and an overlay covering remaining transitions; no preference writes are needed. A future explicit experiment must save the getter value, use setter flags 0 (no `SPIF_UPDATEINIFILE`), and restore in a `finally`/external recovery path immediately.

### Task View

**Proposal/inference:** enumerate visible app windows on the current desktop, capture or register thumbnails, and interpolate from actual positions to our own grid based on upward displacement. Freeze enumeration/layout during the gesture and handle closing windows safely. Cancellation returns each representation to its original position and removes the overlay without opening Task View.

On commit, invoking Win+Tab beneath an opaque overlay can conceal native opening animation. However, we do not know native Task View's exact grid, desktop-strip layout, hover state, or completion point. Revealing it requires a geometric snap or crossfade. Ordinary topmost windows may also lose to shell Task View's z-order band. No native progress-control/handoff interface was found; a screenshot of Task View from an earlier invocation cannot solve changed windows/layout. Native Task View was not invoked during this investigation. Advertise this as a tracked preview followed by native Task View, not a seamless native animation.

### Show desktop

**Source:** [IShellDispatch4::ToggleDesktop](https://learn.microsoft.com/en-us/windows/win32/shell/ishelldispatch4-toggledesktop) implements the shell's show-desktop toggle. Win+D is the shortcut route. Prefer the shell action over independently minimizing every app, which has different restoration semantics.

**Proposal/inference:** composite wallpaper/desktop icons beneath captured windows and move/shrink/fade the window representations according to downward displacement. The real windows remain untouched until commit. Hold the final desktop scene over the screen, invoke show desktop, verify the underlying desktop state, then remove the overlay. Shell minimize animation can occur underneath; no global disable setting is necessary if coverage survives. Tool-window overlay survival under show desktop and modern icon-layer capture still need testing. Because the shell action toggles, maintain/recheck state so a duplicate commit cannot restore windows accidentally. The experiment did not invoke show desktop or alter other apps' minimize state.

## Expected quality, latency, and remaining risks

**Inference, not measured end-to-end:** at 144 Hz, a GPU-resident scene with latest-state sampling should reasonably target about **1–3 refresh intervals (7–21 ms) from a received raw packet to displayed movement**, plus touchpad scan/USB delivery and scanout position. There is no guaranteed bound under CPU/GPU contention. Cold WGC startup and COM switching can take longer and must be outside the finger-motion path. The control painted at roughly 25 Hz, and our capture polling/PNG work was deliberately slow; those frame counts are not a capture or rendering throughput ceiling.

Expected quality is highest with coherent frozen full-desktop textures and exact pixel alignment. Live per-window capture improves freshness but increases reconstruction errors. Cancellation can be especially clean because no real desktop/window state changes before release. Commit quality remains dependent on destination freshness and shell readiness. A high-speed-camera measurement or present/capture timestamp instrumentation is still needed before claiming GNOME-equivalent latency.

Key risks and required next validation:

- **Native gesture competition:** receiving raw contacts does not establish suppression of Windows' own three/four-finger actions. Double execution ruins the illusion. The input agent must resolve this; no touchpad settings were changed here.
- **App coverage:** test Chromium/Electron, UWP/WinUI, accelerated video, games, elevated windows, minimized windows, suspended apps, and capture-excluded content. Preserve black/placeholder semantics for protected windows. Our successful GDI control cannot represent all these cases.
- **Surface freshness:** some apps stop painting while cloaked/minimized. A successful capture API can return stale frames. Keep last-good images and detect timestamps/content age.
- **Private COM stability:** SDK-supported capture does not make desktop-management methods public. Gate by build/IID, recover from Explorer restarts, fail closed on unsupported layouts, and avoid speculative shell calls.
- **Focus and ordering:** complete the switch beneath the overlay and verify again after cleanup; validate all-desktop overlay presence and show-desktop survival. Shell taskbar icon lists, badges, autohide, and wallpaper can update asynchronously.
- **Visual matching:** taskbar treatment must be deliberate (fixed layer versus sliding with the desktop). Cached monitor images include it; live window reconstruction does not. Desktop icons and translucent backdrops need their own strategy.
- **Power and performance:** warming every app on every desktop can defeat suspension and drain battery. Start with whole-desktop caches, capture adjacent desktops' windows only as needed, release sessions after gestures, and do all steady-state work on GPU.
- **Safety/cleanup:** bounded state machine, timeout and crash recovery; release captures, unregister thumbnails, destroy overlays, undo any owned-window pinning, and preserve the user's initial desktop during research. Scratch PNGs of whole monitors may contain private desktop content; do not distribute them as examples.

## Prior art and licenses

These were researched as data; none were installed or run.

- **[wintouchg](https://github.com/GrieferAtWork/wintouchg/tree/23828cb94b177a59cbe91e8d0dc1f12d27001804): closest permissive precedent.** Four-plus-finger touchscreen gestures drive window animations continuously, with commit/cancel. DWM thumbnails represent windows until real operations happen at gesture completion. This proves the technique has prior implementation, not Windows 11 PTP desktop-switch compatibility. Its [license](https://github.com/GrieferAtWork/wintouchg/blob/23828cb94b177a59cbe91e8d0dc1f12d27001804/LICENSE) is zlib-style with an explicit documentation acknowledgment requirement; retain that additional wording rather than calling it plain MIT.
- **[CKFlip3D](https://github.com/CYMERKAROL/CKFlip3D/tree/ef9abff80c31b97357f4be6d41ce2fe95084c3e5): close modern rendering precedent.** Describes a DirectComposition/D3D11 overlay, WGC with thumbnail/PrintWindow fallbacks, Precision Touchpad input, and continuous swipe/drag traversal. It prepares its initial scene before showing the overlay. It replaces window switching, not the exact desktop/Task View gestures here. [PolyForm Noncommercial 1.0.0](https://github.com/CYMERKAROL/CKFlip3D/blob/ef9abff80c31b97357f4be6d41ce2fe95084c3e5/LICENSE.md): source-available, not a permissive open-source dependency. Capture/minimized-window claims are upstream claims, not verified here.
- **[GestureSign](https://github.com/TransposonY/GestureSign): related input/action precedent, GPL-2.0.** Recognizes drawn gestures and invokes commands; it is not evidence of a 1:1 desktop compositor. Useful for gesture UX/input research rather than a drop-in overlay engine.
- **[Switchie](https://github.com/darkguy2008/Switchie): related virtual-desktop pager, MIT.** Visual desktop/window organization and shell desktop management; upstream states Windows 10 LTSC testing and compatibility limitations. Not proof of tracked Windows 11 gestures.
- **[VirtualDesktop](https://github.com/MScholtes/VirtualDesktop), MIT, and [VirtualDesktopAccessor](https://github.com/Ciantic/VirtualDesktopAccessor), MIT:** useful private-COM and desktop notification/pinning references, not overlay renderers. The former's 24H2 source was exercised here. The latter currently documents newer Windows 11 targets, but was not loaded or tested on this machine.

Recommended first implementation: cached whole-desktop horizontal transitions with the tested DirectComposition layered window; separate STA non-animated desktop commits; explicit cancellation and readiness checks. Add live per-window reconstruction and show-desktop animation after pixel/focus correctness, and make a separate product decision about approximating versus replacing Task View.
