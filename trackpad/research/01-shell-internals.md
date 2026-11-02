# Shell-side investigation: interactive touchpad animations

Investigation date: 2026-10-29. Machine: ASUS K3605ZU, x64, Windows **26300.9711 / 26H2**.

## Verdict

**Windows contains a real displacement-driven virtual-desktop animation on this machine. However, I did not find an interface that the existing out-of-process C# gesture application can use to set its progress.** The ordinary undocumented desktop-manager COM API can request a complete animated switch; that is a different operation from driving the touchpad animation.

The strongest practical finding is that the shell's actual desktop-gesture and multitasking-gesture services exist, but requesting their gesture interfaces from a separate process fails with **`REGDB_E_IIDNOTREG` (`0x80040155`)**. Requesting `IUnknown` on the desktop-gesture service succeeds. Merely declaring those interfaces in C# does not supply the missing cross-process marshaller.

| Target | Native machinery present? | Existing separate C# process can drive the real animation interactively? | Plausible route |
|---|---|---|---|
| Four-finger desktop left/right | **Verified:** displacement updates, velocity history, finish and cancel paths | **No working route established.** Manager API has no progress argument; gesture-interface marshalling fails | Prefer native PTP desktop gestures. A custom controller would likely need an Explorer-side bridge to the existing gesture handler; untested and outside this investigation's permissions |
| Three-finger up / Task View | **Verified:** multitasking gesture state machine and Task View-related machinery | **No working route established.** Finding `StartSwipe`/`ContinueSwipe` does not establish that the Task View scene itself can be scrubbed | Native gesture behavior first; additional reverse engineering of the Task View state machine needed before promising GNOME-style overview scrubbing |
| Three-finger down / show desktop | **Verified:** input-operation-driven minimize visuals plus legacy three-finger keyframe animations | **No working route established.** No external progress setter found; same gesture-interface marshalling barrier | Native minimize gesture first. A shell bridge would also have to preserve minimize/restore state and animation completion semantics |

These are feasibility conclusions from the evidence below, **not a proof that no undiscovered private protocol exists**. No hook, injected bridge, synthetic gesture, or physical swipe was tested.

## Evidence labels and scope

- **[M-static]** Read directly from the installed binary, its matching Microsoft public PDB, or disassembly of that binary.
- **[M-runtime]** A read-only query executed on this machine: registry reads, file/module inventory, COM activation/`QueryService`, desktop count/current-desktop getters.
- **[Source]** Public upstream implementation or Microsoft documentation, linked where used.
- **[Inference]** Interpretation or proposed implementation, not demonstrated here.

No REA tools were exposed in `ALL_TOOLS`. Used the existing MSYS `strings.exe`/`objdump.exe`, PowerShell, and the installed .NET Framework 64-bit C# compiler. A small helper parses PE CodeView and MSF/PDB public-symbol records and reads **on-disk** vtables. It does not inspect another process's memory. Matching public PDBs were downloaded from Microsoft's symbol server into `scratch-01/`; downloaded source files were treated as data.

No settings or registry writes, installation, shell restart, injection, debugger attachment, desktop-switch call, notification subscription, or window-state changes were performed. COM objects/proxies were released. Concurrent desktop changes by the user/other work are possible; this investigation itself never changed desktops.

## 1. Identify the actual shell, not just the OS build number

**[M-runtime]** Read `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`: `CurrentBuild=26300`, `UBR=9711`, `DisplayVersion=26H2`, `BuildLabEx=26100.1.amd64fre.ge_release.240331-1435`.

Installed file versions:

