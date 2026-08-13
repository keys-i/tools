use std::collections::VecDeque;
use std::f32::consts::PI;
use std::time::Duration;

use super::{Game, Input, Outcome, Rng, Scene};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Point {
    x: i16,
    y: i16,
}

pub(super) fn serpent(seed: u64) -> Box<dyn Game> {
    Box::new(Serpent::new(seed))
}

const SNAKE_WIDTH: i16 = 30;
const SNAKE_HEIGHT: i16 = 14;

struct Serpent {
    body: VecDeque<Point>,
    direction: Point,
    queued: Point,
    food: Point,
    score: u8,
    paused: bool,
    rng: Rng,
    outcome: Outcome,
}

impl Serpent {
    fn new(seed: u64) -> Self {
        let body = VecDeque::from([
            Point { x: 15, y: 7 },
            Point { x: 14, y: 7 },
            Point { x: 13, y: 7 },
        ]);
        let mut game = Self {
            body,
            direction: Point { x: 1, y: 0 },
            queued: Point { x: 1, y: 0 },
            food: Point { x: 0, y: 0 },
            score: 0,
            paused: false,
            rng: Rng::new(seed),
            outcome: Outcome::Playing,
        };
        game.place_food();
        game
    }

    fn steer(&mut self, direction: Point) {
        if direction.x != -self.direction.x || direction.y != -self.direction.y {
            self.queued = direction;
        }
    }

    fn place_food(&mut self) {
        for _ in 0..SNAKE_WIDTH * SNAKE_HEIGHT {
            let candidate = Point {
                x: self.rng.index(SNAKE_WIDTH as usize) as i16,
                y: self.rng.index(SNAKE_HEIGHT as usize) as i16,
            };
            if !self.body.contains(&candidate) {
                self.food = candidate;
                return;
            }
        }
        self.outcome = Outcome::Won("SERPENT RUN // board filled".into());
    }
}

impl Game for Serpent {
    fn input(&mut self, input: Input) {
        match input {
            Input::Up | Input::Char('w' | 'k') => self.steer(Point { x: 0, y: -1 }),
            Input::Down | Input::Char('s' | 'j') => self.steer(Point { x: 0, y: 1 }),
            Input::Left | Input::Char('a' | 'h') => self.steer(Point { x: -1, y: 0 }),
            Input::Right | Input::Char('d' | 'l') => self.steer(Point { x: 1, y: 0 }),
            Input::Char('p') => self.paused = !self.paused,
            _ => {}
        }
    }

    fn tick(&mut self) {
        if self.paused || self.outcome != Outcome::Playing {
            return;
        }
        self.direction = self.queued;
        let head = self.body[0];
        let next = Point {
            x: head.x + self.direction.x,
            y: head.y + self.direction.y,
        };
        let tail = self.body.back().copied();
        let hits_body = self.body.contains(&next) && !(tail == Some(next) && next != self.food);
        if next.x < 0 || next.y < 0 || next.x >= SNAKE_WIDTH || next.y >= SNAKE_HEIGHT || hits_body
        {
            self.outcome = Outcome::Lost("SERPENT RUN // collision".into());
            return;
        }
        self.body.push_front(next);
        if next == self.food {
            self.score += 1;
            if self.score >= 12 {
                self.outcome = Outcome::Won("SERPENT RUN // twelve sparks collected".into());
            } else {
                self.place_food();
            }
        } else {
            self.body.pop_back();
        }
    }

    fn tick_rate(&self) -> Option<Duration> {
        (!self.paused && self.outcome == Outcome::Playing).then_some(Duration::from_millis(115))
    }

