#import "KitronBridge.h"

#import "kitron.h"

#import <React/RCTDefines.h>
#import <React/RCTBridgeModule.h>
#import <React/RCTDevMenu.h>

#import <ReactCommon/RCTHost.h>
#import <ReactCommon/RCTHost+Internal.h>
#import <ReactCommon/RCTInteropTurboModule.h>
#import <ReactCommon/RCTTurboModule.h>

#import <React-RCTAppDelegate/RCTJSRuntimeConfiguratorProtocol.h>
#import <React-RCTAppDelegate/RCTRootViewFactory.h>
#import <React-RCTAppDelegate/RCTReactNativeFactory.h>

#import <jsi/jsi.h>

#include <memory>
#include <string>
#include <vector>

extern "C" void js_runtime_factory_destroy(void *factory); // JSRuntimeFactoryCAPI

// ---------------------------------------------------------------------------
// Envelope helpers
// ---------------------------------------------------------------------------

static NSString *KitronCopyAndFree(char *pointer) {
  if (pointer == nullptr) {
    return @"{\"ok\":false,\"error\":{\"code\":\"KITRON_BRIDGE_DEAD\",\"message\":\"no response from the Rust core\",\"platform\":\"macos\"}}";
  }
  NSString *value = [NSString stringWithUTF8String:pointer];
  kitron_free_string(pointer);
  return value;
}

static id KitronReadEnvelopeJSON(NSString *raw) {
  if (raw == nil || raw.length == 0) {
    return nil;
  }
  return [NSJSONSerialization JSONObjectWithData:[raw dataUsingEncoding:NSUTF8StringEncoding] options:0 error:NULL];
}

/// Thread-safe snapshot read for the JS thread (kitron_snapshot_json relies on
/// the cached window list and pure-Rust state; never touches AppKit).
static id KitronReadSnapshot() {
  return KitronReadEnvelopeJSON(KitronCopyAndFree(kitron_snapshot_json()));
}

static id KitronSnapshotSubValue(id envelope, NSArray<NSString *> *path) {
  if (envelope == nil || ![envelope isKindOfClass:[NSDictionary class]] ||
      ![envelope[@"ok"] boolValue]) {
    return nil;
  }
  id value = envelope[@"result"];
  for (NSString *segment in path) {
    if (![value isKindOfClass:[NSDictionary class]]) {
      return nil;
    }
    value = [value objectForKey:segment];
  }
  return value;
}

