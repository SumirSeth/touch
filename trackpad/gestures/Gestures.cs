// Gesture recognisers. Each consumes touchpad frames and raises events; none of them touch the system directly,
// so they can be driven by synthetic frames in tests. A "session" lasts from first touch until every finger lifts.
using System;
using System.Collections.Generic;

interface IGesture {
    void OnFrame(Frame f);
    void Reset(); // abandon any gesture in progress (raising its end event if needed)
}

struct Vec { public double X, Y; public double Length { get { return Math.Sqrt(X * X + Y * Y); } } }

static class Geometry {
    public static Vec Centroid(List<Contact> c) {
        var v = new Vec();
        foreach (var p in c) { v.X += p.X; v.Y += p.Y; }
        v.X /= c.Count; v.Y /= c.Count;
        return v;
    }

    // Mean distance of the contacts from their centroid.
    public static double Spread(List<Contact> c) {
        var m = Centroid(c);
        double d = 0;
        foreach (var p in c) d += Distance(p.X, p.Y, m.X, m.Y);
        return d / c.Count;
    }

    // Rotation in degrees between two frames, about each frame's centroid; positive is clockwise (Y points down).
    // Weighted by each finger's distance from the centroid, since fingers near the centre give noisy angles.
    public static double Rotation(List<Contact> before, List<Contact> after) {
        var ca = Centroid(before); var cb = Centroid(after);
        double sum = 0, weight = 0;
        foreach (var p in after) foreach (var q in before) {
            if (p.Id != q.Id) continue;
            double r = Distance(p.X, p.Y, cb.X, cb.Y);
            double d = Math.Atan2(p.Y - cb.Y, p.X - cb.X) - Math.Atan2(q.Y - ca.Y, q.X - ca.X);
            while (d > Math.PI) d -= 2 * Math.PI;
            while (d <= -Math.PI) d += 2 * Math.PI;
            sum += d * r; weight += r;
        }
        return weight == 0 ? 0 : sum / weight * 180 / Math.PI;
    }

    public static double Distance(double x1, double y1, double x2, double y2) {
        return Math.Sqrt((x1 - x2) * (x1 - x2) + (y1 - y2) * (y1 - y2));
    }
}

// 5 fingers: pinch in/out, or twist clockwise/anticlockwise. Fires once per session.
class FiveFingerGestures : IGesture {
    const int Fingers = 5;
    const double PinchRatio = 0.685; // pinch in: spread shrinks to 68.5% of its widest; pinch out: grows to 1/0.685 of its tightest
    const double MinPinchMm = 7.2;   // and changes by at least this much, to ignore jitter
    const double TwistDegrees = 25;

    enum State { Idle, Tracking, Fired }
    State state = State.Idle;
    double maxSpread, minSpread, rotation; // fingers may settle before pinching, so measure from the extremes
    List<Contact> prev;
    public event Action PinchIn, PinchOut, TwistClockwise, TwistAnticlockwise;

    public void Reset() { state = State.Idle; }

    public void OnFrame(Frame f) {
        var t = f.Touching;
        if (t.Count == 0) { state = State.Idle; return; }
        if (state == State.Fired) return; // wait for every finger to lift before re-arming
        if (t.Count < Fingers) { state = State.Idle; return; }

        double spread = Geometry.Spread(t);
        if (state == State.Idle) { state = State.Tracking; maxSpread = minSpread = spread; rotation = 0; prev = t; return; }
        rotation += Geometry.Rotation(prev, t);
        prev = t;
        maxSpread = Math.Max(maxSpread, spread);
        minSpread = Math.Min(minSpread, spread);

        if (spread <= maxSpread * PinchRatio && maxSpread - spread >= MinPinchMm) Fire(PinchIn);
        else if (spread * PinchRatio >= minSpread && spread - minSpread >= MinPinchMm) Fire(PinchOut);
        else if (rotation >= TwistDegrees) Fire(TwistClockwise);
        else if (rotation <= -TwistDegrees) Fire(TwistAnticlockwise);
    }

    void Fire(Action gesture) {
        state = State.Fired;
        if (gesture != null) gesture();
    }
}

