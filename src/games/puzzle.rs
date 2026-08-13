use std::time::Instant;

use super::{Game, Input, Outcome, Rng, Scene};

pub(super) fn merge(seed: u64) -> Box<dyn Game> {
    Box::new(Merge::new(seed))
}

#[derive(Clone, Copy)]
enum Slide {
    Left,
    Right,
    Up,
    Down,
}

struct Merge {
    cells: [u16; 16],
    score: u32,
    rng: Rng,
    message: String,
    outcome: Outcome,
}

impl Merge {
    fn new(seed: u64) -> Self {
        let mut game = Self {
            cells: [0; 16],
            score: 0,
            rng: Rng::new(seed),
            message: "Reach 256".into(),
            outcome: Outcome::Playing,
        };
        game.spawn();
        game.spawn();
        game
    }

    fn spawn(&mut self) {
        let empty = self
            .cells
            .iter()
            .enumerate()
            .filter_map(|(index, value)| (*value == 0).then_some(index))
            .collect::<Vec<_>>();
        if let Some(&index) = empty.get(self.rng.index(empty.len())) {
            self.cells[index] = if self.rng.chance(1, 10) { 4 } else { 2 };
        }
    }

    fn slide(&mut self, direction: Slide) {
        let before = self.cells;
        let mut gained = 0;
        for line in 0..4 {
            let positions = (0..4)
                .map(|offset| match direction {
                    Slide::Left => line * 4 + offset,
                    Slide::Right => line * 4 + 3 - offset,
                    Slide::Up => offset * 4 + line,
                    Slide::Down => (3 - offset) * 4 + line,
                })
                .collect::<Vec<_>>();
            let values = positions
                .iter()
                .map(|position| self.cells[*position])
                .filter(|value| *value != 0)
                .collect::<Vec<_>>();
            let mut merged = Vec::with_capacity(4);
            let mut index = 0;
            while index < values.len() {
                if values.get(index + 1) == values.get(index) {
                    let value = values[index] * 2;
                    merged.push(value);
                    gained += u32::from(value);
                    index += 2;
                } else {
                    merged.push(values[index]);
                    index += 1;
                }
            }
            merged.resize(4, 0);
            for (position, value) in positions.into_iter().zip(merged) {
                self.cells[position] = value;
            }
        }
        if self.cells == before {
            if can_move(&self.cells) {
                self.message = "No tiles moved".into();
            } else {
                self.outcome = Outcome::Lost("NUMBER MERGE // no moves remain".into());
            }
            return;
        }
        self.score += gained;
        self.spawn();
        self.message = if gained > 0 {
            format!("Merged +{gained}")
        } else {
            "Shifted".into()
        };
        if self.cells.iter().any(|value| *value >= 256) {
            self.outcome = Outcome::Won("NUMBER MERGE // 256 reached".into());
        } else if !can_move(&self.cells) {
            self.outcome = Outcome::Lost("NUMBER MERGE // no moves remain".into());
        }
    }
}

fn can_move(cells: &[u16; 16]) -> bool {
    cells.contains(&0)
        || (0..4)
            .any(|row| (0..3).any(|column| cells[row * 4 + column] == cells[row * 4 + column + 1]))
        || (0..3).any(|row| {
            (0..4).any(|column| cells[row * 4 + column] == cells[(row + 1) * 4 + column])
        })
}

impl Game for Merge {
    fn input(&mut self, input: Input) {
        match input {
            Input::Left | Input::Char('a' | 'h') => self.slide(Slide::Left),
            Input::Right | Input::Char('d' | 'l') => self.slide(Slide::Right),
            Input::Up | Input::Char('w' | 'k') => self.slide(Slide::Up),
            Input::Down | Input::Char('s' | 'j') => self.slide(Slide::Down),
            _ => {}
        }
    }

    fn scene(&self, _width: u16, _height: u16) -> Scene {
        let border = "+------+------+------+------+";
        let mut lines = Vec::with_capacity(9);
        lines.push(border.into());
        for row in 0..4 {
            lines.push(
                (0..4)
                    .map(|column| {
                        let value = self.cells[row * 4 + column];
                        if value == 0 {
                            "|      ".to_owned()
                        } else {
                            format!("|{value:^6}")
                        }
                    })
                    .collect::<String>()
                    + "|",
            );
            lines.push(border.into());
        }
        Scene {
            title: "NUMBER MERGE",
            status: format!("score {}  {}", self.score, self.message),
            lines,
            help: "Arrows/WASD slide  r restart  Esc menu  q quit",
        }
    }

