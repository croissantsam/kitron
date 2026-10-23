#pragma once
// The RNW attach glue — `KitronHost.cpp` compiles only inside the machine
// build, with KITRON_ATTACH_REACT defined and Microsoft.ReactNative restored
// (see templates/windows/README.md). Without that build, the host skips
// this file entirely and the shell still boots (the CI probe/tests).
//
// `attach_kitron_react` must be called on the pump thread, after
// `kitron_window_create` answered window 1 (the call site is the #ifdef in
// the host's kitron_host_main). It renders the React surface into the
// window whose HWND arrives as `hwnd` — borrowed, never owned here.

#include <windows.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Machine build: initializes React Native for Windows on `hwnd`.
/// Dies loudly (no silent degradation): a failed init exits the process.
void attach_kitron_react(HWND hwnd);

#ifdef __cplusplus
}
#endif
