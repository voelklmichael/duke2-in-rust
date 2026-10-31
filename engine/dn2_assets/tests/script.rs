mod common;

use dn2_assets::script::{Command, Script};

#[test]
fn all_scripts_parse() {
    let Some(folder) = common::folder() else {
        return;
    };
    let files = common::read_all(&folder);
    for (name, sections, known) in [
        ("TEXT.MNI", 31, "Main_Menu"),
        ("OPTIONS.MNI", 8, "My_Options"),
        ("HELP.MNI", 5, "Hints"),
    ] {
        let (_, bytes) = files.iter().find(|(n, _)| n == name).expect(name);
        let script = Script::parse(name, bytes).unwrap();
        assert_eq!(script.sections.len(), sections, "{name}");
        assert!(script.section(known).is_some(), "{name}");
        for section in &script.sections {
            for command in &section.commands {
                assert!(
                    !matches!(command, Command::Other { .. }),
                    "{name}: {command:?}"
                );
            }
        }
    }
}
