// Runner jetable uniquement. Copie un fichier comme le Finder : l'URL est posée
// sous public.file-url, puis le processus reste vivant (le Finder ne quitte pas
// après une copie). Sur macOS 15, une URL de fichier posée depuis osascript
// n'est plus visible des autres apps une fois osascript terminé : sans cette
// attente, copyq-rs ne lit que des types vides.
ObjC.import("AppKit");
function run(argv) {
    var pb = $.NSPasteboard.generalPasteboard;
    pb.clearContents;
    var url = $.NSURL.fileURLWithPath(argv[0]).absoluteString;
    if (!pb.setStringForType(url, "public.file-url")) {
        throw new Error("écriture du fichier refusée");
    }
    delay(2); // l'app scrute toutes les 300 ms : largement le temps de lire la copie
}
