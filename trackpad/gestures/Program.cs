// Custom Precision Touchpad gestures on top of Windows' own handling:
//   5-finger pinch in or out       -> Win+Tab (Task View)
//   5-finger twist clockwise       -> Ctrl+Shift+T (reopen closed tab)
//   5-finger twist anticlockwise   -> Ctrl+W (close tab)
//   4-finger twist                 -> volume knob
//   3-finger swipe left/right      -> switch desktop, via Windows' own finger-tracked animation (virtual touchpad).
//                                     Only when the fingers land one or two at a time: see
//                                     trackpad/research/05-three-finger-replay.md
//   3-finger swipe up/down         -> Task View / show desktop (or restore windows / close Task View, depending on
//                                     what's showing), finger-tracked by TrackpadOverlay.exe if it's running,
//                                     otherwise Win+Tab / Win+D / Escape on release
//   3-finger drag                  -> drag with the left button held (off by default; replaces 3-finger swipes)
//   (3-finger features need Windows' own 3-finger swipes set to Nothing)
//   1 finger slide on right edge   -> brightness
//   1 finger slide on top edge     -> volume
//   1 finger slide on bottom edge  -> seek (Left/Right arrow keys: 5 s per 3 mm on YouTube)
//   1 finger hold in top-left      -> play/pause
//   1 finger hold in top-right     -> mute
// Build: build.ps1. Run with --log to log gestures to TrackpadGestures.log (--console to also show them).
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;
using Microsoft.Win32;

static class Program {
    const ushort VK_LWIN = 0x5B, VK_TAB = 0x09, VK_CONTROL = 0x11, VK_SHIFT = 0x10, VK_T = 0x54, VK_W = 0x57, VK_D = 0x44, VK_ESCAPE = 0x1B, VK_LEFT = 0x25, VK_RIGHT = 0x27;
    const ushort VK_VOLUME_MUTE = 0xAD, VK_VOLUME_DOWN = 0xAE, VK_VOLUME_UP = 0xAF, VK_MEDIA_PLAY_PAUSE = 0xB3;
    const string SettingsKey = @"Software\TrackpadGestures";
    const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    const string AppName = "TrackpadGestures";

    static bool log;

    class Feature {
        public string Name, Setting; public bool Default; public IGesture Gesture; public bool Enabled;
    }