namespace {
/// JS → unified-log channel: `__kitron.log(level, message)`. The JS logger
/// composes the `[kitron:tag]` prefix itself; the bridge just mirrors it into
/// the unified NSLog stream so JS logs are visible in app stdout (spec §34).
facebook::jsi::Value MakeLogFunction(facebook::jsi::Runtime &runtime) {
  auto body =
      [](facebook::jsi::Runtime &rt, const facebook::jsi::Value &,
         const facebook::jsi::Value *args, size_t count) -> facebook::jsi::Value {
    auto argumentString = [&rt](const facebook::jsi::Value &value) -> NSString * {
      try {
        if (value.isString()) {
          std::string utf8 = value.getString(rt).utf8(rt);
          return [NSString stringWithUTF8String:utf8.c_str()] ?: @"";
        }
        if (value.isUndefined() || value.isNull()) {
          return @"undefined";
        }
        const facebook::jsi::Value stringify =
            rt.global().getProperty(rt, "JSON").asObject(rt).getPropertyAsFunction(rt, "stringify").call(rt, value);
        std::string utf8 = stringify.isString() ? stringify.getString(rt).utf8(rt) : "{}";
        return [NSString stringWithUTF8String:utf8.c_str()] ?: @"{}";
      } catch (const std::exception &) {
        return @"<unprintable>";
      }
    };

    NSString *level = count >= 1 ? argumentString(args[0]) : @"info";
    NSString *message = count >= 2 ? argumentString(args[1]) : @"";
    NSLog(@"[kitron:app] [%@] %@", level, message);
    return facebook::jsi::Value::undefined();
  };
  return facebook::jsi::Function::createFromHostFunction(
      runtime, facebook::jsi::PropNameID::forAscii(runtime, "log"), 2, std::move(body));
}

/// Foundation → jsi conversion used by the sync snapshot getters.
facebook::jsi::Value KitronToJS(facebook::jsi::Runtime &rt, id value) {
  if (value == nullptr || value == [NSNull null]) {
    return facebook::jsi::Value::null();
  }
  if ([value isKindOfClass:[NSNumber class]]) {
    const char *type = [value objCType];
    if (strcmp(type, @encode(BOOL)) == 0 || strcmp(type, "B") == 0 || strcmp(type, "c") == 0) {
      return facebook::jsi::Value([value boolValue]);
    }
    return facebook::jsi::Value([value doubleValue]);
  }
  if ([value isKindOfClass:[NSString class]]) {
    const char *utf8 = [value UTF8String];
    return facebook::jsi::String::createFromAscii(
        rt, utf8 == nullptr ? "" : std::string(utf8, [value lengthOfBytesUsingEncoding:NSUTF8StringEncoding]));
  }
  if ([value isKindOfClass:[NSArray class]]) {
    facebook::jsi::Array array(rt, [value count]);
    // Plain loops: ObjC blocks would capture the move-only jsi objects by copy.
    for (NSUInteger index = 0; index < [value count]; index += 1) {
      array.setValueAtIndex(rt, index, KitronToJS(rt, [value objectAtIndex:index]));
    }
    return array;
  }
  if ([value isKindOfClass:[NSDictionary class]]) {
    facebook::jsi::Object object(rt);
    for (NSString *key in (NSDictionary *)value) {
      const char *utf8 = [key UTF8String];
      object.setProperty(
          rt,
          facebook::jsi::PropNameID::forUtf8(
              rt, utf8 == nullptr ? std::string() : std::string(utf8)),
          KitronToJS(rt, [value objectForKey:key]));
    }
    return object;
  }
  return facebook::jsi::Value::undefined();
}

/// Builds `{"app": {bits()}, "windows": {list()}, "node": {status()}}` leaves;
/// each leaf is a host function reading the cached snapshot synchronously.
facebook::jsi::Value MakeSnapshotFunction(facebook::jsi::Runtime &runtime,
                                          std::vector<std::string> path,
                                          const char *functionName) {
  auto body = [path = std::move(path)](facebook::jsi::Runtime &rt,
                                       const facebook::jsi::Value &,
                                       const facebook::jsi::Value *,
                                       size_t) -> facebook::jsi::Value {
    NSMutableArray<NSString *> *keys = [NSMutableArray<NSString *> new];
    for (const std::string &segment : path) {
      [keys addObject:@(segment.c_str())];
    }
    const id snapshot = KitronReadSnapshot();
    const id value = KitronSnapshotSubValue(snapshot, keys);
    return KitronToJS(rt, value == nil ? [NSNull null] : value);
  };
  return facebook::jsi::Function::createFromHostFunction(
      runtime, facebook::jsi::PropNameID::forAscii(runtime, functionName), 0,
      std::move(body));
}
} // namespace

// ---------------------------------------------------------------------------
// KitronModule — the JS-facing native module ("KitronRuntime")
// ---------------------------------------------------------------------------

/// TurboModule conformance lives here (not in the public header) so the pod's
/// umbrella header stays free of C++ headers — the app's Swift target builds
/// `import Kitron` as a clang module and cannot resolve C++ stdlib headers
/// there. `conformsToProtocol:` sees class-extension conformances, which is
/// what the bridgeless TurboModuleManager provider checks.
@interface KitronModule () <RCTTurboModule>
- (std::shared_ptr<facebook::react::TurboModule>)getTurboModule:
    (const facebook::react::ObjCTurboModule::InitParams &)params;
@end

@implementation KitronModule {
  dispatch_queue_t _invokeQueue;
}

RCT_EXPORT_MODULE(KitronRuntime)

/// RN 0.83 bridgeless installs the TurboModule proxy under
/// `global.nativeModuleProxy`; module class resolution runs through the
/// TurboModuleManager provider. Conforming to RCTTurboModule (and returning
/// the interop wrapper here) makes the classic RCT_EXPORT_METHOD surface
/// resolvable by that provider.
- (std::shared_ptr<facebook::react::TurboModule>)getTurboModule:
    (const facebook::react::ObjCTurboModule::InitParams &)params {
  return std::make_shared<facebook::react::ObjCInteropTurboModule>(params);
}

- (dispatch_queue_t)methodQueue {
  // AppKit-surface calls must run on the main thread.
  return dispatch_get_main_queue();
}

