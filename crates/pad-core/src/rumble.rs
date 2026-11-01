//! Rumble Translation Engine: turns the game's XInput rumble into haptics for the Stadia pad.
//!
//! What a game actually sends (XInput / the virtual Xbox 360 pad): two motor levels, 0..=255, that
//! hold until the game sends new ones. Left = heavy (large, low-frequency mass), right = light (small,
//! high-frequency mass). There is no duration field, so any duration shaping (Competitive's 50 ms
//! tick) is a time-based pulse the engine generates itself; that is why the translator has `tick` and
//! `next_deadline` next to `request`.
//!
//! The translator is pure, allocation-free arithmetic on two bytes plus a few timestamps: well under a
//! microsecond per call (see the `translation_is_far_under_a_millisecond` test). The bridge runs it on its
//! own thread, so a game's rumble call never waits on USB.

use serde::{Deserialize, Serialize};

/// Rumble profile (stored per pad profile).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RumbleMode {
    /// Exactly what the game asks for.
    #[default]
    Default,
    /// No aim-shaking heavy motor; every hit becomes a short, crisp tick on the light motor.
    Competitive,
    /// Heavy thuds pushed to full, high-frequency buzz kept low.
    Immersive,
}

/// Motor levels in XInput units (0..=255).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Motors {
    pub heavy: u8,
    pub light: u8,
}

impl Motors {
    pub const OFF: Motors = Motors { heavy: 0, light: 0 };
    pub const fn new(heavy: u8, light: u8) -> Motors {
        Motors { heavy, light }
    }
}

const fn pct(p: u32) -> u8 {
    ((p * 255 + 50) / 100) as u8
}

// ---- Competitive (tactical) ----
/// Requests below this are ignored (engine idle, footsteps), or every rumble would become a tick.
pub const GATE: u8 = pct(8);
/// Tick strength: anything past the gate maps into 85..=100 %.
const TICK_MIN: u8 = pct(85);
/// The whole tick lasts 50 ms, the first 10 ms at full power: an ERM motor needs ~20-40 ms to spin
/// up, so a plain 50 ms pulse at 85 % would barely register.
pub const TICK_US: u64 = 50_000;
pub const KICK_US: u64 = 10_000;
/// A new tick needs a real new hit: a rise of at least this much, or a fresh hit after silence...
const RETRIGGER_STEP: u8 = pct(20);
/// ...and never faster than this, so a game ramping its rumble every frame can't machine-gun ticks.
pub const MIN_TICK_GAP_US: u64 = 90_000;

// ---- Immersive (deep impact) ----
/// Heavy requests above this are "impacts" and boosted into 95..=100 %.
const IMPACT: u8 = pct(50);
const IMPACT_MIN: u8 = pct(95);
/// The light (buzzy) motor never goes above this.
const LIGHT_CAP: u8 = pct(20);

/// Linear map of `v` from `[from_lo, 255]` to `[to_lo, 255]`.
fn lift(v: u8, from_lo: u8, to_lo: u8) -> u8 {
    let span_in = (255 - from_lo as u32).max(1);
    let x = (v.saturating_sub(from_lo)) as u32;
    (to_lo as u32 + x * (255 - to_lo as u32) / span_in) as u8
}

fn scale(v: u8, strength_pct: u8) -> u8 {
    ((v as u32 * strength_pct.min(100) as u32 + 50) / 100) as u8
}

/// A tick in progress: started at `start` with strength `level`.
#[derive(Clone, Copy, Debug)]
struct Tick {
    start: u64,
    level: u8,
    /// The full-power kick is over and the tick holds `level`.
    kicked: bool,
}

pub struct Translator {
    mode: RumbleMode,
    strength: u8,
    /// Competitive: the request level that armed the last tick, and that tick.
    armed_at: u8,
    tick: Option<Tick>,
    last_tick_start: Option<u64>,
}

impl Translator {
    pub fn new(mode: RumbleMode, strength_pct: u8) -> Translator {
        Translator { mode, strength: strength_pct.min(100), armed_at: 0, tick: None, last_tick_start: None }
    }

    /// Profile or strength changed. Any tick in progress is dropped (the next request re-arms).
    pub fn set(&mut self, mode: RumbleMode, strength_pct: u8) {
        if mode != self.mode {
            self.tick = None;
            self.armed_at = 0;
        }
        self.mode = mode;
        self.strength = strength_pct.min(100);
    }

    pub fn mode(&self) -> RumbleMode {
        self.mode
    }

    /// Forgets any tick in progress and the Competitive arming (e.g. before the Test rumble scene).
    pub fn reset(&mut self) {
        self.tick = None;
        self.armed_at = 0;
        self.last_tick_start = None;
    }