// 4-finger twist as a knob: emits a step per StepDegrees of rotation. Gives way to Windows' 4-finger swipes
// if the hand moves before it turns.
class FourFingerKnob : IGesture {
    const double StepDegrees = 12, MaxDriftMm = 10;

    enum State { Idle, Tracking, Turning, Off }
    State state = State.Idle;
    double rotation;
    Vec start;
    List<Contact> prev;
    public event Action<int> Turned; // +1 per clockwise step, -1 per anticlockwise step

    public void Reset() { state = State.Idle; }

    public void OnFrame(Frame f) {
        var t = f.Touching;
        if (t.Count == 0) { state = State.Idle; return; }
        if (state == State.Off) return;
        if (t.Count != 4) { if (state != State.Idle) state = State.Off; return; }

        if (state == State.Idle) { state = State.Tracking; start = Geometry.Centroid(t); rotation = 0; prev = t; return; }
        rotation += Geometry.Rotation(prev, t);
        prev = t;

        if (state == State.Tracking) {
            var c = Geometry.Centroid(t);
            if (Geometry.Distance(c.X, c.Y, start.X, start.Y) > MaxDriftMm) { state = State.Off; return; }
            if (Math.Abs(rotation) < StepDegrees) return;
            state = State.Turning;
        }
        while (rotation >= StepDegrees) { rotation -= StepDegrees; if (Turned != null) Turned(1); }
        while (rotation <= -StepDegrees) { rotation += StepDegrees; if (Turned != null) Turned(-1); }
    }
}

// Mac-style 3-finger drag: moving 3 fingers holds the left button and moves the pointer; lifting a finger drops.
class ThreeFingerDrag : IGesture {
    const double StartMm = 1.5;
    const double PixelsPerMm = 9;

    enum State { Idle, Pending, Dragging, Off }
    State state = State.Idle;
    Vec start, last;
    double carryX, carryY; // sub-pixel remainder
    public event Action DragStart, DragEnd;
    public event Action<int, int> DragMove; // pixels

    public void Reset() { End(); state = State.Idle; }

    public void OnFrame(Frame f) {
        var t = f.Touching;
        if (t.Count == 0) { End(); state = State.Idle; return; }
        if (state == State.Off) return;
        if (t.Count != 3) {
            if (state == State.Dragging || t.Count > 3) { End(); state = State.Off; }
            else state = State.Idle;
            return;
        }

        var c = Geometry.Centroid(t);
        if (state == State.Idle) { state = State.Pending; start = last = c; carryX = carryY = 0; return; }
        if (state == State.Pending) {
            if (Geometry.Distance(c.X, c.Y, start.X, start.Y) < StartMm) return;
            state = State.Dragging;
            if (DragStart != null) DragStart();
        }

        carryX += (c.X - last.X) * PixelsPerMm;
        carryY += (c.Y - last.Y) * PixelsPerMm;
        last = c;
        int dx = (int)carryX, dy = (int)carryY;
        carryX -= dx; carryY -= dy;
        if ((dx != 0 || dy != 0) && DragMove != null) DragMove(dx, dy);
    }

    void End() {
        if (state == State.Dragging && DragEnd != null) DragEnd();
    }
}

// 3-finger swipes, reported continuously so the result can track the fingers.
// Horizontal is replayed on a virtual touchpad. Windows' gesture engine ignores a touchpad whose fingers land after
// another touchpad's fingers have started moving, so the replay starts the moment three fingers land (ReplayStart)
// and mirrors the horizontal offset from the landing point on every frame (ReplayMove). If the swipe turns out to be
// vertical, the replay is cancelled (ReplayEnd(false)); otherwise it ends when the fingers lift (ReplayEnd(true)).
// While three fingers rest without a direction the swipe is also "armed", so the vertical preview can get ready.
// Vertical locks after EngageMm and measures from that point, so tracking starts at zero instead of jumping:
// progress (travel / FullMm) and velocity (progress per second over the last VelocityMs).
// Lifting a finger releases; a 4th finger cancels.
class ThreeFingerSwipes : IGesture {
    const double EngageMm = 4, HorizontalLockMm = 6;
    public const double FullMm = 35;
    const int VelocityMs = 80;

