use crate::board::Board;

use chess::{
    get_bishop_moves, get_king_moves, get_knight_moves, get_pawn_attacks, get_rook_moves, BitBoard,
    ChessMove, Color, Piece, Square, EMPTY,
};

/// ------------------------------------------------------------
/// PARAMÈTRES
/// ------------------------------------------------------------

const PAWN_VALUE: i32 = 100;
const KNIGHT_VALUE: i32 = 320;
const BISHOP_VALUE: i32 = 330;
const ROOK_VALUE: i32 = 500;
const QUEEN_VALUE: i32 = 900;
const KING_VALUE: i32 = 30_000;

/// ------------------------------------------------------------
/// INFORMATIONS SUR UN SACRIFICE
/// ------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SacrificeType {
    None,
    Pawn,
    Knight,
    Bishop,
    Exchange,
    Rook,
    Queen,
    Positional,
}

#[derive(Clone, Copy, Debug)]
pub struct SacrificeCandidate {
    pub mv: ChessMove,

    pub sacrifice_type: SacrificeType,

    /// Valeur du matériel volontairement exposé.
    pub material_risk: i32,

    /// Pression supplémentaire sur le roi adverse.
    pub king_attack: i32,

    /// Nombre / qualité des coups forcing disponibles.
    pub forcing_score: i32,

    /// Activité gagnée après le sacrifice.
    pub activity_gain: i32,

    /// Bonus pour les lignes ouvertes.
    pub open_lines: i32,

    /// Compensation dynamique totale.
    pub compensation: i32,

    /// Score final utilisé par le style Tal.
    pub tal_score: i32,
}

/// ------------------------------------------------------------
/// OUTILS
/// ------------------------------------------------------------

fn opposite(color: Color) -> Color {
    match color {
        Color::White => Color::Black,
        Color::Black => Color::White,
    }
}

fn piece_value(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => PAWN_VALUE,
        Piece::Knight => KNIGHT_VALUE,
        Piece::Bishop => BISHOP_VALUE,
        Piece::Rook => ROOK_VALUE,
        Piece::Queen => QUEEN_VALUE,
        Piece::King => KING_VALUE,
    }
}

/// Distance roi-case.
fn distance(a: Square, b: Square) -> i32 {
    let af = a.get_file().to_index() as i32;
    let ar = a.get_rank().to_index() as i32;

    let bf = b.get_file().to_index() as i32;
    let br = b.get_rank().to_index() as i32;

    (af - bf).abs().max((ar - br).abs())
}

/// Nombre de pièces attaquant une case.
fn attackers_to(board: &Board, attacker: Color, target: Square) -> i32 {
    let blockers = *board.inner().combined();
    let target_bb = BitBoard::from_square(target);
    let color_bb = *board.inner().color_combined(attacker);

    let mut count = 0;

    // Pions
    for square in *board.inner().pieces(Piece::Pawn) & color_bb {
        if get_pawn_attacks(square, attacker, blockers) & target_bb != EMPTY {
            count += 1;
        }
    }

    // Cavaliers
    for square in *board.inner().pieces(Piece::Knight) & color_bb {
        if get_knight_moves(square) & target_bb != EMPTY {
            count += 1;
        }
    }

    // Fous
    for square in *board.inner().pieces(Piece::Bishop) & color_bb {
        if get_bishop_moves(square, blockers) & target_bb != EMPTY {
            count += 1;
        }
    }

    // Tours
    for square in *board.inner().pieces(Piece::Rook) & color_bb {
        if get_rook_moves(square, blockers) & target_bb != EMPTY {
            count += 1;
        }
    }

    // Dames
    for square in *board.inner().pieces(Piece::Queen) & color_bb {
        let attacks = get_bishop_moves(square, blockers) | get_rook_moves(square, blockers);

        if attacks & target_bb != EMPTY {
            count += 1;
        }
    }

    // Roi
    for square in *board.inner().pieces(Piece::King) & color_bb {
        if get_king_moves(square) & target_bb != EMPTY {
            count += 1;
        }
    }

    count
}