    /// The game set new motor levels at `now` (µs). Returns what the motors should do right now.
    pub fn request(&mut self, game: Motors, now: u64) -> Motors {
        let out = match self.mode {
            RumbleMode::Default => game,
            RumbleMode::Immersive => {
                let heavy = if game.heavy > IMPACT { lift(game.heavy, IMPACT, IMPACT_MIN) } else { game.heavy };
                Motors { heavy, light: game.light.min(LIGHT_CAP) }
            }
            RumbleMode::Competitive => {
                // Hits on either motor become a tick on the light one: many games put impacts on the
                // heavy motor only, which Competitive silences.
                let level = game.heavy.max(game.light);
                if level < GATE {
                    self.armed_at = 0; // silence re-arms
                } else {
                    let fresh = self.armed_at == 0 || level >= self.armed_at.saturating_add(RETRIGGER_STEP);
                    let rested = self.last_tick_start.is_none_or(|t| now.saturating_sub(t) >= MIN_TICK_GAP_US);
                    if fresh && rested {
                        self.tick = Some(Tick { start: now, level: lift(level, GATE, TICK_MIN), kicked: false });
                        self.last_tick_start = Some(now);
                    }
                    // Track the level so a slow rise counts from where it is, not from the first hit.
                    if fresh || level < self.armed_at {
                        self.armed_at = level;
                    }
                }
                self.tick_output(now)
            }
        };
        self.finish(out)
    }

    /// Time passed with no new request: returns the new motor levels if the output changed on its own
    /// (a Competitive tick ending its kick, or ending).
    pub fn tick(&mut self, now: u64) -> Option<Motors> {
        let t = self.tick.as_mut()?;
        let age = now.saturating_sub(t.start);
        if age >= TICK_US {
            self.tick = None;
            return Some(Motors::OFF);
        }
        if age >= KICK_US && !t.kicked {
            t.kicked = true;
            let level = t.level;
            return Some(self.finish(Motors::new(0, level)));
        }
        None
    }

    /// When `tick` next needs calling (µs), if a pulse is shaping the output.
    pub fn next_deadline(&self) -> Option<u64> {
        let t = self.tick?;
        Some(t.start + if t.kicked { TICK_US } else { KICK_US })
    }

    fn tick_output_raw(&self, now: u64) -> u8 {
        match self.tick {
            Some(t) if now.saturating_sub(t.start) < KICK_US => 255,
            Some(t) if now.saturating_sub(t.start) < TICK_US => t.level,
            _ => 0,
        }
    }

    fn tick_output(&self, now: u64) -> Motors {
        Motors::new(0, self.tick_output_raw(now))
    }

    fn finish(&self, m: Motors) -> Motors {
        Motors { heavy: scale(m.heavy, self.strength), light: scale(m.light, self.strength) }
    }
}