| Component | File version |
|---|---|
| `C:\Windows\System32\twinui.pcshell.dll` | `10.0.26100.9278` |
| `C:\Windows\System32\twinui.dll` | `10.0.26100.9549` |
| `C:\Windows\explorer.exe` | `10.0.26100.8117` |
| `C:\Windows\System32\InputHost.dll` | `10.0.26100.9711` |
| `C:\Windows\System32\Windows.UI.dll` | `10.0.26100.7840` |
| `C:\Windows\System32\Windows.UI.Xaml.dll` | `10.0.26100.8972` |
| `C:\Windows\System32\Windows.UI.Shell.dll` | `10.0.26100.8115` |
| `C:\Windows\System32\dcomp.dll` | `10.0.26100.9549` |
| `C:\Windows\System32\dwm.exe`, `udwm.dll` | `10.0.26100.1` |
| `C:\Windows\SystemApps\ShellExperienceHost_cw5n1h2txyewy\ShellExperienceHost.exe` | `10.0.26100.9549` |

**[M-runtime]** Explorer module enumeration confirmed the installed `twinui.pcshell.dll` is loaded in running Explorer processes; one also had `twinui.dll` loaded. See `scratch-01/explorer-modules.txt`. Module enumeration is not debugger attachment.

**[M-static]** PE CodeView symbol identities (GUID without hyphens followed by hexadecimal age):

| Image | PDB identity |
|---|---|
| `twinui.pcshell.dll` | `twinui.pcshell.pdb / 9507F3DAD47F5F026569FC50273C40D7 / age 1` |
| `twinui.dll` | `twinui.pdb / FDDD5C12E412A64AD5FB8AA23B502987 / age 1` |
| `InputHost.dll` | `InputHost.pdb / C86077DEBCC0C420D1F5A68FECD57069 / age 1` |
| `explorer.exe` | `explorer.pdb / 1D666F27BA2F85885888D3E2B94B0FD8 / age 1` |

SHA-256:

```text
twinui.pcshell.dll  8BA440EDED430A2417692A98B34695A12AD0EE000333DFED07E0C2723E17111C
twinui.dll         B2262A842ADF22F78BF596DCF6815F1B3AEC282469995D939901E93BCF943D65
```

**Do not choose offsets or an interface layout solely by `26300`.** These are serviced 26100-family shell components. All RVAs below refer to these particular files; an RVA is not a runtime address. Preferred image base used by the disassembler is `0x180000000`; ASLR changes the loaded base.

## 2. Desktop-manager COM: exact local layout

### Service entry and IIDs

**[M-runtime]** Activate `ImmersiveShell` CLSID **`C2F03A33-21F5-47FA-B4BB-156362A2F239`**, obtain native `IServiceProvider` IID **`6D5140C1-7436-11CE-8034-00AA006009FA`**, and call `QueryService` at slot 3.

**[M-static/runtime]** Desktop-manager service SID is **`C5E0CDCA-7B6E-41B2-9FC4-D93975CC467B`**. Its local PDB differentiates these interfaces:

| PDB interface name | IID | External `QueryService` result |
|---|---|---|
| `IVirtualDesktopManagerInternal` | `4970BA3D-FD4E-4647-BEA3-D89076EF4B9C` | `S_OK` |
| `IVirtualDesktopManagerInternal2` | `53F5CA0B-158F-4124-900C-057158060B27` | `S_OK` |
| `IVirtualDesktopManagerPrivate` | `95BB3D6C-9EBA-40CC-AF4D-1A3255717D9D` | `0x80040155` |

**Naming trap:** public projects commonly call IID `53F5CA0B-...` simply `IVirtualDesktopManagerInternal`. On this binary, the public-symbol class hierarchy calls it **`Internal2`**. `AsIID` at RVA `001041C4`, GUIDs at `007C4A50/60/70`, and the separate vtables establish the mapping. That function also returns a shared chained implementation pointer for the base/extended IIDs; this does not make every method beyond an interface's declared length safe to call through that IID's external proxy.

**[M-runtime]** On IID `53F5CA0B-...`, slot 3 `GetCount` returned `S_OK`, count **3**; slot 6 `GetCurrentDesktop` returned `S_OK`. The resulting desktop's slot 4 `GetID` also returned `S_OK`. Older candidate manager IIDs `A3175F2D-239C-4BD2-8AA0-EEBA8B0B138E`, `B2F925B9-5A0F-4D2E-9F4D-2B1507593C10`, and `F31574D6-B682-4CDC-BD56-1827860ABEC6` returned `E_NOINTERFACE` (`80004002`). Logs: `com-probe.txt`, `com-private-probe.txt`.

