# Drives each gesture recogniser in TrackpadGestures.exe with synthetic frames and checks the events it raises.
# The recognisers only raise events, so this sends no input to the system. Build first with build.ps1.
$asm = [Reflection.Assembly]::Load([IO.File]::ReadAllBytes((Join-Path $PSScriptRoot 'TrackpadGestures.exe')))
$contactT = $asm.GetType('Contact'); $frameT = $asm.GetType('Frame')
$listT = [System.Collections.Generic.List``1].MakeGenericType($contactT)
$W = 126.9; $H = 79.3
$script:failures = 0

function Contact($id, $x, $y) {
    $c = [Activator]::CreateInstance($contactT)
    foreach ($kv in @{ Id = [uint32]$id; Tip = $true; X = [double]$x; Y = [double]$y }.GetEnumerator()) { $contactT.GetField($kv.Key).SetValue($c, $kv.Value) }
    $c
}

# n fingers evenly around (cx, cy) at radius r, rotated by deg (clockwise, since Y points down).
function Hand($n, $r, $deg = 0, $cx = 60, $cy = 40) {
    $list = @()
    for ($i = 0; $i -lt $n; $i++) { $a = 2 * [Math]::PI * $i / $n + $deg * [Math]::PI / 180; $list += Contact $i ($cx + $r * [Math]::Cos($a)) ($cy + $r * [Math]::Sin($a)) }
    , $list
}

function Frame($contacts, $time = 0) {
    $l = [Activator]::CreateInstance($listT); foreach ($c in $contacts) { $l.Add($c) }
    $f = [Activator]::CreateInstance($frameT)
    $frameT.GetField('Touching').SetValue($f, $l); $frameT.GetField('Width').SetValue($f, $W)
    $frameT.GetField('Height').SetValue($f, $H); $frameT.GetField('Time').SetValue($f, [int]$time)
    , $f
}

# Feeds frames to a fresh recogniser and returns its events as "Name" or "Name(arg)", with numeric args summed per event.
function Run($typeName, $frames) {
    $t = $asm.GetType($typeName); $g = [Activator]::CreateInstance($t, $true)
    $script:events = New-Object System.Collections.Generic.List[string]
    foreach ($e in $t.GetEvents()) {
        $name = $e.Name
        $sb = [scriptblock]::Create("`$script:events.Add('$name' + `$(if (`$args.Count) { '(' + (`$args -join ',') + ')' }))")
        $e.AddEventHandler($g, [System.Management.Automation.LanguagePrimitives]::ConvertTo($sb, $e.EventHandlerType))
    }
    foreach ($f in $frames) { $t.GetMethod('OnFrame').Invoke($g, @(, $f)) | Out-Null }
    # Collapse runs of the same numeric event and direction into a total, e.g. Turned(1) x4 -> Turned(+4).
    $out = @(); $last = $null; $sum = 0
    foreach ($ev in $script:events) {
        if ($ev -match '^(\w+)\((-?\d+)\)$' -and $Matches[1] -eq $last -and [Math]::Sign([int]$Matches[2]) -eq [Math]::Sign($sum)) { $sum += [int]$Matches[2]; $out[-1] = "$last($('{0:+0;-0}' -f $sum))"; continue }
        if ($ev -match '^(\w+)\((-?\d+)\)$') { $last = $Matches[1]; $sum = [int]$Matches[2]; $out += "$last($('{0:+0;-0}' -f $sum))" }
        else { $last = $null; $out += $ev }
    }
    $out -join ' '
}

function Check($name, $actual, $expected) {
    $ok = $actual -like $expected
    if (-not $ok) { $script:failures++ }
    "{0} {1,-52} {2}" -f $(if ($ok) { 'PASS' } else { 'FAIL' }), $name, $(if ($ok) { $actual } else { "got [$actual], expected [$expected]" })
}

$lift = Frame @()

