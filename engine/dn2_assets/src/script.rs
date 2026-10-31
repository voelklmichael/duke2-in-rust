//! Menu and text scripts: TEXT.MNI, OPTIONS.MNI, HELP.MNI. See
//! docs/FORMATS.md "Menu and text scripts".

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::str::FromStr;

use crate::AssetError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    pub sections: Vec<Section>,
}

/// The commands between a section's name line and its `//END` (which is not
/// stored).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub commands: Vec<Command>,
}

/// One `//` line. Text stays bytes: F0h-FFh select colours and EFh starts
/// a sprite reference. Columns and rows are in tiles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    FadeIn,
    FadeOut,
    /// A full-screen picture with its palette.
    LoadRaw {
        file: String,
    },
    /// Only a palette; clears the screen.
    GetPal {
        file: String,
    },
    /// Timer ticks.
    Delay {
        ticks: u32,
    },
    /// Wait for a key.
    Wait,
    SetCurrentPage,
    ShiftWin,
    XyText {
        col: u8,
        row: u8,
        text: Vec<u8>,
    },
    CenterWindow {
        row: u8,
        height: u8,
        width: u8,
    },
    SkLine,
    CwText {
        text: Vec<u8>,
    },
    Menu {
        number: u8,
    },
    PagesStart,
    APage,
    PagesEnd,
    /// Row of the spinning cursor.
    Z {
        row: u8,
    },
    /// The raw key bytes; the underscore stands for the space bar.
    SetKeys {
        keys: Vec<u8>,
    },
    /// Check boxes at `col`: the row and the option letter of each.
    Toggs {
        col: u8,
        entries: Vec<(u8, u8)>,
    },
    GetNames {
        slot: u8,
    },
    HelpText {
        episode: u8,
        level: u8,
        text: Vec<u8>,
    },
    Keys,
    ExitToDemo,
    BabbleOn {
        value: u16,
    },
    BabbleOff,
    NoSounds,
    Pak,
    /// A command this crate does not know.
    Other {
        name: String,
        args: Vec<u8>,
    },
}

impl Script {
    /// Reads every section of the file. A section starts at a line outside
    /// any section that is not a command (its name, taken whole) and runs to
    /// `//END`; lines that are not commands are skipped inside it. Lines end
    /// in CR LF, a bare LF works too.
    pub fn parse(file: &str, bytes: &[u8]) -> Result<Self, AssetError> {
        let mut sections = Vec::new();
        let mut open: Option<Section> = None;
        for line in bytes.split(|&b| b == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            match (&mut open, line.strip_prefix(b"//")) {
                (None, None) if !line.iter().all(u8::is_ascii_whitespace) => {
                    open = Some(Section {
                        name: String::from_utf8_lossy(line).into_owned(),
                        commands: Vec::new(),
                    });
                }
                (Some(section), Some(command)) => match parse_command(file, command)? {
                    Some(command) => section.commands.push(command),
                    None => sections.extend(open.take()),
                },
                _ => {}
            }
        }
        sections.extend(open);
        Ok(Self { sections })
    }

    /// The first section with this name. Case matters, like in the game,
    /// which compares the whole line.
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }
}

/// Splits at the first space; the second part is empty without one.
fn split_space(bytes: &[u8]) -> (&[u8], &[u8]) {
    match bytes.iter().position(|&b| b == b' ') {
        Some(i) => (&bytes[..i], &bytes[i + 1..]),
        None => (bytes, &[]),
    }
}

fn number<T: FromStr>(file: &str, token: &[u8]) -> Result<T, AssetError> {
    core::str::from_utf8(token)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AssetError::Malformed {
            file: file.to_string(),
            reason: "bad number in a script command",
        })
}

