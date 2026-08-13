use std::time::Duration;

use super::{Game, Input, Outcome, Rng, Scene};

pub(super) fn orbit(seed: u64) -> Box<dyn Game> {
    Box::new(Orbit::new(seed))
}

const TACTICS: [(&str, &str); 3] = [
    ("DRIVE", "steady two-point attempt"),
    ("ARC", "risky three-point attempt"),
    ("RESET", "one-point cut with stamina recovery"),
];

struct Orbit {
    possession: u8,
    tactic: usize,
    home: u16,
    away: u16,
    stamina: i8,
    rng: Rng,
    message: String,
    outcome: Outcome,
}

impl Orbit {
    fn new(seed: u64) -> Self {
        Self {
            possession: 0,
            tactic: 0,
            home: 0,
            away: 0,
            stamina: 8,
            rng: Rng::new(seed),
            message: "Choose a tactic, then resolve".into(),
            outcome: Outcome::Playing,
        }
    }

    fn resolve(&mut self) {
        let (chance, points, cost) = match self.tactic {
            0 => (7, 2, 2),
            1 => (4, 3, 3),
            _ => (8, 1, -2),
        };
        let adjusted = (chance + self.stamina.max(0) / 3).clamp(1, 9) as u64;
        let scored = self.rng.chance(adjusted, 10);
        if scored {
            self.home += points;
        }
        self.stamina = (self.stamina - cost).clamp(0, 10);
        let rival_points = if self.rng.chance(2, 10) { 3 } else { 2 };
        let rival_scored = self.rng.chance(6, 10);
        if rival_scored {
            self.away += rival_points;
        }
        self.possession += 1;
        self.message = format!(
            "You {} {} // rival {} {}",
            if scored { "score" } else { "miss" },
            points,
            if rival_scored { "scores" } else { "misses" },
            rival_points
        );
        if self.possession == 16 && self.home == self.away {
            if self.rng.chance(1, 2) {
                self.home += 1;
            } else {
                self.away += 1;
            }
        }
        if self.possession >= 12 && self.home != self.away {
            self.outcome = if self.home > self.away {
                Outcome::Won(format!("ORBIT COURT // {}-{}", self.home, self.away))
            } else {
                Outcome::Lost(format!("ORBIT COURT // {}-{}", self.home, self.away))
            };
        } else if self.possession >= 12 {
            self.message.push_str(" // sudden orbit");
        }
    }
}

impl Game for Orbit {
    fn input(&mut self, input: Input) {
        match input {
            Input::Left | Input::Char('a' | 'h') => {
                self.tactic = self.tactic.checked_sub(1).unwrap_or(TACTICS.len() - 1);
            }
            Input::Right | Input::Char('d' | 'l') => {
                self.tactic = (self.tactic + 1) % TACTICS.len();
            }
            Input::Enter | Input::Char(' ') => self.resolve(),
            _ => {}
        }
    }

    fn scene(&self, _width: u16, _height: u16) -> Scene {
        let meter = format!(
            "[{}{}]",
            "=".repeat(self.stamina as usize),
            " ".repeat(10 - self.stamina as usize)
        );
        let tactics = TACTICS
            .iter()
            .enumerate()
            .map(|(index, (name, summary))| {
                format!(
                    "{} {:5}  {}",
                    if index == self.tactic { ">" } else { " " },
                    name,
                    summary
                )
            })
            .collect::<Vec<_>>();
        Scene {
            title: "ORBIT COURT",
            status: format!(
                "possession {}  HOME {} : {} RIVAL  stamina {}  {}",
                self.possession + 1,
                self.home,
                self.away,
                meter,
                self.message
            ),
            lines: [
                vec![
                    "             .       *        .".into(),
                    "        HOME [O]-----------( ) RIVAL".into(),
                    "             '       *        '".into(),
                    String::new(),
                ],
                tactics,
            ]
            .concat(),
            help: "Left/Right tactic  Enter resolve  r restart  Esc menu  q quit",
        }
    }

    fn outcome(&self) -> &Outcome {
        &self.outcome
    }
}