    fn outcome(&self) -> &Outcome {
        &self.outcome
    }
}

pub(super) fn cards(seed: u64) -> Box<dyn Game> {
    Box::new(Cards::new(seed))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Card {
    rank: u8,
    suit: char,
}

struct Cards {
    deck: Vec<Card>,
    hand: [Card; 5],
    selected: [bool; 5],
    round: usize,
    redraw: bool,
    total: u32,
    message: String,
    outcome: Outcome,
}

impl Cards {
    const TARGETS: [u32; 3] = [18, 24, 30];

    fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let mut deck = ['S', 'H', 'D', 'C']
            .into_iter()
            .flat_map(|suit| (1..=13).map(move |rank| Card { rank, suit }))
            .collect::<Vec<_>>();
        for index in (1..deck.len()).rev() {
            let other = rng.index(index + 1);
            deck.swap(index, other);
        }
        let hand = std::array::from_fn(|_| deck.pop().expect("full deck"));
        Self {
            deck,
            hand,
            selected: [false; 5],
            round: 0,
            redraw: true,
            total: 0,
            message: "Select up to three cards".into(),
            outcome: Outcome::Playing,
        }
    }

    fn toggle(&mut self, index: usize) {
        if self.selected[index] {
            self.selected[index] = false;
        } else if self.selected.iter().filter(|value| **value).count() < 3 {
            self.selected[index] = true;
        } else {
            self.message = "Only three cards may score".into();
        }
    }

    fn redraw(&mut self) {
        if !self.redraw {
            self.message = "Redraw already used".into();
            return;
        }
        for index in 0..5 {
            if !self.selected[index]
                && let Some(card) = self.deck.pop()
            {
                self.hand[index] = card;
            }
        }
        self.redraw = false;
        self.message = "Unselected cards replaced".into();
    }

    fn score(&mut self) {
        let selected = self
            .hand
            .iter()
            .zip(self.selected)
            .filter_map(|(card, selected)| selected.then_some(*card))
            .collect::<Vec<_>>();
        if selected.is_empty() {
            self.message = "Select at least one card".into();
            return;
        }
        let score = score_cards(&selected);
        let target = Self::TARGETS[self.round];
        if score < target {
            self.outcome =
                Outcome::Lost(format!("HAND THRESHOLD // {score} falls short of {target}"));
            return;
        }
        self.total += score;
        self.round += 1;
        if self.round == Self::TARGETS.len() {
            self.outcome = Outcome::Won(format!(
                "HAND THRESHOLD // all rounds cleared ({})",
                self.total
            ));
            return;
        }
        if self.deck.len() < 5 {
            self.outcome = Outcome::Lost("HAND THRESHOLD // deck exhausted".into());
            return;
        }
        self.hand = std::array::from_fn(|_| self.deck.pop().expect("checked deck"));
        self.selected = [false; 5];
        self.redraw = true;
        self.message = format!("Round cleared with {score}");
    }
}

fn score_cards(cards: &[Card]) -> u32 {
    let mut counts = [0_u8; 14];
    let mut score = 0;
    for card in cards {
        counts[card.rank as usize] += 1;
        score += u32::from(card.rank.min(10));
    }
    score
        + counts
            .iter()
            .map(|count| match count {
                2 => 8,
                3 => 20,
                _ => 0,
            })
            .sum::<u32>()
}

fn card_label(card: Card) -> String {
    let rank = match card.rank {
        1 => "A".into(),
        11 => "J".into(),
        12 => "Q".into(),
        13 => "K".into(),
        value => value.to_string(),
    };
    format!("{rank}{}", card.suit)
}

impl Game for Cards {
    fn input(&mut self, input: Input) {
        match input {
            Input::Char(character @ '1'..='5') => {
                self.toggle(character as usize - '1' as usize);
            }
            Input::Char('d') => self.redraw(),
            Input::Enter => self.score(),
            _ => {}
        }
    }

