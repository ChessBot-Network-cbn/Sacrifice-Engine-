use crate::board::Board;
use chess::{Color, Piece, Square};

#[inline]
fn piece_value(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => 100,
        Piece::Knight => 320,
        Piece::Bishop => 335,
        Piece::Rook => 500,
        Piece::Queen => 900,
        Piece::King => 20_000,
    }
}

#[inline]
fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

/// Score positif : avantage blanc.
/// Score négatif : avantage noir.
pub fn attack_score(board: &Board) -> i32 {
    let mut pressure = [0i32; 2];
    let mut attackers = [[0i32; 64]; 2];

    // Génère les coups légaux une seule fois.
    for mv in board.legal_moves() {
        let Some(color) = board.color_on(mv.get_source()) else {
            continue;
        };

        attackers[color_index(color)][mv.get_dest().to_index()] += 1;
    }

    for index in 0..64 {
        let square = Square::make_square(
            chess::Rank::from_index(index / 8),
            chess::File::from_index(index % 8),
        );

        let Some((victim_color, victim_piece)) = board.piece_at(square) else {
            continue;
        };

        if victim_piece == Piece::King {
            continue;
        }

        let enemy_index = color_index(!victim_color);
        let own_index = color_index(victim_color);

        let enemy_attackers = attackers[enemy_index][index];
        let defenders = attackers[own_index][index];

        if enemy_attackers == 0 {
            continue;
        }

        let fraction = if defenders == 0 {
            12
        } else if enemy_attackers > defenders {
            8
        } else {
            3
        };

        pressure[enemy_index] += piece_value(victim_piece) * fraction / 100;
    }

    (pressure[0] - pressure[1]).clamp(-180, 180)
}
