
use crate::board::Board;
use crate::evaluate::{evaluate, MATE_SCORE};
use crate::pgn::StyleBook;
use chess::{BoardStatus, ChessMove, Piece};
use std::cmp::Reverse;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const INF: i32 = 32_000;
const MAX_PLY: usize = 128;
const TT_SIZE: usize = 1 << 20;
const MATE_BOUND: i32 = MATE_SCORE - MAX_PLY as i32;
const ASPIRATION_WINDOW: i32 = 35;
const STOP_CHECK_MASK: u64 = 1023;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy, Debug)]
struct TTEntry {
    key: u64,
    depth: u8,
    score: i32,
    bound: Bound,
    best_move: Option<ChessMove>,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResult {
    pub best_move: Option<ChessMove>,
    pub score: i32,
    pub depth: u32,
    pub nodes: u64,
    pub time_ms: u128,
}

#[derive(Clone, Debug)]
pub struct SacrificeCandidate {
    pub mv: ChessMove,
    pub piece: Piece,
    pub score: i32,
    pub recaptures: u32,
    pub best_recapture: Option<ChessMove>,
}

#[derive(Clone, Debug, Default)]
pub struct SacrificeResult {
    pub best_move: Option<ChessMove>,
    pub score: i32,
    pub depth: u32,
    pub candidates: Vec<SacrificeCandidate>,
    pub nodes: u64,
    pub time_ms: u128,
}

pub struct Searcher {
    position: Board,
    tt: Vec<Option<TTEntry>>,
    killers: [[Option<ChessMove>; 2]; MAX_PLY],
    history: [[i32; 64]; 64],
    stop: AtomicBool,
    nodes: u64,
    start: Instant,
    time_limit: Option<Duration>,
}

impl Default for Searcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Searcher {
    pub fn new() -> Self {
        Self {
            position: Board::startpos(),
            tt: vec![None; TT_SIZE],
            killers: [[None; 2]; MAX_PLY],
            history: [[0; 64]; 64],
            stop: AtomicBool::new(false),
            nodes: 0,
            start: Instant::now(),
            time_limit: None,
        }
    }

    pub fn set_position(&mut self, board: Board) {
        self.position = board;
    }