    fn scene(&self, _width: u16, _height: u16) -> Scene {
        let mut board = vec![vec![' '; SNAKE_WIDTH as usize]; SNAKE_HEIGHT as usize];
        board[self.food.y as usize][self.food.x as usize] = '*';
        for (index, point) in self.body.iter().enumerate() {
            board[point.y as usize][point.x as usize] = if index == 0 { '@' } else { 'o' };
        }
        let border = format!("+{}+", "-".repeat(SNAKE_WIDTH as usize));
        let mut lines = Vec::with_capacity(SNAKE_HEIGHT as usize + 2);
        lines.push(border.clone());
        lines.extend(
            board
                .into_iter()
                .map(|row| format!("|{}|", row.into_iter().collect::<String>())),
        );
        lines.push(border);
        Scene {
            title: "SERPENT RUN",
            status: format!(
                "sparks {}/12{}",
                self.score,
                if self.paused { " // paused" } else { "" }
            ),
            lines,
            help: "Arrows/WASD steer  p pause  r restart  Esc menu  q quit",
        }
    }

    fn outcome(&self) -> &Outcome {
        &self.outcome
    }
}

pub(super) fn delve(seed: u64) -> Box<dyn Game> {
    Box::new(Delve::new(seed))
}

const FLOORS: [[&str; 9]; 3] = [
    [
        "###############",
        "#@....#....K..#",
        "#.....#.......#",
        "#.....#..M....#",
        "#.............#",
        "#...M.#.......#",
        "#.....#..+....#",
        "#.........>...#",
        "###############",
    ],
    [
        "###############",
        "#@......#.....#",
        "#.#####.#.###.#",
        "#.....#...#K..#",
        "#..M..#####...#",
        "#.............#",
        "#.###..M..###.#",
        "#..+......>...#",
        "###############",
    ],
    [
        "###############",
        "#@............#",
        "#.#####.#####.#",
        "#.#...#.....#.#",
        "#.#.R...M...#.#",
        "#.#...#####.#.#",
        "#...M.....+...#",
        "#............>#",
        "###############",
    ],
];

struct Delve {
    floor: usize,
    map: Vec<Vec<char>>,
    player: Point,
    enemies: Vec<Point>,
    hp: i8,
    key: bool,
    relic: bool,
    rng: Rng,
    message: String,
    outcome: Outcome,
}

impl Delve {
    fn new(seed: u64) -> Self {
        let mut game = Self {
            floor: 0,
            map: Vec::new(),
            player: Point { x: 1, y: 1 },
            enemies: Vec::new(),
            hp: 8,
            key: false,
            relic: false,
            rng: Rng::new(seed),
            message: "Find the floor key, then reach >".into(),
            outcome: Outcome::Playing,
        };
        game.load_floor();
        game
    }

    fn load_floor(&mut self) {
        self.map = FLOORS[self.floor]
            .iter()
            .map(|row| row.chars().collect())
            .collect();
        self.enemies.clear();
        for (y, row) in self.map.iter_mut().enumerate() {
            for (x, cell) in row.iter_mut().enumerate() {
                match *cell {
                    '@' => {
                        self.player = Point {
                            x: x as i16,
                            y: y as i16,
                        };
                        *cell = '.';
                    }
                    'M' => {
                        self.enemies.push(Point {
                            x: x as i16,
                            y: y as i16,
                        });
                        *cell = '.';
                    }
                    _ => {}
                }
            }
        }
        self.key = false;
    }

    fn tile(&self, point: Point) -> char {
        self.map
            .get(point.y as usize)
            .and_then(|row| row.get(point.x as usize))
            .copied()
            .unwrap_or('#')
    }

    fn turn(&mut self, direction: Point) {
        let target = Point {
            x: self.player.x + direction.x,
            y: self.player.y + direction.y,
        };
        if self.tile(target) == '#' {
            self.message = "Stone blocks the way".into();
            return;
        }
        if let Some(index) = self.enemies.iter().position(|enemy| *enemy == target) {
            self.enemies.remove(index);
            self.message = "You drive back a shade".into();
        } else {
            self.player = target;
            self.message.clear();
        }
        self.enemy_turn();
    }