### Vtable slots

**[M-static]** Zero-based slots, including `IUnknown` at 0–2. Physical vtable RVAs: base `00793F58`; extended `00793FF8`; private chained implementation `00794118`. Evidence: `vtable-manager-internal.txt`, `vtable-manager-internal2.txt`, `vtable-manager-private.txt`.

| Slot | Method | Important arguments after `this` |
|---:|---|---|
| 3 | `GetCount` | `UINT*` |
| 4 | `MoveViewToDesktop` | `IApplicationView*, IVirtualDesktop*` |
| 5 | `CanViewMoveDesktops` | `IApplicationView*, BOOL*` |
| 6 | `GetCurrentDesktop` | `IVirtualDesktop**` |
| 7 | `GetDesktops` | `IObjectArray**` |
| 8 | `GetAdjacentDesktop` | `IVirtualDesktop*, UINT direction, IVirtualDesktop**` |
| 9 | `SwitchDesktop` | `IVirtualDesktop*` |
| 10 | `SwitchDesktopAndMoveForegroundView` | `IVirtualDesktop*` |
| 11 | `CreateDesktopW` | `IVirtualDesktop**` |
| 12 | `MoveDesktop` | `IVirtualDesktop*, UINT index` |
| 13 | `RemoveDesktop` | `IVirtualDesktop*, IVirtualDesktop* fallback` |
| 14 | `FindDesktop` | `GUID const&, IVirtualDesktop**` |
| 15 | `GetDesktopSwitchIncludeExcludeViews` | `IVirtualDesktop*, IObjectArray**, IObjectArray**` |
| 16 | `SetDesktopName` | `IVirtualDesktop*, HSTRING` |
| 17 | `SetDesktopWallpaper` | `IVirtualDesktop*, HSTRING` |
| 18 | `UpdateWallpaperPathForAllDesktops` | `HSTRING` |
| 19 | `CopyDesktopState` | `IApplicationView*, IApplicationView*` |
| 20 | `CreateRemoteDesktop` — extended interface starts adding methods here | `HSTRING, IVirtualDesktop**` |
| 21 | `SwitchRemoteDesktop` | `IVirtualDesktop*, VirtualDesktopSwitchType` |
| **22** | **`SwitchDesktopWithAnimation`** | **`IVirtualDesktop*` only** |
| 23 | `GetLastActiveDesktop` | `IVirtualDesktop**` |
| 24 | `WaitForAnimationToComplete` | no arguments |

These methods return native `HRESULT`; C# COM declarations may translate output parameters to return values. Only the stated getters were executed, not slots 9/22 or other mutators.

**[M-static]** `CVirtualDesktopManager::SwitchDesktopWithAnimation` is RVA `005918F0`; its extended-interface adjustment thunk is `00591A30`. Disassembly shows fallback calls to ordinary `SwitchDesktop` and a call to `SwitchContextForVDAnimation` at `00590BE0`. It receives a target desktop, **no displacement/progress, cancellation token, or gesture-session object**. `WaitForAnimationToComplete` waits; it does not control animation time. See `dis-switch-with-animation.txt`.

The larger private implementation adds initialization, recovery, visibility and internal-switch functions, including `SwitchDesktop2(desktop, switchType)` at slot 42. Its inspected vtable contains no begin/update-progress/end desktop gesture sequence. The separate gesture handler below owns that functionality.

### Notification service and callback

