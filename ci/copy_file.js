// Runner jetable uniquement. Écrire les données directement : les objets NSURL
// écrits depuis osascript laissent le presse-papiers vide sur macOS 15.
ObjC.import("AppKit");
function run(argv) {
    var pb = $.NSPasteboard.generalPasteboard;
    pb.clearContents;
    var url = $.NSURL.fileURLWithPath(argv[0]).absoluteString;
    if (!pb.setStringForType(url, "public.file-url")) {
        throw new Error("écriture du fichier refusée");
    }
}