    [STAThread]
    static void Main(string[] args) {
        log = Array.IndexOf(args, "--log") >= 0;
        if (Array.IndexOf(args, "--console") >= 0) { log = true; Native.AllocConsole(); } // windowless app: a console must be made
        bool created;
        using (new System.Threading.Mutex(true, "TrackpadGestures.SingleInstance", out created)) {
            if (!created) return;

            var five = new FiveFingerGestures();
            five.PinchIn += () => Do("5-finger pinch in: Win+Tab", VK_LWIN, VK_TAB);
            five.PinchOut += () => Do("5-finger pinch out: Win+Tab", VK_LWIN, VK_TAB);
            five.TwistClockwise += () => Do("5-finger twist clockwise: Ctrl+Shift+T", VK_CONTROL, VK_SHIFT, VK_T);
            five.TwistAnticlockwise += () => Do("5-finger twist anticlockwise: Ctrl+W", VK_CONTROL, VK_W);

            var knob = new FourFingerKnob();
            knob.Turned += dir => Do("4-finger twist: volume " + (dir > 0 ? "up" : "down"), dir > 0 ? VK_VOLUME_UP : VK_VOLUME_DOWN);

            var drag = new ThreeFingerDrag();
            drag.DragStart += () => { Log("3-finger drag start"); Native.SendLeftButton(true); };
            drag.DragMove += (dx, dy) => Native.SendMouseMove(dx, dy);
            drag.DragEnd += () => { Log("3-finger drag end"); Native.SendLeftButton(false); };

            var edges = new EdgeSlides();
            edges.SlideStart += edge => {
                Log("edge slide start: " + edge);
                if (edge == EdgeSlides.Edge.Right) Brightness.Resync();
                FreezeCursor(); // keep the pointer still while the finger slides along the edge
            };
            edges.SlideEnd += edge => { Log("edge slide end: " + edge); Native.UnclipCursor(IntPtr.Zero); };
            edges.Brightness += delta => { Log("brightness " + delta.ToString("+0;-0")); Brightness.Adjust(delta); };
            edges.Volume += steps => {
                Log("volume " + steps.ToString("+0;-0"));
                for (int i = 0; i < Math.Abs(steps); i++) Native.SendChord(steps > 0 ? VK_VOLUME_UP : VK_VOLUME_DOWN);
            };

            edges.Seek += steps => {
                Log("seek " + steps.ToString("+0;-0"));
                for (int i = 0; i < Math.Abs(steps); i++) Native.SendChord(steps > 0 ? VK_RIGHT : VK_LEFT);
            };

            var corners = new CornerHolds();
            corners.TopLeft += () => Do("top-left hold: play/pause", VK_MEDIA_PLAY_PAUSE);
            corners.TopRight += () => Do("top-right hold: mute", VK_VOLUME_MUTE);

            double padWidth = 0, padHeight = 0; // from the latest frame; sizes the virtual touchpad
            VirtualTouchpad vpad = null;
            var overlay = new OverlayLink();
            overlay.LaunchOverlay();

            var swipes = new ThreeFingerSwipes();
            swipes.ReplayStart += () => {
                try {
                    if (vpad == null) vpad = new VirtualTouchpad(padWidth, padHeight);
                    vpad.Begin(0, 0); // centred: the direction isn't known yet
                } catch (InvalidOperationException e) { Log(e.Message); }
            };
            swipes.ReplayMove += dx => { if (vpad != null) vpad.MoveTo(dx, 0); }; // stay on the horizontal rail
            swipes.ReplayEnd += lifted => { if (vpad != null) { if (lifted) vpad.End(); else vpad.Cancel(); } };
            swipes.HorizontalStart += dir => Log("3-finger swipe " + (dir < 0 ? "left" : "right") + ": native desktop switch");
            // Vertical swipes depend on what's on screen, like Windows' own: up opens Task View, or brings the
            // windows back if we showed the desktop; down shows the desktop, or closes Task View if it's open.
            const uint CloseTaskView = 100, Nothing = 0;
            uint vertical = Nothing;
            bool useOverlay = false, desktopShown = false;
            swipes.Armed += () => { if (OverlayLink.OverlayRunning()) overlay.Arm(); };
            swipes.Disarmed += overlay.Disarm;
            swipes.VerticalStart += up => {
                string fg = Native.ForegroundClass();
                bool onDesktop = fg == "Progman" || fg == "WorkerW";
                if (!onDesktop) desktopShown = false; // the user went back to a window
                if (fg == "XamlExplorerHostIslandWindow") vertical = up ? Nothing : CloseTaskView;
                else if (desktopShown) vertical = up ? OverlayLink.GestureRestoreWindows : Nothing;
                else vertical = up ? OverlayLink.GestureTaskView : OverlayLink.GestureShowDesktop;
                useOverlay = vertical != Nothing && vertical != CloseTaskView && OverlayLink.OverlayRunning();
                Log("3-finger swipe " + (up ? "up" : "down") + " (foreground " + fg + "): " + VerticalName(vertical) + (useOverlay ? " (overlay)" : ""));
                if (useOverlay) overlay.Begin(vertical); else overlay.Disarm();
            };
            swipes.VerticalMove += (p, v) => { if (useOverlay) overlay.Update(vertical, (float)p, (float)v); };
            swipes.VerticalEnd += (released, p, v) => {
                bool commit = released && vertical != Nothing && ThreeFingerSwipes.ShouldCommit(p, v);
                Log(string.Format("3-finger swipe {0} at progress {1:F2}, velocity {2:F2}/s: {3}",
                    released ? "released" : "abandoned", p, v, commit ? "commit" : "cancel"));
                if (vertical == OverlayLink.GestureShowDesktop && commit) desktopShown = true;
                if (vertical == OverlayLink.GestureRestoreWindows && commit) desktopShown = false;
                if (useOverlay) { overlay.Finish(vertical, commit, (float)p, (float)v); return; }
                if (!commit) return;
                if (vertical == CloseTaskView) Native.SendChord(VK_ESCAPE);
                else if (vertical == OverlayLink.GestureTaskView) Native.SendChord(VK_LWIN, VK_TAB);
                else Native.SendChord(VK_LWIN, VK_D); // show desktop and restore are the same toggle
            };
            var features = new List<Feature> {
                new Feature { Name = "5-finger pinch && twist", Setting = "FiveFinger", Default = true, Gesture = five },
                new Feature { Name = "4-finger twist volume knob", Setting = "VolumeKnob", Default = true, Gesture = knob },
                new Feature { Name = "3-finger swipes (tracked desktops, Task View, show desktop)", Setting = "ThreeFingerSwipes", Default = true, Gesture = swipes },
                new Feature { Name = "3-finger drag", Setting = "ThreeFingerDrag", Default = false, Gesture = drag },
                new Feature { Name = "Edge slides (brightness, volume, seek)", Setting = "EdgeSlides", Default = true, Gesture = edges },
                new Feature { Name = "Corner holds (play/pause, mute)", Setting = "CornerHolds", Default = true, Gesture = corners },
            };
            foreach (var f in features) f.Enabled = ReadSetting(f.Setting, f.Default);
            if (features[2].Enabled && features[3].Enabled) features[3].Enabled = false; // swipes and drag both use 3 fingers

            var sink = new TouchpadSink();
            sink.FrameReady += frame => {
                padWidth = frame.Width; padHeight = frame.Height;
                foreach (var f in features) if (f.Enabled) f.Gesture.OnFrame(frame);
            };

            var tray = new NotifyIcon { Icon = SystemIcons.Application, Text = "Trackpad gestures", Visible = true };
            tray.ContextMenuStrip = BuildMenu(features, tray);
            if (features[2].Enabled && WindowsThreeFingerSwipesOn())
                tray.ShowBalloonTip(10000, "Trackpad gestures", WindowsSwipesWarning, ToolTipIcon.Warning);

            Application.Run();
            foreach (var f in features) f.Gesture.Reset();
            if (vpad != null) vpad.Dispose();
            overlay.Dispose();
            tray.Visible = false;
            sink.DestroyHandle();
        }
    }