pub(super) fn seedling(seed: u64) -> Box<dyn Game> {
    Box::new(Seedling::new(seed))
}

struct Seedling {
    day: u8,
    moisture: i8,
    health: i8,
    growth: u8,
    watered: bool,
    rng: Rng,
    weather: &'static str,
    message: String,
    outcome: Outcome,
}

impl Seedling {
    fn new(seed: u64) -> Self {
        Self {
            day: 0,
            moisture: 5,
            health: 5,
            growth: 0,
            watered: false,
            rng: Rng::new(seed),
            weather: "mild",
            message: "Water once if needed, then advance the day".into(),
            outcome: Outcome::Playing,
        }
    }

    fn water(&mut self) {
        if self.watered {
            self.message = "Already watered today".into();
        } else {
            self.moisture = (self.moisture + 3).min(10);
            self.watered = true;
            self.message = "The soil darkens".into();
        }
    }

    fn next_day(&mut self) {
        self.day += 1;
        let roll = self.rng.index(3);
        self.weather = ["sunny", "mild", "rainy"][roll];
        self.moisture += match roll {
            0 => -3,
            1 => -2,
            _ => 1,
        };
        self.moisture = self.moisture.clamp(0, 10);
        if (2..=8).contains(&self.moisture) {
            self.growth += 1;
            self.message = "A fresh leaf unfolds".into();
        } else {
            self.health -= 1;
            self.message = if self.moisture < 2 {
                "The soil is too dry"
            } else {
                "The roots are waterlogged"
            }
            .into();
        }
        self.watered = false;
        if self.health <= 0 {
            self.outcome = Outcome::Lost("SEEDLING // the cycle ended early".into());
        } else if self.day >= 7 {
            self.outcome = Outcome::Won(format!("SEEDLING // seven days, {} leaves", self.growth));
        }
    }

    fn plant(&self) -> Vec<String> {
        match self.growth {
            0 => vec!["     .".into(), "____/ \\____".into()],
            1..=2 => vec!["     |".into(), "    /".into(), "____|_____".into()],
            3..=4 => vec![
                "   \\ | /".into(),
                "    \\|".into(),
                "     |".into(),
                "_____|_____".into(),
            ],
            _ => vec![
                "   .-.-.".into(),
                " \\  |  /".into(),
                "  \\ | /".into(),
                "    |".into(),
                "____|_____".into(),
            ],
        }
    }
}

impl Game for Seedling {
    fn input(&mut self, input: Input) {
        match input {
            Input::Char('w') => self.water(),
            Input::Char('n') | Input::Enter => self.next_day(),
            _ => {}
        }
    }

    fn scene(&self, _width: u16, _height: u16) -> Scene {
        Scene {
            title: "SEEDLING",
            status: format!(
                "day {}/7  weather {}  moisture {}/10  health {}/5  {}",
                self.day, self.weather, self.moisture, self.health, self.message
            ),
            lines: self.plant(),
            help: "w water once  n/Enter next day  r restart  Esc menu  q quit",
        }
    }

    fn outcome(&self) -> &Outcome {
        &self.outcome
    }
}

pub(super) fn keybed(seed: u64) -> Box<dyn Game> {
    Box::new(Keybed::new(seed))
}

const NOTES: [char; 8] = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k'];

struct Keybed {
    sequence: [u8; 8],
    shown: usize,
    listening: bool,
    cursor: usize,
    misses: u8,
    message: String,
    outcome: Outcome,
}

impl Keybed {
    fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self {
            sequence: std::array::from_fn(|_| rng.index(NOTES.len()) as u8),
            shown: 0,
            listening: false,
            cursor: 0,
            misses: 0,
            message: "Watch the melody".into(),
            outcome: Outcome::Playing,
        }
    }

    fn play(&mut self, note: char) {
        if !self.listening {
            self.message = "Wait until the melody is hidden".into();
            return;
        }
        if note == NOTES[self.sequence[self.cursor] as usize] {
            self.cursor += 1;
            self.message = "Correct note".into();
            if self.cursor == self.sequence.len() {
                self.outcome = Outcome::Won("KEYBED // melody repeated".into());
            }
        } else {
            self.misses += 1;
            self.message = "Wrong note".into();
            if self.misses >= 3 {
                self.outcome = Outcome::Lost("KEYBED // three notes missed".into());
            }
        }
    }
}

