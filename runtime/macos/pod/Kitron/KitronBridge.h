#import <Foundation/Foundation.h>
#import <React/RCTBridgeModule.h>

@class RCTReactNativeFactory;
@class NSWindow;

NS_ASSUME_NONNULL_BEGIN

/// The Kitron React Native module. Surfaced to JS as `KitronRuntime`; the
/// `@kitron/core` bridge resolves it through RN 0.83 bridgeless
/// `nativeModuleProxy` (spec §8, §31). Runs on the main queue (AppKit
/// main-thread rule) except the blocking `nodeInvoke` work-queue.
///
/// NOTE: the `RCTTurboModule` conformance is declared in the implementation
/// file on purpose — `RCTTurboModule.h` drags in C++ headers, and this header
/// is part of the pod's umbrella header, which the app's Swift target has to
/// build as a clang module (Swift cannot resolve C++ standard library headers
/// there). Bridgeless TurboModuleManager providers reject classes that are
/// neither TurboModules nor resolvable through the legacy provider installed
/// under interop, so the conformance must exist at runtime — a class
/// extension is enough for `conformsToProtocol:`.
@interface KitronModule : NSObject <RCTBridgeModule>
- (dispatch_queue_t)methodQueue;
@end

/// Runtime delegate installed on `RCTHost.runtimeDelegate` BEFORE the host
/// starts; injects the synchronous `__kitron` JSI host object into every
/// Hermes runtime the app creates (spec §31 hot path).
@interface KitronRuntimeDelegate : NSObject
+ (instancetype)shared;
- (void)installIntoRuntime:(void *)runtime;
@end

/// Forwards an app lifecycle event ("activate", "ready", "beforeQuit", "quit")
/// into the runtime event bus so `app.on(...)` receives it (spec §11).
/// Called by the platform shell (the app's `AppDelegate`).
FOUNDATION_EXPORT void KitronEmitAppEvent(NSString *name);

/// Application bootstrap. Pre-creates `RCTHost` (so Kitron can hook the JSI
/// runtime before engine start), creates the main native window through the
/// Rust core, then hands the window to the React Native factory.
///
/// Returns `{ windowId, moduleName, windowPtr }`.
@interface KitronRuntimeHost : NSObject
+ (NSDictionary<NSString *, id> *)bootstrapWithFactory:(RCTReactNativeFactory *)factory
                                             moduleName:(NSString *)moduleName;
/// `YES` while the Node backend still has pending in-flight work (used to
/// veto Cmd+Q from applicationShouldTerminate, spec §14).
+ (BOOL)canTerminate;
@end

NS_ASSUME_NONNULL_END