- (instancetype)init {
  if (self = [super init]) {
    _invokeQueue = dispatch_queue_create("dev.kitron.invoke", DISPATCH_QUEUE_SERIAL);
  }
  return self;
}

RCT_EXPORT_METHOD(appQuit:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_app_request_quit()));
}

RCT_EXPORT_METHOD(appCancelTermination:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_app_cancel_termination()));
}

RCT_EXPORT_METHOD(windowCreate:(NSString *)argsJSON
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_window_create(argsJSON.UTF8String ?: "{}")));
}

RCT_EXPORT_METHOD(windowGet:(nonnull NSNumber *)windowId
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_window_get(windowId.unsignedLongLongValue)));
}

RCT_EXPORT_METHOD(windowOp:(nonnull NSNumber *)windowId
                  op:(NSString *)op
                  argsJSON:(NSString *)argsJSON
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_window_op(windowId.unsignedLongLongValue,
                                             op.UTF8String ?: "",
                                             argsJSON.UTF8String ?: "{}")));
}

RCT_EXPORT_METHOD(eventsDrain:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_events_drain()));
}

RCT_EXPORT_METHOD(nodeStart:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_node("start")));
}

RCT_EXPORT_METHOD(nodeStop:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_node("stop")));
}

RCT_EXPORT_METHOD(nodeInvoke:(NSString *)method
                  payloadJSON:(NSString *)payloadJSON
                  timeoutMs:(nonnull NSNumber *)timeoutMs
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  // Blocking call — run outside the main queue so slow backend handlers never
  // stall the UI (spec §30). kitron_node_invoke_wait is pure Rust/thread-safe.
  NSString *methodCopy = [method copy] ?: @"";
  NSString *payloadCopy = [payloadJSON copy] ?: @"null";
  uint32_t timeout = timeoutMs.unsignedIntValue;
  dispatch_async(_invokeQueue, ^{
    char *response =
        kitron_node_invoke_wait(methodCopy.UTF8String, payloadCopy.UTF8String, timeout);
    resolve(KitronCopyAndFree(response));
  });
}

// ---------------------------------------------------------------------------
// Phase-2 native APIs (spec §42) — clipboard, shell, dialogs
// ---------------------------------------------------------------------------

RCT_EXPORT_METHOD(clipboardRead:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_clipboard_read()));
}

RCT_EXPORT_METHOD(clipboardWrite:(NSString *)text
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  NSString *value = [text copy] ?: @"";
  resolve(KitronCopyAndFree(kitron_clipboard_write(value.UTF8String)));
}

RCT_EXPORT_METHOD(shellOpenUrl:(NSString *)url
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  NSString *value = [url copy] ?: @"";
  resolve(KitronCopyAndFree(kitron_shell_open_url(value.UTF8String)));
}

RCT_EXPORT_METHOD(shellOpenPath:(NSString *)path
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  NSString *value = [path copy] ?: @"";
  resolve(KitronCopyAndFree(kitron_shell_open_path(value.UTF8String)));
}

RCT_EXPORT_METHOD(shellShowItemInFolder:(NSString *)path
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  NSString *value = [path copy] ?: @"";
  resolve(KitronCopyAndFree(kitron_shell_show_item_in_folder(value.UTF8String)));
}

RCT_EXPORT_METHOD(dialogMessage:(NSString *)argsJSON
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_dialog_message(argsJSON.UTF8String ?: "{}")));
}

RCT_EXPORT_METHOD(dialogOpenFile:(NSString *)argsJSON
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_dialog_open_file(argsJSON.UTF8String ?: "{}")));
}

RCT_EXPORT_METHOD(dialogSaveFile:(NSString *)argsJSON
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(__unused RCTPromiseRejectBlock)reject) {
  resolve(KitronCopyAndFree(kitron_dialog_save_file(argsJSON.UTF8String ?: "{}")));
}

@end

// ---------------------------------------------------------------------------
// Swift-facing helpers (the app's AppDelegate drives the lifecycle)
// ---------------------------------------------------------------------------

void KitronEmitAppEvent(NSString *name) {
  NSString *value = [name copy] ?: @"";
  char *response = kitron_emit_app_event(value.UTF8String);
  KitronCopyAndFree(response);
}

// ---------------------------------------------------------------------------
// KitronRuntimeDelegate — injects the __kitron JSI host object
// ---------------------------------------------------------------------------