/// Parses `//command args`; `None` for `END`.
fn parse_command(file: &str, line: &[u8]) -> Result<Option<Command>, AssetError> {
    let (name, args) = split_space(line);
    let bad = |reason| AssetError::Malformed {
        file: file.to_string(),
        reason,
    };
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_string();
    // The numbers up front, then the text after the last space.
    let split = |n: usize| -> Result<(Vec<u32>, &[u8]), AssetError> {
        let mut rest = args;
        let mut numbers = Vec::new();
        for _ in 0..n {
            let (token, tail) = split_space(rest);
            numbers.push(number(file, token)?);
            rest = tail;
        }
        Ok((numbers, rest))
    };
    let byte = |value: u32| u8::try_from(value).map_err(|_| bad("script number too large"));
    Ok(Some(match name {
        b"END" => return Ok(None),
        b"FADEIN" => Command::FadeIn,
        b"FADEOUT" => Command::FadeOut,
        b"LOADRAW" => Command::LoadRaw { file: text(args) },
        b"GETPAL" => Command::GetPal { file: text(args) },
        b"DELAY" => Command::Delay {
            ticks: number(file, args)?,
        },
        b"WAIT" => Command::Wait,
        b"SETCURRENTPAGE" => Command::SetCurrentPage,
        b"SHIFTWIN" => Command::ShiftWin,
        b"XYTEXT" => {
            let (n, rest) = split(2)?;
            Command::XyText {
                col: byte(n[0])?,
                row: byte(n[1])?,
                text: rest.to_vec(),
            }
        }
        b"CENTERWINDOW" => {
            let (n, _) = split(3)?;
            Command::CenterWindow {
                row: byte(n[0])?,
                height: byte(n[1])?,
                width: byte(n[2])?,
            }
        }
        b"SKLINE" => Command::SkLine,
        b"CWTEXT" => Command::CwText {
            text: args.to_vec(),
        },
        b"MENU" => Command::Menu {
            number: number(file, args)?,
        },
        b"PAGESSTART" => Command::PagesStart,
        b"APAGE" => Command::APage,
        b"PAGESEND" => Command::PagesEnd,
        b"Z" => Command::Z {
            row: number(file, args)?,
        },
        b"SETKEYS" => Command::SetKeys {
            keys: split_space(args).0.to_vec(),
        },
        b"TOGGS" => {
            let mut tokens = args.split(|&b| b == b' ');
            let col = number(file, tokens.next().unwrap_or_default())?;
            let count: usize = number(file, tokens.next().unwrap_or_default())?;
            let mut entries = Vec::new();
            for _ in 0..count {
                let row = number(file, tokens.next().unwrap_or_default())?;
                let letter = tokens.next().and_then(<[u8]>::first);
                entries.push((row, *letter.ok_or_else(|| bad("missing TOGGS letter"))?));
            }
            Command::Toggs { col, entries }
        }
        b"GETNAMES" => Command::GetNames {
            slot: number(file, args)?,
        },
        b"HELPTEXT" => {
            let (n, rest) = split(2)?;
            Command::HelpText {
                episode: byte(n[0])?,
                level: byte(n[1])?,
                text: rest.to_vec(),
            }
        }
        b"KEYS" => Command::Keys,
        b"EXITTODEMO" => Command::ExitToDemo,
        b"BABBLEON" => Command::BabbleOn {
            value: number(file, args)?,
        },
        b"BABBLEOFF" => Command::BabbleOff,
        b"NOSOUNDS" => Command::NoSounds,
        b"PAK" => Command::Pak,
        _ => Command::Other {
            name: String::from_utf8_lossy(name).into_owned(),
            args: args.to_vec(),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"junk\r\n\r\nAlpha\r\nignored line\r\n//LOADRAW PIC.MNI\r\n\
//XYTEXT 3 4   padded \xF2x\r\n//TOGGS 8 2 7 P 9 S\r\n//SETKEYS ab\r\n//ZZZ 1 2\r\n//END\r\n\
//FADEIN\r\nBeta\r\n//HELPTEXT 2 5 hi there\r\n//DELAY 30\r\n//CWTEXT  x\r\n//END";

    #[test]
    fn sections_and_commands() {
        let script = Script::parse("T", SAMPLE).unwrap();
        let names: Vec<_> = script.sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["junk", "Beta"]);
        let junk = script.section("junk").unwrap();
        assert_eq!(junk.commands.len(), 5);
        assert_eq!(
            junk.commands[0],
            Command::LoadRaw {
                file: "PIC.MNI".into()
            }
        );
        assert_eq!(
            junk.commands[1],
            Command::XyText {
                col: 3,
                row: 4,
                text: b"  padded \xF2x".to_vec()
            }
        );
        assert_eq!(
            junk.commands[2],
            Command::Toggs {
                col: 8,
                entries: [(7, b'P'), (9, b'S')].to_vec()
            }
        );
        assert_eq!(
            junk.commands[3],
            Command::SetKeys {
                keys: b"ab".to_vec()
            }
        );
        assert!(matches!(junk.commands[4], Command::Other { .. }));
    }

    #[test]
    fn later_section_and_case() {
        let script = Script::parse("T", SAMPLE).unwrap();
        let beta = script.section("Beta").unwrap();
        assert_eq!(
            beta.commands,
            [
                Command::HelpText {
                    episode: 2,
                    level: 5,
                    text: b"hi there".to_vec()
                },
                Command::Delay { ticks: 30 },
                Command::CwText {
                    text: b" x".to_vec()
                },
            ]
        );
        assert!(script.section("beta").is_none());
    }

    #[test]
    fn bad_numbers_are_errors() {
        assert!(Script::parse("T", b"S\n//DELAY x\n//END\n").is_err());
        assert!(Script::parse("T", b"S\n//XYTEXT 300 1 a\n//END\n").is_err());
        assert!(Script::parse("T", b"S\n//TOGGS 1 2 3 P\n//END\n").is_err());
    }
}