/// The "Test rumble" scene: the same game-side script for every profile, so the user feels how the
/// selected profile changes it. (ms from start, what the game asks for). Ends with all motors off.
pub const DEMO: &[(u32, Motors)] = &[
    // A big explosion: both motors, heavy dominant, held like a game would.
    (0, Motors::new(pct(90), pct(60))),
    (450, Motors::new(pct(35), pct(20))),
    (700, Motors::OFF),
    // A three-round burst: short light-motor hits.
    (950, Motors::new(pct(20), pct(75))),
    (1030, Motors::OFF),
    (1110, Motors::new(pct(20), pct(75))),
    (1190, Motors::OFF),
    (1270, Motors::new(pct(20), pct(75))),
    (1350, Motors::OFF),
    // An engine hum: low and steady on both motors.
    (1600, Motors::new(pct(30), pct(25))),
    (2200, Motors::OFF),
];

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1000;

    #[test]
    fn default_is_exact_passthrough() {
        let mut t = Translator::new(RumbleMode::Default, 100);
        for m in [Motors::new(0, 0), Motors::new(1, 254), Motors::new(255, 255), Motors::new(128, 7)] {
            assert_eq!(t.request(m, 0), m);
        }
        assert_eq!(t.next_deadline(), None, "no time shaping");
    }

    #[test]
    fn immersive_boosts_impacts_and_caps_buzz() {
        let mut t = Translator::new(RumbleMode::Immersive, 100);
        let o = t.request(Motors::new(pct(51), pct(80)), 0);
        assert!(o.heavy >= pct(95), "impact boosted to 95%+, got {}", o.heavy);
        assert_eq!(o.light, pct(20), "light capped at 20%");
        assert_eq!(t.request(Motors::new(255, 255), 0), Motors::new(255, pct(20)), "full impact stays full");
        assert_eq!(t.request(Motors::new(pct(40), pct(10)), 0), Motors::new(pct(40), pct(10)), "below 50%: passthrough");
        assert_eq!(t.next_deadline(), None, "duration is the game's");
    }

    #[test]
    fn competitive_turns_a_held_hit_into_one_50ms_tick() {
        let mut t = Translator::new(RumbleMode::Competitive, 100);
        let o = t.request(Motors::new(0, pct(60)), 0);
        assert_eq!(o, Motors::new(0, 255), "starts with a full-power kick");
        assert_eq!(t.next_deadline(), Some(KICK_US));
        let o = t.tick(KICK_US).unwrap();
        assert_eq!(o.heavy, 0, "heavy motor is never used");
        assert!((pct(85)..=pct(100)).contains(&o.light), "tick level 85-100%, got {}", o.light);
        assert_eq!(t.next_deadline(), Some(TICK_US));
        assert_eq!(t.tick(TICK_US), Some(Motors::OFF), "hard stop at 50 ms");
        assert_eq!(t.tick(300 * MS), None);
        // The game still holds the same rumble: no new tick.
        assert_eq!(t.request(Motors::new(0, pct(60)), 300 * MS), Motors::OFF);
    }

    #[test]
    fn competitive_ticks_from_heavy_only_hits_too() {
        let mut t = Translator::new(RumbleMode::Competitive, 100);
        assert_eq!(t.request(Motors::new(pct(90), 0), 0), Motors::new(0, 255));
    }

    #[test]
    fn competitive_ignores_idle_rumble_and_rearms_after_silence() {
        let mut t = Translator::new(RumbleMode::Competitive, 100);
        assert_eq!(t.request(Motors::new(pct(5), pct(5)), 0), Motors::OFF, "under the gate: nothing");
        assert_eq!(t.request(Motors::new(0, pct(50)), 0).light, 255);
        t.tick(TICK_US);
        t.request(Motors::OFF, 60 * MS);
        assert_eq!(t.request(Motors::new(0, pct(50)), 200 * MS).light, 255, "silence re-arms");
    }

    #[test]
    fn competitive_needs_a_real_new_hit_and_a_gap() {
        let mut t = Translator::new(RumbleMode::Competitive, 100);
        t.request(Motors::new(0, pct(40)), 0);
        t.tick(TICK_US);
        // A game easing its rumble up by a few % per frame must not retrigger.
        for (i, p) in [42, 44, 46, 48, 50, 52, 54, 56].into_iter().enumerate() {
            let now = 100 * MS + i as u64 * 16 * MS;
            assert_eq!(t.request(Motors::new(0, pct(p)), now), Motors::OFF, "{p}% is a ramp, not a hit");
        }
        // A sharp rise is a new hit...
        assert_eq!(t.request(Motors::new(0, pct(80)), 300 * MS).light, 255);
        // ...but not faster than the minimum gap: a hit just after that tick ended (at 350 ms) waits.
        t.request(Motors::OFF, 320 * MS);
        let held = t.request(Motors::new(0, pct(80)), 330 * MS).light;
        assert!(held >= pct(85) && held < 255, "the running tick holds its level until 350 ms, got {held}");
        assert_eq!(t.tick(350 * MS), Some(Motors::OFF));
        t.request(Motors::OFF, 355 * MS);
        assert_eq!(t.request(Motors::new(0, pct(80)), 360 * MS), Motors::OFF, "60 ms after the last tick: too soon");
        assert_eq!(t.request(Motors::OFF, 380 * MS), Motors::OFF);
        assert_eq!(t.request(Motors::new(0, pct(80)), 400 * MS).light, 255, "100 ms after: a new tick");
    }

    #[test]
    fn strength_scales_the_final_output() {
        let mut t = Translator::new(RumbleMode::Default, 50);
        assert_eq!(t.request(Motors::new(200, 100), 0), Motors::new(100, 50));
        t.set(RumbleMode::Default, 0);
        assert_eq!(t.request(Motors::new(255, 255), 0), Motors::OFF);
    }

    #[test]
    fn demo_ends_with_motors_off() {
        assert_eq!(DEMO.last().unwrap().1, Motors::OFF);
        assert!(DEMO.windows(2).all(|w| w[0].0 < w[1].0), "script is in time order");
    }

    #[test]
    fn translation_is_far_under_a_millisecond() {
        // The translator is branch-only arithmetic (no loops, allocation, locks or syscalls), so its cost
        // per call is constant. Wall-clock samples also catch the OS pausing this thread (Windows slices
        // are ~15 ms, and other tests run in parallel), so the budget is checked on the distribution:
        // the median and the 99.9th percentile, not on one unlucky sample.
        let n = 200_000u64;
        let mut samples = Vec::with_capacity(3 * n as usize);
        for mode in [RumbleMode::Default, RumbleMode::Competitive, RumbleMode::Immersive] {
            let mut t = Translator::new(mode, 80);
            for i in 0..n {
                let m = Motors::new((i * 37 % 256) as u8, (i * 91 % 256) as u8);
                let s = std::time::Instant::now();
                std::hint::black_box(t.request(std::hint::black_box(m), i * 4 * MS));
                std::hint::black_box(t.tick(i * 4 * MS + 2 * MS));
                samples.push(s.elapsed());
            }
        }
        samples.sort_unstable();
        let at = |q: f64| samples[((samples.len() - 1) as f64 * q) as usize];
        let (p50, p999, worst) = (at(0.5), at(0.999), *samples.last().unwrap());
        println!("request+tick: median {p50:?}, p99.9 {p999:?}, worst sample {worst:?} (includes OS preemption)");
        assert!(p50 < std::time::Duration::from_micros(1), "median {p50:?}");
        assert!(p999 < std::time::Duration::from_micros(50), "p99.9 {p999:?}: 20x under the 1 ms budget");
    }
}
