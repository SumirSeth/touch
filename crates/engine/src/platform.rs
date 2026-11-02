use crate::{
    actions::{parse_chord, preset_chord},
    gestures::{Contact, Frame, GestureEvent, Recognizer},
    *,
};
use std::{
    collections::HashMap,
    io,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::Instant,
};
use windows_sys::Win32::{
    Devices::HumanInterfaceDevice::*,
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, Registry::*},
    UI::{
        Input::{KeyboardAndMouse::*, *},
        Shell::ShellExecuteW,
        WindowsAndMessaging::*,
    },
};
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn emit(tx: &Option<Sender<EngineEvent>>, event: EngineEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(event);
    }
}
enum Command {
    Config(Config),
    Pause(bool),
}
pub struct Engine {
    hwnd: usize,
    commands: Sender<Command>,
    actions: Sender<Work>,
    thread: Option<JoinHandle<()>>,
    worker: Option<JoinHandle<()>>,
}
impl Engine {
    pub fn start(config: Config, events: Option<Sender<EngineEvent>>) -> io::Result<Self> {
        let errors = validate(&config);
        if !errors.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                errors.join("; "),
            ));
        }
        let (actions, rx) = mpsc::channel();
        let ev = events.clone();
        let worker = thread::Builder::new()
            .name("gesture-actions".into())
            .spawn(move || worker(rx, ev))?;
        let (commands, rx) = mpsc::channel();
        let (ready, wait) = mpsc::sync_channel(1);
        let tx = actions.clone();
        let thread = thread::Builder::new()
            .name("gesture-input".into())
            .spawn(move || unsafe { input_thread(config, events, tx, rx, ready) })?;
        match wait.recv() {
            Ok(Ok(hwnd)) => Ok(Self {
                hwnd,
                commands,
                actions,
                thread: Some(thread),
                worker: Some(worker),
            }),
            result => {
                let _ = thread.join();
                let _ = actions.send(Work::Stop);
                let _ = worker.join();
                Err(match result {
                    Ok(Err(e)) => e,
                    _ => io::Error::other("Input thread failed to initialize"),
                })
            }
        }
    }
    fn command(&self, c: Command) {
        let _ = self.commands.send(c);
        unsafe {
            PostMessageW(self.hwnd as HWND, WM_APP, 0, 0);
        }
    }
    pub fn set_config(&self, config: Config) {
        self.command(Command::Config(config));
    }
    pub fn set_paused(&self, paused: bool) {
        self.command(Command::Pause(paused));
    }
    pub fn run_action(&self, action: &Action) {
        if !matches!(action, Action::Adjust { .. } | Action::None) {
            let _ = self.actions.send(Work::Action(action.clone(), 0));
        }
    }
    pub fn stop(mut self) {
        self.shutdown();
    }
    fn shutdown(&mut self) {
        if let Some(t) = self.thread.take() {
            unsafe {
                PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0);
            }
            let _ = t.join();
        }
        if let Some(w) = self.worker.take() {
            let _ = self.actions.send(Work::Stop);
            let _ = w.join();
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}
struct State {
    config: Config,
    pending: Option<Config>,
    paused: bool,
    recognizer: Recognizer,
    devices: HashMap<usize, Device>,
    current_device: Option<usize>,
    events: Option<Sender<EngineEvent>>,
    actions: Sender<Work>,
    commands: Receiver<Command>,
    clock: Instant,
    frozen: bool,
    raw_buffer: Vec<u64>,
}
impl State {
    fn output(&mut self, events: Vec<GestureEvent>) {
        for event in events {
            match event {
                GestureEvent::Fired(trigger) => {
                    emit(
                        &self.events,
                        EngineEvent::Fired {
                            trigger: trigger.clone(),
                        },
                    );
                    if let Some(a) = self.config.bindings.get(&trigger) {
                        let _ = self.actions.send(Work::Action(a.clone(), 0));
                    }
                }
                GestureEvent::Adjusted { trigger, steps } => {
                    emit(
                        &self.events,
                        EngineEvent::Adjusted {
                            trigger: trigger.clone(),
                            steps,
                        },
                    );
                    if let Some(a) = self.config.bindings.get(&trigger) {
                        let _ = self.actions.send(Work::Action(a.clone(), steps));
                    }
                }
                GestureEvent::Freeze(freeze) => unsafe {
                    if freeze {
                        let mut p = zeroed();
                        if GetCursorPos(&mut p) != 0 {
                            let r = RECT {
                                left: p.x,
                                top: p.y,
                                right: p.x + 1,
                                bottom: p.y + 1,
                            };
                            self.frozen = ClipCursor(&r) != 0;
                        }
                    } else if self.frozen {
                        ClipCursor(null());
                        self.frozen = false;
                    }
                },
                other => {
                    let _ = self.actions.send(Work::Gesture(other));
                }
            }
        }
    }
    fn reset(&mut self) {
        let events = self.recognizer.reset();
        self.output(events);
    }
    fn commands(&mut self) {
        while let Ok(c) = self.commands.try_recv() {
            match c {
                Command::Config(config) => {
                    let errors = validate(&config);
                    if errors.is_empty() {
                        if !config.enabled {
                            self.reset();
                        }
                        self.pending = Some(config);
                    } else {
                        emit(&self.events, EngineEvent::Error(errors.join("; ")));
                    }
                }
                Command::Pause(p) => {
                    self.paused = p;
                    if p {
                        self.reset();
                    }
                }
            }
        }
    }
    fn frame(&mut self, frame: &Frame) {
        if !self.recognizer.is_active() {
            if let Some(c) = self.pending.take() {
                self.config = c.clone();
                self.recognizer = Recognizer::new(c);
            }
        }
        if !self.paused {
            let events = self.recognizer.on_frame(frame);
            self.output(events);
        }
    }
    unsafe fn raw(&mut self, handle: HRAWINPUT) {
        let mut buf = std::mem::take(&mut self.raw_buffer);
        self.raw_buffered(handle, &mut buf);
        self.raw_buffer = buf;
    }
    unsafe fn raw_buffered(&mut self, handle: HRAWINPUT, buf: &mut Vec<u64>) {
        let hdr = size_of::<RAWINPUTHEADER>() as u32;
        let mut size = 0;
        GetRawInputData(handle, RID_INPUT, null_mut(), &mut size, hdr);
        if size < (hdr + 8) || size > 1024 * 1024 {
            return;
        }
        buf.resize((size as usize).div_ceil(8), 0);
        if GetRawInputData(handle, RID_INPUT, buf.as_mut_ptr().cast(), &mut size, hdr) == u32::MAX {
            return;
        }
        let header = &*(buf.as_ptr() as *const RAWINPUTHEADER);
        if header.dwType != RIM_TYPEHID {
            return;
        }
        let key = header.hDevice as usize;
        if let std::collections::hash_map::Entry::Vacant(entry) = self.devices.entry(key) {
            match Device::load(header.hDevice) {
                Ok(d) => {
                    if !d.synthetic {
                        emit(
                            &self.events,
                            EngineEvent::Touchpad {
                                width_mm: d.width,
                                height_mm: d.height,
                            },
                        );
                    }
                    entry.insert(d);
                }
                Err(e) => {
                    emit(&self.events, EngineEvent::Error(e.to_string()));
                    return;
                }
            }
        }
        if self.devices.get(&key).is_some_and(|d| d.synthetic) {
            return;
        }
        if self.current_device != Some(key) {
            self.reset();
            self.current_device = Some(key);
        }
        let bytes = std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), size as usize);
        let rep_size =
            u32::from_ne_bytes(bytes[hdr as usize..hdr as usize + 4].try_into().unwrap()) as usize;
        let count = u32::from_ne_bytes(
            bytes[hdr as usize + 4..hdr as usize + 8]
                .try_into()
                .unwrap(),
        ) as usize;
        if rep_size == 0 || count > bytes.len() / rep_size {
            return;
        }
        for i in 0..count {
            let offset = hdr as usize + 8 + i * rep_size;
            if let Some(report) = bytes.get(offset..offset + rep_size) {
                let frame = self
                    .devices
                    .get_mut(&key)
                    .unwrap()
                    .report(report, self.clock.elapsed().as_millis() as u64);
                if let Some(mut frame) = frame {
                    self.frame(&frame);
                    frame.touching.clear();
                    self.devices.get_mut(&key).unwrap().pending = frame.touching;
                }
            }
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.reset();
        if self.frozen {
            unsafe {
                ClipCursor(null());
            }
        }
    }
}
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    if !ptr.is_null() {
        let state = &mut *ptr;
        match msg {
            WM_INPUT => {
                state.raw(l as HRAWINPUT);
            }
            WM_APP => state.commands(),
            WM_INPUT_DEVICE_CHANGE if w == GIDC_REMOVAL as usize => {
                state.devices.remove(&(l as usize));
                if state.current_device == Some(l as usize) {
                    state.reset();
                    state.current_device = None;
                }
                if state.devices.values().all(|d| d.synthetic) {
                    emit(&state.events, EngineEvent::NoTouchpad);
                }
            }
            WM_CLOSE => {
                state.reset();
                DestroyWindow(hwnd);
                return 0;
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                return 0;
            }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, msg, w, l)
}
unsafe fn input_thread(
    config: Config,
    events: Option<Sender<EngineEvent>>,
    actions: Sender<Work>,
    commands: Receiver<Command>,
    ready: mpsc::SyncSender<io::Result<usize>>,
) {
    let class = wide("TrackpadGesturesInput");
    let instance = GetModuleHandleW(null());
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..zeroed()
    };
    RegisterClassW(&wc);
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        class.as_ptr(),
        0,
        0,
        0,
        0,
        0,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    if hwnd.is_null() {
        let _ = ready.send(Err(io::Error::last_os_error()));
        return;
    }
    let mut state = Box::new(State {
        recognizer: Recognizer::new(config.clone()),
        config,
        pending: None,
        paused: false,
        devices: HashMap::new(),
        current_device: None,
        events,
        actions,
        commands,
        clock: Instant::now(),
        frozen: false,
        raw_buffer: Vec::new(),
    });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&mut *state as *mut State) as isize);
    let device = RAWINPUTDEVICE {
        usUsagePage: 0x0d,
        usUsage: 5,
        dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
        hwndTarget: hwnd,
    };
    if RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32) == 0 {
        let _ = ready.send(Err(io::Error::last_os_error()));
        DestroyWindow(hwnd);
        return;
    }
    // Enumerate existing pads so the UI receives dimensions before the first touch.
    let mut n = 0;
    GetRawInputDeviceList(null_mut(), &mut n, size_of::<RAWINPUTDEVICELIST>() as u32);
    let mut list = vec![zeroed::<RAWINPUTDEVICELIST>(); n as usize];
    let result = GetRawInputDeviceList(
        list.as_mut_ptr(),
        &mut n,
        size_of::<RAWINPUTDEVICELIST>() as u32,
    );
    let mut found = false;
    if result != u32::MAX {
        for d in list.into_iter().take(result as usize) {
            let mut info: RID_DEVICE_INFO = zeroed();
            info.cbSize = size_of::<RID_DEVICE_INFO>() as u32;
            let mut size = info.cbSize;
            if GetRawInputDeviceInfoW(
                d.hDevice,
                RIDI_DEVICEINFO,
                (&mut info as *mut RID_DEVICE_INFO).cast(),
                &mut size,
            ) != u32::MAX
                && info.dwType == RIM_TYPEHID
                && info.Anonymous.hid.usUsagePage == 13
                && info.Anonymous.hid.usUsage == 5
            {
                if let Ok(dev) = Device::load(d.hDevice) {
                    if !dev.synthetic {
                        found = true;
                        emit(
                            &state.events,
                            EngineEvent::Touchpad {
                                width_mm: dev.width,
                                height_mm: dev.height,
                            },
                        );
                    }
                    state.devices.insert(d.hDevice as usize, dev);
                }
            }
        }
    }
    if !found {
        emit(&state.events, EngineEvent::NoTouchpad);
    }
    let _ = ready.send(Ok(hwnd as usize));
    let mut msg = zeroed();
    loop {
        let status = GetMessageW(&mut msg, null_mut(), 0, 0);
        if status <= 0 {
            if status < 0 {
                emit(
                    &state.events,
                    EngineEvent::Error(io::Error::last_os_error().to_string()),
                );
            }
            break;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    if IsWindow(hwnd) != 0 {
        DestroyWindow(hwnd);
    }
    drop(state);
}
struct Device {
    pp: Vec<u64>,
    synthetic: bool,
    width: f64,
    height: f64,
    sx: f64,
    sy: f64,
    min_x: f64,
    min_y: f64,
    pending: Vec<Contact>,
    expected: usize,
    button: bool,
}
impl Device {
    unsafe fn load(handle: HANDLE) -> io::Result<Self> {
        let mut name = vec![0u16; 1024];
        let mut n = name.len() as u32;
        GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, name.as_mut_ptr().cast(), &mut n);
        let synthetic = String::from_utf16_lossy(&name[..(n as usize).min(name.len())])
            .to_ascii_lowercase()
            .starts_with(r"\\?\microsoft hid rid");
        let mut d = Self {
            pp: vec![],
            synthetic,
            width: 100.,
            height: 100.,
            sx: 1.,
            sy: 1.,
            min_x: 0.,
            min_y: 0.,
            pending: vec![],
            expected: 0,
            button: false,
        };
        if synthetic {
            return Ok(d);
        }
        let mut size = 0;
        GetRawInputDeviceInfoW(handle, RIDI_PREPARSEDDATA, null_mut(), &mut size);
        if size == 0 {
            return Err(io::Error::other("Missing HID descriptor"));
        }
        d.pp = vec![0u64; (size as usize).div_ceil(8)];
        if GetRawInputDeviceInfoW(
            handle,
            RIDI_PREPARSEDDATA,
            d.pp.as_mut_ptr().cast(),
            &mut size,
        ) == u32::MAX
        {
            return Err(io::Error::last_os_error());
        }
        (d.width, d.sx, d.min_x) = d.axis(0x30)?;
        (d.height, d.sy, d.min_y) = d.axis(0x31)?;
        Ok(d)
    }
    fn pp(&self) -> PHIDP_PREPARSED_DATA {
        self.pp.as_ptr() as isize
    }
    unsafe fn axis(&self, usage: u16) -> io::Result<(f64, f64, f64)> {
        let mut cap = HIDP_VALUE_CAPS::default();
        let mut n = 1;
        if HidP_GetSpecificValueCaps(HidP_Input, 1, 1, usage, &mut cap, &mut n, self.pp())
            != HIDP_STATUS_SUCCESS
            || n == 0
        {
            return Err(io::Error::other("Missing HID axis caps"));
        }
        let logical = (cap.LogicalMax - cap.LogicalMin) as f64;
        let physical = (cap.PhysicalMax - cap.PhysicalMin) as f64;
        if logical <= 0. {
            return Err(io::Error::other("Invalid HID logical extent"));
        }
        let mut exp = (cap.UnitsExp & 15) as i32;
        if exp > 7 {
            exp -= 16;
        }
        let mm = if physical > 0. {
            physical * 10f64.powi(exp) * if cap.Units & 15 == 3 { 25.4 } else { 10. }
        } else {
            logical
        };
        Ok((mm, mm / logical, cap.LogicalMin as f64))
    }
    unsafe fn value(&self, page: u16, link: u16, usage: u16, r: &[u8]) -> u32 {
        let mut value = 0;
        HidP_GetUsageValue(
            HidP_Input,
            page,
            link,
            usage,
            &mut value,
            self.pp(),
            r.as_ptr(),
            r.len() as u32,
        );
        value
    }
    unsafe fn usages(&self, page: u16, link: u16, r: &[u8]) -> ([u16; 32], usize) {
        let mut buf = [0u16; 32];
        let mut n = buf.len() as u32;
        if HidP_GetUsages(
            HidP_Input,
            page,
            link,
            buf.as_mut_ptr(),
            &mut n,
            self.pp(),
            r.as_ptr() as *mut u8,
            r.len() as u32,
        ) != HIDP_STATUS_SUCCESS
        {
            return (buf, 0);
        }
        (buf, (n as usize).min(buf.len()))
    }
    unsafe fn report(&mut self, r: &[u8], time_ms: u64) -> Option<Frame> {
        let count = self.value(13, 0, 0x54, r) as usize;
        if count > 0 {
            if count > 32 {
                return None;
            }
            self.pending.clear();
            self.expected = count;
            self.button = false;
        } else if self.expected == 0 {
            return None;
        }
        self.button |= self.usages(9, 0, r).1 != 0;
        let (usages, n) = self.usages(13, 1, r);
        let tip = usages[..n].contains(&0x42);
        let p = Contact {
            id: self.value(13, 1, 0x51, r),
            x: (self.value(1, 1, 0x30, r) as f64 - self.min_x) * self.sx,
            y: (self.value(1, 1, 0x31, r) as f64 - self.min_y) * self.sy,
        };
        self.pending.push(p); // keep report count separately from touching count
        if !tip {
            self.pending.last_mut().unwrap().id = u32::MAX;
        }
        if self.pending.len() < self.expected {
            return None;
        }
        self.expected = 0;
        self.pending.retain(|p| p.id != u32::MAX);
        Some(Frame {
            touching: std::mem::take(&mut self.pending),
            width_mm: self.width,
            height_mm: self.height,
            time_ms,
            button: self.button,
        })
    }
}
#[derive(Clone)]
enum Work {
    Action(Action, i32),
    Gesture(GestureEvent),
    Stop,
}
fn worker(rx: Receiver<Work>, events: Option<Sender<EngineEvent>>) {
    let mut queued = std::collections::VecDeque::new();
    let mut dragging = false;
    let mut brightness = Brightness::default();
    loop {
        let work = queued.pop_front().or_else(|| rx.recv().ok());
        let Some(work) = work else {
            break;
        };
        let result = match work {
            Work::Stop => break,
            Work::Gesture(event) => {
                match event {
                    GestureEvent::DragStart => dragging = true,
                    GestureEvent::DragEnd => dragging = false,
                    _ => {}
                }
                mouse(event)
            }
            Work::Action(
                Action::Adjust {
                    target: AdjustTarget::Brightness,
                    ..
                },
                mut delta,
            ) => {
                while let Ok(next) = rx.try_recv() {
                    match next {
                        Work::Action(
                            Action::Adjust {
                                target: AdjustTarget::Brightness,
                                ..
                            },
                            d,
                        ) => delta = delta.saturating_add(d),
                        other => {
                            queued.push_back(other);
                            break;
                        }
                    }
                }
                brightness.adjust(delta)
            }
            Work::Action(a, steps) => execute(&a, steps, &mut brightness),
        };
        if let Err(e) = result {
            emit(&events, EngineEvent::Error(e.to_string()));
        }
    }
    if dragging {
        let _ = mouse(GestureEvent::DragEnd);
    }
}
fn send(inputs: &[INPUT]) -> io::Result<()> {
    if unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    } != inputs.len() as u32
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn chord(keys: &str) -> io::Result<()> {
    let keys = parse_chord(keys).map_err(io::Error::other)?;
    let mut inputs = vec![];
    for &vk in &keys {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    ..unsafe { zeroed() }
                },
            },
        });
    }
    for &vk in keys.iter().rev() {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..unsafe { zeroed() }
                },
            },
        });
    }
    let result = send(&inputs);
    if result.is_err() {
        // A partial SendInput must not leave a modifier held.
        let _ = send(&inputs[keys.len()..]);
    }
    result
}
fn mouse(event: GestureEvent) -> io::Result<()> {
    let (dx, dy, data, flags) = match event {
        GestureEvent::DragStart => (0, 0, 0, MOUSEEVENTF_LEFTDOWN),
        GestureEvent::DragEnd => (0, 0, 0, MOUSEEVENTF_LEFTUP),
        GestureEvent::DragMove(x, y) => (x, y, 0, MOUSEEVENTF_MOVE),
        _ => return Ok(()),
    };
    send(&[INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                ..unsafe { zeroed() }
            },
        },
    }])
}
fn shell(path: &str, args: Option<&str>) -> io::Result<()> {
    let path = wide(path);
    let args = args.map(wide);
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            wide("open").as_ptr(),
            path.as_ptr(),
            args.as_ref().map_or(null(), |s| s.as_ptr()),
            null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        Err(io::Error::other(format!(
            "ShellExecute failed: {}",
            result as isize
        )))
    } else {
        Ok(())
    }
}
/// Runs a discrete action once on the calling thread (the settings window's Test button). Continuous
/// (adjust) actions need a gesture and are ignored.
pub fn perform(action: &Action) -> io::Result<()> {
    if matches!(action, Action::Adjust { .. }) {
        return Ok(());
    }
    execute(action, 0, &mut Brightness::default())
}
fn execute(action: &Action, steps: i32, brightness: &mut Brightness) -> io::Result<()> {
    match action {
        Action::None => Ok(()),
        Action::Preset { name } if name == "lock" => {
            if unsafe { windows_sys::Win32::System::Shutdown::LockWorkStation() } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        Action::Preset { name } => {
            chord(preset_chord(name).ok_or_else(|| io::Error::other("Unknown preset"))?)
        }
        Action::Keys { keys } => chord(keys),
        Action::Launch { path, args } => shell(path, args.as_deref()),
        Action::Url { url } => shell(url, None),
        Action::Adjust { target, .. } => {
            let up = steps > 0;
            for _ in 0..steps.unsigned_abs() {
                match target {
                    AdjustTarget::Brightness => brightness.adjust(if up { 1 } else { -1 })?,
                    AdjustTarget::ScrollH => {
                        send(&[INPUT {
                            r#type: INPUT_MOUSE,
                            Anonymous: INPUT_0 {
                                mi: MOUSEINPUT {
                                    mouseData: if up { 120 } else { (-120i32) as u32 },
                                    dwFlags: MOUSEEVENTF_HWHEEL,
                                    ..unsafe { zeroed() }
                                },
                            },
                        }])?;
                    }
                    other => chord(match other {
                        AdjustTarget::Volume => {
                            if up {
                                "volume_up"
                            } else {
                                "volume_down"
                            }
                        }
                        AdjustTarget::Seek => {
                            if up {
                                "Right"
                            } else {
                                "Left"
                            }
                        }
                        AdjustTarget::Zoom => {
                            if up {
                                "Ctrl+="
                            } else {
                                "Ctrl+-"
                            }
                        }
                        AdjustTarget::Tabs => {
                            if up {
                                "Ctrl+Tab"
                            } else {
                                "Ctrl+Shift+Tab"
                            }
                        }
                        AdjustTarget::Desktops => {
                            if up {
                                "Win+Ctrl+Right"
                            } else {
                                "Win+Ctrl+Left"
                            }
                        }
                        AdjustTarget::Custom { increase, decrease } => {
                            if up {
                                increase
                            } else {
                                decrease
                            }
                        }
                        _ => unreachable!(),
                    })?,
                }
            }
            Ok(())
        }
    }
}
// COM objects stay on the action thread. Failed sessions are discarded and rebuilt once.
#[derive(Default)]
struct Brightness {
    session: Option<BrightnessSession>,
}
struct BrightnessSession {
    con: wmi::WMIConnection,
    paths: Vec<String>,
    params: wmi::IWbemClassWrapper,
    current: i32,
    last: Option<Instant>,
}
fn brightness_needs_resync(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.duration_since(last) > std::time::Duration::from_secs(1))
}
impl BrightnessSession {
    fn connect() -> Result<Self, Box<dyn std::error::Error>> {
        let con = wmi::WMIConnection::with_namespace_path("ROOT\\WMI")?;
        let paths = con
            .exec_query("SELECT * FROM WmiMonitorBrightnessMethods WHERE Active=TRUE")?
            .map(|row| row.and_then(|row| row.path()))
            .collect::<Result<Vec<_>, _>>()?;
        let params = con
            .get_object("WmiMonitorBrightnessMethods")?
            .get_method("WmiSetBrightness")?
            .ok_or_else(|| io::Error::other("Missing WmiSetBrightness"))?
            .spawn_instance()?;
        params.put_property("Timeout", wmi::Variant::UI4(1))?;
        Ok(Self {
            con,
            paths,
            params,
            current: 0,
            last: None,
        })
    }
    fn adjust(&mut self, delta: i32) -> Result<(), Box<dyn std::error::Error>> {
        use wmi::Variant;
        let now = Instant::now();
        if brightness_needs_resync(self.last, now) {
            let rows: Vec<HashMap<String, Variant>> = self.con.raw_query(
                "SELECT CurrentBrightness FROM WmiMonitorBrightness WHERE Active=TRUE",
            )?;
            self.current = rows
                .first()
                .and_then(|r| r.get("CurrentBrightness"))
                .and_then(|v| match v {
                    Variant::UI1(n) => Some(*n as i32),
                    Variant::UI4(n) => Some(*n as i32),
                    Variant::I4(n) => Some(*n),
                    _ => None,
                })
                .ok_or_else(|| io::Error::other("No active WMI brightness display"))?;
        }
        let next = self.current.saturating_add(delta).clamp(0, 100);
        self.params
            .put_property("Brightness", Variant::UI1(next as u8))?;
        for path in &self.paths {
            if let Some(result) =
                self.con
                    .exec_method(path, "WmiSetBrightness", Some(&self.params))?
            {
                match result.get_property("ReturnValue")? {
                    Variant::UI4(0) | Variant::I4(0) => {}
                    status => {
                        return Err(io::Error::other(format!(
                            "WmiSetBrightness failed: {status:?}"
                        ))
                        .into())
                    }
                }
            }
        }
        self.current = next;
        self.last = Some(Instant::now());
        Ok(())
    }
}
impl Brightness {
    fn adjust(&mut self, delta: i32) -> io::Result<()> {
        for attempt in 0..2 {
            let result = (|| {
                if self.session.is_none() {
                    self.session = Some(BrightnessSession::connect()?);
                }
                self.session.as_mut().unwrap().adjust(delta)
            })();
            match result {
                Ok(()) => return Ok(()),
                Err(e) => {
                    self.session = None;
                    if attempt == 1 {
                        return Err(io::Error::other(e.to_string()));
                    }
                }
            }
        }
        unreachable!()
    }
}
#[cfg(test)]
mod brightness_tests {
    use super::*;
    #[test]
    fn resync_at_start_and_after_idle() {
        let start = Instant::now();
        assert!(brightness_needs_resync(None, start));
        assert!(!brightness_needs_resync(
            Some(start),
            start + std::time::Duration::from_millis(999)
        ));
        assert!(!brightness_needs_resync(
            Some(start),
            start + std::time::Duration::from_secs(1)
        ));
        assert!(brightness_needs_resync(
            Some(start),
            start + std::time::Duration::from_millis(1001)
        ));
    }
}
pub fn windows_conflicts(c: &Config) -> Vec<Conflict> {
    let mut out = vec![];
    unsafe {
        let mut key = null_mut();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(r"Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad").as_ptr(),
            0,
            KEY_READ,
            &mut key,
        ) != 0
        {
            return out;
        }
        for (prefix, name, label) in [
            ("swipe3", "ThreeFingerSlideEnabled", "3-finger swipes"),
            ("tap3", "ThreeFingerTapEnabled", "3-finger taps"),
            ("swipe4", "FourFingerSlideEnabled", "4-finger swipes"),
            ("tap4", "FourFingerTapEnabled", "4-finger taps"),
        ] {
            if c.drag3 && (prefix == "swipe3" || prefix == "tap3") {
                continue;
            }
            if !c.enabled
                || !c
                    .bindings
                    .iter()
                    .any(|(id, a)| id.starts_with(prefix) && !matches!(a, Action::None))
            {
                continue;
            }
            let mut value = 1u32;
            let mut size = 4;
            let mut kind = 0;
            let result = RegQueryValueExW(
                key,
                wide(name).as_ptr(),
                null(),
                &mut kind,
                (&mut value as *mut u32).cast(),
                &mut size,
            );
            if result != 0 || kind != REG_DWORD || value != 0 {
                out.push(Conflict {
                    trigger_prefix: prefix,
                    message: format!(
                        "Windows' {label} are on; set them to Nothing in Touchpad settings"
                    ),
                });
            }
        }
        RegCloseKey(key);
    }
    out
}
