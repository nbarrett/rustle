import UIKit

final class KeyboardViewController: UIInputViewController {
    private let groupId = "group.com.annix.rustle"
    private let textKey = "pendingText"
    private let tokenKey = "pendingToken"
    private let phaseKey = "phase"
    private let lastTokenKey = "lastInsertedToken"
    private let stopName = "com.annix.rustle.keyboard-stop"
    private let idleGreen = UIColor(red: 0.15, green: 0.83, blue: 0.40, alpha: 1)
    private let listeningGreen = UIColor(red: 0.07, green: 0.55, blue: 0.49, alpha: 1)
    private let micButton = UIButton(type: .custom)
    private let caption = UILabel()
    private var poll: Timer?

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(red: 0.11, green: 0.12, blue: 0.13, alpha: 1)
        buildBar()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        insertPendingTranscript()
        refreshFromInbox()
        poll?.invalidate()
        poll = Timer.scheduledTimer(withTimeInterval: 0.35, repeats: true) { [weak self] _ in
            self?.insertPendingTranscript()
            self?.refreshFromInbox()
        }
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        poll?.invalidate()
        poll = nil
    }

    private func inbox() -> UserDefaults? {
        UserDefaults(suiteName: groupId)
    }

    private func phase() -> String {
        inbox()?.string(forKey: phaseKey) ?? "idle"
    }

    private func buildBar() {
        let next = UIButton(type: .system)
        if let image = UIImage(systemName: "globe") {
            next.setImage(image, for: .normal)
        } else {
            next.setTitle("ABC", for: .normal)
        }
        next.tintColor = .white
        next.addTarget(self, action: #selector(goToNextKeyboard), for: .touchUpInside)
        next.translatesAutoresizingMaskIntoConstraints = false

        let backspace = UIButton(type: .system)
        if let image = UIImage(systemName: "delete.left") {
            backspace.setImage(image, for: .normal)
        } else {
            backspace.setTitle("⌫", for: .normal)
        }
        backspace.tintColor = .white
        backspace.addTarget(self, action: #selector(deleteBackwardTapped), for: .touchUpInside)
        backspace.translatesAutoresizingMaskIntoConstraints = false

        micButton.backgroundColor = idleGreen
        micButton.layer.cornerRadius = 36
        micButton.clipsToBounds = true
        if let image = UIImage(systemName: "mic.fill") {
            micButton.setImage(image, for: .normal)
        } else {
            micButton.setTitle("Mic", for: .normal)
        }
        micButton.tintColor = .white
        micButton.addTarget(self, action: #selector(micTapped), for: .touchUpInside)
        micButton.translatesAutoresizingMaskIntoConstraints = false

        caption.text = "Tap to talk"
        caption.textColor = UIColor(white: 0.78, alpha: 1)
        caption.font = UIFont.systemFont(ofSize: 13, weight: .medium)
        caption.textAlignment = .center
        caption.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(next)
        view.addSubview(micButton)
        view.addSubview(backspace)
        view.addSubview(caption)

        let height = view.heightAnchor.constraint(equalToConstant: 168)
        height.priority = .defaultHigh

        NSLayoutConstraint.activate([
            height,
            micButton.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            micButton.topAnchor.constraint(equalTo: view.topAnchor, constant: 18),
            micButton.widthAnchor.constraint(equalToConstant: 72),
            micButton.heightAnchor.constraint(equalToConstant: 72),
            caption.topAnchor.constraint(equalTo: micButton.bottomAnchor, constant: 10),
            caption.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            caption.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: 12),
            caption.trailingAnchor.constraint(lessThanOrEqualTo: view.trailingAnchor, constant: -12),
            next.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 22),
            next.centerYAnchor.constraint(equalTo: micButton.centerYAnchor),
            next.widthAnchor.constraint(equalToConstant: 44),
            next.heightAnchor.constraint(equalToConstant: 44),
            backspace.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -22),
            backspace.centerYAnchor.constraint(equalTo: micButton.centerYAnchor),
            backspace.widthAnchor.constraint(equalToConstant: 44),
            backspace.heightAnchor.constraint(equalToConstant: 44),
        ])
    }

    private func refreshFromInbox() {
        switch phase() {
        case "listening":
            caption.text = "Listening… tap when done"
            micButton.backgroundColor = listeningGreen
        case "transcribing":
            caption.text = "Transcribing…"
            micButton.backgroundColor = listeningGreen
        default:
            if caption.text == "Typed" {
                micButton.backgroundColor = idleGreen
                return
            }
            caption.text = "Tap to talk"
            micButton.backgroundColor = idleGreen
        }
    }

    private func insertPendingTranscript() {
        guard let defaults = inbox() else {
            return
        }
        guard let text = defaults.string(forKey: textKey), !text.isEmpty else {
            return
        }
        let token = defaults.string(forKey: tokenKey) ?? text
        if token == UserDefaults.standard.string(forKey: lastTokenKey) {
            return
        }
        textDocumentProxy.insertText(text)
        UserDefaults.standard.set(token, forKey: lastTokenKey)
        defaults.set("", forKey: textKey)
        defaults.synchronize()
        caption.text = "Typed"
        micButton.backgroundColor = idleGreen
    }

    @objc private func goToNextKeyboard() {
        advanceToNextInputMode()
    }

    @objc private func deleteBackwardTapped() {
        textDocumentProxy.deleteBackward()
    }

    @objc private func micTapped() {
        insertPendingTranscript()
        let current = phase()
        if current == "listening" {
            postStop()
            caption.text = "Transcribing…"
            return
        }
        if current == "transcribing" {
            return
        }
        openRustleToDictate()
    }

    private func postStop() {
        let name = CFNotificationName(stopName as CFString)
        CFNotificationCenterPostNotification(
            CFNotificationCenterGetDarwinNotifyCenter(),
            name,
            nil,
            nil,
            true
        )
    }

    private func openRustleToDictate() {
        guard let url = URL(string: "rustle://dictate") else {
            return
        }
        caption.text = "Starting…"
        var responder: UIResponder? = self
        while let current = responder {
            if let application = current as? UIApplication {
                application.open(url, options: [:], completionHandler: nil)
                return
            }
            responder = current.next
        }
        caption.text = "Open Rustle once, then try the mic again"
    }
}