    enum State { Idle, Pending, Horizontal, Vertical, Off }
    State state = State.Idle;
    Vec start, landed;
    bool up;
    double progress;
    readonly List<KeyValuePair<int, double>> history = new List<KeyValuePair<int, double>>(); // (time, progress)

    public event Action Armed;                          // every frame while three fingers rest without a direction
    public event Action Disarmed;                       // armed ended without a vertical swipe
    public event Action ReplayStart;                    // three fingers landed
    public event Action<double> ReplayMove;             // horizontal offset from where they landed, mm
    public event Action<bool> ReplayEnd;                // true = fingers lifted, false = cancelled
    public event Action<int> HorizontalStart;          // the swipe locked horizontal: -1 = left, +1 = right
    public event Action<bool> VerticalStart;           // true = up
    public event Action<double, double> VerticalMove;   // progress, velocity
    public event Action<bool, double, double> VerticalEnd; // released?, progress, velocity

    // Whether a released vertical swipe should complete: a flick decides by its direction, otherwise distance does.
    public static bool ShouldCommit(double progress, double velocity) {
        if (progress < 0.04) return false;
        if (velocity >= 0.8) return true;
        if (velocity <= -0.8) return false;
        return progress >= 0.4;
    }

    public void Reset() { Finish(false); state = State.Idle; }

    public void OnFrame(Frame f) {
        var t = f.Touching;
        if (t.Count == 0) { Finish(true); state = State.Idle; return; }
        if (state == State.Off) return;
        if (t.Count != 3) {
            Finish(t.Count < 3);
            state = t.Count > 3 || state != State.Pending && state != State.Idle ? State.Off : State.Idle;
            return;
        }

        var c = Geometry.Centroid(t);
        if (state == State.Idle) {
            state = State.Pending; landed = c;
            if (ReplayStart != null) ReplayStart();
        }
        double lx = c.X - landed.X, ly = c.Y - landed.Y;

        if (state == State.Pending) {
            if (Math.Abs(ly) >= EngageMm && Math.Abs(ly) > Math.Abs(lx)) {
                if (ReplayEnd != null) ReplayEnd(false);
                state = State.Vertical;
                up = ly < 0; start = c;
                progress = 0; history.Clear();
                if (VerticalStart != null) VerticalStart(up);
            } else if (Math.Abs(lx) >= HorizontalLockMm && Math.Abs(lx) > Math.Abs(ly)) {
                state = State.Horizontal;
                if (Disarmed != null) Disarmed();
                if (HorizontalStart != null) HorizontalStart(lx < 0 ? -1 : 1);
            } else {
                if (ReplayMove != null) ReplayMove(lx);
                if (Armed != null) Armed();
                return;
            }
        }

        if (state == State.Horizontal) {
            if (ReplayMove != null) ReplayMove(lx);
        } else {
            double dy = c.Y - start.Y;
            progress = (up ? -dy : dy) / FullMm;
            history.Add(new KeyValuePair<int, double>(f.Time, progress));
            while (history.Count > 2 && f.Time - history[1].Key >= VelocityMs) history.RemoveAt(0);
            if (VerticalMove != null) VerticalMove(progress, Velocity());
        }
    }
    double Velocity() {
        if (history.Count < 2) return 0;
        var a = history[0]; var b = history[history.Count - 1];
        return b.Key > a.Key ? (b.Value - a.Value) * 1000.0 / (b.Key - a.Key) : 0;
    }

    void Finish(bool released) {
        if (state == State.Pending && Disarmed != null) Disarmed();
        if ((state == State.Pending || state == State.Horizontal) && ReplayEnd != null) ReplayEnd(released);
        if (state == State.Vertical && VerticalEnd != null) VerticalEnd(released, progress, Velocity());
    }
}

// One finger that lands on the right edge and slides vertically (brightness), on the top edge and slides
// horizontally (volume), or on the bottom edge and slides horizontally (seek, like a video player's progress bar).
// A second finger, moving into the pad, or pressing the button (click-and-drag) cancels it.
class EdgeSlides : IGesture {
    const double EdgeMm = 6, EngageMm = 3;
    const double BrightnessPercentPerMm = 1.5;
    const double VolumeStepMm = 4;
    const double SeekStepMm = 3;

