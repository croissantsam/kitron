import AppKit
import React
import React_RCTAppDelegate
import ReactAppDependencyProvider
import Kitron

/// Kitron app entry — programmatic NSApplication bootstrap (no SwiftUI scene):
/// the Rust core owns the main window; React Native renders into it.
@main
final class KitronAppMain: NSObject {
  static func main() {
    let application = NSApplication.shared
    application.setActivationPolicy(.regular)
    let delegate = KitronAppDelegate()
    application.delegate = delegate
    application.run()
  }
}

final class KitronAppDelegate: NSObject, NSApplicationDelegate {
  private let reactNativeDelegate: ReactNativeDelegate
  private let reactNativeFactory: RCTReactNativeFactory

  override init() {
    let delegate = ReactNativeDelegate()
    let factory = RCTReactNativeFactory(delegate: delegate)
    delegate.dependencyProvider = RCTAppDependencyProvider()

    reactNativeDelegate = delegate
    reactNativeFactory = factory
    super.init()
  }

  /// Boots the Kitron runtime (Rust core + main native window + RN host with
  /// the `__kitron` JSI object already installed), then starts React Native
  /// inside the Kitron-owned window.
  func applicationDidFinishLaunching(_ notification: Notification) {
    let bootstrap = KitronRuntimeHost.bootstrap(with: reactNativeFactory, moduleName: "KitronApp")
    if let failure = bootstrap["envelope"] as? String, !failure.isEmpty {
      NSLog("[kitron] bootstrap error: %@", failure)
      NSAlert(error: NSError(
        domain: "dev.kitron.runtime",
        code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Kitron bootstrap failed: \(failure)"]
      )).runModal()
      NSApp.terminate(nil)
    }
  }

  /// Consults the Rust veto flag so `app.preventDefaultExit()` can cancel a
  /// pending termination triggered from the native menu (spec §11).
  func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
    return KitronRuntimeHost.canTerminate() ? .terminateNow : .terminateCancel
  }

  /// The app became active — forwarded to the runtime event bus so
  /// `app.on("activate", …)` fires (spec §11).
  func applicationDidBecomeActive(_ notification: Notification) {
    KitronEmitAppEvent("activate")
  }

  /// The app is about to terminate — `app.on("quit", …)` fires (spec §11).
  func applicationWillTerminate(_ notification: Notification) {
    KitronEmitAppEvent("quit")
  }

  /// Deep link: another process opened a custom-scheme URL targeting this app.
  func application(_ application: NSApplication, open urls: [URL]) {
    let payload = ["urls": urls.map { $0.absoluteString }]
    if let data = try? JSONSerialization.data(withJSONObject: payload),
       let json = String(data: data, encoding: .utf8) {
      KitronEmitAppEventPayload("openUrl", json)
    }
  }

  /// File association: the user opened a file bound to this app.
  func application(_ sender: NSApplication, openFiles filenames: [String]) {
    let payload = ["paths": filenames]
    if let data = try? JSONSerialization.data(withJSONObject: payload),
       let json = String(data: data, encoding: .utf8) {
      KitronEmitAppEventPayload("openFile", json)
    }
    sender.reply(toOpenOrPrint: .success)
  }
}

// MARK: - React Native Delegate

final class ReactNativeDelegate: RCTDefaultReactNativeFactoryDelegate {
  override func sourceURL(for bridge: RCTBridge) -> URL? {
    bundleURL()
  }

  override func bundleURL() -> URL? {
#if DEBUG
    RCTBundleURLProvider.sharedSettings().jsBundleURL(forBundleRoot: "index")
#else
    Bundle.main.url(forResource: "main", withExtension: "jsbundle")
#endif
  }
}