    fn enemy_turn(&mut self) {
        for enemy in &mut self.enemies {
            let dx = (self.player.x - enemy.x).signum();
            let dy = (self.player.y - enemy.y).signum();
            if (self.player.x - enemy.x).abs() + (self.player.y - enemy.y).abs() == 1 {
                self.hp -= 1;
                self.message = "A shade hits you".into();
                continue;
            }
            let step = if self.rng.chance(1, 2) {
                Point {
                    x: enemy.x + dx,
                    y: enemy.y,
                }
            } else {
                Point {
                    x: enemy.x,
                    y: enemy.y + dy,
                }
            };
            let tile = self.map[step.y as usize][step.x as usize];
            if tile != '#' && step != self.player {
                *enemy = step;
            }
        }
        if self.hp <= 0 {
            self.outcome = Outcome::Lost("DELVE // the dungeon takes its toll".into());
        }
    }

    fn gather(&mut self) {
        let tile = self.tile(self.player);
        match tile {
            'K' => {
                self.key = true;
                self.map[self.player.y as usize][self.player.x as usize] = '.';
                self.message = "Floor key secured".into();
            }
            'R' => {
                self.relic = true;
                self.map[self.player.y as usize][self.player.x as usize] = '.';
                self.message = "The signal relic hums".into();
            }
            '+' => {
                self.hp = (self.hp + 3).min(8);
                self.map[self.player.y as usize][self.player.x as usize] = '.';
                self.message = "Restored three health".into();
            }
            _ => self.message = "Nothing to gather here".into(),
        }
    }

    fn descend(&mut self) {
        if self.tile(self.player) != '>' {
            self.message = "Stand on > to descend".into();
        } else if self.floor < 2 && self.key {
            self.floor += 1;
            self.load_floor();
            self.message = "A colder floor opens below".into();
        } else if self.floor == 2 && self.relic {
            self.outcome = Outcome::Won("DELVE // relic recovered".into());
        } else {
            self.message = if self.floor == 2 {
                "The exit needs the relic"
            } else {
                "The stairs need the floor key"
            }
            .into();
        }
    }
}

impl Game for Delve {
    fn input(&mut self, input: Input) {
        match input {
            Input::Up | Input::Char('w' | 'k') => self.turn(Point { x: 0, y: -1 }),
            Input::Down | Input::Char('s' | 'j') => self.turn(Point { x: 0, y: 1 }),
            Input::Left | Input::Char('a' | 'h') => self.turn(Point { x: -1, y: 0 }),
            Input::Right | Input::Char('d' | 'l') => self.turn(Point { x: 1, y: 0 }),
            Input::Char('g') => self.gather(),
            Input::Char('>') | Input::Enter => self.descend(),
            _ => {}
        }
    }

    fn scene(&self, _width: u16, _height: u16) -> Scene {
        let mut map = self.map.clone();
        for enemy in &self.enemies {
            map[enemy.y as usize][enemy.x as usize] = 'M';
        }
        map[self.player.y as usize][self.player.x as usize] = '@';
        Scene {
            title: "DELVE",
            status: format!(
                "floor {}/3  hp {}  key {}  relic {}  {}",
                self.floor + 1,
                self.hp.max(0),
                if self.key { "yes" } else { "no" },
                if self.relic { "yes" } else { "no" },
                self.message
            ),
            lines: map
                .into_iter()
                .map(|row| row.into_iter().collect())
                .collect(),
            help: "Arrows/WASD move  g gather  Enter descend  r restart  Esc menu",
        }
    }

    fn outcome(&self) -> &Outcome {
        &self.outcome
    }
}

pub(super) fn vector(seed: u64) -> Box<dyn Game> {
    Box::new(Vector::new(seed))
}

