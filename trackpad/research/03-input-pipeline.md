# Windows Precision Touchpad input pipeline

Research date: 2026-10-29. Machine: ASUS K3605ZU, ELAN ASUE1306, x64. Locally read OS version: **10.0.26300.9711**, DisplayVersion **26H2**. This is an Insider machine; findings about new APIs must not be generalized to all Windows 11 installations.

Evidence labels throughout:

- **[M]** verified by a read-only query, static binary inspection, or explicitly described safe experiment on this machine.
- **[D]** documented by Microsoft or a project's own source/documentation; not an end-to-end machine test.
- **[I]** inference or proposed design.
- **[U]** unresolved; do not build a dependency on it yet.

## Main findings and verdicts

| Question | Verdict |
| --- | --- |
| Selectively suppress native gestures? | **Yes at the settings/foreground-app level; no general background Raw Input veto.** Three/four-finger shell assignments can be disabled independently of cursor, pan and zoom. The new foreground controller offers selective interception without changing settings. Arbitrating arbitrary physical contact sequences system-wide still needs a suitable driver or another proven interception mechanism. [D/I] |
| Apply settings live? | Settings does this. A private Explorer COM cache is a credible route: its interface and relevant getters work here. **The notification/write/apply path was not tested**, and there is no verified WNF/broadcast recipe in this report. [M/U] |
| Inject native tracked touchpad gestures from user mode? | **Promising and materially better than the old APIs suggest:** `CreateSyntheticPointerDevice2` creates PT_TOUCHPAD devices successfully here, both normal and gesture-only. Microsoft documents feeding them contact frames. **No contacts were injected**, so native shell animation, tracking, cancellation and settings interactions remain unverified. [M/D/U] |
| ASUS contact/suppression hook? | **A real private control interface exists.** Static service code demonstrates specific IOCTLs and vendor-event callbacks. No supported raw-contact stream, per-contact suppression command or generic gesture hook was established. [M/U] |

The old conclusion “Windows has no user-mode PT_TOUCHPAD injection” is incorrect for this build. Conversely, creating an injector does not solve suppression of the physical pad or expose arbitrary control of shell animations.

## Pipeline and evidence

```text
ELAN ASUE1306 sensor/firmware
  | I2C HID input reports, contact positions/state [supplied machine facts]
  v
ACPI\ASUE1306\4&2BA988C7&0
  hidi2c; UpperFilters = mshidkmdf                         [M1]
  HIDCLASS / HID framework bridge enumerates TLCs         [D1]
  |
  +-- Col01 -> mouhid -> mouse compatibility path         [M1]
  |
  +-- Col02 -> AsusPTPDrv (AsusPTPFilter.sys)              [M1,M2]
  |             | HID parser + read-completion processing [M3]
  |             v
  |    Windows Raw Input / PTP classification pipeline
  |       |                         |
  |       |                         +--> WM_INPUT copy to registered app
  |       |                              (0x0D/0x05 INPUTSINK; observer)
  |       v
  |    win32kbase / win32kfull PTP processing             [M4]
  |       |
  |       +-- mousing -> generated mouse input           [D2]
  |       |
  |       +-- 2-finger manipulation -> hit-tested app     [D2]
  |       |    DirectManipulation / InteractionTracker,
  |       |    or touchpad-capable WM_POINTER consumer;
  |       |    fallback ninput.dll -> wheel messages
  |       |
  |       +-- global >=3-finger gestures/actions          [D3]
  |            foreground TouchpadGesturesController?
  |            yes -> its pointer/action events
  |            no  -> default shell handler
  |                       |
  |             InputHost shellgestures client plumbing  [M5]
  |                       |
  |             Explorer/twinui.pcshell experience        [M6,I]
  |             -> task switching / desktop / other action
  |
  +-- Col03 -> MTConfig (configuration, not interception) [M1]

Additional ASUS branch:
  AsusPTPFilter.sys -> private control devices / IOCTLs
      -> AsusPTPService.exe (LocalSystem) -> vendor actions [M2,M3,M8]

Synthetic branch available on this build:
  app -> CreateSyntheticPointerDevice2(PT_TOUCHPAD)
      -> InjectSyntheticPointerInput(contact frames)
      -> Windows touchpad processing/routing                [M7,D4]
```