// The factory delegate implements RCTJSRuntimeConfiguratorProtocol
// (createJSRuntimeFactory → jsrt_create_hermes_factory()).

@implementation KitronRuntimeDelegate

+ (instancetype)shared {
  static KitronRuntimeDelegate *delegate = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    delegate = [KitronRuntimeDelegate new];
  });
  return delegate;
}

- (void)host:(RCTHost *)host didInitializeRuntime:(facebook::jsi::Runtime &)runtime {
  NSLog(@"[kitron] bridge: Hermes runtime initialized — injecting __kitron");
  [self installIntoRuntime:&runtime];
}

- (void)installIntoRuntime:(void *)runtimeRef {
  auto &runtime = *reinterpret_cast<facebook::jsi::Runtime *>(runtimeRef);
  using namespace facebook;

  try {
    jsi::Object kitron(runtime);
    kitron.setProperty(runtime, "version", jsi::String::createFromAscii(runtime, "0.1.0"));
    kitron.setProperty(runtime, "platform", jsi::String::createFromAscii(runtime, "macos"));
    kitron.setProperty(runtime, "apiVersion", jsi::Value(1));

    jsi::Object appObject(runtime);
    appObject.setProperty(runtime, "bits", MakeSnapshotFunction(runtime, {"app"}, "bits"));
    kitron.setProperty(runtime, "app", std::move(appObject));

    jsi::Object windowsObject(runtime);
    windowsObject.setProperty(runtime, "list", MakeSnapshotFunction(runtime, {"windows"}, "list"));
    kitron.setProperty(runtime, "windows", std::move(windowsObject));

    jsi::Object nodeObject(runtime);
    nodeObject.setProperty(runtime, "status", MakeSnapshotFunction(runtime, {"node"}, "status"));
    kitron.setProperty(runtime, "node", std::move(nodeObject));

    kitron.setProperty(runtime, "log", MakeLogFunction(runtime));

    runtime.global().setProperty(runtime, "__kitron", std::move(kitron));
  } catch (const std::exception &error) {
    NSLog(@"[kitron] JSI install failed: %s", error.what());
  }
}

@end

// ---------------------------------------------------------------------------
// KitronRuntimeHost — application bootstrap wiring
// ---------------------------------------------------------------------------

/// Wraps the factory delegate's engine (Hermes defaults via
/// RCTDefaultReactNativeFactoryDelegate) in the C++ shared_ptr shape RCTHost
/// expects. The JSRuntimeFactoryCAPI deleter keeps ownership symmetric.
static std::shared_ptr<facebook::react::JSRuntimeFactory>
KitronEngineFromDelegate(id delegate) {
  if (delegate == nil || ![delegate respondsToSelector:@selector(createJSRuntimeFactory)]) {
    [NSException raise:@"KitronRuntimeHost::engine"
                format:@"the React Native factory delegate must implement createJSRuntimeFactory (hermes defaults)"];
  }
  JSRuntimeFactoryRef ref =
      [(id<RCTJSRuntimeConfiguratorProtocol>)delegate createJSRuntimeFactory];
  return std::shared_ptr<facebook::react::JSRuntimeFactory>(
      reinterpret_cast<facebook::react::JSRuntimeFactory *>(ref),
      &js_runtime_factory_destroy);
}

@implementation KitronRuntimeHost

/// For packaged (double-click) launches: derive the KITRON_* env contract
/// from `Contents/Resources/kitron/` (written by `kitron build`). Dev runs
/// always have the env set by the CLI, which takes precedence.
static void KitronSetUpBundledEnvironment(void) {
  if (getenv("KITRON_ROOT") != NULL) {
    return; // CLI-managed dev run.
  }
  NSURL *kitronResources =
      [[NSBundle mainBundle] URLForResource:@"kitron" withExtension:nil subdirectory:nil];
  if (kitronResources == nil) {
    return;
  }
  NSString *root = kitronResources.path;
  setenv("KITRON_ROOT", root.UTF8String, 0);
  setenv("KITRON_DEV", "0", 0);
  setenv("KITRON_RUNTIME_NODE_DIR", [root stringByAppendingPathComponent:@"node-host"].UTF8String, 0);

  NSString *nodeBinary = [root stringByAppendingPathComponent:@"node/bin/node"];
  if ([[NSFileManager defaultManager] fileExistsAtPath:nodeBinary]) {
    setenv("KITRON_NODE_PATH", nodeBinary.UTF8String, 0);
  }
  NSString *configPath = [root stringByAppendingPathComponent:@"app-config.json"];
  if ([[NSFileManager defaultManager] fileExistsAtPath:configPath]) {
    setenv("KITRON_CONFIG_JSON", [NSString stringWithContentsOfFile:configPath encoding:NSUTF8StringEncoding error:NULL].UTF8String, 0);
  }
}