const VECTOR_MAP: [&str; 12] = [
    "################",
    "#..............#",
    "#......#..C....#",
    "#......#.......#",
    "#..............#",
    "#..#######.....#",
    "#..............#",
    "#.....#........#",
    "#.....#..###...#",
    "#.....#......E.#",
    "#@.............#",
    "################",
];

#[derive(Clone, Copy)]
struct Target {
    x: f32,
    y: f32,
    alive: bool,
}

struct Vector {
    x: f32,
    y: f32,
    angle: f32,
    hp: i8,
    ammo: u8,
    card: bool,
    targets: [Target; 2],
    message: String,
    outcome: Outcome,
}

impl Vector {
    fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self {
            x: 1.5,
            y: 10.5,
            angle: 0.0,
            hp: 6,
            ammo: 8,
            card: false,
            targets: [
                Target {
                    x: 11.5,
                    y: 4.5,
                    alive: true,
                },
                Target {
                    x: 3.5 + rng.index(3) as f32,
                    y: 7.5,
                    alive: true,
                },
            ],
            message: "Locate C, then activate E".into(),
            outcome: Outcome::Playing,
        }
    }

    fn tile(x: f32, y: f32) -> char {
        VECTOR_MAP
            .get(y.floor().max(0.0) as usize)
            .and_then(|row| row.as_bytes().get(x.floor().max(0.0) as usize))
            .map_or('#', |byte| *byte as char)
    }

    fn walk(&mut self, distance: f32, strafe: f32) {
        let next_x = self.x + self.angle.cos() * distance - self.angle.sin() * strafe;
        let next_y = self.y + self.angle.sin() * distance + self.angle.cos() * strafe;
        if Self::tile(next_x, self.y) != '#' {
            self.x = next_x;
        }
        if Self::tile(self.x, next_y) != '#' {
            self.y = next_y;
        }
        if Self::tile(self.x, self.y) == 'C' {
            self.card = true;
            self.message = "Access card secured".into();
        }
        for target in self.targets.iter().filter(|target| target.alive) {
            if (target.x - self.x).hypot(target.y - self.y) < 0.9 {
                self.hp -= 1;
                self.message = "A sentinel strikes".into();
            }
        }
        if self.hp <= 0 {
            self.outcome = Outcome::Lost("EXIT VECTOR // signal lost".into());
        }
    }

    fn fire(&mut self) {
        if self.ammo == 0 {
            self.message = "No pulses remain".into();
            return;
        }
        self.ammo -= 1;
        let mut hit = None;
        let mut distance = f32::MAX;
        for (index, target) in self.targets.iter().enumerate() {
            if !target.alive {
                continue;
            }
            let dx = target.x - self.x;
            let dy = target.y - self.y;
            let target_angle = dy.atan2(dx);
            let delta = normalize_angle(target_angle - self.angle).abs();
            let target_distance = dx.hypot(dy);
            if delta < 0.14 && target_distance < distance && self.line_clear(target.x, target.y) {
                distance = target_distance;
                hit = Some(index);
            }
        }
        if let Some(index) = hit {
            self.targets[index].alive = false;
            self.message = "Sentinel dispersed".into();
        } else {
            self.message = "Pulse misses".into();
        }
    }

    fn line_clear(&self, target_x: f32, target_y: f32) -> bool {
        let distance = (target_x - self.x).hypot(target_y - self.y);
        let steps = (distance / 0.1) as usize;
        (1..steps).all(|step| {
            let ratio = step as f32 / steps as f32;
            Self::tile(
                self.x + (target_x - self.x) * ratio,
                self.y + (target_y - self.y) * ratio,
            ) != '#'
        })
    }

    fn activate(&mut self) {
        if Self::tile(self.x, self.y) != 'E' {
            self.message = "Stand on E to activate the lift".into();
        } else if self.card {
            self.outcome = Outcome::Won("EXIT VECTOR // lift engaged".into());
        } else {
            self.message = "The lift needs an access card".into();
        }
    }

    fn ray_view(&self, width: usize, height: usize) -> Vec<Vec<char>> {
        let mut view = vec![vec![' '; width]; height];
        for column in 0..width {
            let ray = self.angle + (column as f32 / width as f32 - 0.5) * PI / 3.0;
            let mut distance = 0.05;
            while distance < 18.0 {
                let x = self.x + ray.cos() * distance;
                let y = self.y + ray.sin() * distance;
                if Self::tile(x, y) == '#' {
                    break;
                }
                distance += 0.05;
            }
            let wall = ((height as f32 / distance.max(0.2)) * 1.6) as usize;
            let top = height.saturating_sub(wall) / 2;
            let bottom = (top + wall).min(height);
            for (row, cells) in view.iter_mut().enumerate() {
                cells[column] = if row >= top && row < bottom {
                    if distance < 3.0 { '#' } else { '|' }
                } else if row >= height / 2 {
                    '.'
                } else {
                    ' '
                };
            }
        }
        view
    }
}

