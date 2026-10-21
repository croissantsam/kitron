# Kitron — native macOS runtime pod (spec §8, §40)
#
# Vendored pieces (provided by `kitron create`/`dev`):
#   Kitron/kitron.h        — Rust C-ABI declarations
#   Kitron/KitronBridge.h  — public Swift-importable surface
#   Kitron/KitronBridge.mm — RCT module ("KitronRuntime") + JSI delegate + starter
#   libkitron_macos.a      — static library built from the Rust workspace
#
# The pod links the Rust core into the app target and installs the `__kitron`
# JSI host object through RCTHost.runtimeDelegate before engine start.

Pod::Spec.new do |s|
  s.name         = "Kitron"
  s.version      = "0.1.0"
  s.summary      = "Kitron native runtime: Rust core, window ops, embedded Node supervision, JSI host object."
  s.description  = "Bridges the Kitron Rust core (libkitron_macos.a) into React Native via a classic RCT module and a JSI runtime delegate."
  s.homepage     = "https://github.com/croissantlabs/kitron"
  s.license      = { :type => "MIT", :file => "LICENSE" }
  s.authors      = { "Croissant Labs" => "oss@croissantlabs.dev" }
  s.platform     = :osx, "14.0"
  s.source       = { :path => "." }

  s.source_files           = "Kitron/*.{h,mm}"
  s.public_header_files    = "Kitron/KitronBridge.h"
  s.vendored_libraries     = "libkitron_macos.a"
  # Static framework → modulemap exposed like the other RN pods; also keeps
  # `import Kitron` resolvable from the app's Swift sources.
  s.static_framework        = true

  # Force-load the Rust staticlib so extern "C" symbols survive dead-stripping.
  # DEFINES_MODULE makes CocoaPods generate the Kitron modulemap+umbrella so the
  # app's Swift sources can `import Kitron` (static-library pods are module-less
  # unless they opt in like this).
  s.pod_target_xcconfig = {
    "OTHER_LDFLAGS" => "$(inherited) -ObjC",
    "DEFINES_MODULE" => "YES",
    "HEADER_SEARCH_PATHS" => [
      "$(inherited)",
      # <ReactCommon/RCTHost.h> + RCTReactNativeFactory.h live in the npm tree.
      '"$(PODS_ROOT)/../../node_modules/react-native-macos/ReactCommon/react/runtime/platform/ios"',
      '"$(PODS_ROOT)/../../node_modules/react-native-macos/ReactCommon/react/runtime/platform/ios/ReactCommon"',
      '"$(PODS_ROOT)/../../node_modules/react-native-macos/Libraries/AppDelegate"',
    ].join(" "),
    "CLANG_CXX_LANGUAGE_STANDARD" => "c++20",
  }

  s.dependency "React-Core"
  s.dependency "React-RCTAppDelegate"
  s.dependency "React-Fabric"
  s.dependency "React-RuntimeCore"
  s.dependency "React-RuntimeApple"
  s.dependency "React-jsi"
  # RCTTurboModule.h (bridgeless TurboModule conformance) lives here.
  s.dependency "React-NativeModulesApple"
end
