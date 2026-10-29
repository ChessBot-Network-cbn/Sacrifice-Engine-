use chess::{Board as ChessBoard, BoardStatus, ChessMove, Color, MoveGen, Piece, Square};
use std::str::FromStr;

#[derive(Clone, PartialEq, Eq)]
pub struct Board {
    inner: ChessBoard,
}

impl Board {
    pub fn new() -> Self {
        Self {
            inner: ChessBoard::default(),
        }
    }

    pub fn startpos() -> Self {
        Self::new()
    }

    pub fn from_fen(fen: &str) -> Result<Self, String> {
        ChessBoard::from_str(fen)
            .map(|inner| Self { inner })
            .map_err(|e| format!("FEN invalide: {e:?}"))
    }

    pub fn inner(&self) -> &ChessBoard {
        &self.inner
    }

    pub fn hash(&self) -> u64 {
        self.inner.get_hash()
    }

    pub fn side_to_move(&self) -> Color {
        self.inner.side_to_move()
    }

    pub fn status(&self) -> BoardStatus {
        self.inner.status()
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.inner.status(),
            BoardStatus::Checkmate | BoardStatus::Stalemate
        )
    }

    pub fn in_check(&self) -> bool {
        self.inner.checkers().popcnt() != 0
    }

    pub fn piece_at(&self, square: Square) -> Option<(Color, Piece)> {
        let piece = self.inner.piece_on(square)?;
        let color = self.inner.color_on(square)?;
        Some((color, piece))
    }

    pub fn piece_on(&self, square: Square) -> Option<Piece> {
        self.inner.piece_on(square)
    }

    pub fn color_on(&self, square: Square) -> Option<Color> {
        self.inner.color_on(square)
    }

    pub fn legal_moves(&self) -> Vec<ChessMove> {
        MoveGen::new_legal(&self.inner).collect()
    }

    pub fn legal_move_count(&self) -> usize {
        MoveGen::new_legal(&self.inner).len()
    }

    pub fn is_legal(&self, mv: ChessMove) -> bool {
        self.inner.legal(mv)
    }

    pub fn make_move_new(&self, mv: ChessMove) -> Self {
        Self {
            inner: self.inner.make_move_new(mv),
        }
    }

    pub fn make_uci_move(&mut self, text: &str) -> Result<(), String> {
        let text = text.trim();

        if !(4..=5).contains(&text.len()) {
            return Err(format!("coup UCI invalide: {text}"));
        }

        let mv = ChessMove::from_str(text).map_err(|_| format!("coup UCI invalide: {text}"))?;

        if !self.is_legal(mv) {
            return Err(format!("coup illégal: {text}"));
        }

        self.inner = self.inner.make_move_new(mv);
        Ok(())
    }

    pub fn king_square(&self, color: Color) -> Square {
        self.inner.king_square(color)
    }

    pub fn fen(&self) -> String {
        self.inner.to_string()
    }

    pub fn perft(&self, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }

        let moves = self.legal_moves();

        if depth == 1 {
            return moves.len() as u64;
        }

        moves
            .into_iter()
            .map(|mv| self.make_move_new(mv).perft(depth - 1))
            .sum()
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Board {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl std::fmt::Debug for Board {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Board")
            .field("fen", &self.fen())
            .field("side_to_move", &self.side_to_move())
            .field("status", &self.status())
            .finish()
    }
}

impl FromStr for Board {
    type Err = String;

    fn from_str(fen: &str) -> Result<Self, Self::Err> {
        Self::from_fen(fen)
    }
}