    fn scene(&self, _width: u16, _height: u16) -> Scene {
        let cards = self
            .hand
            .iter()
            .enumerate()
            .map(|(index, card)| {
                format!(
                    "{}{:^5}{}",
                    if self.selected[index] { '[' } else { ' ' },
                    card_label(*card),
                    if self.selected[index] { ']' } else { ' ' }
                )
            })
            .collect::<Vec<_>>();
        Scene {
            title: "HAND THRESHOLD",
            status: format!(
                "round {}/3  target {}  total {}  redraw {}  {}",
                self.round + 1,
                Self::TARGETS[self.round.min(2)],
                self.total,
                if self.redraw { "ready" } else { "used" },
                self.message
            ),
            lines: vec![
                "Build a scoring set of at most three cards".into(),
                String::new(),
                cards.join("  "),
                "  1        2        3        4        5".into(),
                String::new(),
                "Ranks score face value; pairs +8, triples +20".into(),
            ],
            help: "1-5 select  d redraw unselected  Enter score  r restart  Esc menu",
        }
    }

    fn outcome(&self) -> &Outcome {
        &self.outcome
    }
}

pub(super) fn sprint(seed: u64) -> Box<dyn Game> {
    Box::new(Sprint::new(seed))
}

const PROMPTS: &[&str] = &[
    "quiet terminals make sharp tools",
    "small loops keep latency honest",
    "measure twice then remove the wrapper",
    "clean code leaves the cursor where it found it",
];

struct Sprint {
    prompt: &'static str,
    typed: String,
    mistakes: u8,
    started: Instant,
    message: String,
    outcome: Outcome,
}

impl Sprint {
    fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self {
            prompt: PROMPTS[rng.index(PROMPTS.len())],
            typed: String::new(),
            mistakes: 0,
            started: Instant::now(),
            message: "Type the prompt exactly".into(),
            outcome: Outcome::Playing,
        }
    }

    fn type_character(&mut self, character: char) {
        if character.is_control() || self.typed.len() >= self.prompt.len() {
            return;
        }
        let expected = self.prompt.as_bytes()[self.typed.len()] as char;
        if character != expected {
            self.mistakes += 1;
            self.message = format!("Expected {expected:?}");
            if self.mistakes > 5 {
                self.outcome = Outcome::Lost("TYPE SPRINT // too many mistakes".into());
            }
        }
        self.typed.push(character);
        if self.typed == self.prompt {
            let seconds = self.started.elapsed().as_secs_f64().max(0.1);
            let words = self.prompt.split_whitespace().count() as f64;
            self.outcome = Outcome::Won(format!(
                "TYPE SPRINT // {:.0} wpm with {} mistakes",
                words / seconds * 60.0,
                self.mistakes
            ));
        }
    }
}

impl Game for Sprint {
    fn input(&mut self, input: Input) {
        match input {
            Input::Char(character) => self.type_character(character),
            Input::Backspace => {
                self.typed.pop();
                self.message = "Correct the line".into();
            }
            Input::Enter if self.typed != self.prompt => {
                self.message = "The prompt is not complete".into();
            }
            _ => {}
        }
    }

    fn scene(&self, _width: u16, _height: u16) -> Scene {
        let caret = format!("{}^", " ".repeat(self.typed.len()));
        Scene {
            title: "TYPE SPRINT",
            status: format!(
                "progress {}/{}  mistakes {}/5  {}",
                self.typed.len(),
                self.prompt.len(),
                self.mistakes,
                self.message
            ),
            lines: vec![
                "TARGET".into(),
                self.prompt.into(),
                String::new(),
                "INPUT".into(),
                self.typed.clone(),
                caret,
            ],
            help: "Type  Backspace correct  Ctrl-R restart  Esc menu  Ctrl-C quit",
        }
    }

    fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    fn accepts_text(&self) -> bool {
        true
    }
}

pub(super) fn glyphs(seed: u64) -> Box<dyn Game> {
    Box::new(Glyphs::new(seed))
}

const ANSWERS: &[&str] = &[
    "AMBER", "BRISK", "CLOUD", "EMBER", "FROST", "GRAIN", "HONEY", "IVORY", "MANGO", "NORTH",
    "OCEAN", "PRISM", "RIVER", "SOLAR", "THORN", "VIVID",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Absent,
    Present,
    Correct,
}

struct Glyphs {
    answer: &'static str,
    current: String,
    rows: Vec<(String, [Mark; 5])>,
    message: String,
    outcome: Outcome,
}

