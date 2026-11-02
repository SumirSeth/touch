# GPUI provenance and local patch

This directory is the unmodified source distribution of crates.io gpui 0.2.2,
except for the two changes below.

`src/platform/windows/platform.rs`, `close_one_window`, has the final
expression `false` instead of `lock.is_empty()`.

That suppresses automatic WM_QUIT when the last settings window is destroyed.
Explicit `App::quit` still posts WM_QUIT. No hidden GPUI window is retained.
The release API has no QuitMode setting. Keep this patch until upgrading to
an upstream release that exposes explicit lifetime management.

Upstream distribution VCS: 69e2130295c2649963eb639fc70b4f2ee8ea1624
(the published `.cargo_vcs_info.json` marks the distribution dirty).
License: LICENSE-APACHE. Source: https://crates.io/crates/gpui/0.2.2

`build.rs` additionally searches installed Windows SDK x64 directories for
fxc.exe when the upstream PATH/26100-only lookup fails. Tested with SDK 22000.

The `examples/` directory and its `[[example]]` manifest entries were removed to keep the repository small; they aren't needed to build.

The vsync thread (`begin_vsync_thread` in `src/platform/windows/platform.rs`) parks while no window is open and is
unparked by `open_window`. Upstream wakes it at the display refresh rate forever, which kept the tray-only app
using CPU.