impl Game for Keybed {
    fn input(&mut self, input: Input) {
        if let Input::Char(note) = input
            && NOTES.contains(&note)
        {
            self.play(note);
        }
    }

    fn tick(&mut self) {
        if self.listening {
            return;
        }
        if self.shown < self.sequence.len() {
            self.shown += 1;
        } else {
            self.listening = true;
            self.message = "Repeat the eight notes".into();
        }
    }

    fn tick_rate(&self) -> Option<Duration> {
        (!self.listening).then_some(Duration::from_millis(450))
    }

    fn scene(&self, _width: u16, _height: u16) -> Scene {
        let melody = if self.listening {
            "_ ".repeat(self.sequence.len())
        } else {
            self.sequence
                .iter()
                .take(self.shown)
                .map(|note| NOTES[*note as usize].to_ascii_uppercase().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let keys = NOTES
            .iter()
            .map(|note| format!("[{note}]"))
            .collect::<Vec<_>>()
            .join(" ");
        Scene {
            title: "KEYBED",
            status: format!(
                "notes {}/8  misses {}/3  {}",
                self.cursor, self.misses, self.message
            ),
            lines: vec![
                "MELODY".into(),
                melody,
                String::new(),
                "+--+--+--+--+--+--+--+--+".into(),
                format!("|  |  |  |  |  |  |  |  |"),
                "+--+--+--+--+--+--+--+--+".into(),
                keys,
            ],
            help: "a s d f g h j k play  r restart  Esc menu  q quit",
        }
    }

    fn outcome(&self) -> &Outcome {
        &self.outcome
    }
}

pub(super) fn scout(seed: u64) -> Box<dyn Game> {
    Box::new(Scout::new(seed))
}

const FIELD: [&str; 8] = [
    "##########",
    "#@..\"....#",
    "#..##..\".#",
    "#...\"....#",
    "#.###....##",
    "#..\"..\"..#",
    "#.......G#",
    "##########",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Position {
    x: usize,
    y: usize,
}

struct Foe {
    hp: i8,
    strength: i8,
}

struct Scout {
    player: Position,
    hp: i8,
    foe: Option<Foe>,
    bind_charges: u8,
    recruited: bool,
    encountered: bool,
    rng: Rng,
    message: String,
    outcome: Outcome,
}

impl Scout {
    fn new(seed: u64) -> Self {
        Self {
            player: Position { x: 1, y: 1 },
            hp: 10,
            foe: None,
            bind_charges: 3,
            recruited: false,
            encountered: false,
            rng: Rng::new(seed),
            message: "Find a field creature before reaching G".into(),
            outcome: Outcome::Playing,
        }
    }

    fn tile(position: Position) -> char {
        FIELD[position.y].as_bytes()[position.x] as char
    }

    fn walk(&mut self, dx: isize, dy: isize) {
        if self.foe.is_some() {
            self.message = "Resolve the encounter first".into();
            return;
        }
        let next = Position {
            x: self.player.x.saturating_add_signed(dx),
            y: self.player.y.saturating_add_signed(dy),
        };
        if Self::tile(next) == '#' {
            self.message = "Dense brush blocks the path".into();
            return;
        }
        self.player = next;
        match Self::tile(next) {
            '"' if !self.recruited && (!self.encountered || self.rng.chance(1, 2)) => {
                self.encountered = true;
                self.foe = Some(Foe { hp: 6, strength: 2 });
                self.message = "A bright field creature appears".into();
            }
            'G' if self.recruited => {
                self.outcome = Outcome::Won("FIELD SCOUT // recruit returned safely".into());
            }
            'G' => self.message = "Recruit a creature before leaving".into(),
            _ => self.message = "The field rustles".into(),
        }
    }

    fn strike(&mut self) {
        let damage = 2 + self.rng.index(3) as i8;
        if let Some(foe) = &mut self.foe {
            foe.hp -= damage;
            self.message = format!("Strike deals {damage}");
            if foe.hp <= 0 {
                self.foe = None;
                self.message = "The creature flees; find another".into();
                return;
            }
        }
        self.counterattack();
    }

    fn bind(&mut self) {
        if self.bind_charges == 0 {
            self.message = "No bind charges remain".into();
            self.counterattack();
            return;
        }
        self.bind_charges -= 1;
        if self.foe.as_ref().is_some_and(|foe| foe.hp <= 3) {
            self.recruited = true;
            self.foe = None;
            self.message = "Creature recruited; reach G".into();
        } else {
            self.message = "The creature breaks free".into();
            self.counterattack();
        }
    }

    fn counterattack(&mut self) {
        if let Some(foe) = &self.foe {
            self.hp -= foe.strength;
            if self.hp <= 0 {
                self.outcome = Outcome::Lost("FIELD SCOUT // forced to retreat".into());
            }
        }
    }
}

impl Game for Scout {
    fn input(&mut self, input: Input) {
        if self.foe.is_some() {
            match input {
                Input::Char('1') | Input::Enter => self.strike(),
                Input::Char('2') => self.bind(),
                Input::Char('3') => {
                    self.foe = None;
                    self.message = "You retreat into the grass".into();
                }
                _ => {}
            }
            return;
        }
        match input {
            Input::Up | Input::Char('w' | 'k') => self.walk(0, -1),
            Input::Down | Input::Char('s' | 'j') => self.walk(0, 1),
            Input::Left | Input::Char('a' | 'h') => self.walk(-1, 0),
            Input::Right | Input::Char('d' | 'l') => self.walk(1, 0),
            _ => {}
        }
    }

    fn scene(&self, _width: u16, _height: u16) -> Scene {
        let mut map = FIELD
            .iter()
            .map(|row| row.chars().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        map[self.player.y][self.player.x] = '@';
        let mut lines = map
            .into_iter()
            .map(|row| row.into_iter().collect())
            .collect::<Vec<_>>();
        if let Some(foe) = &self.foe {
            lines.extend([
                String::new(),
                format!("ENCOUNTER  creature hp {}/6", foe.hp.max(0)),
                "1 strike   2 bind   3 retreat".into(),
            ]);
        }
        Scene {
            title: "FIELD SCOUT",
            status: format!(
                "hp {}/10  bind {}  recruit {}  {}",
                self.hp.max(0),
                self.bind_charges,
                if self.recruited { "yes" } else { "no" },
                self.message
            ),
            lines,
            help: "WASD move  encounter: 1 strike 2 bind 3 retreat  Esc menu",
        }
    }

    fn outcome(&self) -> &Outcome {
        &self.outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_ends_after_regulation_or_sudden_death() {
        for seed in 0..256 {
            let mut game = Orbit::new(seed);
            for _ in 0..16 {
                if game.outcome != Outcome::Playing {
                    break;
                }
                game.resolve();
            }
            assert_ne!(game.outcome, Outcome::Playing, "seed {seed}");
            assert!(game.possession <= 16);
        }
    }

    #[test]
    fn seedling_and_keybed_have_finite_win_paths() {
        let mut seedling = Seedling::new(5);
        for _ in 0..7 {
            seedling.water();
            seedling.next_day();
        }
        assert_ne!(seedling.outcome, Outcome::Playing);

        let mut keybed = Keybed::new(6);
        keybed.listening = true;
        let sequence = keybed.sequence;
        for note in sequence {
            keybed.play(NOTES[note as usize]);
        }
        assert!(matches!(keybed.outcome, Outcome::Won(_)));
    }

    #[test]
    fn scout_recruits_a_weakened_creature() {
        let mut game = Scout::new(7);
        game.foe = Some(Foe { hp: 3, strength: 1 });
        game.bind();
        assert!(game.recruited);
        assert!(game.foe.is_none());
    }
}
