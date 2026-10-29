use crate::board::Board;
use crate::evaluate::evaluate;
use crate::pgn::{move_to_uci, StyleBook};
use crate::search::{SearchResult, Searcher};

use std::io::{self, BufRead, Write};

const DEFAULT_DEPTH: u32 = 15;
const MAX_DEPTH: u32 = 128;
const DEFAULT_CLOCK_MS: u64 = 1_000;
const MIN_SEARCH_TIME_MS: u64 = 10;
const CLOCK_RESERVE_MS: u64 = 100;

pub struct UciEngine {
    searcher: Searcher,
    board: Board,
    style: StyleBook,
}

impl Default for UciEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl UciEngine {
    pub fn new() -> Self {
        let board = Board::startpos();
        let mut searcher = Searcher::new();
        searcher.set_position(board.clone());

        Self {
            searcher,
            board,
            style: StyleBook::new(),
        }
    }

    pub fn run(&mut self) {
        let stdin = io::stdin();

        for line in stdin.lock().lines() {
            let Ok(line) = line else {
                break;
            };

            let command = line.trim();

            if command.is_empty() {
                continue;
            }

            if command == "quit" {
                self.searcher.stop();
                break;
            }

            self.handle_command(command);

            let _ = io::stdout().flush();
        }
    }

    pub fn handle_command(&mut self, command: &str) {
        let first = command.split_whitespace().next().unwrap_or("");

        match first {
            "uci" => self.cmd_uci(),
            "isready" => println!("readyok"),
            "ucinewgame" => self.cmd_new_game(),
            "stop" => self.searcher.stop(),
            "position" => self.cmd_position(command),
            "go" => self.cmd_go(command),
            "perft" => self.cmd_perft(command),
            "d" => self.cmd_display(),
            "eval" => self.cmd_eval(),
            "style" => self.cmd_style(command),
            _ => println!("info string unknown command: {}", command),
        }
    }

    fn cmd_uci(&self) {
        println!("id name Sacrifice-Engine");
        println!("id author Yann");
        println!("uciok");
    }

    fn cmd_new_game(&mut self) {
        self.searcher.stop();
        self.searcher.clear();

        self.board = Board::startpos();
        self.searcher.set_position(self.board.clone());
    }

    fn cmd_display(&self) {
        println!("{}", self.board);
        println!("fen {}", self.board.fen());
        println!("side {:?}", self.board.side_to_move());
        println!("legal moves {}", self.board.legal_move_count());
    }

    fn cmd_eval(&self) {
        let score = evaluate(&self.board, &self.style);
        println!("info score cp {}", score);
    }

    fn cmd_style(&mut self, command: &str) {
        let mut tokens = command.split_whitespace();
        let _ = tokens.next();

        match tokens.next() {
            Some("load") => {
                let Some(path) = tokens.next() else {
                    println!("info string usage: style load FILE");
                    return;
                };

                self.style.clear();

                match self.style.load_file(path) {
                    Ok(()) => {
                        println!(
                            "info string style loaded games {} positions {} moves {} sacrifices {} checks {} captures {}",
                            self.style.games(),
                            self.style.positions(),
                            self.style.moves(),
                            self.style.sacrifices(),
                            self.style.checks(),
                            self.style.captures()
                        );
                    }
                    Err(error) => {
                        println!("info string style error {}", error);
                    }
                }
            }

            Some("stats") => {
                println!(
                    "info string games {} positions {} moves {} sacrifices {} checks {} captures {}",
                    self.style.games(),
                    self.style.positions(),
                    self.style.moves(),
                    self.style.sacrifices(),
                    self.style.checks(),
                    self.style.captures()
                );
            }

            Some("move") => self.cmd_style_moves(),

            _ => {
                println!("info string usage: style load FILE | style stats | style move");
            }
        }
    }

    fn cmd_style_moves(&self) {
        let mut moves = self.board.legal_moves();

        moves.sort_unstable_by(|a, b| {
            self.style
                .move_style_score(&self.board, *b)
                .cmp(&self.style.move_style_score(&self.board, *a))
        });

        let mut shown = 0usize;

        for mv in moves {
            let score = self.style.move_style_score(&self.board, mv);

            if score == 0 {
                continue;
            }

            println!(
                "info string Tal move {} style_score {}",
                move_to_uci(mv),
                score
            );

            shown += 1;

            if shown >= 10 {
                break;
            }
        }

        if shown == 0 {
            println!("info string no exact Tal move found for this position");
        }
    }

