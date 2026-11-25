//! Portable weather-particle behavior from the actor weather profiles.
//!
//! The C implementation stores particles in a fixed private array owned by
//! `WEATHER_ACTOR`.  [`WeatherField`] keeps the same first-free allocation
//! rule and exposes the profile operations without making a Rust renderer or
//! a C ABI part of the candidate.

use core::f32::consts::TAU;

/// `PRIVATE_COUNT` from the corresponding C source.
pub const PRIVATE_COUNT: usize = 100;
/// `NO_EXIST_ID` from the corresponding C source.
pub const NO_EXIST_ID: i32 = -1;
/// `HOLD_TIMER` from the corresponding C source.
pub const HOLD_TIMER: f32 = -100.0;

/// The five profile IDs in `ac_weather.c`'s profile table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Profile {
    /// `Fine` as defined by the C source.
    Fine = 0,
    /// `Rain` as defined by the C source.
    Rain = 1,
    /// `Snow` as defined by the C source.
    Snow = 2,
    /// `Sakura` as defined by the C source.
    Sakura = 3,
    /// `Leaf` as defined by the C source.
    Leaf = 4,
}

impl Profile {
    /// `all` as defined by the C source.
    pub const fn all() -> [Self; 5] {
        [Self::Fine, Self::Rain, Self::Snow, Self::Sakura, Self::Leaf]
    }
}

/// Equivalent to the C `xyz_t` used by weather actors.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Vec3 {
    /// `x` as defined by the C source.
    pub x: f32,
    /// `y` as defined by the C source.
    pub y: f32,
    /// `z` as defined by the C source.
    pub z: f32,
}

impl core::ops::AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

impl core::ops::Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

/// One entry in `WEATHER_ACTOR.priv` / `aWeather_Priv`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Particle {
    /// `pos` as defined by the C source.
    pub pos: Vec3,
    /// `speed` as defined by the C source.
    pub speed: Vec3,
    /// `current_y` as defined by the C source.
    pub current_y: f32,
    /// `unknown_1c` as defined by the C source.
    pub unknown_1c: f32,
    /// `timer` as defined by the C source.
    pub timer: f32,
    /// `work` as defined by the C source.
    pub work: [i16; 5],
    /// `use_flag` as defined by the C source.
    pub use_flag: bool,
    /// `id` as defined by the C source.
    pub id: u8,
    /// `status` as defined by the C source.
    pub status: u8,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            pos: Vec3::default(),
            speed: Vec3::default(),
            current_y: 0.0,
            unknown_1c: 0.0,
            timer: 0.0,
            work: [0; 5],
            use_flag: false,
            id: 0,
            status: 0,
        }
    }
}

/// State shared by the five weather profile implementations.
#[derive(Clone, Debug, PartialEq)]
pub struct WeatherField {
    /// `origin` as defined by the C source.
    pub origin: Vec3,
    /// `wind` as defined by the C source.
    pub wind: Vec3,
    /// `player_y` as defined by the C source.
    pub player_y: f32,
    /// `horizontal_scale` as defined by the C source.
    pub horizontal_scale: f32,
    /// `current_level` as defined by the C source.
    pub current_level: i16,
    /// `spawn_accum` as defined by the C source.
    pub spawn_accum: f32,
    /// `should_spawn_counter` as defined by the C source.
    pub should_spawn_counter: f32,
    /// `particles` as defined by the C source.
    pub particles: [Particle; PRIVATE_COUNT],
}

impl Default for WeatherField {
    fn default() -> Self {
        Self {
            origin: Vec3::default(),
            wind: Vec3::default(),
            player_y: 0.0,
            horizontal_scale: 1.0,
            current_level: 1,
            spawn_accum: 0.0,
            should_spawn_counter: 0.0,
            particles: [Particle::default(); PRIVATE_COUNT],
        }
    }
}

impl WeatherField {
    /// C `aWeather_GetWeatherPrvNum`: return the first unused entry.
    pub fn first_free(&self) -> i32 {
        self.particles
            .iter()
            .position(|particle| !particle.use_flag)
            .map_or(NO_EXIST_ID, |id| id as i32)
    }