fn normalize_angle(mut angle: f32) -> f32 {
    while angle > PI {
        angle -= 2.0 * PI;
    }
    while angle < -PI {
        angle += 2.0 * PI;
    }
    angle
}

impl Game for Vector {
    fn input(&mut self, input: Input) {
        match input {
            Input::Up | Input::Char('w') => self.walk(0.35, 0.0),
            Input::Down | Input::Char('s') => self.walk(-0.35, 0.0),
            Input::Left | Input::Char('j') => self.angle -= 0.16,
            Input::Right | Input::Char('l') => self.angle += 0.16,
            Input::Char('a') => self.walk(0.0, -0.3),
            Input::Char('d') => self.walk(0.0, 0.3),
            Input::Char(' ') | Input::Enter => self.fire(),
            Input::Char('x') => self.activate(),
            _ => {}
        }
        self.angle = normalize_angle(self.angle);
    }

    fn scene(&self, width: u16, height: u16) -> Scene {
        let ray_width = usize::from(width.saturating_sub(22)).clamp(24, 50);
        let ray_height = usize::from(height.saturating_sub(8)).clamp(10, 14);
        let view = self.ray_view(ray_width, ray_height);
        let mut map = VECTOR_MAP
            .iter()
            .map(|row| row.chars().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        map[self.y.floor() as usize][self.x.floor() as usize] = '@';
        for target in self.targets.iter().filter(|target| target.alive) {
            map[target.y.floor() as usize][target.x.floor() as usize] = 'M';
        }
        let lines = (0..ray_height)
            .map(|row| {
                let mini = if row < map.len() {
                    map[row].iter().collect::<String>()
                } else {
                    " ".repeat(16)
                };
                format!("{mini}  {}", view[row].iter().collect::<String>())
            })
            .collect();
        Scene {
            title: "EXIT VECTOR",
            status: format!(
                "hp {}  pulses {}  card {}  {}",
                self.hp.max(0),
                self.ammo,
                if self.card { "yes" } else { "no" },
                self.message
            ),
            lines,
            help: "W/S move  A/D strafe  J/L turn  Space fire  x exit  Esc menu",
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
    fn serpent_rejects_reverse_and_wins_at_twelve() {
        let mut game = Serpent::new(1);
        game.steer(Point { x: -1, y: 0 });
        assert_eq!(game.queued, Point { x: 1, y: 0 });
        game.score = 11;
        game.food = Point { x: 16, y: 7 };
        game.tick();
        assert!(matches!(game.outcome, Outcome::Won(_)));
    }

    #[test]
    fn delve_gathers_key_and_vector_respects_walls() {
        let mut delve = Delve::new(2);
        delve.player = Point { x: 11, y: 1 };
        delve.gather();
        assert!(delve.key);

        let mut vector = Vector::new(3);
        vector.x = 1.1;
        vector.angle = PI;
        vector.walk(0.35, 0.0);
        assert!(vector.x >= 1.0);
    }
}