**[M-runtime]** Notification service SID **`A501FDEC-4A09-464C-AE4E-1B9C21B84918`**, IID **`0CD45E71-D927-4F15-8B0A-8FEF525337BF`**, is obtainable (`S_OK`). Its known subscription ABI is slot 3 `Register(IVirtualDesktopNotification*, DWORD* cookie)`, slot 4 `Unregister(DWORD cookie)`; **no subscription was made**. This service method order is corroborated by [VirtualDesktopAccessor's declarations](https://github.com/Ciantic/VirtualDesktopAccessor/blob/rust/src/interfaces.rs).

**[M-static]** Callback IID **`B9E5E94D-233E-49AB-AF5C-2B4541C3AADE`**, verified by local QI disassembly at `001C7F50` and GUID data at `007C27E0`. Callback vtable at `00755F60`:

| Slot | Callback |
|---:|---|
| 3 | `VirtualDesktopCreated(desktop)` |
| 4 | `VirtualDesktopDestroyBegin(desktop, fallback)` |
| 5 | `VirtualDesktopDestroyFailed(desktop, fallback)` |
| 6 | `VirtualDesktopDestroyed(desktop, fallback)` |
| 7 | `VirtualDesktopMoved(desktop, UINT oldIndex, UINT newIndex)` |
| 8 | `VirtualDesktopNameChanged(desktop, HSTRING)` |
| 9 | `ViewVirtualDesktopChanged(view)` |
| 10 | `CurrentVirtualDesktopChanged(oldDesktop, newDesktop)` |
| 11 | `VirtualDesktopWallpaperChanged(desktop, HSTRING)` |
| 12 | **`VirtualDesktopSwitched(desktop, VirtualDesktopSwitchType)`** |
| 13 | `RemoteVirtualDesktopConnected(desktop)` |

Evidence: `vtable-notification.txt`, `dis-notification-qi.txt`. **No progress callback is present.** Notifications describe lifecycle/state changes, not an animation controller. Local slot 12 has an additional enum argument omitted by some public declarations; do not copy a web ABI blindly.

## 3. The real desktop swipe implementation

### Finger displacement reaches native visuals

**[M-static]** `twinui.pcshell.dll` embeds source paths ending in `virtualdesktopgesturehandler.cpp`, `virtualdesktopswitchanimator.cpp`, `virtualdesktophotkeyhandler.cpp`, and `multitaskingviewgesturehandler.cpp`. Matching symbols and disassembly establish:

```text
VirtualDesktopGestureHandler::StartSwipe(int,int,UINT,TOUCHPAD_GESTURE_CONTEXT)
  RVA 0059D7A0 -> InitializeContext(UINT), RVA 0059B56C

VirtualDesktopGestureHandler::ContinueSwipe(int,int,int,int,UINT)
  RVA 00226180 -> VirtualDesktopGestureContext::UpdateOffset(int,int)
                  RVA 002646C8
                -> VirtualDesktopGestureWindow::UpdateOffset(float)
                  RVA 002B03A0, called once per relevant gesture window

VirtualDesktopGestureHandler::FinishSwipe(...)
  RVA 00599AF0 -> VirtualDesktopGestureContext::Commit(handler,int,bool)
                  RVA 00597D88
                -> VirtualDesktopGestureWindow::Commit(handler,UINT,float,bool)
                  RVA 00598524

VirtualDesktopGestureHandler::CancelSwipe()
  RVA 00597BD0 -> Cleanup(0), RVA 00597D00
```

In `ContinueSwipe`, the call to context `UpdateOffset` is at image address `1802261C1`. In context `UpdateOffset`, displacement is accumulated, velocity history updated, adjacent previews created/removed, and integer displacement divided by a stored dimension at `18026489D`. It applies edge resistance with constant `0.8`, then calls each window's `UpdateOffset(float)` at `1802648F2`.

Window `UpdateOffset` multiplies the float by a stored rectangle width, converts to an integer position and invokes its thumbnail-related object's rectangle update via vtable slot 3 (`1802B03D0–1802B0404`). This is direct evidence of a **finger-displacement-driven preview**, not just a gesture threshold launching a fixed-duration switch. The mapping includes normalization/resistance; it is not a promise of literal physical millimetre-to-pixel equality.

Context `Commit` uses velocity history and position to select a terminal state before calling window `Commit`; release is a settle phase. `CancelSwipe` directly cleans up the gesture. **I did not fully reconstruct the release threshold formula or demonstrate that simply lifting at exactly halfway always cancels.** A velocity-dependent finish can commit before a particular distance, unlike an explicit cancellation call.

Evidence: `dis-start-swipe.txt`, `dis-continue-swipe.txt`, `dis-context-update.txt`, `dis-window-update.txt`, `dis-finish-swipe.txt`, `dis-context-commit.txt`, `dis-cancel-swipe.txt`.

### What interface owns it?

**[M-static]** Desktop gesture service SID **`E487360E-F644-4672-8037-97A6F2DC1BC6`**, stored at RVA `007E6AD8`. The object implements:

| Interface | IID | Relevant vtable |
|---|---|---|
| `ITouchpadGesture` | `2EFBB7B2-92AB-4D26-9E07-39C400CA8603` | RVA `007946F8` |
| `ITouchSwipe` — separate touch/swipe ABI | `9A6CBE27-4D5E-49FE-ADAA-8738C2848795` | RVA `00794748` |
| `IVirtualDesktopGestureHandlerPrivate` | `78D4BBF6-0C6E-4A17-AE0C-AFF5DC4295E9` | chained vtable RVA `007946D0` |
| `IVirtualDesktopGestureHandler` | `9CDD8553-04F2-4BE1-8210-4911258C7D0E` | same chained implementation |

QI evidence: `AsIID` RVA `001CC6E4`, `ChainInterfaces<Private,Base>::CanCastTo` RVA `001CC7C0`, and GUID data `007C1BC0`, `007C77E0`, `007C77F0`, `007C7800`. See `dis-gesture-qi.txt`, `guids.txt`, `service-guids.txt`.

`ITouchpadGesture` slots are 0–2 `IUnknown`, **3 `StartSwipe`, 4 `ContinueSwipe`, 5 `FinishSwipe`, 6 `CancelSwipe`, 7 `CanBePreempted`, 8 `CanPreemptOtherGestures`**. The desktop-private interface instead adds slot 3 `DelayInitialize` with shell dependency objects and slot 4 `Cleanup(UINT)`. It is important to query the touchpad interface, not mistake the private initialization interface for the progress-control ABI. `vtable-gesture-private.txt` dumps past its five entries into the adjacent touchpad vtable; its later printed slots are **not** additional private-interface methods.

**[M-runtime]** With the correct service SID, requesting either desktop-handler IID or `ITouchpadGesture` returned **`80040155`**. Requesting `IUnknown` returned **`S_OK`**. See `com-gesture-sid-probe.txt`. Initial tests using an interface IID as the SID returned `E_NOTIMPL`; those were superseded by the correct-SID tests.

**[Inference]** These errors strongly support an in-process interface with no usable registered external marshalling path, rather than a missing service. They do not prove a custom proxy could never be written. No IDL, supported external proxy, or ready-made bridge was established, and the handlers need real pointer/source identity and shell initialization, not merely three floating-point values.

### Does Windows itself track four fingers on this exact machine?

**Verified statically:** this installed shell has the tracking implementation above. **Corroborated by source:** [Smooth Virtual Desktop Switching](https://github.com/ramensoftware/windhawk-mods/blob/main/mods/smooth-virtual-desktop-switching.wh.cpp) distinguishes native dragging from the touchpad release animation and hooks the same `VirtualDesktopGestureWindow::Commit` method. Its adjustments leave displacement updates alone.

**Not physically verified:** no human swipe/video observation, tracing, or synthetic input experiment was performed. I therefore cannot claim that the current settings, feature configuration, hardware routing and desktop composition actually exercise this path on every swipe. Read-only touchpad settings showed three-finger overrides (`ThreeFingerUp=16`, `Down=17`, `Left=10`, `Right=12`, `ThreeFingerSlideEnabled=65535`); no interpretation of those numeric actions was established here. Check the visible native mappings and any existing shortcut remapping before concluding Windows lacks tracking.

## 4. Task View and show-desktop paths

**[M-static]** `MultitaskingViewGestureHandler` implements the same `ITouchpadGesture` ABI. Its service SID is **`4658AF09-F6DD-4DE4-B3C1-BAD2F41304A9`** (RVA `007C1AF0`), primary handler IID **`DE8D7855-1EDF-411B-99BF-0226775C0B72`** (GUID at `007C1BB0`; QI at `0010A698`). Touchpad vtable RVA `007573E8`:

| Slot | Method | Implementation RVA |
|---:|---|---|
| 3 | `StartSwipe(int,int,UINT,TOUCHPAD_GESTURE_CONTEXT)` | `0016BEC0` |
| 4 | `ContinueSwipe(int,int,int,int,UINT)` | `0016C2C0` |
| 5 | `FinishSwipe(int,int,int,int,UINT)` | `0016BDE0` |
| 6 | `CancelSwipe()` | `004D5EE0` |
| 7 | `CanBePreempted()` | `004D5EA0` |
| 8 | `CanPreemptOtherGestures()` | `002B98B0` |

**[M-runtime]** Correct service SID plus primary IID or touchpad IID both returned **`80040155`** (`com-multitasking-sid-probe.txt`).

**[M-static]** Start/continue/finish call `ProcessTouchpadGestureInput` at `0016C034`. Continue accumulates integer X/Y deltas. The state machine consults multitasking visibility, touchpad settings, device dimensions, elapsed time and gesture direction, then calls `DetermineTouchpadGestureAction` (`002C62E4`) and `ActivateGestureAction` (`0016C4C4`). Initialization obtains pointer/device information using `GetPointerInfo` and `GetPointerDeviceRects`. Supplying arbitrary raw-HID deltas to this ABI without a corresponding pointer/source context is not a demonstrated solution.

There are **both input-driven and keyframe paths**, so the existence of a method named `ContinueSwipe` must not be interpreted as proof of reversible scrubbing of every shell view:

- `TryStartGlobalOperation` (`002AECAC`) calls internal `InputGestureManager::TryStartGlobalOperationAsync(UINT, UINT64)`; it references `Feature_InputDrivenGTGAnimations` and `Feature_3FingerAnimationSwitchApps`. The operation is tied to input source/operation state. Feature enablement and branch selection were not measured.
- `InputGestureOperationStarted` / `Ended` are at `004D7950` / `004D78F0`; `InitiateSwipeDownAsync` is at `004D7860`.
- `CAppThumbnailWindow::StartMinimizeGestureOperation` (`004E344C`) reads the operation's `AnimationData` and calls `InputAnimationTarget::GetInputAnimationObject`; it also obtains taskbar height and window minimize rectangles. This is concrete evidence of an input-connected minimize-animation route. It is not an external setter for Explorer's animation.
- `CAppThumbnailWindow::PlayThreeFingerDownKeyframeAnimation` / `PlayThreeFingerUpKeyframeAnimation` are at `004E1F10` / `004E2300`. Additional helpers include expression-animation creation and drag/flick scale-factor setup.
- `MultitaskingViewGestureHandlerHelpers::RaiseDesktop` (`002AFCC0`) finds a shell window and posts message **`0x579`, `wParam=3`, `lParam=0`**. That is a discrete shell action, not a progress protocol. It was **not sent**.

Evidence: `dis-multitasking-touchpad.txt`, `dis-try-global-operation.txt`, `dis-minimize-operation.txt`, `dis-raise-desktop.txt`, and the PDB symbol listing.

**Task View conclusion [Inference]:** the shell manages gesture state and view activation internally, but this investigation did not establish a float-progress controller for the complete Task View scene or decode all asynchronous action branches. Claiming that calling the handler automatically produces GNOME's reversible overview would overstate the findings. `Win+Tab` is a discrete activation, not access to that state machine.

**Show-desktop conclusion [Inference]:** real input-connected minimize visuals exist, but an external client would need access to the owning gesture operation/visuals and correct completion/restore bookkeeping. `Win+D`, minimizing individual HWNDs, and the posted desktop action do not expose a shared animation progress value. No non-injected external controller was demonstrated.

## 5. Component responsibilities and limits

**[M-static]** Matching `InputHost.dll` symbols include `TouchpadGesturesController::OnPointerPressed/Moved/Released` (`000EB220/000EB1A0/000EB2A0`), `ShellGesturesController`, `InputGestureManager`, `InputGestureOperation::get_AnimationData` (`000CA1F0`), and `InputGestureOperation::GetInputAnimationObject` (`000C5580`). `CreateReaderForGestureAnimation` is `000BD24C`. These establish the input-to-composition infrastructure, but the complete HID/kernel-to-`ITouchpadGesture` dispatch chain was **not reconstructed**.

**[M-static]** Explorer's PDB includes `CTray::_RegisterTouchpadActions` (`0006EBA8`) and `ShellGesturesController::CreateForInputSite` (`0009E51C`). `twinui.dll` supplies `CSwitchThumbnailFactory`, `CSwitchThumbnailDeviceManager`, `CDCompAbstractThumbnail`, instantaneous transitions, and UIAnimation/DComp plumbing. This is consistent with Explorer hosting shell policy/gesture objects and DWM composing the resulting visuals.

**[M-static, limited inspection]** `Windows.UI.dll` contains a `PhysicalGestureRecognizer` source-path string; `dcomp.dll` contains gesture-targeting telemetry strings. `dwm.exe`, `udwm.dll`, and `ShellExperienceHost.exe` received version/string inventory, not full symbol/disassembly analysis. No relevant external progress API was established in them. Absence of a string is not evidence that a capability does not exist. The verified gesture implementations are in `twinui.pcshell.dll`; there is no demonstrated reason to inject into DWM for this Windows 11 path.

**[Source]** Microsoft's [Precision Touchpad Input documentation](https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/precision-touchpad-portal) distinguishes input reception/recognition from shell global gestures. Its new `TouchpadGesturesController` allows foreground applications to receive global gestures; it does not document taking control of Explorer's desktop/Task View animations. Likewise, creating an application's own `InteractionTracker` does not attach it to another process's composition tree.

## 6. Prior art and licenses

These sources were inspected, **not installed or executed**. Local snapshots are in `scratch-01/`; public sources may change after the investigation date.

| Project / source | License | Approach and relevance |
|---|---|---|
| [MScholtes/VirtualDesktop, 24H2 implementation](https://github.com/MScholtes/VirtualDesktop/blob/master/VirtualDesktop11-24H2.cs) | [MIT](https://github.com/MScholtes/VirtualDesktop/blob/master/LICENSE) | External COM desktop commands, including target-only `SwitchDesktopWithAnimation` and completion waiting. Useful ABI starting point; no gesture-progress API |
| [Ciantic/VirtualDesktopAccessor](https://github.com/Ciantic/VirtualDesktopAccessor), [interfaces](https://github.com/Ciantic/VirtualDesktopAccessor/blob/rust/src/interfaces.rs) | [MIT](https://github.com/Ciantic/VirtualDesktopAccessor/blob/rust/LICENSE.txt) | COM-backed DLL usable from automation tools, desktop management and notification callbacks. No demonstrated native progress controller; local notification signatures require care |
| [Smooth Virtual Desktop Switching 1.0.0](https://github.com/ramensoftware/windhawk-mods/blob/main/mods/smooth-virtual-desktop-switching.wh.cpp) | MIT, explicit file header | Explorer injection and private-symbol hooks. Hooks gesture/hotkey `Commit` and `CDCompAbstractThumbnail::_AddTransition`; adjusts release duration/curve. Does not turn external finger samples into a gesture session |
| [Virtual Desktop Swipe Taskbar Fix 1.0.5](https://github.com/ramensoftware/windhawk-mods/blob/main/mods/virtual-desktop-swipe-taskbar-fix.wh.cpp) | MIT, explicit file header | Explorer injection; obtains gesture windows and thumbnails through private symbols to repair incoming-taskbar previews. Useful evidence that native swipe previews precede committed desktop state |
| [Disable Virtual Desktop Transition Animation 1.1](https://github.com/ramensoftware/windhawk-mods/blob/main/mods/disable-virtual-desktop-transition.wh.cpp) | MIT under [repository default for unspecified licenses](https://github.com/ramensoftware/windhawk-mods/blob/main/README.md) | On Windows 11 hooks Explorer's desktop-switch implementation to use the nonanimated path; older Windows 10 branch hooks DWM/`CVirtualDesktopSwitch`. Suppression, not interactive driving |

No inspected project provides an out-of-process, ready-to-use `BeginDesktopGesture/SetProgress/EndGesture` API. Windhawk's working examples use process-local objects/private symbols; none establishes that equivalent calls can be made through a normal external COM proxy.

## 7. Recommended implementation direction and remaining risks

1. **First verify the native experience manually with Windows' native desktop-switch gesture selected.** Avoid translating that gesture into `Ctrl+Win+Arrow`; the shortcut loses the displacement path verified above. The same check should establish the actual native up/down behavior and whether it meets the user's cancellation expectations. No settings were changed in this investigation.
2. **Do not add `SwitchDesktopWithAnimation` to the C# app expecting tracking.** It can improve command-triggered switching, but its ABI has no progress control. Keep command execution until commitment if that fallback is retained.
3. **If native gestures do not meet the requirement, separate two possible projects.** A process-local Explorer bridge could receive begin/delta/end/cancel messages from the existing reader and invoke the native handler. This is a research proposal requiring explicit authorization for injection, correct UI-thread dispatch, initialization, pointer/source handling and arbitration with Windows' own handler. Task View needs further investigation even after gaining process-local access. An application-owned DirectComposition/thumbnail overlay could instead provide cancellable visual feedback and perform the real shell action only on commit; that is a replacement preview, not driving the actual Windows animation.
4. **Keep deployment conditional on exact ABI identification.** Resolve symbols or validate module/PDB identities and interface availability; fail closed on a mismatch. Never reuse the RVAs above after a servicing update without validation.

Material risks: Insider and cumulative-update ABI churn; absent COM marshallers; unknown `TOUCHPAD_GESTURE_CONTEXT` layout/source requirements; UI-thread/apartment and lifetime rules; races when both Windows and the Raw Input app consume the same gesture; multi-monitor and wallpaper/taskbar previews; remote sessions; feature-controlled animation branches; velocity-sensitive commit decisions; losing minimize/restore state; and Explorer crashes if private calls are wrong. An injected bridge would enlarge the failure surface of the shell; it was neither implemented nor proven safe here.

## Reproduction and evidence files

All investigation-created auxiliary files are under `trackpad/research/scratch-01/`. No file in `trackpad/gestures/` was edited.

- `Symbols.cs` / `Symbols.exe`: PE CodeView identification, matching PDB download/parsing, on-disk vtable reading and GUID extraction. `*.pdb.symbols.txt` preserves all public symbol records, including aliases.
- `ComProbe.cs` / `ComProbe.exe`: STA COM service queries and the three read-only desktop getters. `com-*.txt` contains results. QueryService arguments are `SID=IID`; no switch function is called.
- `Disassemble.ps1` and `dis-*.txt`: bounded disassembly with symbol annotations.
- `vtable-*.txt`, `guids.txt`, `service-guids.txt`, `component-strings.txt`, `explorer-modules.txt`: local evidence.
- `smooth.cpp`, `disable-transition.cpp`, `swipe-taskbar-fix.cpp`, `VirtualDesktop24H2.cs`, `vda-interfaces.rs`, license/readme snapshots: upstream source references, not code executed against the shell.

Example safe reproduction from the repository root:

```powershell
.\trackpad\research\scratch-01\ComProbe.exe
.\trackpad\research\scratch-01\Symbols.exe C:\Windows\System32\twinui.pcshell.dll .\trackpad\research\scratch-01 793FF8 25
```

Symbol-analysis limitation: the helper's address-to-name lookup may select one of several identical-code-folded aliases when annotating a call; some generic WinRT wrapper annotations are therefore unrelated names with the same implementation address. The full symbol listing retains aliases. Conclusions above use class-specific symbols, QI comparisons, physical vtable boundaries and actual instructions; incidental generic-wrapper aliases must not be treated as additional code-path evidence. Public symbols are not full private types/IDL, and this investigation is not a complete decompilation.