    /// C `aWeather_AbolishPrivate`: only a live entry is changed.
    pub fn remove(&mut self, id: usize) {
        if let Some(particle) = self.particles.get_mut(id) {
            if particle.use_flag {
                particle.use_flag = false;
            }
        }
    }

    /// C `aWeather_GetWeatherPrv`, including its failure mutation semantics.
    /// An occupied slot or an out-of-range ID leaves the field untouched.
    pub fn allocate(
        &mut self,
        status: u8,
        timer: f32,
        position: Option<Vec3>,
        speed: Option<Vec3>,
        id: i32,
    ) -> Option<&mut Particle> {
        let index = usize::try_from(id)
            .ok()
            .filter(|&value| value < PRIVATE_COUNT)?;
        let particle = &mut self.particles[index];
        if particle.use_flag {
            return None;
        }

        particle.use_flag = true;
        particle.status = status;
        particle.timer = timer;
        particle.id = index as u8;
        if let Some(position) = position {
            particle.pos = position;
        }
        if let Some(speed) = speed {
            particle.speed = speed;
        }
        Some(particle)
    }

    /// Source `aWeather_ShouldSpawnEvery`: consume elapsed frames and return
    /// one for each crossed period.  The C helper is called once per actor
    /// frame, so this returns at most one event for a normal positive period.
    pub fn should_spawn_every(&mut self, elapsed_frames: f32, period: f32) -> bool {
        if period <= 0.0 {
            return false;
        }
        self.should_spawn_counter += elapsed_frames;
        if self.should_spawn_counter >= period {
            self.should_spawn_counter -= period;
            true
        } else {
            false
        }
    }

    /// C integer conversion used for the profile's phase accumulators.
    fn add_phase(value: &mut i16, speed: f32, dt: f32) {
        *value = value.wrapping_add((speed * dt) as i16);
    }

    fn scroll_particle(
        origin: Vec3,
        horizontal_scale: f32,
        particle: &mut Particle,
        y_threshold: f32,
    ) {
        let half_width = 100.0 * horizontal_scale;
        let mut border = 0u8;
        if particle.pos.x < origin.x - half_width {
            border |= 2;
        }
        if particle.pos.x > origin.x + half_width {
            border |= 8;
        }
        if particle.pos.z > origin.z + 180.0 {
            border |= 4;
        }
        if particle.pos.z < origin.z - 200.0 {
            border |= 1;
        }

        let horizontal_width = 200.0 * horizontal_scale;
        if border & 2 != 0 {
            particle.pos.x += horizontal_width;
        }
        if border & 8 != 0 {
            particle.pos.x -= horizontal_width;
        }
        if border & 4 != 0 {
            particle.pos.z -= 380.0;
        }
        if border & 1 != 0 {
            particle.pos.z += 380.0;
        }

        if particle.pos.y < y_threshold {
            particle.pos.y = origin.y + 230.0 + (particle.pos.y - particle.current_y);
            particle.current_y = origin.y + 230.0;
        }
    }

    /// Move a rain drop.  Picha particles (`work[0] == 1`) remain stationary
    /// exactly as in `aWeatherRain_move`.
    pub fn move_rain(&mut self, id: usize, dt: f32) {
        let Some(particle) = self.particles.get_mut(id) else {
            return;
        };
        if particle.use_flag && particle.work[0] == 0 {
            particle.pos += particle.speed * dt;
        }
    }

    /// Move a snow particle, including wind, phase, and player-relative wrap.
    pub fn move_snow(&mut self, id: usize, dt: f32) {
        let wind = self.wind;
        let origin = self.origin;
        let horizontal_scale = self.horizontal_scale;
        let player_y = self.player_y;
        let Some(particle) = self.particles.get_mut(id) else {
            return;
        };
        if !particle.use_flag {
            return;
        }
        particle.pos += particle.speed * dt;
        let phase_speed = particle.work[1] as f32;
        Self::add_phase(&mut particle.work[0], phase_speed, dt);
        particle.pos += wind * dt;
        Self::scroll_particle(origin, horizontal_scale, particle, 40.0 + player_y);
    }