/// Activité approximative des pièces.
fn piece_activity(board: &Board, color: Color) -> i32 {
    let blockers = *board.inner().combined();
    let pieces = [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen];

    let mut score = 0;

    for piece in pieces {
        let bitboard = *board.inner().pieces(piece) & *board.inner().color_combined(color);

        for square in bitboard {
            let mobility = match piece {
                Piece::Knight => get_knight_moves(square).popcnt(),

                Piece::Bishop => get_bishop_moves(square, blockers).popcnt(),

                Piece::Rook => get_rook_moves(square, blockers).popcnt(),

                Piece::Queen => {
                    (get_bishop_moves(square, blockers) | get_rook_moves(square, blockers)).popcnt()
                }

                _ => 0,
            };

            score += mobility as i32;
        }
    }

    score
}

/// ------------------------------------------------------------
/// DÉTECTION DU TYPE DE SACRIFICE
/// ------------------------------------------------------------

fn sacrifice_type(piece: Piece, captured: Option<Piece>) -> SacrificeType {
    match piece {
        Piece::Pawn => SacrificeType::Pawn,

        Piece::Knight => SacrificeType::Knight,

        Piece::Bishop => SacrificeType::Bishop,

        Piece::Rook => match captured {
            Some(Piece::Rook) => SacrificeType::Exchange,
            _ => SacrificeType::Rook,
        },

        Piece::Queen => SacrificeType::Queen,

        Piece::King => SacrificeType::None,
    }
}

/// ------------------------------------------------------------
/// COUPS FORCING
/// ------------------------------------------------------------

fn forcing_score(board: &Board, attacker: Color) -> i32 {
    let mut score = 0;

    let enemy = opposite(attacker);

    // Le board doit être évalué du point de vue
    // du joueur qui vient de sacrifier.
    if board.side_to_move() != attacker {
        return 0;
    }

    for mv in board.legal_moves() {
        let child = board.make_move_new(mv);

        // Échec = extrêmement forcing.
        if child.in_check() {
            score += 80;
        }

        // Capture.
        if board.piece_on(mv.get_dest()).is_some() {
            score += 25;
        }

        // Menace autour du roi.
        let king = child.king_square(enemy);
        let attackers = attackers_to(&child, attacker, king);

        score += attackers * 30;
    }

    score.min(250)
}

/// ------------------------------------------------------------
/// PRESSION SUR LE ROI
/// ------------------------------------------------------------

fn king_attack_score(board: &Board, attacker: Color) -> i32 {
    let defender = opposite(attacker);
    let king = board.king_square(defender);

    let mut score = 0;

    for rank in 0..8 {
        for file in 0..8 {
            let square =
                Square::make_square(chess::Rank::from_index(rank), chess::File::from_index(file));

            let d = distance(square, king);

            if d > 2 {
                continue;
            }

            let attackers = attackers_to(board, attacker, square);

            if attackers == 0 {
                continue;
            }

            score += match d {
                0 => attackers * 120,
                1 => attackers * 50,
                2 => attackers * 15,
                _ => 0,
            };
        }
    }

    if board.side_to_move() != attacker && board.in_check() {
        score += 200;
    }

    score.min(600)
}

/// ------------------------------------------------------------
/// LIGNES OUVERTES
///
/// V1 volontairement simple :
/// on regarde si la case de départ était occupée
/// par une pièce qui bloquait une ligne.
/// ------------------------------------------------------------

fn open_lines_score(board_before: &Board, board_after: &Board, mv: ChessMove) -> i32 {
    let source = mv.get_source();

    let Some(piece) = board_before.piece_on(source) else {
        return 0;
    };

    // Les pièces glissantes sont les plus importantes
    // pour l'ouverture de lignes.
    if !matches!(piece, Piece::Bishop | Piece::Rook | Piece::Queen) {
        return 0;
    }

    let before_activity = piece_activity(board_before, board_before.side_to_move());

    let after_activity = piece_activity(board_after, board_before.side_to_move());

    let gain = after_activity - before_activity;

    gain.max(0).min(150)
}

/// ------------------------------------------------------------
/// LE SACRIFICE EST-IL RÉELLEMENT EXPOSÉ ?
/// ------------------------------------------------------------

fn is_exposed(board_after: &Board, attacker: Color, square: Square) -> bool {
    let defender = opposite(attacker);

    attackers_to(board_after, defender, square) > 0
}

/// ------------------------------------------------------------
/// ANALYSE PRINCIPALE
/// ------------------------------------------------------------