    public enum Edge { None, Right, Top, Bottom }
    enum State { Idle, Candidate, Sliding, Off }
    State state = State.Idle;
    Edge edge;
    bool onRight, onTop, onBottom;
    double startX, startY, lastPos, carry;
    public event Action<Edge> SlideStart, SlideEnd;
    public event Action<int> Brightness; // percent; positive = brighter (sliding up)
    public event Action<int> Volume;     // steps; positive = louder (sliding right)
    public event Action<int> Seek;       // steps; positive = forward (sliding right)

    public void Reset() { End(); state = State.Idle; }

    public void OnFrame(Frame f) {
        var t = f.Touching;
        if (t.Count == 0) { End(); state = State.Idle; return; }
        if (state == State.Off) return;
        if (t.Count != 1 || f.Button) { End(); state = State.Off; return; }

        var p = t[0];
        if (state == State.Idle) {
            onRight = p.X >= f.Width - EdgeMm;
            onTop = p.Y <= EdgeMm;
            onBottom = p.Y >= f.Height - EdgeMm;
            if (!onRight && !onTop && !onBottom) { state = State.Off; return; }
            state = State.Candidate; startX = p.X; startY = p.Y;
            return;
        }

        if (state == State.Candidate) {
            double dx = p.X - startX, dy = p.Y - startY;
            if (onRight && Math.Abs(dy) >= EngageMm && Math.Abs(dy) > 2 * Math.Abs(dx)) { edge = Edge.Right; lastPos = startY; }
            else if ((onTop || onBottom) && Math.Abs(dx) >= EngageMm && Math.Abs(dx) > 2 * Math.Abs(dy)) { edge = onTop ? Edge.Top : Edge.Bottom; lastPos = startX; }
            else { if (Geometry.Distance(p.X, p.Y, startX, startY) >= 2 * EngageMm) state = State.Off; return; }
            state = State.Sliding; carry = 0;
            if (SlideStart != null) SlideStart(edge);
        }

        if (edge == Edge.Right) {
            carry += (lastPos - p.Y) * BrightnessPercentPerMm; // up is brighter
            lastPos = p.Y;
            int step = (int)carry; carry -= step;
            if (step != 0 && Brightness != null) Brightness(step);
        } else {
            carry += (p.X - lastPos) / (edge == Edge.Top ? VolumeStepMm : SeekStepMm);
            lastPos = p.X;
            int step = (int)carry; carry -= step;
            var handler = edge == Edge.Top ? Volume : Seek;
            if (step != 0 && handler != null) handler(step);
        }
    }

    void End() {
        if (state == State.Sliding && SlideEnd != null) SlideEnd(edge);
    }
}

// One finger resting still in a top corner for HoldMs. A hold rather than a tap, because Windows turns taps into clicks.
class CornerHolds : IGesture {
    const double CornerMm = 12, MaxMoveMm = 1.5;
    const int HoldMs = 450;

    enum State { Idle, Holding, Done }
    State state = State.Idle;
    bool left;
    double startX, startY;
    int startTime;
    public event Action TopLeft, TopRight;

    public void Reset() { state = State.Idle; }

    public void OnFrame(Frame f) {
        var t = f.Touching;
        if (t.Count == 0) { state = State.Idle; return; }
        if (state == State.Done) return;
        if (t.Count != 1) { state = State.Done; return; }

        var p = t[0];
        if (state == State.Idle) {
            bool top = p.Y <= CornerMm;
            left = p.X <= CornerMm;
            bool right = p.X >= f.Width - CornerMm;
            if (!top || !(left || right)) { state = State.Done; return; }
            state = State.Holding; startX = p.X; startY = p.Y; startTime = f.Time;
            return;
        }

        if (Geometry.Distance(p.X, p.Y, startX, startY) > MaxMoveMm) { state = State.Done; return; }
        if (f.Time - startTime < HoldMs) return;
        state = State.Done;
        var e = left ? TopLeft : TopRight;
        if (e != null) e();
    }
}
