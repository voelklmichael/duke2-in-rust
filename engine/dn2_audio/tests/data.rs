//! The game's sounds and songs through the mixer.

#[path = "../../dn2_assets/tests/common/mod.rs"]
mod common;

use dn2_assets::Files;
use dn2_assets::sound::Sounds;
use dn2_audio::Audio;
use dn2_game::Sound;

const RATE: u32 = 48_000;

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, s| m.max(s.abs()))
}

#[test]
fn sounds_and_songs_play() {
    let Some(folder) = common::folder() else {
        return;
    };
    let files = Files::open(&folder).unwrap();
    let sounds = Sounds::load(&files).unwrap();
    let adlib = sounds.adlib.iter().flatten().count();
    let digitized = sounds.digitized.iter().flatten().count();
    eprintln!("{adlib} AdLib effects, {digitized} digitized");
    assert!(adlib > 20);
    assert!(digitized > 20);

    let song = dn2_assets::sound::parse_imf(&files.read("RANGEA.IMF").unwrap());
    let mut audio = Audio::new(RATE);
    audio.start_music(&song);
    let mut out = vec![0.0; RATE as usize * 4];
    audio.render(&sounds, &mut out);
    eprintln!("music peak {}", peak(&out));
    assert!(peak(&out) > 0.05, "music is silent");
    assert!(peak(&out) < 1.0, "music clips");

    audio.stop_music();
    for sound in [Sound::Shot, Sound::Pickup, Sound::Jump, Sound::Door] {
        audio.play(&sounds, sound);
        let mut out = vec![0.0; RATE as usize / 2];
        audio.render(&sounds, &mut out);
        eprintln!("{sound:?} peak {}", peak(&out));
        assert!(peak(&out) > 0.05, "{sound:?} is silent");
    }
}

#[test]
fn every_song_named_exists() {
    let Some(folder) = common::folder() else {
        return;
    };
    let data = dn2_assets::GameData::load(&Files::open(&folder).unwrap()).unwrap();
    for (name, level) in &data.levels {
        let song = level.music.to_ascii_uppercase();
        assert!(data.songs.contains_key(&song), "{name}: {song} missing");
    }
    for song in [
        "CALM.IMF",
        "DUKEIIA.IMF",
        "RANGEA.IMF",
        "FANFAREA.IMF",
        "OPNGATEA.IMF",
    ] {
        assert!(data.songs.contains_key(song), "{song} missing");
    }
}

#[test]
fn fanfare_nears_its_end() {
    let Some(folder) = common::folder() else {
        return;
    };
    let files = Files::open(&folder).unwrap();
    let sounds = Sounds::load(&files).unwrap();
    let song = dn2_assets::sound::parse_imf(&files.read("FANFAREA.IMF").unwrap());
    let mut audio = Audio::new(RATE);
    audio.start_music(&song);
    let mut out = vec![0.0; RATE as usize / 10];
    let mut tenths = 0;
    while !audio.music_near_end() {
        audio.render(&sounds, &mut out);
        tenths += 1;
        assert!(tenths < 600, "no end within a minute");
    }
    eprintln!("FANFAREA near its end after {} s", tenths as f32 / 10.0);
}
