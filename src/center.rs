use crate::board::Board;
use chess::{Color, Piece, Square};

const CENTER: [Square; 4] = [Square::D4, Square::E4, Square::D5, Square::E5];

#[inline]
fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

/// Score positif : avantage blanc au centre.
/// Score négatif : avantage noir au centre.
pub fn center_control_score(board: &Board) -> i32 {
    let mut attacks = [[0i32; 64]; 2];

    // Compte les coups légaux vers chaque case.
    for mv in board.legal_moves() {
        let Some(color) = board.color_on(mv.get_source()) else {
            continue;
        };

        attacks[color_index(color)][mv.get_dest().to_index()] += 1;
    }

    let mut score = 0;

    for square in CENTER {
        let index = square.to_index();

        score += (attacks[0][index] - attacks[1][index]) * 4;

        match board.piece_at(square) {
            Some((Color::White, Piece::Pawn)) => score += 16,
            Some((Color::Black, Piece::Pawn)) => score -= 16,
            Some((Color::White, Piece::Knight)) => score += 9,
            Some((Color::Black, Piece::Knight)) => score -= 9,
            Some((Color::White, Piece::Bishop)) => score += 5,
            Some((Color::Black, Piece::Bishop)) => score -= 5,
            Some((Color::White, Piece::Queen)) => score += 2,
            Some((Color::Black, Piece::Queen)) => score -= 2,
            _ => {}
        }
    }

    score.clamp(-100, 100)
}
