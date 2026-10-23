// The Windows host entry (compiled into the Rust binary by build.rs).
//
// Responsibilities, mirroring the macOS shell process wiring:
//
//   1. bootstrap the core (config comes in via KITRON_CONFIG_JSON),
//   2. install the push event sink (NDJSON on stdout),
//   3. create the shell window — window 1, the RNW host — plus the app menu,
//   4. run the Win32 message pump, translating keyboard accelerators first
//      (`TranslateAcceleratorW` with the shell's table),
//   5. smoke mode (KITRON_HOST_SMOKE=1): drive window ops + a menu action
//      through real window messages on a timer, then exit — the CI probe.
//
// The RNW renderer attach (machine step) lands behind KITRON_ATTACH_REACT:
// Microsoft.ReactNative links here and renders into the window-1 HWND that
// `kitron_window_create` already answers on `windowPtr`.

#include "kitron.h"

#include <windows.h>

#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>

namespace {

/// Push deliver — every core event lands here (the drain API stays the
/// pull-fallback for workers).
void event_sink(uintptr_t /*context*/, const char *name, const char *payload) {
  std::printf("[event] %s %s\n", name, payload);
  std::fflush(stdout);
}

/// Calls one envelope-returning ABI function, forwards its JSON result to
/// stdout and dies loudly on failure — the host cannot degrade silently.
std::string envelope(char *raw, const char *what) {
  if (raw == nullptr) {
    std::fprintf(stderr, "[host] %s: null envelope\n", what);
    std::exit(2);
  }
  std::string json(raw);
  kitron_free_string(raw);
  std::printf("[host] %s → %s\n", what, json.c_str());
  if (json.rfind("{\"ok\":false", 0) == 0) {
    std::fprintf(stderr, "[host] %s failed: %s\n", what, json.c_str());
    std::exit(2);
  }
  return json; // ownership already handed back via kitron_free_string
}

constexpr int kTimerProbeAction = 1;   // posts the WM_COMMAND probe
constexpr int kTimerTeardown = 2;      // closes the window → app quits

HWND g_main_hwnd = nullptr;

/// First fire: exercise the window surface + menu through real messages.
/// Second fire: destroy the window; the shell sees the last window close
/// and pushes app.quit → WM_QUIT → the pump returns.
void CALLBACK smoke_timer(HWND /*hwnd*/, UINT /*message*/, UINT_PTR id,
                          DWORD /*elapsed*/) {
  switch (id) {
  case kTimerProbeAction: {
    // Delivered through the window's own pane-out (WM_COMMAND → wndproc →
    // menu registry → menu.action event). Command id 1 is the first
    // allocated action id in the default smoke menu.
    PostMessageW(g_main_hwnd, WM_COMMAND, MAKEWPARAM(1, 0), 0);
    envelope(kitron_window_op(1, "setTitle", R"({"title":"smoke"})"), "setTitle");
    envelope(kitron_window_op(1, "setSize", R"({"width":640,"height":480})"),
             "setSize");
    envelope(kitron_window_get(1), "windowGet");
    KillTimer(g_main_hwnd, kTimerProbeAction);
    SetTimer(g_main_hwnd, kTimerTeardown, 250, smoke_timer);
    break;
  }
  case kTimerTeardown: {
    KillTimer(g_main_hwnd, kTimerTeardown);
    envelope(kitron_window_op(1, "close", nullptr), "close");
    break;
  }
  default:
    break;
  }
}

/// The default smoke menu — exercised on the CI runner (gated, non-modal).
const char *kDefaultSmokeMenu =
    R"([{"label":"File","items":[{"label":"Ping","action":"host.ping",)"
    R"("accelerator":"CmdOrCtrl+P"},{"label":"Quit","action":"app.quit"}]}])";

} // namespace

// The Rust `main` entry point.
extern "C" int kitron_host_main(int /*argc*/, char ** /*argv*/) {
  const char *smoke = std::getenv("KITRON_HOST_SMOKE");

  // 0. Per-monitor-v2 DPI awareness — the shell reads physical pixels with
  //    GetDpiForWindow and hands JS logical points, so virtualized DPI would
  //    lie about the scale. Must be set before the first HWND exists; it
  //    fails harmlessly (E_ACCESSDENIED) when already declared elsewhere
  //    (e.g. the app manifest on the machine build).
  SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

  // 1. Core bootstrap — config snapshot arrives via KITRON_CONFIG_JSON (set
  //    by the CLI in dev; the probe/CI sets it itself).
  envelope(kitron_bootstrap(), "bootstrap");
  envelope(kitron_app_bits_json(), "appBits");

  // 2. Push events: sink first, then anything the bootstrap queued is
  //    flushed through it by the core (install-time flush contract).
  kitron_set_event_sink(0, event_sink);

  // 3. Window 1 — the RNW host window. Smoke keeps it tucked away and small.
  char window_args[128];
  const char *title = smoke != nullptr ? "Kitron smoke" : "Kitron";
  std::snprintf(window_args, sizeof window_args,
                R"({"title":"%s","width":1024,"height":768,"focus":false})",
                title);
  std::string create_result =
      envelope(kitron_window_create(window_args), "windowCreate");
  // windowPtr (the HWND address) rides inside the create envelope; the
  // dedicated hook below avoids parsing JSON in C++ for it.
  (void)create_result;

  g_main_hwnd = reinterpret_cast<HWND>(kitron_windows_renderer_hwnd());
  if (g_main_hwnd == nullptr) {
    std::fprintf(stderr, "[host] no renderer HWND after window create\n");
    return 3;
  }

  if (smoke != nullptr) {
    // 4. App menu with one accelerator — proves both the HMENU build and
    //    the table handed to TranslateAcceleratorW.
    envelope(kitron_menu_set_application_menu(kDefaultSmokeMenu), "menu");

    // Hide the window: the CI runner has no one to watch it; ops still run.
    envelope(kitron_window_op(1, "hide", nullptr), "hide");

    // 5. Timers drive the smoke scenario through the message pump (no
    //    sleeps in callbacks, no ambiguity about ordering).
    SetTimer(g_main_hwnd, kTimerProbeAction, 200, smoke_timer);
  }
#ifdef KITRON_ATTACH_REACT
  // Machine step: the RNW attach lands here and owns the content of
  // window 1 (renders into its HWND). See docs/shell-contracts.md §7.
  void attach_kitron_react(HWND hwnd);
  attach_kitron_react(g_main_hwnd);
#endif

  // 6. The pump. Accelerators are checked before every plain dispatch,
  //    exactly the Win32 discipline for keystroke shortcuts.
  MSG message{};
  while (GetMessageW(&message, nullptr, 0, 0) > 0) {
    bool handled = false;
    if (g_main_hwnd != nullptr) {
      const HACCEL accel = reinterpret_cast<HACCEL>(kitron_windows_accelerators());
      if (accel != nullptr) {
        handled = TranslateAcceleratorW(g_main_hwnd, accel, &message) != 0;
      }
    }
    if (!handled) {
      TranslateMessage(&message);
      DispatchMessageW(&message);
    }
  }

  std::printf("[host] pump exited cleanly\n");
  return 0;
}