"--- 5 fingers"
Check 'pinch in 30 -> 20mm'                (Run FiveFingerGestures (30..20 | % { Frame (Hand 5 $_) })) 'PinchIn'
Check 'pinch in 30 -> 21mm (too small)'    (Run FiveFingerGestures (30..21 | % { Frame (Hand 5 $_) })) ''
Check 'pinch out 15 -> 23mm'               (Run FiveFingerGestures (15..23 | % { Frame (Hand 5 $_) })) 'PinchOut'
Check 'twist clockwise 30 deg'             (Run FiveFingerGestures (0..30 | % { Frame (Hand 5 25 $_) })) 'TwistClockwise'
Check 'twist anticlockwise 30 deg'         (Run FiveFingerGestures (0..30 | % { Frame (Hand 5 25 (-$_)) })) 'TwistAnticlockwise'
Check 'twist 20 deg (too small)'           (Run FiveFingerGestures (0..20 | % { Frame (Hand 5 25 $_) })) ''
Check 'fires once until lift, then again'  (Run FiveFingerGestures (@(30..10 | % { Frame (Hand 5 $_) }) + $lift + (0..30 | % { Frame (Hand 5 25 $_) }))) 'PinchIn TwistClockwise'
Check '4 fingers pinch (ignored)'          (Run FiveFingerGestures (30..10 | % { Frame (Hand 4 $_) })) ''

"--- 4-finger knob"
Check 'clockwise 50 deg = 4 steps'         (Run FourFingerKnob (0..50 | % { Frame (Hand 4 20 $_) })) 'Turned(+4)'
Check 'anticlockwise 40 deg = 3 steps'     (Run FourFingerKnob (0..40 | % { Frame (Hand 4 20 (-$_)) })) 'Turned(-3)'
Check 'there and back'                     (Run FourFingerKnob (@(0..40 | % { Frame (Hand 4 20 $_) }) + (40..-2 | % { Frame (Hand 4 20 $_) }))) 'Turned(+3) Turned(-3)'
Check 'swipe first (left to Windows)'      (Run FourFingerKnob (@(0..15 | % { Frame (Hand 4 20 0 (60 + $_)) }) + (0..40 | % { Frame (Hand 4 20 $_ 75) }))) ''
Check 'small wobble (no steps)'            (Run FourFingerKnob (0..10 | % { Frame (Hand 4 20 $_) })) ''

"--- 3-finger drag"
Check 'drag 10mm right'                    (Run ThreeFingerDrag (@(0..10 | % { Frame (Hand 3 10 0 (60 + $_)) }) + $lift)) 'DragStart DragMove(*) DragEnd'
Check 'tap without moving (no drag)'       (Run ThreeFingerDrag (@(1..5 | % { Frame (Hand 3 10) }) + $lift)) ''
Check 'lifting a finger ends the drag'     (Run ThreeFingerDrag (@(0..10 | % { Frame (Hand 3 10 0 (60 + $_)) }) + (Frame (Hand 2 10)) + (Frame (Hand 3 10)))) 'DragStart DragMove(*) DragEnd'
Check '4 fingers (ignored)'                (Run ThreeFingerDrag (0..10 | % { Frame (Hand 4 10 0 (60 + $_)) })) ''

"--- 3-finger swipes"
# Hand 3 r deg cx cy: centroid at (cx, cy). Up = decreasing y.
Check 'left swipe, lift'                   (Run ThreeFingerSwipes (@(0..20 | % { Frame (Hand 3 10 0 (60 - $_)) }) + $lift)) 'ReplayStart ReplayMove(+0) Armed ReplayMove(-1) Armed * ReplayMove(-5) Armed Disarmed HorizontalStart(-1) ReplayMove(*) ReplayEnd(True)'
Check 'right swipe, 4th finger cancels'    (Run ThreeFingerSwipes (@(0..20 | % { Frame (Hand 3 10 0 (60 + $_)) }) + (Frame (Hand 4 10 0 80)))) 'ReplayStart * Disarmed HorizontalStart(+1) ReplayMove(*) ReplayEnd(False)'
Check 'up swipe, progress starts at 0'     (Run ThreeFingerSwipes (@(0..35 | % { Frame (Hand 3 10 0 60 (60 - $_)) ($_ * 8) }) + $lift)) 'ReplayStart ReplayMove(+0) Armed * ReplayEnd(False) VerticalStart(True) VerticalMove(0,0) * VerticalEnd(True,0.88*,3.57*)'
Check 'down swipe, finger lifts first'     (Run ThreeFingerSwipes (@(0..10 | % { Frame (Hand 3 10 0 60 (20 + $_)) ($_ * 8) }) + (Frame (Hand 2 10 0 60 30) 88))) 'ReplayStart * ReplayEnd(False) VerticalStart(False) VerticalMove(*) VerticalEnd(True,0.14*,*)'
Check 'up then back before lifting'        (Run ThreeFingerSwipes (@(0..24 | % { Frame (Hand 3 10 0 60 (60 - $_)) ($_ * 8) }) + (1..8 | % { Frame (Hand 3 10 0 60 (36 + $_)) (192 + $_ * 8) }) + $lift)) '* VerticalEnd(True,0.34*,-2.14*)'
Check 'small wobble then up = vertical'    (Run ThreeFingerSwipes (@(Frame (Hand 3 10 0 60 60) 0) + (Frame (Hand 3 10 0 61.5 60) 8) + (1..20 | % { Frame (Hand 3 10 0 61.5 (60 - $_)) (8 + $_ * 8) }) + $lift)) 'ReplayStart * ReplayMove(1.5) Armed ReplayEnd(False) VerticalStart(True) VerticalMove(0,0) * VerticalEnd(True,0.45*'
Check 'still fingers (no swipe)'           (Run ThreeFingerSwipes (@(1..10 | % { Frame (Hand 3 10) }) + $lift)) 'ReplayStart * Armed Disarmed ReplayEnd(True)'
Check 'two fingers (nothing)'              (Run ThreeFingerSwipes (@(0..20 | % { Frame (Hand 2 10 0 60 (60 - $_)) }) + $lift)) ''

