pub fn make_shortcuts_dialog() -> libadwaita::ShortcutsDialog {
    gtk::Builder::from_resource("/io/github/gusthanks/Canto/components/shortcuts.ui")
        .object("shortcuts_dialog")
        .unwrap()
}