    fn cmd_perft(&self, command: &str) {
        let tokens: Vec<_> = command.split_whitespace().collect();

        if tokens.len() != 2 {
            println!("info string usage: perft N");
            return;
        }

        let depth = match tokens[1].parse::<u32>() {
            Ok(depth) if depth <= 10 => depth,
            _ => {
                println!("info string invalid perft depth (expected 0..=10)");
                return;
            }
        };

        println!("nodes {}", self.board.perft(depth));
    }

    fn cmd_position(&mut self, command: &str) {
        let tokens: Vec<_> = command.split_whitespace().collect();

        if tokens.len() < 2 {
            println!("info string invalid position command");
            return;
        }

        let mut index = 1;

        let new_board = match tokens[index] {
            "startpos" => {
                index += 1;
                Board::startpos()
            }

            "fen" => {
                index += 1;

                if tokens.len() < index + 6 {
                    println!("info string incomplete FEN");
                    return;
                }

                let fen = tokens[index..index + 6].join(" ");
                index += 6;

                match Board::from_fen(&fen) {
                    Ok(board) => board,
                    Err(error) => {
                        println!("info string invalid FEN {}", error);
                        return;
                    }
                }
            }

            other => {
                println!("info string unknown position type {}", other);
                return;
            }
        };

        self.searcher.stop();
        self.board = new_board;

        if index < tokens.len() {
            if tokens[index] != "moves" {
                println!("info string expected 'moves'");
                return;
            }

            index += 1;

            while index < tokens.len() {
                let move_text = tokens[index];

                if let Err(error) = self.board.make_uci_move(move_text) {
                    println!("info string invalid move {}: {}", move_text, error);
                    return;
                }

                index += 1;
            }
        }

        self.searcher.set_position(self.board.clone());
    }

    fn cmd_go(&mut self, command: &str) {
        let tokens: Vec<_> = command.split_whitespace().collect();

        if tokens.get(1) == Some(&"sacrifice") {
            self.cmd_go_sacrifice(&tokens);
            return;
        }

        let mut depth: Option<u32> = None;
        let mut movetime: Option<u64> = None;
        let mut infinite = false;

        let mut wtime: Option<u64> = None;
        let mut btime: Option<u64> = None;
        let mut winc = 0u64;
        let mut binc = 0u64;

        let mut index = 1;

        while index < tokens.len() {
            match tokens[index] {
                "depth" if index + 1 < tokens.len() => {
                    depth = tokens[index + 1]
                        .parse::<u32>()
                        .ok()
                        .map(|d| d.clamp(1, MAX_DEPTH));
                    index += 2;
                }

                "movetime" if index + 1 < tokens.len() => {
                    movetime = tokens[index + 1].parse::<u64>().ok();
                    index += 2;
                }

                "wtime" if index + 1 < tokens.len() => {
                    wtime = tokens[index + 1].parse::<u64>().ok();
                    index += 2;
                }

                "btime" if index + 1 < tokens.len() => {
                    btime = tokens[index + 1].parse::<u64>().ok();
                    index += 2;
                }

                "winc" if index + 1 < tokens.len() => {
                    winc = tokens[index + 1].parse::<u64>().unwrap_or(0);
                    index += 2;
                }

                "binc" if index + 1 < tokens.len() => {
                    binc = tokens[index + 1].parse::<u64>().unwrap_or(0);
                    index += 2;
                }

                "infinite" => {
                    infinite = true;
                    index += 1;
                }

                _ => index += 1,
            }
        }

        self.searcher.set_position(self.board.clone());

        let result = if infinite {
            self.searcher
                .search_infinite(&self.style, |d, s, n, t| Self::print_info(d, s, n, t))
        } else if let Some(ms) = movetime {
            let safe_ms = ms.max(MIN_SEARCH_TIME_MS);

            self.searcher
                .search_time(&self.style, safe_ms, |d, s, n, t| {
                    Self::print_info(d, s, n, t)
                })
        } else if let Some(requested_depth) = depth {
            self.searcher
                .search_depth(&self.style, requested_depth, |d, s, n, t| {
                    Self::print_info(d, s, n, t)
                })
        } else if wtime.is_some() || btime.is_some() {
            let milliseconds = self.clock_time(wtime, btime, winc, binc);

            self.searcher
                .search_time(&self.style, milliseconds, |d, s, n, t| {
                    Self::print_info(d, s, n, t)
                })
        } else {
            self.searcher
                .search_depth(&self.style, DEFAULT_DEPTH, |d, s, n, t| {
                    Self::print_info(d, s, n, t)
                })
        };

        self.print_bestmove(result);
    }

