//! Saved games, high scores and the save slot names, in our own versioned
//! format (not the original's files), kept in the platform's [`Store`].
//! Integers are little endian, strings are a length byte and the bytes.

use alloc::string::String;
use alloc::vec::Vec;

use dn2_game::player::Weapon;

use crate::Store;

/// The weapons in the order of their codes in the saved games.
const WEAPONS: [Weapon; 4] = [
    Weapon::Regular,
    Weapon::Laser,
    Weapon::Rocket,
    Weapon::FlameThrower,
];

/// The longest high score name.
pub const NAME_LEN: usize = 15;
/// The longest save slot name.
pub const SLOT_NAME_LEN: usize = 18;
/// Entries in a high score table.
pub const SCORE_ENTRIES: usize = 10;
/// Save slots.
pub const SLOTS: usize = 8;

/// Whether `slot` is a save slot (1 to 8).
pub fn is_slot(slot: u8) -> bool {
    (1..=SLOTS).contains(&usize::from(slot))
}

/// The start of a file in our format: the magic, then version 1.
pub(crate) fn header(magic: &[u8; 4]) -> Vec<u8> {
    let mut out = Vec::from(*magic);
    out.push(1);
    out
}

/// The parsed bytes under `key` in the store; the default value if there are
/// none or they are bad.
pub(crate) fn load_or_default<T: Default>(
    store: &dyn Store,
    key: &str,
    parse: fn(&[u8]) -> Option<T>,
) -> T {
    store
        .load_store(key)
        .and_then(|b| parse(&b))
        .unwrap_or_default()
}

/// Appends a length-prefixed string cut to `max` bytes (names are ASCII).
fn put_str(out: &mut Vec<u8>, s: &str, max: usize) {
    let bytes = &s.as_bytes()[..s.len().min(max)];
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
}

/// A reader over bytes that gives `None` instead of panicking.
pub(crate) struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    /// Checks the magic and the version byte.
    pub(crate) fn new(bytes: &'a [u8], magic: &[u8; 4], version: u8) -> Option<Self> {
        let rest = bytes.strip_prefix(magic)?.strip_prefix(&[version])?;
        Some(Reader(rest))
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.0.split_at_checked(n)?;
        self.0 = tail;
        Some(head)
    }

    pub(crate) fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    /// A string of at most `max` bytes, ASCII only.
    fn string(&mut self, max: usize) -> Option<String> {
        let len = usize::from(self.u8()?);
        let bytes = self.take(len)?;
        if len > max || !bytes.is_ascii() {
            return None;
        }
        String::from_utf8(bytes.to_vec()).ok()
    }

    /// Nothing may be left over.
    pub(crate) fn end(&self) -> Option<()> {
        self.0.is_empty().then_some(())
    }
}

/// One episode's table of the ten best scores, best first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighScores {
    pub entries: Vec<(String, u32)>,
}

impl Default for HighScores {
    /// The table of a fresh install: ten empty names with score 0 (as the
    /// original gives for a missing file).
    fn default() -> Self {
        Self {
            entries: alloc::vec![(String::new(), 0); SCORE_ENTRIES],
        }
    }
}

impl HighScores {
    /// The store key of an episode (1 to 4).
    pub fn key(episode: u8) -> String {
        alloc::format!("scores{episode}")
    }

    /// Where a score would go: before the first entry it beats; an equal score
    /// does not displace one.
    pub fn position(&self, score: u32) -> Option<usize> {
        self.entries.iter().position(|(_, s)| *s < score)
    }

    /// Puts a score in at `position`; the last entry drops off. The name is
    /// cut to [`NAME_LEN`] characters.
    pub fn insert(&mut self, position: usize, name: &str, score: u32) {
        let name: String = name.chars().take(NAME_LEN).collect();
        let at = position.min(self.entries.len());
        self.entries.insert(at, (name, score));
        self.entries.truncate(SCORE_ENTRIES);
    }

