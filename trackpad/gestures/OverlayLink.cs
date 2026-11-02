using System;
using System.Diagnostics;
using System.IO;
using System.IO.MemoryMappedFiles;
using System.Threading;

// Writer side of trackpad/overlay/PROTOCOL.md: publishes vertical 3-finger swipe progress to TrackpadOverlay.exe.
class OverlayLink : IDisposable {
    public const uint GestureTaskView = 1, GestureShowDesktop = 2, GestureRestoreWindows = 3;
    public const uint PhaseIdle = 0, PhaseTracking = 1, PhaseCommit = 2, PhaseCancel = 3, PhaseArmed = 4;
    const uint Magic = 0x32505754; // "TWP2"
    const string RunningMutex = @"Local\TrackpadOverlay.Running";

    readonly MemoryMappedFile map;
    readonly MemoryMappedViewAccessor view;
    readonly EventWaitHandle wake;
    uint seq, session;
    bool armed;
    Process launched;

    public OverlayLink() {
        map = MemoryMappedFile.CreateOrOpen(@"Local\TrackpadGestures.Overlay", 4096);
        view = map.CreateViewAccessor(0, 4096);
        wake = new EventWaitHandle(false, EventResetMode.AutoReset, @"Local\TrackpadGestures.OverlayWake");
        seq = view.ReadUInt32(4) & ~1u;
        session = view.ReadUInt32(32);
        Write(0, PhaseIdle, 0, 0);
        view.Write(0, Magic);
    }

    public static bool OverlayRunning() {
        Mutex m;
        if (!Mutex.TryOpenExisting(RunningMutex, out m)) return false;
        m.Dispose();
        return true;
    }

    // Starts trackpad\overlay\bin\TrackpadOverlay.exe (relative to this exe's folder) if it exists and isn't running.
    public void LaunchOverlay() {
        if (OverlayRunning()) return;
        string exe = Path.GetFullPath(Path.Combine(AppDomain.CurrentDomain.BaseDirectory, @"..\overlay\bin\TrackpadOverlay.exe"));
        if (File.Exists(exe)) launched = Process.Start(new ProcessStartInfo(exe) { UseShellExecute = false });
    }

    // Three fingers are resting: the overlay prepares silently. Called every frame to keep the watchdog fed.
    public void Arm() {
        if (!armed) { session++; armed = true; }
        Write(0, PhaseArmed, 0, 0);
    }
    public void Disarm() { if (armed) { armed = false; Write(0, PhaseIdle, 0, 0); } }

    public void Begin(uint gesture) {
        if (!armed) session++; // otherwise tracking continues the armed session, using its preparation
        armed = false;
        Write(gesture, PhaseTracking, 0, 0);
    }
    public void Update(uint gesture, float progress, float velocity) { Write(gesture, PhaseTracking, progress, velocity); }
    public void Finish(uint gesture, bool commit, float progress, float velocity) {
        Write(gesture, commit ? PhaseCommit : PhaseCancel, progress, velocity);
    }

    // Seqlock write: seq is odd while fields change, even once they're stable.
    void Write(uint gesture, uint phase, float progress, float velocity) {
        view.Write(4, ++seq);
        Thread.MemoryBarrier();
        view.Write(8, gesture);
        view.Write(12, phase);
        view.Write(16, progress);
        view.Write(20, velocity);
        view.Write(24, Stopwatch.GetTimestamp());
        view.Write(32, session);
        Thread.MemoryBarrier();
        view.Write(4, ++seq);
        wake.Set();
    }

    public void Dispose() {
        if (launched != null && !launched.HasExited) { try { launched.Kill(); } catch (InvalidOperationException) { } }
        view.Dispose(); map.Dispose(); wake.Dispose();
    }
}
