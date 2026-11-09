import UIKit

// leanote iOS host: GPUI owns the whole screen (issue 02 shell).
// Embedded mode per gpui-mobile example — UIKit drives frames, GPUI renders.

final class GPUITextView: UIView {
    private let gpuiWindow: UnsafeMutableRawPointer
    private let contentController: UIViewController

    override init(frame: CGRect) {
        gpui_ios_set_embedded()
        leanote_ios_register_app()
        gpui_ios_run_demo()
        guard let window = gpui_ios_get_window(),
              let controller = gpui_ios_view_controller(window) else {
            fatalError("Could not create the leanote GPUI view")
        }
        gpuiWindow = window
        contentController = Unmanaged<UIViewController>.fromOpaque(controller).takeUnretainedValue()
        super.init(frame: frame)
        clipsToBounds = true
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    func attach(to parent: UIViewController) {
        parent.addChild(contentController)
        addSubview(contentController.view)
        contentController.didMove(toParent: parent)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds.width > 0, bounds.height > 0,
              contentController.view.frame != bounds else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        contentController.view.frame = bounds
        gpui_ios_layout_view(gpuiWindow)
        drawFrame()
        CATransaction.commit()
    }

    func drawFrame() { _ = gpui_ios_request_frame(gpuiWindow) }
}

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(_ application: UIApplication,
                     configurationForConnecting connectingSceneSession: UISceneSession,
                     options: UIScene.ConnectionOptions) -> UISceneConfiguration {
        let config = UISceneConfiguration(name: nil, sessionRole: connectingSceneSession.role)
        config.delegateClass = SceneDelegate.self
        return config
    }

    func applicationDidBecomeActive(_ application: UIApplication) {
        gpui_ios_did_become_active(nil)
    }

    func applicationWillResignActive(_ application: UIApplication) {
        gpui_ios_will_resign_active(nil)
    }
}

// iOS 26+ SDK requires UIScene adoption (UIKit traps NoSceneLifecycleAdoption
// otherwise); window setup moved here from didFinishLaunchingWithOptions.
final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?
    private var displayLink: CADisplayLink?

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession,
               options connectionOptions: UIScene.ConnectionOptions) {
        let root = UIViewController()
        root.view.backgroundColor = .systemBackground

        let gpui = GPUITextView(frame: .zero)
        gpui.translatesAutoresizingMaskIntoConstraints = false
        root.view.addSubview(gpui)
        gpui.attach(to: root)
        NSLayoutConstraint.activate([
            gpui.topAnchor.constraint(equalTo: root.view.topAnchor),
            gpui.leadingAnchor.constraint(equalTo: root.view.leadingAnchor),
            gpui.trailingAnchor.constraint(equalTo: root.view.trailingAnchor),
            gpui.bottomAnchor.constraint(equalTo: root.view.bottomAnchor),
        ])

        guard let windowScene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = root
        window.makeKeyAndVisible()
        self.window = window

        displayLink = CADisplayLink(target: self, selector: #selector(renderFrame))
        displayLink?.add(to: .main, forMode: .common)
    }

    @objc private func renderFrame() {
        (window?.rootViewController?.view.subviews.first as? GPUITextView)?.drawFrame()
    }
}