    /// Our file format: "DN2H", version 1, ten times name and score.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = header(b"DN2H");
        for (name, score) in self.entries.iter().take(SCORE_ENTRIES) {
            put_str(&mut out, name, NAME_LEN);
            out.extend(score.to_le_bytes());
        }
        out
    }

    /// Reads [`HighScores::to_bytes`]; `None` if it is not that.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let mut r = Reader::new(bytes, b"DN2H", 1)?;
        let mut entries = Vec::new();
        for _ in 0..SCORE_ENTRIES {
            let name = r.string(NAME_LEN)?;
            entries.push((name, r.u32()?));
        }
        r.end()?;
        Some(Self { entries })
    }

    /// The stored table of an episode; the default one if none or a bad one.
    pub fn load(store: &dyn Store, episode: u8) -> Self {
        load_or_default(store, &Self::key(episode), Self::parse)
    }

    pub fn save(&self, store: &mut dyn Store, episode: u8) {
        store.save_store(&Self::key(episode), &self.to_bytes());
    }
}

/// What a saved game restores: the state at the start of a level (the 'T'
/// save). Health is 9 then, so it is not kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedGame {
    /// 1 to 4.
    pub episode: u8,
    /// 1 to 8.
    pub level: u8,
    /// 1 to 3.
    pub difficulty: u8,
    pub score: u32,
    pub weapon: Weapon,
    pub ammo: u8,
    /// The one-shot messages already shown (`Messages::shown`).
    pub shown: u32,
}

impl SavedGame {
    /// The store key of a slot (1 to 8).
    pub fn key(slot: u8) -> String {
        alloc::format!("save{slot}")
    }

    /// Our file format: "DN2S", version 1, episode, level, difficulty,
    /// weapon, ammo, score, shown.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = header(b"DN2S");
        let weapon = WEAPONS.iter().position(|&w| w == self.weapon).unwrap_or(0);
        out.extend([
            self.episode,
            self.level,
            self.difficulty,
            weapon as u8,
            self.ammo,
        ]);
        out.extend(self.score.to_le_bytes());
        out.extend(self.shown.to_le_bytes());
        out
    }

    /// Reads [`SavedGame::to_bytes`]; `None` for anything else, including an
    /// episode, level or skill level out of range.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let mut r = Reader::new(bytes, b"DN2S", 1)?;
        let episode = r.u8()?;
        let level = r.u8()?;
        let difficulty = r.u8()?;
        let weapon = *WEAPONS.get(usize::from(r.u8()?))?;
        let ammo = r.u8()?;
        let score = r.u32()?;
        let shown = r.u32()?;
        r.end()?;
        let ok =
            (1..=4).contains(&episode) && (1..=8).contains(&level) && (1..=3).contains(&difficulty);
        ok.then_some(Self {
            episode,
            level,
            difficulty,
            score,
            weapon,
            ammo,
            shown,
        })
    }

    /// The game in a slot (1 to 8), if there is a good one.
    pub fn load(store: &dyn Store, slot: u8) -> Option<Self> {
        Self::parse(&store.load_store(&Self::key(slot))?)
    }

    pub fn save(&self, store: &mut dyn Store, slot: u8) {
        store.save_store(&Self::key(slot), &self.to_bytes());
    }
}

/// The names of the eight save slots, as the save and restore menus show them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlotNames(pub [String; SLOTS]);

impl SlotNames {
    /// The store key.
    pub const KEY: &str = "saves";

    /// Sets a name (slot 1 to 8), cut to [`SLOT_NAME_LEN`] characters.
    pub fn set(&mut self, slot: u8, name: &str) {
        if let Some(entry) = self.0.get_mut(usize::from(slot).wrapping_sub(1)) {
            *entry = name.chars().take(SLOT_NAME_LEN).collect();
        }
    }

