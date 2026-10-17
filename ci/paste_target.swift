// Petite application cible pour tester le collage automatique en CI :
// une fenêtre avec un champ de texte ; son contenu est écrit dans /tmp/pasted.txt.
import AppKit

final class Delegate: NSObject, NSApplicationDelegate, NSTextFieldDelegate {
    var window: NSWindow!
    let field = NSTextField(frame: NSRect(x: 20, y: 20, width: 360, height: 24))

    func applicationDidFinishLaunching(_ n: Notification) {
        window = NSWindow(contentRect: NSRect(x: 200, y: 200, width: 400, height: 64),
                          styleMask: [.titled], backing: .buffered, defer: false)
        window.title = "Cible collage"
        field.delegate = self
        window.contentView?.addSubview(field)
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(field)
        NSApp.activate(ignoringOtherApps: true)
        try? "".write(toFile: "/tmp/pasted.txt", atomically: true, encoding: .utf8)
    }

    func controlTextDidChange(_ obj: Notification) {
        try? field.stringValue.write(toFile: "/tmp/pasted.txt", atomically: true, encoding: .utf8)
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = Delegate()
app.delegate = delegate
app.run()