impl Glyphs {
    fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self {
            answer: ANSWERS[rng.index(ANSWERS.len())],
            current: String::new(),
            rows: Vec::new(),
            message: "Enter five letters".into(),
            outcome: Outcome::Playing,
        }
    }

    fn submit(&mut self) {
        if self.current.len() != 5 {
            self.message = "A guess needs five letters".into();
            return;
        }
        let guess = std::mem::take(&mut self.current);
        let marks = grade(self.answer, &guess);
        let won = marks.iter().all(|mark| *mark == Mark::Correct);
        self.rows.push((guess, marks));
        if won {
            self.outcome = Outcome::Won(format!("FIVE GLYPHS // {} found", self.answer));
        } else if self.rows.len() == 6 {
            self.outcome = Outcome::Lost(format!("FIVE GLYPHS // answer was {}", self.answer));
        } else {
            self.message = format!("{} attempts remain", 6 - self.rows.len());
        }
    }
}

fn grade(answer: &str, guess: &str) -> [Mark; 5] {
    let answer = answer.as_bytes();
    let guess = guess.as_bytes();
    let mut marks = [Mark::Absent; 5];
    let mut used = [false; 5];
    for index in 0..5 {
        if guess[index] == answer[index] {
            marks[index] = Mark::Correct;
            used[index] = true;
        }
    }
    for guess_index in 0..5 {
        if marks[guess_index] == Mark::Correct {
            continue;
        }
        if let Some(answer_index) =
            (0..5).find(|index| !used[*index] && answer[*index] == guess[guess_index])
        {
            marks[guess_index] = Mark::Present;
            used[answer_index] = true;
        }
    }
    marks
}

fn marked_row(word: &str, marks: [Mark; 5]) -> String {
    word.chars()
        .zip(marks)
        .map(|(character, mark)| match mark {
            Mark::Correct => format!("[{character}]"),
            Mark::Present => format!("({character})"),
            Mark::Absent => format!(" {character} "),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Game for Glyphs {
    fn input(&mut self, input: Input) {
        match input {
            Input::Char(character) if character.is_ascii_alphabetic() && self.current.len() < 5 => {
                self.current.push(character.to_ascii_uppercase());
            }
            Input::Backspace => {
                self.current.pop();
            }
            Input::Enter => self.submit(),
            _ => {}
        }
    }

    fn scene(&self, _width: u16, _height: u16) -> Scene {
        let mut lines = self
            .rows
            .iter()
            .map(|(word, marks)| marked_row(word, *marks))
            .collect::<Vec<_>>();
        while lines.len() < 6 {
            if lines.len() == self.rows.len() {
                let mut input = self.current.clone();
                input.push_str(&"_".repeat(5 - input.len()));
                lines.push(
                    input
                        .chars()
                        .map(|character| format!(" {character} "))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            } else {
                lines.push(" _   _   _   _   _ ".into());
            }
        }
        Scene {
            title: "FIVE GLYPHS",
            status: self.message.clone(),
            lines,
            help: "Letters type  Backspace erase  Enter submit  Ctrl-R restart  Esc menu",
        }
    }

    fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    fn accepts_text(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_combines_each_tile_once() {
        let mut game = Merge::new(1);
        game.cells = [2, 2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        game.slide(Slide::Left);
        assert_eq!(&game.cells[..4], &[4, 4, 0, 0]);
        assert_eq!(game.score, 8);

        game.cells = [2, 4, 2, 4, 4, 2, 4, 2, 2, 4, 2, 4, 4, 2, 4, 2];
        game.slide(Slide::Left);
        assert!(matches!(game.outcome, Outcome::Lost(_)));
    }

    #[test]
    fn cards_reward_sets_and_glyphs_handle_duplicates() {
        let pair = [
            Card { rank: 7, suit: 'S' },
            Card { rank: 7, suit: 'H' },
            Card { rank: 2, suit: 'D' },
        ];
        assert_eq!(score_cards(&pair), 24);
        assert_eq!(
            grade("AMBER", "ARRAY"),
            [
                Mark::Correct,
                Mark::Present,
                Mark::Absent,
                Mark::Absent,
                Mark::Absent
            ]
        );
    }

    #[test]
    fn sprint_has_a_finite_failure_path() {
        let mut game = Sprint::new(1);
        for _ in 0..6 {
            game.type_character('!');
            game.typed.clear();
        }
        assert!(matches!(game.outcome, Outcome::Lost(_)));
    }
}
