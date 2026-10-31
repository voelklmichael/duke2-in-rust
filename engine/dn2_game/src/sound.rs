//! The sound effects the game asks for.

/// A sound effect, by the original's id. Names say what it sounds like; several
/// things share most of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Sound {
    /// The regular gun and the rocket launcher firing.
    Shot = 0x00,
    /// Bombs, big explosions, exploding walls.
    BigExplosion = 0x01,
    /// The player is hurt; also blown out of an airlock.
    Hurt = 0x02,
    /// The player dies.
    Die = 0x03,
    /// An explosion (one of two, see [`crate::effects::explosion_sound`]);
    /// a rocket hitting.
    Explosion = 0x04,
    /// Glass, shells and other things breaking.
    /// A menu's selection moves.
    MenuMove = 0x05,
    Break = 0x06,
    /// The player's laser firing.
    Laser = 0x07,
    /// Picking up an item.
    Pickup = 0x08,
    /// Picking up a weapon; a shot hitting a force field.
    WeaponPickup = 0x09,
    /// An actor hit that lives on.
    Hit = 0x0a,
    /// Air fans, a laser turret rising.
    Whir = 0x0b,
    /// The flame thrower, missiles and rockets, the turbo lift.
    Roar = 0x0c,
    /// Jumps and bounces.
    Jump = 0x0d,
    /// A fire jet.
    FireJet = 0x0e,
    /// The player landing.
    Land = 0x0f,
    /// Grabbing a pipe; things tipping over or splitting open.
    Clank = 0x10,
    /// A letter of a message typed.
    Type = 0x11,
    /// Heavy things landing or crushing, a map section sinking.
    Thud = 0x12,
    /// A falling map section coming to rest.
    SectionLand = 0x13,
    /// The other explosion.
    Explosion2 = 0x14,
    /// Slime dripping.
    Drip = 0x15,
    /// Sparks and force fields.
    Spark = 0x16,
    /// A door moving.
    Door = 0x18,
    /// A map section starting to move.
    /// An option of the options menu is switched.
    MenuToggle = 0x19,
    SectionStart = 0x1a,
    /// An enemy's laser.
    EnemyLaser = 0x1b,
    /// The earthquake rumbling.
    Earthquake = 0x1c,
    /// Something squashed.
    Splat = 0x1d,
    /// A transporter.
    Transporter = 0x1e,
    /// Eating or drinking.
    Food = 0x20,
    /// A bonus collected in full.
    Fanfare = 0x21,
    // The movies.
    /// Title movie F1, frame 0.
    Movie2a = 0x2a,
    /// Title movie F3, frames 0, 3 and 6.
    Movie2b = 0x2b,
    /// Title movie F4, frame 0.
    Movie2c = 0x2c,
    /// Title movie F4, frame 7.
    Movie2d = 0x2d,
    /// Title movie F4, frame 31.
    Movie2e = 0x2e,
    /// Title movie F4, frame 33.
    Movie2f = 0x2f,
    /// Title movie F4, frame 37.
    Movie30 = 0x30,
}

impl Sound {
    /// The original's sound id.
    pub fn id(self) -> u8 {
        self as u8
    }
}