The diagram is a **logical data-flow**, not a captured kernel stack trace. In particular, Raw Input observation is not a filter inserted before recognition. Exact private calls between the kernel processor, input services and shell were not traced. The location of every recognizer stage cannot be proved from strings alone.

### Local observations supporting the diagram

**M1 — PnP topology.** `Get-PnpDevice` and `Get-PnpDeviceProperty` returned:

| Instance | Service | Parent / filter |
| --- | --- | --- |
| `ACPI\ASUE1306\4&2BA988C7&0` | `hidi2c` | UpperFilters `{mshidkmdf}` |
| `HID\ASUE1306&COL01\5&2501CC81&0&0000` | `mouhid` | ACPI device above |
| `HID\ASUE1306&COL02\5&2501CC81&0&0001` | `AsusPTPDrv` | ACPI device above |
| `HID\ASUE1306&COL03\5&2501CC81&0&0002` | `MTConfig` | ACPI device above |

The three collection nodes had no reported UpperFilters/LowerFilters. `mshidkmdf` is part of the HID framework bridge here, **not evidence that it is the three/four-finger recognizer**. The exact report descriptor, report 0x04, hybrid format and five-contact capacity were supplied in the task; they were not re-probed by this investigation.

**M2 — ASUS installation.** The installed INF matches `HID\ASUE1306&Col02`. Its `[AsusFilter_Inst.NT.Services]` installs `AsusPTPDrv` as the associated service (`SPSVCINST_ASSOCSERVICE`), with `AsusPTPFilter.sys`, kernel service type 1. Despite the filename, this is the associated collection function service, not a separately listed Col02 UpperFilter. DriverVer is `09/04/2024,16.0.0.31`. `AsusPTPService` is running as LocalSystem from the same DriverStore package.

**M3 — ASUS static inspection.** The driver imports `HidP_GetCaps`, `HidP_GetUsageValue`, `HidP_SetUsageValue`, `HidP_GetUsages`, `HidP_SetUsages`, `IoCreateDevice`, `IoCreateSymbolicLink` and `IoBuildDeviceIoControlRequest`. Embedded names include `AsFilter_PTP_ReadCompletion`, `AsFilter_PTP_PNPNotifyArrival` and `PTP_ALGO`. These establish parsing/modification primitives and a completion path, but do not establish that a given contact is modified or expose a callable policy API. Its inspected imports use WDM/kernel primitives; do not assume ASUS provides a KMDF filter API.

**M4 — Windows kernel binary evidence.** Read-only printable-string extraction found:

- `win32kbase.sys`: `ProcessTouchpadGesture`, `TouchpadShellGesture`, `Pointer::GetPointerTouchpadInfo`, `TouchPadHIDProcessing`, `TouchPadWMPointerProcessing`, `TouchPadGesture`, `ThreeFingerTapCount`, `NtUserRegisterTouchPadCapable` and touchpad configuration paths.
- `win32kfull.sys`: `CPTPProcessor::InjectTouchpadAction` (decorated symbol), `NtUserInjectTouchpadAction`, `NtUserRegisterTouchpadCapableWindow`, `_GetPrecisionTouchPadConfiguration`, and `InitializeTouchPadSysParams`.

These substantiate kernel PTP processing and injection plumbing. They do not identify the complete recognition algorithm or all boundaries between kernel classification and user-mode manipulation recognition. [M/U]

**M5 — InputHost.** `InputHost.dll` embeds source paths under `inputv2\contextualprocessors\processors\shellgestures\client\touchpadgesturescontroller.cpp` and names `BamoShellGesturesClientPrincipalImpl::SetTouchpadEnabled`, `SetTouchpadSupportedGestures`, `TouchpadActionPerformed` and `Windows.UI.Input.TouchpadGesturesController`. This identifies client/routing implementation in InputHost. It does **not** show that `TextInputHost.exe` itself owns all gesture recognition. No such process-level attribution was verified.