+ (NSDictionary<NSString *, id> *)bootstrapWithFactory:(RCTReactNativeFactory *)factory
                                            moduleName:(NSString *)moduleName {
  // 0. Mirror RCTRootViewFactory's new-arch setup BEFORE we create the RCTHost:
  //    the TurboModuleManager is built (and installs its JS bindings) during
  //    host start, so the TurboModuleInterop statics must be enabled first or
  //    the bridgeless proxy gets installed without the legacy provider and
  //    classic RCT_EXPORT_* modules resolve as `null` from JS.
  RCTEnableTurboModuleInterop(YES);
  RCTEnableTurboModuleInteropBridgeProxy(YES);

  // 1. Rust core bootstrap (config from env, or packaged bundle resources).
  KitronSetUpBundledEnvironment();
  NSString *bootEnvelope = KitronCopyAndFree(kitron_bootstrap());
  if (![KitronReadEnvelopeJSON(bootEnvelope)[@"ok"] boolValue]) {
    NSLog(@"[kitron] bootstrap failed: %@", bootEnvelope);
    return @{ @"error": @YES, @"envelope": bootEnvelope ?: @"" };
  }

  // 2. Pre-create the React host so KitronRuntimeDelegate can hook EVERY
  //    Hermes runtime before the engine spins up. Assign to the root view
  //    factory's settable reactHost property so the factory reuses it.
  RCTRootViewFactory *rootViewFactory = factory.rootViewFactory;
  RCTHost *host = [[RCTHost alloc]
      initWithBundleURLProvider:^NSURL *() {
        return [factory.delegate bundleURL];
      }
      hostDelegate:factory
      turboModuleManagerDelegate:factory
      jsEngineProvider:^std::shared_ptr<facebook::react::JSRuntimeFactory>() {
        return KitronEngineFromDelegate(factory.delegate);
      }
      launchOptions:nil
      devMenuConfiguration:[RCTDevMenuConfiguration defaultConfiguration]];
  // RCTHost's designated initializer does NOT store the bundleURLProvider —
  // the default RN factory wires it post-init via -setBundleURLProvider:;
  // do the same or the JS bundle never loads (URL stays nil).
  [host setBundleURLProvider:^NSURL *() {
    return [factory.delegate bundleURL];
  }];
  [host setRuntimeDelegate:[KitronRuntimeDelegate shared]];
  NSLog(@"[kitron] bridge: host created; runtime delegate installed");
  [host start];
  rootViewFactory.reactHost = host;
  NSLog(@"[kitron] bridge: host started");

  // 3. Create the main window via the Rust core (it owns the NSWindow).
  NSString *windowEnvelope =
      KitronCopyAndFree(kitron_window_create("{\"focus\": true}"));
  NSDictionary *windowInfo = KitronReadEnvelopeJSON(windowEnvelope);
  if (![windowInfo[@"ok"] boolValue]) {
    NSLog(@"[kitron] window create failed: %@", windowEnvelope);
    return @{ @"error": @YES, @"envelope": windowEnvelope ?: @"" };
  }
  const unsigned long long windowId = [windowInfo[@"result"][@"id"] unsignedLongLongValue];
  NSLog(@"[kitron] bridge: main window id=%llu", windowId);
  NSWindow *window =
      (__bridge NSWindow *)(void *)[
          windowInfo[@"result"][@"windowPtr"] unsignedLongLongValue];

  // 4. Hand the window to React Native.
  NSLog(@"[kitron] bridge: starting React Native in window %@", window);
  [factory startReactNativeWithModuleName:moduleName inWindow:window];
  NSLog(@"[kitron] bridge: React Native started");

  return @{ @"windowId": @(windowId), @"moduleName": moduleName ?: @"KitronApp" };
}

+ (BOOL)canTerminate {
  return kitron_app_can_terminate() != 0;
}

@end
