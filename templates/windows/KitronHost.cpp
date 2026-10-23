// KitronHost.cpp — the React Native for Windows attach (machine build only).
//
// CI never compiles this file: Microsoft.ReactNative is a NuGet dependency
// restored on a Windows machine (templates/windows/README.md, machine-step
// checklist). The glue keeps ONE shape: everything Kitron controls lives in
// host_main.cpp (window 1, pump, events); every RNW call sits in this file,
// so machine-step API drift can never spread past it.
//
// Expected restore layout (`nuget restore` from the app's windows/ folder):
//   packages/Microsoft.ReactNative.Cxx/include   ← the WinRT projection
//   packages/Microsoft.ReactNative/lib/uap10.0   ← the .lib/.winmd hand-off
// The versions must mirror the app's `react-native-windows` npm package.

#include "KitronHost.h"

#include <winrt/Microsoft.ReactNative.h>

#include <cstdio>
#include <cstdlib>

namespace {

// terminates loudly — an app without its UI is dead on arrival.
[[noreturn]] void dead(const char *what, const char *detail) {
  std::fprintf(stderr, "[host] RNW attach failed: %s (%s)\n", what, detail);
  std::fflush(stderr);
  std::exit(3);
}

} // namespace

void attach_kitron_react(HWND hwnd) {
  if (hwnd == nullptr) {
    dead("attach", "no renderer window");
  }

  // ---------------------------------------------------------------
  // MACHINE STEP — the exact RNW init to run once reviewed against the
  // restored headers (the API surface below is the 0.7x shape; verify
  // against `packages/Microsoft.ReactNative.Cxx/include`):
  //
  //   winrt::Microsoft::ReactNative::ReactNativeHost host;
  //   auto &settings = host.InstanceSettings();
  //   settings.Properties().Set(...)          // bridge wiring if needed
  //   settings.UseDeveloperMode(KITRON_DEBUG) // Metro/localhost
  //   settings.UseWebDebugger(false)          // native surface, no devtools
  //   // bundle root = the app root handed through KITRON_CONFIG_JSON
  //   // jsBundleName = "index.windows" (release: *.bundle in resources)
  //
  //   winrt::Microsoft::ReactNative::ReactViewHost viewHost{host};
  //   // hand the React surface to `hwnd` (XAML Islands / view attach),
  //   // then drive the instance start.
  //
  // Compile gate for the machine step: the exe boots with a visible React
  // tree inside the Kitron window frame, hot reload through Metro works,
  // and window 1's frame (Kitron snapshot) tracks the OS window.
  // ---------------------------------------------------------------

  // Until the machine step lands the real instance, the app refuses to
  // run half-wired: window 1 stays blank rather than pretending.
  dead("attach", "RNW init not wired yet (machine step)");
}