    pub fn clear(&mut self) {
        self.tt.fill(None);
        self.killers = [[None; 2]; MAX_PLY];
        self.history = [[0; 64]; 64];
        self.nodes = 0;
        self.stop.store(false, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn search_depth<F>(
        &mut self,
        style: &StyleBook,
        depth: u32,
        mut on_iteration: F,
    ) -> SearchResult
    where
        F: FnMut(u32, i32, u64, u128),
    {
        self.run_search(
            style,
            depth.clamp(1, (MAX_PLY - 1) as u32),
            None,
            false,
            &mut on_iteration,
        )
    }

    pub fn search_time<F>(
        &mut self,
        style: &StyleBook,
        milliseconds: u64,
        mut on_iteration: F,
    ) -> SearchResult
    where
        F: FnMut(u32, i32, u64, u128),
    {
        self.run_search(
            style,
            (MAX_PLY - 1) as u32,
            Some(Duration::from_millis(milliseconds.max(1))),
            false,
            &mut on_iteration,
        )
    }

    pub fn search_infinite<F>(
        &mut self,
        style: &StyleBook,
        mut on_iteration: F,
    ) -> SearchResult
    where
        F: FnMut(u32, i32, u64, u128),
    {
        self.run_search(
            style,
            (MAX_PLY - 1) as u32,
            None,
            true,
            &mut on_iteration,
        )
    }

    fn run_search<F>(
        &mut self,
        style: &StyleBook,
        max_depth: u32,
        time_limit: Option<Duration>,
        infinite: bool,
        callback: &mut F,
    ) -> SearchResult
    where
        F: FnMut(u32, i32, u64, u128),
    {
        self.stop.store(false, Ordering::Relaxed);
        self.nodes = 0;
        self.start = Instant::now();
        self.time_limit = time_limit;
        self.killers = [[None; 2]; MAX_PLY];

        let legal_moves = self.position.legal_moves();

        if legal_moves.is_empty() {
            let score = if self.position.status() == BoardStatus::Checkmate {
                -MATE_SCORE
            } else {
                0
            };

            return SearchResult {
                best_move: None,
                score,
                depth: 0,
                nodes: 0,
                time_ms: self.elapsed_ms(),
            };
        }

        // Toujours garder un coup légal si la recherche est interrompue.
        let mut result = SearchResult {
            best_move: legal_moves.first().copied(),
            score: evaluate(&self.position, style),
            depth: 0,
            nodes: 0,
            time_ms: 0,
        };

        let mut previous_best = None;
        let mut previous_score = result.score;

        for depth in 1..=max_depth {
            if self.should_stop() {
                break;
            }

            let (score, best_move) = if depth >= 4 {
                let mut window = ASPIRATION_WINDOW;

                loop {
                    let alpha = (previous_score - window).max(-INF);
                    let beta = (previous_score + window).min(INF);

                    let (score, mv) =
                        self.search_root(depth, style, previous_best, alpha, beta);

                    if self.should_stop() {
                        break (score, mv);
                    }

                    if score <= alpha || score >= beta {
                        if window >= INF {
                            break (score, mv);
                        }

                        window = (window.saturating_mul(2)).min(INF);
                        continue;
                    }

                    break (score, mv);
                }
            } else {
                self.search_root(depth, style, previous_best, -INF, INF)
            };

            // Ne pas valider une profondeur incomplète.
            if self.should_stop() {
                break;
            }

            if let Some(mv) = best_move {
                result.best_move = Some(mv);
                result.score = score;
                result.depth = depth;
                result.nodes = self.nodes;
                result.time_ms = self.elapsed_ms();

                previous_best = Some(mv);
                previous_score = score;

                callback(depth, score, self.nodes, self.elapsed_ms());
            }

            if result.score.abs() >= MATE_BOUND {
                break;
            }

            if !infinite
                && self
                    .time_limit
                    .is_some_and(|limit| self.start.elapsed() >= limit)
            {
                break;
            }
        }

        result.nodes = self.nodes;
        result.time_ms = self.elapsed_ms();
        result
    }

    fn search_root(
        &mut self,
        depth: u32,
        style: &StyleBook,
        previous_best: Option<ChessMove>,
        mut alpha: i32,
        beta: i32,
    ) -> (i32, Option<ChessMove>) {
        let root = self.position.clone();
        let original_alpha = alpha;
        let hash = root.hash();
        let mut moves = root.legal_moves();

        let tt_move = self.probe(hash).and_then(|entry| entry.best_move);
        let preferred = previous_best
            .or(tt_move)
            .or_else(|| style.preferred_move(&root));

        self.order_moves(&root, &mut moves, style, 0, preferred);

        let mut best_score = -INF;
        let mut best_move = None;
        let mut searched = 0usize;

        for mv in moves {
            if self.should_stop() {
                break;
            }

            let child = root.make_move_new(mv);

            let score = if searched == 0 {
                -self.negamax(&child, depth - 1, -beta, -alpha, 1, style)
            } else {
                let scout =
                    -self.negamax(&child, depth - 1, -alpha - 1, -alpha, 1, style);

                if self.should_stop() {
                    break;
                }

                if scout > alpha && scout < beta {
                    -self.negamax(&child, depth - 1, -beta, -alpha, 1, style)
                } else {
                    scout
                }
            };

            if self.should_stop() {
                break;
            }

            searched += 1;

            if score > best_score {
                best_score = score;
                best_move = Some(mv);
            }

            alpha = alpha.max(score);

            if alpha >= beta {
                break;
            }
        }

        if self.should_stop() {
            return (best_score, best_move);
        }

        if let Some(mv) = best_move {
            let bound = if best_score <= original_alpha {
                Bound::Upper
            } else if best_score >= beta {
                Bound::Lower
            } else {
                Bound::Exact
            };

            self.store(
                hash,
                depth,
                score_to_tt(best_score, 0),
                bound,
                Some(mv),
            );
        }

        if best_score == -INF {
            (evaluate(&root, style), None)
        } else {
            (best_score, best_move)
        }
    }

    fn negamax(
        &mut self,
        board: &Board,
        depth: u32,
        mut alpha: i32,
        beta: i32,
        ply: usize,
        style: &StyleBook,
    ) -> i32 {
        self.nodes = self.nodes.saturating_add(1);

        if self.nodes & STOP_CHECK_MASK == 0 && self.should_stop() {
            return evaluate(board, style);
        }

        if ply >= MAX_PLY - 1 {
            return evaluate(board, style);
        }

        if depth == 0 {
            return self.quiescence(board, alpha, beta, ply, style);
        }

        let in_check = board.in_check();
        let hash = board.hash();
        let original_alpha = alpha;

        let tt_entry = self.probe(hash);
        let tt_move = tt_entry.and_then(|entry| entry.best_move);

        if let Some(entry) = tt_entry {
            if entry.depth as u32 >= depth {
                let score = score_from_tt(entry.score, ply);

                match entry.bound {
                    Bound::Exact => return score,
                    Bound::Lower if score >= beta => return score,
                    Bound::Upper if score <= alpha => return score,
                    _ => {}
                }
            }
        }

        let mut moves = board.legal_moves();

        if moves.is_empty() {
            return if in_check {
                -MATE_SCORE + ply as i32
            } else {
                0
            };
        }

        self.order_moves(board, &mut moves, style, ply, tt_move);

        let mut best_score = -INF;
        let mut best_move = None;
        let mut searched = 0usize;

        for mv in moves {
            if self.should_stop() {
                return if best_score == -INF {
                    evaluate(board, style)
                } else {
                    best_score
                };
            }

            let capture = is_capture(board, mv);
            let promotion = mv.get_promotion().is_some();
            let quiet = !capture && !promotion;
            let child = board.make_move_new(mv);

            // Ne pas réduire les coups qui donnent échec.
            let gives_check = child.in_check();

            let reduction = if depth >= 3
                && searched >= 3
                && quiet
                && !in_check
                && !gives_check
                && !is_killer(&self.killers, ply, mv)
            {
                if depth >= 6 && searched >= 8 {
                    2
                } else {
                    1
                }
            } else {
                0
            };

            let score = if searched == 0 {
                -self.negamax(
                    &child,
                    depth - 1,
                    -beta,
                    -alpha,
                    ply + 1,
                    style,
                )
            } else {
                let reduced_depth = depth.saturating_sub(1 + reduction);

                // PVS : commencer par une recherche à fenêtre nulle.
                let mut score = -self.negamax(
                    &child,
                    reduced_depth,
                    -alpha - 1,
                    -alpha,
                    ply + 1,
                    style,
                );

                if self.should_stop() {
                    return score;
                }

                // Si le coup réduit semble prometteur, rétablir la profondeur.
                if reduction > 0 && score > alpha {
                    score = -self.negamax(
                        &child,
                        depth - 1,
                        -alpha - 1,
                        -alpha,
                        ply + 1,
                        style,
                    );
                }

                if self.should_stop() {
                    return score;
                }

                // Un score intermédiaire mérite une recherche complète.
                if score > alpha && score < beta {
                    score = -self.negamax(
                        &child,
                        depth - 1,
                        -beta,
                        -alpha,
                        ply + 1,
                        style,
                    );
                }

                score
            };

            if self.should_stop() {
                return if best_score == -INF {
                    evaluate(board, style)
                } else {
                    best_score
                };
            }

            searched += 1;

            if score > best_score {
                best_score = score;
                best_move = Some(mv);
            }

            alpha = alpha.max(score);

            if alpha >= beta {
                if quiet {
                    self.record_killer(ply, mv);

                    let from = mv.get_source().to_index();
                    let to = mv.get_dest().to_index();
                    let bonus = (depth * depth).min(400) as i32;

                    self.history[from][to] = self.history[from][to]
                        .saturating_add(bonus)
                        .min(20_000);
                }

                break;
            }
        }

        if self.should_stop() {
            return if best_score == -INF {
                evaluate(board, style)
            } else {
                best_score
            };
        }

        if best_score == -INF {
            return evaluate(board, style);
        }

        let bound = if best_score <= original_alpha {
            Bound::Upper
        } else if best_score >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };

        self.store(
            hash,
            depth,
            score_to_tt(best_score, ply),
            bound,
            best_move,
        );

        best_score
    }

    fn quiescence(
        &mut self,
        board: &Board,
        mut alpha: i32,
        beta: i32,
        ply: usize,
        style: &StyleBook,
    ) -> i32 {
        self.nodes = self.nodes.saturating_add(1);

        if self.nodes & STOP_CHECK_MASK == 0 && self.should_stop() {
            return evaluate(board, style);
        }

        if ply >= MAX_PLY - 1 {
            return evaluate(board, style);
        }

        let in_check = board.in_check();
        let mut moves = board.legal_moves();

        // Détecter échec et mat et pat avant l'évaluation statique.
        if moves.is_empty() {
            return if in_check {
                -MATE_SCORE + ply as i32
            } else {
                0
            };
        }

        if !in_check {
            let stand_pat = evaluate(board, style);

            if stand_pat >= beta {
                return stand_pat;
            }

            alpha = alpha.max(stand_pat);

            // Hors échec, seules captures et promotions sont recherchées.
            moves.retain(|mv| is_capture(board, *mv) || mv.get_promotion().is_some());
        }

        self.order_moves(board, &mut moves, style, ply, None);

        for mv in moves {
            if self.should_stop() {
                return evaluate(board, style);
            }

            let child = board.make_move_new(mv);
            let score = -self.quiescence(&child, -beta, -alpha, ply + 1, style);

            // Ne pas utiliser un résultat de quiescence interrompu.
            if self.should_stop() {
                return evaluate(board, style);
            }

            if score >= beta {
                return score;
            }

            alpha = alpha.max(score);
        }

        alpha
    }

    fn order_moves(
        &self,
        board: &Board,
        moves: &mut [ChessMove],
        style: &StyleBook,
        ply: usize,
        preferred: Option<ChessMove>,
    ) {
        moves.sort_unstable_by_key(|mv| {
            let mut score = 0i32;

            if Some(*mv) == preferred {
                score += 2_000_000;
            }

            let capture = is_capture(board, *mv);

            if capture {
                let victim = captured_piece_value(board, *mv);
                let attacker = board
                    .piece_on(mv.get_source())
                    .map(piece_value)
                    .unwrap_or(100);

                // MVV-LVA : préférer les captures avantageuses.
                score += 1_000_000 + victim * 10 - attacker;
            }

            if let Some(promotion) = mv.get_promotion() {
                score += 900_000 + piece_value(promotion);
            }

            if ply < MAX_PLY {
                if self.killers[ply][0] == Some(*mv) {
                    score += 80_000;
                } else if self.killers[ply][1] == Some(*mv) {
                    score += 60_000;
                }
            }

            let from = mv.get_source().to_index();
            let to = mv.get_dest().to_index();

            score += self.history[from][to].clamp(0, 20_000);

            // Le style de Tal influence uniquement l'ordre des coups à la racine.
            if ply == 0 {
                score += style.move_style_score(board, *mv);
            }

            Reverse(score)
        });
    }

    fn record_killer(&mut self, ply: usize, mv: ChessMove) {
        if ply >= MAX_PLY || self.killers[ply][0] == Some(mv) {
            return;
        }

        self.killers[ply][1] = self.killers[ply][0];
        self.killers[ply][0] = Some(mv);
    }

    fn probe(&self, hash: u64) -> Option<TTEntry> {
        self.tt[(hash as usize) % self.tt.len()]
            .filter(|entry| entry.key == hash)
    }

    fn store(
        &mut self,
        hash: u64,
        depth: u32,
        score: i32,
        bound: Bound,
        best_move: Option<ChessMove>,
    ) {
        let index = (hash as usize) % self.tt.len();

        let replace = self.tt[index]
            .map(|old| old.key == hash || depth >= old.depth as u32)
            .unwrap_or(true);

        if replace {
            self.tt[index] = Some(TTEntry {
                key: hash,
                depth: depth.min(255) as u8,
                score,
                bound,
                best_move,
            });
        }
    }

    fn should_stop(&self) -> bool {
        if self.stop.load(Ordering::Relaxed) {
            return true;
        }

        if self
            .time_limit
            .is_some_and(|limit| self.start.elapsed() >= limit)
        {
            self.stop.store(true, Ordering::Relaxed);
            return true;
        }

        false
    }

    fn elapsed_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }

    pub fn search_sacrifices<F>(
        &mut self,
        style: &StyleBook,
        depth: u32,
        mut on_candidate: F,
    ) -> SacrificeResult
    where
        F: FnMut(&SacrificeCandidate),
    {
        self.stop.store(false, Ordering::Relaxed);
        self.nodes = 0;
        self.start = Instant::now();
        self.time_limit = None;

        let root = self.position.clone();
        let mut candidates = Vec::new();

        for mv in root.legal_moves() {
            if self.should_stop() {
                break;
            }

            let Some(piece) = root.piece_on(mv.get_source()) else {
                continue;
            };

            let child = root.make_move_new(mv);
            let mut recaptures = Vec::new();

            for reply in child.legal_moves() {
                if reply.get_dest() == mv.get_dest() && is_capture(&child, reply) {
                    recaptures.push(reply);
                }
            }

            let moving_value = piece_value(piece);
            let captured_value = captured_piece_value(&root, mv);

            let is_sacrifice = !recaptures.is_empty()
                && (captured_value < moving_value || !is_capture(&root, mv));

            if !is_sacrifice {
                continue;
            }

            let score = if depth > 1 {
                -self.negamax(
                    &child,
                    depth.saturating_sub(1),
                    -INF,
                    INF,
                    1,
                    style,
                )
            } else {
                -evaluate(&child, style)
            };

            if self.should_stop() {
                break;
            }

            let candidate = SacrificeCandidate {
                mv,
                piece,
                score,
                recaptures: recaptures.len() as u32,
                best_recapture: recaptures.first().copied(),
            };

            on_candidate(&candidate);
            candidates.push(candidate);
        }

        candidates.sort_unstable_by(|a, b| b.score.cmp(&a.score));

        let best = candidates.first();

        SacrificeResult {
            best_move: best.map(|candidate| candidate.mv),
            score: best
                .map(|candidate| candidate.score)
                .unwrap_or_else(|| evaluate(&root, style)),
            depth,
            candidates,
            nodes: self.nodes,
            time_ms: self.elapsed_ms(),
        }
    }
}

fn is_killer(
    killers: &[[Option<ChessMove>; 2]; MAX_PLY],
    ply: usize,
    mv: ChessMove,
) -> bool {
    ply < MAX_PLY && (killers[ply][0] == Some(mv) || killers[ply][1] == Some(mv))
}

fn is_capture(board: &Board, mv: ChessMove) -> bool {
    board.piece_on(mv.get_dest()).is_some()
        || (board.piece_on(mv.get_source()) == Some(Piece::Pawn)
            && mv.get_source().get_file() != mv.get_dest().get_file()
            && board.piece_on(mv.get_dest()).is_none())
}

fn captured_piece_value(board: &Board, mv: ChessMove) -> i32 {
    board
        .piece_on(mv.get_dest())
        .map(piece_value)
        .unwrap_or_else(|| {
            if is_capture(board, mv) {
                piece_value(Piece::Pawn)
            } else {
                0
            }
        })
}

fn piece_value(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => 100,
        Piece::Knight => 320,
        Piece::Bishop => 330,
        Piece::Rook => 500,
        Piece::Queen => 900,
        Piece::King => 20_000,
    }
}

fn score_to_tt(score: i32, ply: usize) -> i32 {
    if score > MATE_BOUND {
        score + ply as i32
    } else if score < -MATE_BOUND {
        score - ply as i32
    } else {
        score
    }
}

fn score_from_tt(score: i32, ply: usize) -> i32 {
    if score > MATE_BOUND {
        score - ply as i32
    } else if score < -MATE_BOUND {
        score + ply as i32
    } else {
        score
    }
}