    /// Move a sakura particle according to `aWeatherSakura_move`.
    pub fn move_sakura(&mut self, id: usize, dt: f32) {
        let origin = self.origin;
        let horizontal_scale = self.horizontal_scale;
        let Some(particle) = self.particles.get_mut(id) else {
            return;
        };
        if !particle.use_flag {
            return;
        }
        particle.pos += particle.speed * dt;
        let phase_speed = particle.work[4] as f32;
        Self::add_phase(&mut particle.work[3], phase_speed, dt);
        particle.pos.x += 0.45 * dt;
        Self::scroll_particle(origin, horizontal_scale, particle, origin.y);
        Self::add_phase(&mut particle.work[0], 0x200 as f32, dt);
        Self::add_phase(&mut particle.work[1], 0x8dc as f32, dt);
        Self::add_phase(&mut particle.work[2], 0x474 as f32, dt);
    }

    /// Move a leaf particle according to `aWeatherLeaf_move`.
    pub fn move_leaf(&mut self, id: usize, dt: f32) {
        let origin = self.origin;
        let horizontal_scale = self.horizontal_scale;
        let Some(particle) = self.particles.get_mut(id) else {
            return;
        };
        if !particle.use_flag {
            return;
        }
        particle.pos += particle.speed * dt;
        let phase_speed = particle.work[4] as f32;
        Self::add_phase(&mut particle.work[3], phase_speed, dt);
        particle.pos.x += 0.45 * dt;
        Self::scroll_particle(origin, horizontal_scale, particle, origin.y);
        Self::add_phase(&mut particle.work[1], 0x8dc as f32, dt);
        Self::add_phase(&mut particle.work[2], 0x474 as f32, dt);
    }

    /// The profile's visible position before the renderer applies its matrix.
    pub fn visual_position(&self, id: usize) -> Option<Vec3> {
        let particle = self.particles.get(id)?;
        if !particle.use_flag {
            return None;
        }
        let angle = particle.work[3] as u16 as f32 * TAU / 65536.0;
        Some(Vec3 {
            x: particle.pos.x + 6.0 * angle.sin(),
            y: particle.pos.y,
            z: particle.pos.z + 6.0 * angle.cos(),
        })
    }
}

/// `aWeatherRain_DecideMakeRainCount` with the source's fractional carry.
pub fn decide_rain_count(field: &mut WeatherField) -> usize {
    let base = match field.current_level {
        1 => 1.0,
        2 => 2.0,
        _ => 3.0,
    };
    field.spawn_accum += base * field.horizontal_scale;
    let count = field.spawn_accum as usize;
    field.spawn_accum -= count as f32;
    count
}

/// The spawn cadence selected by leaf, sakura, and snow profiles.
pub fn should_spawn(field: &mut WeatherField, profile: Profile, dt: f32) -> bool {
    let period = match profile {
        Profile::Snow => match field.current_level {
            1 => 8.0,
            2 => 4.0,
            _ => 2.0,
        },
        Profile::Sakura => {
            if field.current_level == 1 {
                8.0
            } else {
                4.0
            }
        }
        Profile::Leaf => {
            if field.current_level == 1 {
                16.0
            } else {
                8.0
            }
        }
        Profile::Fine | Profile::Rain => return false,
    };
    field.should_spawn_every(dt, period / field.horizontal_scale)
}

/// Initialize the profile-specific state performed by each `*_ct` function.
pub fn construct_particle(profile: Profile, particle: &mut Particle, random: [f32; 5]) {
    match profile {
        Profile::Fine | Profile::Rain | Profile::Snow => {}
        Profile::Sakura => {
            particle.work[0] = random[0] as i16;
            particle.work[1] = random[1] as i16;
            particle.work[2] = random[2] as i16;
        }
        Profile::Leaf => {
            particle.work[0] = random[0] as i16;
            particle.work[1] = random[1] as i16;
            particle.work[2] = random[2] as i16;
        }
    }
}