**M6 — Shell.** `twinui.pcshell.dll` contains `PTPGestureActionCompleted` and touchpad experience-manager paths. Some nearby strings explicitly concern the **on-screen virtual touchpad**, so they must not all be treated as evidence about physical-pad shell swipes. Explorer's immersive settings cache was queried separately below. No debugger/ETW trace established a precise shell COM method or compositor progress interface.

Microsoft's [HID architecture][D1] explains the collection/client layer. Its newer [PTP portal][D2] distinguishes mousing, gestures and actions; gesture data can be preserved for an app or the shell, while ordinary mousing is converted to mouse events. It names DirectManipulation/InteractionTracker and the ninput fallback. **Do not model all Windows gestures as keystroke generation.**

### Is shell input continuous or a trigger?

**Both forms exist.** The foreground controller receives contact press/move/release events for manipulation gestures, separately from tap/press/release actions. That documents a continuous global-gesture input path. [D3] PhysicalGestureRecognizer supports translation, scale and rotation in device-relative units, with manipulation events. [D5]

**[I]** A native swipe can therefore be fed as a trajectory rather than translated to Win+Tab or Ctrl+Win+Arrow. **[U]** This does not prove that the current virtual-desktop animation follows that trajectory linearly, or that Windows exposes its interpolation parameters. No desktop switch or task-view activation was performed. A recognizer's continuous input and the shell's eventual visual behavior are separate questions. Keyboard shortcuts/custom action bindings can still be discrete triggers.

## Selective suppression and live settings

### What is present locally

**[M]** Read of `HKCU\Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad`:

```text
PanEnabled=1                 ZoomEnabled=1
ThreeFingerTapEnabled=3      CustomThreeFingerTap=3
ThreeFingerSlideEnabled=65535
ThreeFingerUp=16             ThreeFingerDown=17
ThreeFingerLeft=10           ThreeFingerRight=12
```

No `FourFinger*` values were present in that key at read time. **Absent does not mean disabled**: defaults/cache still apply. `ThreeFingerSlideEnabled=65535` also shows why this should not be treated as a simple Boolean; do not overwrite it with a guessed 1. Settings supports separate three/four-finger assignments, including disabling their actions, independently of the two-finger controls. [D6] Direction-specific and custom bindings need preservation if a future utility changes settings. [I]

### The live notification lead, with a local read-only check

An [NVDA discussion containing original reverse engineering][D7] describes Settings notifying Explorer through undocumented `IImmersiveSettingsCache::OnSettingChanged`, rather than merely writing registry values. The proposed activation chain is:

```text
CLSID_ImmersiveShell = c2f03a33-21f5-47fa-b4bb-156362a2f239
IServiceProvider::QueryService
  SID = 53660488-8855-460b-a9ab-5cfc6b5012ca
  IID = 4214f6fa-eb36-4e2f-9ca2-23fdc1832df7
vtable after IUnknown: OnSettingChanged, GetBOOL, GetDWORD,
                      RegisterForSettingChange, UnregisterForSettingChange
```

**[M]** QueryService returned `S_OK` here. Only `GetDWORD` was called:

| Proposed setting ID | Proposed meaning | HRESULT | Value |
| --- | --- | --- | --- |
| 0x0F | ThreeFingerTapEnabled | S_OK | 3 |
| 0x10 | FourFingerTapEnabled | S_OK | 2 |
| 0x11 | ThreeFingerSlideEnabled | S_OK | 65535 |
| 0x12 | FourFingerSlideEnabled | S_OK | 2 |

The three-finger values match the registry. The four-finger getters return defaults despite absent HKCU values. This supports the proposed mapping on 26300.9711 but does not prove every ID's semantics. All COM references were released. **No `OnSettingChanged`, registry write or listener registration was performed.**