"--- swipe commit rule"
function Commit($p, $v) { "$($asm.GetType('ThreeFingerSwipes').GetMethod('ShouldCommit').Invoke($null, @([double]$p, [double]$v)))" }
Check 'past 40%, resting'                  (Commit 0.5 0) 'True'
Check 'short of 40%, resting'              (Commit 0.3 0) 'False'
Check 'short flick forwards'               (Commit 0.15 1.2) 'True'
Check 'far, but moving back'               (Commit 0.7 -1) 'False'
Check 'barely moved, fast'                 (Commit 0.02 3) 'False'
"--- edge slides"
Check 'right edge up 30mm = brighter'      (Run EdgeSlides (@(60..30 | % { Frame @(Contact 0 ($W - 3) $_) }) + $lift)) 'SlideStart(Right) Brightness(+45) SlideEnd(Right)'
Check 'right edge down 20mm = dimmer'      (Run EdgeSlides (@(30..50 | % { Frame @(Contact 0 ($W - 3) $_) }) + $lift)) 'SlideStart(Right) Brightness(-30) SlideEnd(Right)'
Check 'top edge right 20mm = 5 vol steps'  (Run EdgeSlides (@(40..60 | % { Frame @(Contact 0 $_ 3) }) + $lift)) 'SlideStart(Top) Volume(+5) SlideEnd(Top)'
Check 'bottom edge left 15mm = 5 seeks back' (Run EdgeSlides (@(60..45 | % { Frame @(Contact 0 $_ ($H - 3)) }) + $lift)) 'SlideStart(Bottom) Seek(-5) SlideEnd(Bottom)'
Check 'middle of pad (ignored)'            (Run EdgeSlides (40..60 | % { Frame @(Contact 0 $_ 40) })) ''
Check 'right edge moving inward (ignored)' (Run EdgeSlides (0..20 | % { Frame @(Contact 0 ($W - 3 - $_) 40) })) ''
Check 'second finger cancels'              (Run EdgeSlides (@(60..40 | % { Frame @(Contact 0 ($W - 3) $_) }) + (Frame @((Contact 0 ($W - 3) 40), (Contact 1 50 50))))) 'SlideStart(Right) Brightness(+*) SlideEnd(Right)'

"--- corner holds"
Check 'top-left hold 500ms'                (Run CornerHolds (0..10 | % { Frame @(Contact 0 5 5) ($_ * 50) })) 'TopLeft'
Check 'top-right hold 500ms'               (Run CornerHolds (0..10 | % { Frame @(Contact 0 ($W - 5) 5) ($_ * 50) })) 'TopRight'
Check 'short hold 300ms (ignored)'         (Run CornerHolds (0..6 | % { Frame @(Contact 0 5 5) ($_ * 50) })) ''
Check 'moving in corner (ignored)'         (Run CornerHolds (0..10 | % { Frame @(Contact 0 (5 + $_) 5) ($_ * 50) })) ''
Check 'bottom corner (ignored)'            (Run CornerHolds (0..10 | % { Frame @(Contact 0 5 ($H - 5)) ($_ * 50) })) ''

""
if ($script:failures) { "$($script:failures) FAILED"; exit 1 } else { "All passed." }