    fn cmd_go_sacrifice(&mut self, tokens: &[&str]) {
        let mut depth = 8u32;
        let mut index = 2usize;

        while index < tokens.len() {
            match tokens[index] {
                "depth" if index + 1 < tokens.len() => {
                    depth = tokens[index + 1].parse::<u32>().unwrap_or(8).clamp(1, 16);
                    index += 2;
                }
                _ => index += 1,
            }
        }

        self.searcher.set_position(self.board.clone());

        println!(
            "info string sacrifice search depth {} position {}",
            depth,
            self.board.fen()
        );

        let result = self.searcher.search_sacrifices(
            &self.style,
            depth,
            |candidate| {
                let piece = match candidate.piece {
                    chess::Piece::Pawn => "Pawn",
                    chess::Piece::Knight => "Knight",
                    chess::Piece::Bishop => "Bishop",
                    chess::Piece::Rook => "Rook",
                    chess::Piece::Queen => "Queen",
                    chess::Piece::King => "King",
                };

                let recapture = candidate
                    .best_recapture
                    .map(move_to_uci)
                    .unwrap_or_else(|| "none".to_string());

                println!(
                    "info string sacrifice candidate {} piece {} score cp {} recaptures {} best_recapture {}",
                    move_to_uci(candidate.mv),
                    piece,
                    candidate.score,
                    candidate.recaptures,
                    recapture
                );
            },
        );

        println!(
            "info string sacrifice completed depth {} candidates {} nodes {} time {}",
            result.depth,
            result.candidates.len(),
            result.nodes,
            result.time_ms
        );

        match result.best_move {
            Some(mv) => {
                println!(
                    "info string best sacrifice {} score cp {}",
                    move_to_uci(mv),
                    result.score
                );
                println!("bestmove {}", move_to_uci(mv));
            }
            None => {
                println!("info string no immediate sacrifice found");
                println!("bestmove 0000");
            }
        }
    }

    fn clock_time(&self, wtime: Option<u64>, btime: Option<u64>, winc: u64, binc: u64) -> u64 {
        let (remaining, increment) = match self.board.side_to_move() {
            chess::Color::White => (wtime.unwrap_or(DEFAULT_CLOCK_MS), winc),
            chess::Color::Black => (btime.unwrap_or(DEFAULT_CLOCK_MS), binc),
        };

        // Keep a reserve so the engine does not spend its entire clock
        // on one move, especially in time trouble.
        let reserve = CLOCK_RESERVE_MS.min(remaining / 5);
        let usable = remaining.saturating_sub(reserve);

        if usable == 0 {
            return MIN_SEARCH_TIME_MS.min(remaining.max(1));
        }

        // A simple allocation: approximately 1/25 of the usable time,
        // plus part of the increment.
        let base = usable / 25;
        let bonus = increment.saturating_mul(3) / 4;

        base.saturating_add(bonus).max(1).min(usable)
    }

    fn print_info(depth: u32, score: i32, nodes: u64, time_ms: u128) {
        let safe_time = time_ms.max(1);
        let nps = (nodes as u128 * 1000 / safe_time) as u64;

        println!(
            "info depth {} score cp {} nodes {} time {} nps {}",
            depth, score, nodes, time_ms, nps
        );
    }

    fn print_bestmove(&self, result: SearchResult) {
        println!(
            "info string completed_depth {} score {} nodes {} time {}",
            result.depth, result.score, result.nodes, result.time_ms
        );

        match result.best_move {
            Some(mv) => println!("bestmove {}", move_to_uci(mv)),
            None => println!("bestmove 0000"),
        }
    }
}

pub fn run_uci_with_style(style: StyleBook) {
    let mut engine = UciEngine::new();
    engine.style = style;
    engine.run();
}

pub fn run_uci() {
    UciEngine::new().run();
}