    /// Our file format: "DN2N", version 1, eight strings.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = header(b"DN2N");
        for name in &self.0 {
            put_str(&mut out, name, SLOT_NAME_LEN);
        }
        out
    }

    /// Reads [`SlotNames::to_bytes`]; `None` if it is not that.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let mut r = Reader::new(bytes, b"DN2N", 1)?;
        let mut names: [String; SLOTS] = Default::default();
        for name in &mut names {
            *name = r.string(SLOT_NAME_LEN)?;
        }
        r.end()?;
        Some(Self(names))
    }

    /// The stored names; empty ones if none or bad.
    pub fn load(store: &dyn Store) -> Self {
        load_or_default(store, Self::KEY, Self::parse)
    }

    pub fn save(&self, store: &mut dyn Store) {
        store.save_store(Self::KEY, &self.to_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;

    #[derive(Default)]
    struct Mem(BTreeMap<String, Vec<u8>>);

    impl Store for Mem {
        fn load_store(&self, key: &str) -> Option<Vec<u8>> {
            self.0.get(key).cloned()
        }
        fn save_store(&mut self, key: &str, bytes: &[u8]) {
            self.0.insert(String::from(key), bytes.to_vec());
        }
    }

    /// Every prefix, trailing bytes and a wrong version must fail without
    /// panicking.
    fn check_bad<T>(bytes: &[u8], parse: impl Fn(&[u8]) -> Option<T>) {
        for len in 0..bytes.len() {
            assert!(parse(&bytes[..len]).is_none(), "length {len}");
        }
        let mut long = bytes.to_vec();
        long.push(0);
        assert!(parse(&long).is_none());
        let mut version = bytes.to_vec();
        version[4] = 9;
        assert!(parse(&version).is_none());
        assert!(parse(&[]).is_none());
        assert!(parse(&[0xff; 100]).is_none());
    }

    #[test]
    fn high_scores() {
        let mut table = HighScores::default();
        assert_eq!(table.position(0), None);
        assert_eq!(table.position(1), Some(0));
        table.insert(0, "DUKE", 5000);
        table.insert(1, "A VERY LONG NAME INDEED", 100);
        assert_eq!(table.entries.len(), 10);
        assert_eq!(table.entries[1].0, "A VERY LONG NAM");
        assert_eq!(table.position(100), Some(2));
        assert_eq!(table.position(2000), Some(1));
        let bytes = table.to_bytes();
        assert_eq!(HighScores::parse(&bytes), Some(table.clone()));
        check_bad(&bytes, HighScores::parse);

        let mut store = Mem::default();
        assert_eq!(HighScores::load(&store, 2), HighScores::default());
        table.save(&mut store, 2);
        assert_eq!(HighScores::load(&store, 2), table);
        assert_eq!(HighScores::key(3), "scores3");
        store.save_store("scores2", b"junk");
        assert_eq!(HighScores::load(&store, 2), HighScores::default());
    }

    #[test]
    fn saved_games() {
        for weapon in WEAPONS {
            let game = SavedGame {
                episode: 3,
                level: 8,
                difficulty: 2,
                score: 4_000_000_000,
                weapon,
                ammo: 17,
                shown: 0xdead_beef,
            };
            assert_eq!(SavedGame::parse(&game.to_bytes()), Some(game));
        }
        let game = SavedGame {
            episode: 1,
            level: 1,
            difficulty: 1,
            score: 0,
            weapon: Weapon::Regular,
            ammo: 32,
            shown: 0,
        };
        let bytes = game.to_bytes();
        check_bad(&bytes, SavedGame::parse);
        for (at, value) in [(5, 0), (5, 5), (6, 0), (6, 9), (7, 0), (7, 4), (8, 4)] {
            let mut bad = bytes.clone();
            bad[at] = value;
            assert!(SavedGame::parse(&bad).is_none(), "byte {at} = {value}");
        }
        let mut store = Mem::default();
        assert_eq!(SavedGame::load(&store, 4), None);
        game.save(&mut store, 4);
        assert_eq!(SavedGame::load(&store, 4), Some(game));
        assert_eq!(SavedGame::key(8), "save8");
    }

    #[test]
    fn slot_names() {
        let mut names = SlotNames::default();
        names.set(3, "Level three");
        names.set(8, "a name that is far too long");
        names.set(9, "ignored");
        assert_eq!(names.0[7], "a name that is far");
        let bytes = names.to_bytes();
        assert_eq!(SlotNames::parse(&bytes), Some(names.clone()));
        check_bad(&bytes, SlotNames::parse);
        let mut store = Mem::default();
        assert_eq!(SlotNames::load(&store), SlotNames::default());
        names.save(&mut store);
        assert_eq!(SlotNames::load(&store), names);
    }
}