**Verdict [I/U]:** Live changes without reboot are plausible through this private cache, but the write-and-apply behavior remains unverified here. Setting IDs vary between releases. Do not broadcast a range of guessed IDs, hard-code old IDs without version checks, or promise that WM_SETTINGCHANGE alone reloads these shell settings. No WNF state name or required broadcast was established. There may be internal downstream notifications; this investigation did not trace them.

The public [TOUCHPAD_PARAMETERS_V1][D8] / `SPI_SETTOUCHPADPARAMETERS` API covers cursor, tap, pan, zoom and related options on 24H2+, but its defined fields do **not** include three/four-finger shell mappings. It is not a public substitute for that private notification path. No SET call was made.

### What a user-mode observer can and cannot block

- **[D] Raw Input is observation.** `RIDEV_INPUTSINK` supplies background input; [RIDEV_NOLEGACY][D9] is documented for mouse/keyboard and affects legacy messages for the registering application. It is not a global HID/PTP suppression switch. Returning from WM_INPUT, unregistering the observer, or discarding its packets does not retract the system's copy. [I]
- **[D] Foreground selective interception exists on this build's API surface.** `TouchpadGesturesController` can claim three-, four- or five-finger manipulations/actions for its own foreground process. Background controllers are ignored. **[M]** metadata presence and `IsSupported() == true` were verified; no controller was created or enabled. It needs a message-pumping thread. [D3]
- **[I]** That API is a useful app-local solution for custom pinch, twist and drag, but not a transparent background remapper while another app retains focus. A focus-stealing overlay would change routing and UX; it is not an established workaround.
- **[D]** `RegisterTouchpadCapableWindow/Thread` enables richer two-finger WM_POINTER processing in the registered app, with DefWindowProc wheel fallback. It does not claim every desktop's gestures. [D10]
- **[U]** No supported exclusive-open technique that removes only selected physical gestures was identified. The task's read/write Col03 access is configuration access, not evidence of an input veto. No HID feature report was written.

## Injection: safe experiment and alternatives

### M7 — actual API probes, no injected contacts

All calls ran from ordinary Windows PowerShell using Add-Type/P/Invoke. Any successful synthetic device was immediately destroyed. No `InjectSyntheticPointerInput`, `InjectTouchInput`, `InjectTouchpadAction` or `SendInput` call was made.

| Probe | Result |
| --- | --- |
| Old `CreateSyntheticPointerDevice(PT_TOUCH=2, 5, NONE=3)` | nonzero handle, destroyed |
| Old `CreateSyntheticPointerDevice(PT_PEN=3, 1, NONE=3)` | nonzero handle, destroyed |
| Old `CreateSyntheticPointerDevice(PT_TOUCHPAD=5, 5, NONE=3)` | NULL, GetLastError **87 / ERROR_INVALID_PARAMETER** |
| New `CreateSyntheticPointerDevice2`, PT_TOUCHPAD, options 1 | nonzero handle, destroyed |
| New `CreateSyntheticPointerDevice2`, PT_TOUCHPAD, options 3 | nonzero handle, destroyed |

GetLastError was 203 after successful calls; it is stale/undefined for interpretation on success and is **not** evidence of failure. The type-2/type-3 controls show that the old-call failure was specific to the requested touchpad type/contract.

**[M]** `GetProcAddress` resolved `CreateSyntheticPointerDevice2`, `InjectTouchpadAction`, `RegisterTouchpadCapableWindow` and `GetPointerTouchpadInfo` **by name** in this machine's user32.dll. Hard-coded ordinals were unnecessary. The installed SDK include directory is only `10.0.22000.0`, so absence from those headers is not absence from this OS.

The creation-only experiment can be reproduced with this C# 5-compatible P/Invoke layout (inside a short-lived console process or Add-Type class):