pub fn analyze_sacrifice(board: &Board, mv: ChessMove) -> Option<SacrificeCandidate> {
    let attacker = board.side_to_move();
    let defender = opposite(attacker);

    let source = mv.get_source();
    let destination = mv.get_dest();

    let piece = board.piece_on(source)?;

    // Pas de sacrifice de roi.
    if piece == Piece::King {
        return None;
    }

    let captured = board.piece_on(destination);

    let moving_value = piece_value(piece);

    let captured_value = captured.map(piece_value).unwrap_or(0);

    let child = board.make_move_new(mv);

    // --------------------------------------------------------
    // MATÉRIEL EXPOSÉ
    // --------------------------------------------------------

    let material_risk = (moving_value - captured_value).max(0);

    let exposed = is_exposed(&child, attacker, destination);

    /*
        Un sacrifice classique doit généralement
        exposer du matériel.

        Pour une V1, on demande soit :
        - que la pièce puisse être prise ;
        - soit que le coup crée une forte activité
          autour du roi.
    */

    let king_attack = king_attack_score(&child, attacker);

    let activity_before = piece_activity(board, attacker);

    let activity_after = piece_activity(&child, attacker);

    let activity_gain = (activity_after - activity_before).max(0).min(200);

    let forcing = forcing_score(&child, attacker);

    let open_lines = open_lines_score(board, &child, mv);

    // --------------------------------------------------------
    // CAS PARTICULIER :
    // SACRIFICE SANS CAPTURE
    // --------------------------------------------------------

    let positional_candidate = captured.is_none()
        && (king_attack >= 120 || forcing >= 100 || activity_gain >= 40 || open_lines >= 40);

    // --------------------------------------------------------
    // SACRIFICE MATÉRIEL
    // --------------------------------------------------------

    let material_candidate = material_risk >= 80 && exposed;

    if !material_candidate && !positional_candidate {
        return None;
    }

    // --------------------------------------------------------
    // COMPENSATION
    // --------------------------------------------------------

    let mut compensation = 0;

    compensation += king_attack;
    compensation += forcing;
    compensation += activity_gain * 2;
    compensation += open_lines;

    // Une pièce réellement offerte compte davantage.
    if exposed {
        compensation += 60;
    }

    // Échec immédiat.
    if child.in_check() {
        compensation += 180;
    }

    // Proximité du roi adverse.
    let enemy_king = child.king_square(defender);

    let king_distance = distance(destination, enemy_king);

    compensation += match king_distance {
        0 => 150,
        1 => 100,
        2 => 50,
        _ => 0,
    };

    compensation = compensation.min(1_200);

    // --------------------------------------------------------
    // SCORE TAL
    // --------------------------------------------------------

    /*
        L'idée fondamentale :

            compensation
                    -
            matériel investi

        Un sacrifice n'est donc PAS automatiquement bon.

        Il faut que l'initiative compense réellement
        une partie du matériel.
    */

    let material_penalty = match piece {
        Piece::Pawn => material_risk / 2,

        Piece::Knight | Piece::Bishop => material_risk * 3 / 4,

        Piece::Rook => material_risk * 4 / 5,

        Piece::Queen => material_risk,

        Piece::King => KING_VALUE,
    };

    let tal_score = compensation - material_penalty;

    /*
        On évite les candidats complètement absurdes.
    */
    if tal_score < -250 && !child.in_check() {
        return None;
    }

    Some(SacrificeCandidate {
        mv,
        sacrifice_type: sacrifice_type(piece, captured),
        material_risk,
        king_attack,
        forcing_score: forcing,
        activity_gain,
        open_lines,
        compensation,
        tal_score,
    })
}

/// ------------------------------------------------------------
/// TROUVER TOUS LES SACRIFICES CANDIDATS
/// ------------------------------------------------------------

pub fn find_sacrifices(board: &Board) -> Vec<SacrificeCandidate> {
    let mut candidates = Vec::new();

    for mv in board.legal_moves() {
        if let Some(candidate) = analyze_sacrifice(board, mv) {
            candidates.push(candidate);
        }
    }

    candidates.sort_by(|a, b| b.tal_score.cmp(&a.tal_score));

    candidates
}

/// ------------------------------------------------------------
/// MEILLEUR CANDIDAT
/// ------------------------------------------------------------

pub fn best_sacrifice(board: &Board) -> Option<SacrificeCandidate> {
    find_sacrifices(board).into_iter().next()
}
