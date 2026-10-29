use crate::board::Board;
use crate::pgn::StyleBook;
use chess::{Color, Piece, Square};

pub const MATE_SCORE: i32 = 30_000;

const PAWN: i32 = 100;
const KNIGHT: i32 = 320;
const BISHOP: i32 = 330;
const ROOK: i32 = 500;
const QUEEN: i32 = 900;

/// Évalue la position du point de vue du joueur qui doit jouer.
///
/// Le style historique est volontairement exclu du score positionnel :
/// il intervient dans l'ordre des coups de search.rs.
pub fn evaluate(board: &Board, style: &StyleBook) -> i32 {
    let _ = style;

    match board.status() {
        chess::BoardStatus::Checkmate => return -MATE_SCORE,
        chess::BoardStatus::Stalemate => return 0,
        chess::BoardStatus::Ongoing => {}
    }

    let white_score = material_score(board)
        + piece_square_score(board)
        + mobility_score(board)
        + king_safety_score(board)
        + activity_score(board)
        + crate::center::center_control_score(board).clamp(-100, 100)
        + crate::attack::attack_score(board).clamp(-150, 150);

    if board.side_to_move() == Color::White {
        white_score
    } else {
        -white_score
    }
}

fn material_score(board: &Board) -> i32 {
    let mut score = 0;

    for index in 0..64 {
        let square = square_from_index(index);

        if let Some((color, piece)) = board.piece_at(square) {
            let value = piece_value(piece);

            score += if color == Color::White { value } else { -value };
        }
    }

    score
}

fn piece_square_score(board: &Board) -> i32 {
    let mut score = 0;

    for index in 0..64 {
        let square = square_from_index(index);

        let Some((color, piece)) = board.piece_at(square) else {
            continue;
        };

        let file = square.get_file().to_index() as i32;
        let rank = square.get_rank().to_index() as i32;

        let relative_rank = if color == Color::White {
            rank
        } else {
            7 - rank
        };

        let center_distance = (file * 2 - 7).abs() + (rank * 2 - 7).abs();

        let center_bonus = (14 - center_distance).max(0);

        let value = match piece {
            Piece::Pawn => relative_rank * 7 + center_bonus / 3,
            Piece::Knight => center_bonus * 4,
            Piece::Bishop => center_bonus * 2 + relative_rank,
            Piece::Rook => relative_rank * 2 + if relative_rank == 6 { 12 } else { 0 },
            Piece::Queen => center_bonus,
            Piece::King => -center_bonus * 2,
        };

        score += if color == Color::White { value } else { -value };
    }

    score
}

fn mobility_score(board: &Board) -> i32 {
    let mobility = board.legal_move_count().min(80) as i32;

    if board.side_to_move() == Color::White {
        mobility
    } else {
        -mobility
    }
}

fn activity_score(board: &Board) -> i32 {
    let mut score = 0;

    for index in 0..64 {
        let square = square_from_index(index);

        let Some((color, piece)) = board.piece_at(square) else {
            continue;
        };

        let rank = square.get_rank().to_index() as i32;

        let relative_rank = if color == Color::White {
            rank
        } else {
            7 - rank
        };

        let bonus = match piece {
            Piece::Knight | Piece::Bishop => (relative_rank - 1).max(0) * 2,
            Piece::Rook => {
                if relative_rank == 6 {
                    10
                } else {
                    0
                }
            }
            Piece::Queen => {
                if relative_rank >= 3 {
                    3
                } else {
                    0
                }
            }
            Piece::Pawn | Piece::King => 0,
        };

        score += if color == Color::White { bonus } else { -bonus };
    }

    score
}

fn king_safety_score(board: &Board) -> i32 {
    let mut score = 0;

    for color in [Color::White, Color::Black] {
        let king = board.king_square(color);

        let file = king.get_file().to_index() as i32;
        let rank = king.get_rank().to_index() as i32;

        let relative_rank = if color == Color::White {
            rank
        } else {
            7 - rank
        };

        let forward = if color == Color::White { 1 } else { -1 };
        let mut shelter = 0;

        for df in -1..=1 {
            let f = file + df;
            let r = rank + forward;

            if !(0..8).contains(&f) || !(0..8).contains(&r) {
                continue;
            }

            let square = Square::make_square(
                chess::Rank::from_index(r as usize),
                chess::File::from_index(f as usize),
            );

            if board.piece_at(square) == Some((color, Piece::Pawn)) {
                shelter += 5;
            }
        }

        let castled_shelter = if relative_rank <= 1 && (file <= 2 || file >= 5) {
            8
        } else {
            0
        };

        let king_score = shelter + castled_shelter;

        score += if color == Color::White {
            king_score
        } else {
            -king_score
        };
    }

    score
}

fn square_from_index(index: usize) -> Square {
    Square::make_square(
        chess::Rank::from_index(index / 8),
        chess::File::from_index(index % 8),
    )
}

fn piece_value(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => PAWN,
        Piece::Knight => KNIGHT,
        Piece::Bishop => BISHOP,
        Piece::Rook => ROOK,
        Piece::Queen => QUEEN,
        Piece::King => 0,
    }
}
