import whiteKing from "../../assets/chess-pieces/split-svg/white-king.svg";
import whiteQueen from "../../assets/chess-pieces/split-svg/white-queen.svg";
import whiteBishop from "../../assets/chess-pieces/split-svg/white-bishop.svg";
import whiteKnight from "../../assets/chess-pieces/split-svg/white-knight.svg";
import whiteRook from "../../assets/chess-pieces/split-svg/white-rook.svg";
import whitePawn from "../../assets/chess-pieces/split-svg/white-pawn.svg";
import blackKing from "../../assets/chess-pieces/split-svg/black-king.svg";
import blackQueen from "../../assets/chess-pieces/split-svg/black-queen.svg";
import blackBishop from "../../assets/chess-pieces/split-svg/black-bishop.svg";
import blackKnight from "../../assets/chess-pieces/split-svg/black-knight.svg";
import blackRook from "../../assets/chess-pieces/split-svg/black-rook.svg";
import blackPawn from "../../assets/chess-pieces/split-svg/black-pawn.svg";

export const pieceImages: Record<string, string> = {
  K: whiteKing,
  Q: whiteQueen,
  B: whiteBishop,
  N: whiteKnight,
  R: whiteRook,
  P: whitePawn,
  k: blackKing,
  q: blackQueen,
  b: blackBishop,
  n: blackKnight,
  r: blackRook,
  p: blackPawn
};