```csharp
[StructLayout(LayoutKind.Sequential)]
public struct Params {
    public uint type, count, feedback;
    public IntPtr monitor;
    public uint width, height, options;
}
[DllImport("user32.dll", SetLastError = true)]
static extern IntPtr CreateSyntheticPointerDevice2(ref Params p);
[DllImport("user32.dll")]
static extern void DestroySyntheticPointerDevice(IntPtr p);

// sizeof(Params) was 40 on x64. Width/height are himetric, not pixels.
foreach (uint options in new uint[] { 1, 3 }) {
    Params p = new Params {
        type = 5, count = 5, feedback = 3, monitor = IntPtr.Zero,
        width = 12000, height = 7500, options = options
    };
    IntPtr h = CreateSyntheticPointerDevice2(ref p);
    int error = Marshal.GetLastWin32Error();
    Console.WriteLine("options={0} handle={1} error={2}", options, h, error);
    if (h != IntPtr.Zero) DestroySyntheticPointerDevice(h);
}
```

### What the new route means

**[D]** [CreateSyntheticPointerDevice2][D4] adds physical size, required for PT_TOUCHPAD; this explains why the old API rejects it. A touchpad supports up to five contacts, feedback NONE, null monitor, nonzero physical dimensions and `SDCO_PHYSICAL_SIZE=1`; `SDCO_TOUCHPAD_GESTURE_ONLY=2` is optional. Gesture-only prevents recognition as cursor motion/clicks. Input uses himetric positions.

Microsoft's [programming guide][D11] demonstrates PT_TOUCHPAD contact arrays passed through `InjectSyntheticPointerInput`, using `POINTER_TYPE_INFO.touchInfo`, stable IDs, confidence/in-range/in-contact flags, frame timestamps, and departing contacts. This deserves a controlled follow-up rather than repurposing touchscreen DOWN/UPDATE/UP code blindly. **[I]** Map the custom gesture's progress to a synthetic four-contact translation, then let the native recognizer/shell consume it. **[U]** Need explicit hands-on validation of target desktop action, thresholds, gain, reversal, release/cancel, foreground app routing and the current shell's animation.

**[D]** [InjectTouchpadAction][D12] supplies multi-finger tap/press/release and inertia actions. It does not expose an arbitrary “desktop swipe progress” parameter. Continuous swipes use contact frames.

**Critical coupling [I/U]:** Physical and synthetic native touchpad gestures may both respect the same shell gesture settings. Disabling a four-finger assignment to free it for our observer might also disable the synthetic four-finger output intended to drive that action. No per-device exception was established. Also, a foreground controller might receive our injected gesture again. Source tagging/filtering and routing must be tested before claiming a complete remapper.

### Other injection routes

| Route | Assessment |
| --- | --- |
| Old CreateSyntheticPointerDevice + InjectSyntheticPointerInput | Old creation contract accepts touch/pen, not touchpad; locally confirmed failure. [M,D13] |
| InjectTouchInput | Injects display-space touchscreen contacts, not an indirect PTP device. Touchscreen shell gestures, where enabled, are a different input path. No test performed. [D14] |
| Windows.UI.Input.Preview.Injection.InputInjector | Metadata type exists here. Its documented devices are mouse, keyboard, touch, pen and gamepad, with a restricted capability; no touchpad-specific injection method is documented. Type presence does not grant that capability. No instance/injection attempted. [M,D15] |
| Virtual HID Framework | A valid virtual PTP descriptor/report stream can enter the native HID path. VHF source drivers are kernel-mode KMDF/WDM, not an ordinary user-mode DLL. A user app can feed a purpose-built signed driver; VHF alone does not mute the physical device. [D16,I] |
| UMDF virtual HID minidriver | Separate from VHF: Microsoft supports UMDF 2 HID minidrivers and has a vhidmini2 sample. It is still an installed PnP driver package with an inbox kernel bridge, not zero-install app injection. PTP-specific acceptance/behavior on this build was not tested. [D17,U] |

For distribution on normal modern x64 Windows systems, new kernel drivers require Microsoft's signing process and appropriate PnP package signing; an ordinary self-signed application certificate is insufficient. Microsoft's policy describes dashboard registration/EV requirements and production HLK submissions. UMDF does not remove package installation/signing obligations, although its user-mode binary is not a VHF kernel source driver. No driver was built, installed or loaded here. [D18]

## ASUS private interface: what was actually found

Package directory:

```text
C:\Windows\System32\DriverStore\FileRepository\asusptpfilter.inf_amd64_1e467870260bcd2f\
```

**[M]** SHA-256 fingerprints:

```text
AsusPTPFilter.sys
BAB8B658816EDBD5479A4EC6C07038FF07DA145C60E38A0D645A3D8CC3BD6F31
AsusPTPService.exe
1157950F280C0137796DA4F1D75EA21809F5BB91B602B2CE47CE7D98C21EBC3A
```

Both returned Valid Authenticode signatures with Microsoft Windows Hardware Compatibility Publisher as signer. This authenticates the inspected binaries; it does not document or authorize third-party use of their private API.

### M8 — named devices and code

`QueryDosDevice`, which resolves names without opening a device, confirmed:

```text
AsusHidService -> \Device\AsusHidService
AsusHITTP     -> \Device\AsusHITTP
AsusTPCC      -> \Device\AsusTPCC
AsusTPGM      -> \Device\AsusTPGM
AsusTPPC      -> \Device\AsusTPPC
FingerPrint   -> \Device\FingerPrint
```

The driver also contains AsusCOSUMRCDO and AsusKPICDO symbolic-link strings; those two names did not resolve (error 2). Strings alone would therefore have overstated the active interface surface.

The service exports a C++ `CAsusTPDrvService` wrapper, including `Initialize(callback)`, `PacketThread`, `GetDrvRegistryPath`, `Get/SetLEDState`, `Get/SetDemoMode`, `Get/SetkeyBMappingInfo`, `SetEventToNotifyNumPad`, `SetSysEvent`, `NotifySessionChange` and `NotifyDialSwitchDial`.

Static disassembly using the already-installed `C:\msys64\ucrt64\bin\objdump.exe` establishes the following, without executing ASUS code:

| Service RVA / function | Observed behavior |
| --- | --- |
| 0x56E0, Initialize(callback) | opens UTF-16 `\\.\AsusHidService` at image VA 0x14002A5F0 using CreateFileW; desired access 0xC0000000, sharing 3, OPEN_EXISTING, FILE_FLAG_OVERLAPPED |
| 0x54C0, PacketThread | DeviceIoControl at VA 0x140005550, IOCTL **0x220404**, no input, 0x800-byte output; waits on pending completion and passes received data to callback |
| 0x5910, GetDrvRegistryPath | IOCTL **0x220408**, 0x390-byte packet; header begins command 0x1003, size 0x390, marker 0xFF0000FF |
| 0x5A30 / 0x5AF0 | GetLEDState **0x220410** / SetLEDState **0x22059C** |
| 0x5B90 | SetSysEvent **0x2205B0** |
| 0x5C30 / 0x5CF0 | GetDemoMode **0x22040C** / SetDemoMode **0x220598** |
| 0x5D90 / 0x5E70 | GetkeyBMappingInfo **0x220414** / SetkeyBMappingInfo **0x2205A0** |
| 0x5F50 / 0x6010 | SetEventToNotifyNumPad **0x2205A4** / NotifySessionChange **0x2205A8** |

These are **observed caller constants, not a supported protocol specification**. For example, 0x220404 decodes as FILE_DEVICE_UNKNOWN / function 0x101 / METHOD_BUFFERED / FILE_ANY_ACCESS. That access field does not bypass the device ACL or prove an ordinary user can open it. No open/ACL/IOCTL experiment was performed because a create handler or packet read might initialize state or consume events needed by the running service.

Service debug strings include `AsusPTPService::TouchpadCallbackProcess`, `ASSERVICE_IOCTL_PKT_DRIVER_REMOVE`, `ASSERVICE_IOCTL_PKT_GET_SLIDER_EVENT`, and `asus-screenxpert://ControlCenter`. The INF documents algorithm/slider parameters and driver flags. **[I]** This looks like a vendor auxiliary-event/configuration channel, including touchpad slider/NumberPad integrations. It is useful reverse-engineering evidence, but **[U]** a general contacts callback, contact replacement command or gesture-suppression bit has not been found. `Initialize(callback)` means a callback for this private packet protocol, not automatically raw contacts.