    const string WindowsSwipesWarning =
        "Windows' own 3-finger swipes look enabled, so both will act. Set Settings > Bluetooth & devices > Touchpad > " +
        "Three-finger gestures > Swipes to \"Nothing\". Keep four-finger swipes on \"Switch desktops\".";

    // Heuristic: 0 appears to mean "Nothing" (65535 is a custom mapping); not documented.
    static bool WindowsThreeFingerSwipesOn() {
        using (var k = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad")) {
            var v = k == null ? null : k.GetValue("ThreeFingerSlideEnabled");
            return v == null || (int)v != 0;
        }
    }

    static ContextMenuStrip BuildMenu(List<Feature> features, NotifyIcon tray) {
        var menu = new ContextMenuStrip();
        var items = new Dictionary<IGesture, ToolStripMenuItem>();
        foreach (var feature in features) {
            var f = feature;
            var item = new ToolStripMenuItem(f.Name) { Checked = f.Enabled, CheckOnClick = true };
            items[f.Gesture] = item;
            item.CheckedChanged += (s, e) => {
                f.Enabled = item.Checked;
                if (!f.Enabled) f.Gesture.Reset();
                WriteSetting(f.Setting, f.Enabled);
                if (!f.Enabled) return;
                // 3-finger swipes and 3-finger drag both claim 3 fingers; enabling one turns the other off.
                foreach (var other in features) {
                    bool clash = (f.Gesture is ThreeFingerDrag && other.Gesture is ThreeFingerSwipes) ||
                                 (f.Gesture is ThreeFingerSwipes && other.Gesture is ThreeFingerDrag);
                    if (clash) items[other.Gesture].Checked = false;
                }
                if ((f.Gesture is ThreeFingerDrag || f.Gesture is ThreeFingerSwipes) && WindowsThreeFingerSwipesOn())
                    tray.ShowBalloonTip(10000, f.Name, WindowsSwipesWarning, ToolTipIcon.Info);
            };
            menu.Items.Add(item);
        }
        menu.Items.Add(new ToolStripSeparator());
        var startup = new ToolStripMenuItem("Start with Windows") { Checked = AutostartEnabled(), CheckOnClick = true };
        startup.CheckedChanged += (s, e) => SetAutostart(startup.Checked);
        menu.Items.Add(startup);
        menu.Items.Add("Quit", null, (s, e) => Application.Exit());
        return menu;
    }

    static void Do(string what, params ushort[] keys) {
        Log(what);
        Native.SendChord(keys);
    }

    static string VerticalName(uint g) {
        switch (g) {
            case OverlayLink.GestureTaskView: return "Task View";
            case OverlayLink.GestureShowDesktop: return "show desktop";
            case OverlayLink.GestureRestoreWindows: return "restore windows";
            case 100: return "close Task View";
            default: return "nothing";
        }
    }

    // --log appends to TrackpadGestures.log next to the exe; --console also prints to a console window.
    internal static void Log(string what) {
        if (!log) return;
        string line = string.Format("{0:HH:mm:ss.fff} {1}", DateTime.Now, what);
        Console.WriteLine(line);
        try { System.IO.File.AppendAllText(System.IO.Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "TrackpadGestures.log"), line + Environment.NewLine); }
        catch (Exception) { } // logging must never break a gesture
    }

    static void FreezeCursor() {
        Native.POINT p;
        Native.GetCursorPos(out p);
        var r = new Native.RECT { Left = p.X, Top = p.Y, Right = p.X + 1, Bottom = p.Y + 1 };
        Native.ClipCursor(ref r);
    }

    static bool ReadSetting(string name, bool fallback) {
        using (var k = Registry.CurrentUser.OpenSubKey(SettingsKey)) {
            var v = k == null ? null : k.GetValue(name);
            return v == null ? fallback : (int)v != 0;
        }
    }

    static void WriteSetting(string name, bool value) {
        using (var k = Registry.CurrentUser.CreateSubKey(SettingsKey)) k.SetValue(name, value ? 1 : 0, RegistryValueKind.DWord);
    }

    static bool AutostartEnabled() {
        using (var k = Registry.CurrentUser.OpenSubKey(RunKey)) return k != null && k.GetValue(AppName) != null;
    }

    static void SetAutostart(bool on) {
        using (var k = Registry.CurrentUser.CreateSubKey(RunKey)) {
            if (on) k.SetValue(AppName, "\"" + Application.ExecutablePath + "\"");
            else k.DeleteValue(AppName, false);
        }
    }
}