No REA `mcp__rea__*` tools were exposed in the available tool inventory. Inspection used strings, PE imports/exports and bounded disassembly; no PDB was fetched and no full driver dispatch decompilation was completed. Further static work could map the driver's IRP_MJ_DEVICE_CONTROL dispatcher, packet schemas, ACL setup and read-completion transformations before any hardware-facing experiment.

## Prior art

All entries below are project/source reviews, **not installed or tested locally**. They demonstrate approaches, not guaranteed support for this ASUS machine.

| Project | License | Approach and relevance |
| --- | --- | --- |
| [imbushuo/mac-precision-touchpad][P1] | USB projects GPLv2; SPI project MIT, per the [project license notice][P2] | Converts Apple hardware reports into Windows PTP input through real drivers. Useful descriptor/report and driver-stack reference; not a drop-in ASUS user-mode interception hook. |
| [fakebob2004/touchpad][P3] | Apache-2.0 | Experimental Mac-to-Windows bridge: raw contacts over TCP into KMDF/VHF PTP. Authors report native pointer, scroll and pinch. Their README does not establish a completed tracked-shell-gesture solution. Requires a test-signed driver in its current prototype. |
| [Shiroo09/SmoothPad][P4] | MIT | Raw Input motion engine with runtime detection of CreateSyntheticPointerDevice2 and a proposed HID filter/watchdog. README explicitly says production replacement is gated and filter suppression is not implemented in the pass-through skeleton. Especially relevant to the distinction between observing/injecting and suppressing physical input. |
| [TransposonY/GestureSign][P5] | GPL-2.0 | Gesture recognition and command automation: keyboard/mouse simulation, window commands and other actions. Useful recognizer/action-binding prior art; reviewed README does not prove native tracked PTP output or selective contact suppression. |
| [Microsoft vhidmini2][P6] | [MS-PL][P7] | UMDF/KMDF virtual HID sample and custom-feature communication. Starting point for virtual descriptors/driver transport, not a ready-made PTP remapper. |

## Recommended approach

1. **Keep the current Raw Input contact observer as the initial system-wide recognizer.** [I] Build deterministic gesture state from scan frames/contact IDs, including hybrid-report assembly, debouncing, palm/confidence handling and explicit end/cancel. Track gesture progress independently of any eventual action. Five-finger pinch and twist are viable recognition targets if the hardware actually supplies the required contacts; native four-finger recognition may already start while the fifth finger arrives.
2. **For UI owned by our app, prefer the new foreground API.** [I] The supported controller plus PhysicalGestureRecognizer is a cleaner route to app-local three/four/five-finger continuous interaction than globally disabling shell gestures. Runtime checks already look favorable on this machine, but receiving actual frames still needs validation.
3. **Make the next shell experiment a controlled test of CreateSyntheticPointerDevice2 contact frames.** [I] The creation prerequisite is verified. With the user present, test one normal four-finger trajectory and reversal/cancel, recording whether the shell consumes continuous progress. Keep it separate from suppression experiments so failures are diagnosable. The gesture-only option avoids accidental synthetic mousing but does not solve physical duplication.
4. **For a user-mode-only global prototype, use explicit user-configured native gesture assignments.** [I] Disable only conflicting three/four-finger actions through Settings when later authorized, leaving cursor/pan/zoom enabled. Treat five-finger gestures as potentially conflicting too. Do not ship automatic registry edits/private cache notifications until tested with preservation/restoration and build checks. This research did not change those settings.
5. **If transparent global coexistence is essential, budget for a signed filter.** [I] A driver can arbitrate contact streams before Windows recognizes them and expose a separate private feed to the app. If it suppresses reports before Raw Input, the app cannot rely on WM_INPUT still seeing them. It must forward nonclaimed input coherently and fail open on app failure. Kernel work is substantial; first prove the animation behavior with the injector. Do not make ASUS's undocumented vendor protocol a prerequisite.

**Decision boundary:** Driverless recognition plus custom animation is feasible in principle. Native synthetic PTP input is now a concrete option on this build. Exact GNOME-style 1:1 shell transitions remain **unproven**: a native recognizer can apply its own thresholds/gain and the shell owns its animation. A custom overview/desktop visual layer may be necessary if native behavior cannot meet the requirement. This investigation found no public setter for shell animation progress.

## Risks and remaining validation

- **Build/API stability:** The new PTP documentation is marked prerelease. Check exports and `IsSupported` at runtime; do not assume availability from the Windows 11 name or stale SDK headers. Private cache IDs and ASUS packet formats are less stable still.
- **Double actions and feedback:** Physical Windows recognition can fire before the custom recognizer commits. Injected input may reach our observer/controller, may obey disabled mappings, and may interfere with physical input. These behaviors need explicit tests; creation alone proves none of them.
- **Continuity and latency:** Finger count changes, hybrid report splitting, scan timestamps, contact-ID reuse and releases matter. Holding reports for classification adds latency; dropping partial sequences can cause phantom/stuck state or broken scroll. A filter needs cancellation, watchdog, suspend/resume and device-removal handling.
- **Scope and permissions:** Foreground interception is not background desktop control. Injection across integrity levels, secure desktops, sessions and remote desktop was not tested. Do not infer privilege bypass from a successful device creation.
- **Driver reliability:** Bad filtering can remove the laptop's primary pointing device or destabilize the kernel. Signed deployment and recovery planning are real engineering requirements, not supplied by a user-mode remapper.
- **ASUS contention:** Packet reads may compete with the running LocalSystem service; private writes may alter vendor or hardware state. No private request should be guessed from a name or IOCTL access bits.
- **Licensing:** Keep GPL-derived code separated from any intended permissive implementation; inspect per-file/subproject licenses before reuse. The ASUS binaries are proprietary evidence, not reusable source.

## Safety/accounting

Only this report was created in the repository by this investigation. No scratch source/binary files were created, so `trackpad/research/scratch-03/` needed no cleanup. PowerShell Add-Type probes ran in short-lived processes; all created synthetic handles were destroyed and queried COM objects released. There were no registry writes, settings changes, feature-report writes, software/driver installs, desktop input injections, service changes, explorer/DWM restarts, or boot configuration changes. `trackpad/gestures/` was not edited.

## Sources

[D1]: https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/hid-architecture
[D2]: https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/precision-touchpad-portal
[D3]: https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/touchpadgesturescontroller
[D4]: https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/createsyntheticpointerdevice2
[D5]: https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/physicalgesturerecognizer
[D6]: https://support.microsoft.com/en-au/windows/hardware/input-devices/touch-gestures-for-windows
[D7]: https://github.com/nvaccess/nvda/discussions/19841
[D8]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-touchpad_parameters_v1
[D9]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice
[D10]: https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/registertouchpadcapable
[D11]: https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/precision-touchpad-guide
[D12]: https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/injecttouchpadaction
[D13]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createsyntheticpointerdevice
[D14]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-injecttouchinput
[D15]: https://learn.microsoft.com/en-us/uwp/api/windows.ui.input.preview.injection?view=winrt-26100
[D16]: https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/virtual-hid-framework--vhf-
[D17]: https://learn.microsoft.com/en-us/windows-hardware/drivers/wdf/creating-umdf-hid-minidrivers
[D18]: https://learn.microsoft.com/en-us/windows-hardware/drivers/install/kernel-mode-code-signing-policy--windows-vista-and-later-
[P1]: https://github.com/imbushuo/mac-precision-touchpad
[P2]: https://github.com/imbushuo/mac-precision-touchpad/blob/master/LICENSE.md
[P3]: https://github.com/fakebob2004/touchpad
[P4]: https://github.com/Shiroo09/SmoothPad
[P5]: https://github.com/TransposonY/GestureSign
[P6]: https://github.com/microsoft/Windows-driver-samples/blob/main/hid/vhidmini2/README.md
[P7]: https://raw.githubusercontent.com/microsoft/Windows-driver-samples/main/LICENSE
