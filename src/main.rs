use iced::advanced::svg;
use iced::widget::{Action, canvas, center};
use rodio::{Decoder, MixerDeviceSink};
use shakmaty::Position;

// sounds
const MOVE_SOUND: &[u8] = include_bytes!("../assets/sounds/Move.ogg");
const CAPTURE_SOUND: &[u8] = include_bytes!("../assets/sounds/Capture.ogg");

// White pieces
const WK: &[u8] = include_bytes!("../assets/pieces/wK.svg");
const WQ: &[u8] = include_bytes!("../assets/pieces/wQ.svg");
const WB: &[u8] = include_bytes!("../assets/pieces/wB.svg");
const WN: &[u8] = include_bytes!("../assets/pieces/wN.svg");
const WR: &[u8] = include_bytes!("../assets/pieces/wR.svg");
const WP: &[u8] = include_bytes!("../assets/pieces/wP.svg");

// Black pieces
const BK: &[u8] = include_bytes!("../assets/pieces/bK.svg");
const BQ: &[u8] = include_bytes!("../assets/pieces/bQ.svg");
const BB: &[u8] = include_bytes!("../assets/pieces/bB.svg");
const BN: &[u8] = include_bytes!("../assets/pieces/bN.svg");
const BR: &[u8] = include_bytes!("../assets/pieces/bR.svg");
const BP: &[u8] = include_bytes!("../assets/pieces/bP.svg");

// Colors
const WHITE_SQUARE_COLOR: iced::Color = iced::Color::from_rgb8(238, 238, 210);
const BLACK_SQUARE_COLOR: iced::Color = iced::Color::from_rgb8(118, 150, 86);

const SQUARE_WIDTH: f32 = 80.0;

struct App {
    screen: Screen,
    _sink_handle: MixerDeviceSink,
    audio_player: rodio::Player,
}

enum Screen {
    Play(Board),
}

struct SvgPieces {
    wk: svg::Handle,
    wq: svg::Handle,
    wb: svg::Handle,
    wn: svg::Handle,
    wr: svg::Handle,
    wp: svg::Handle,
    bk: svg::Handle,
    bq: svg::Handle,
    bb: svg::Handle,
    bn: svg::Handle,
    br: svg::Handle,
    bp: svg::Handle,
}

enum Message {
    ChessMove([shakmaty::Square; 2]),
}

struct Board {
    svg_pieces: SvgPieces,
    black_orientation: bool,
    position: shakmaty::Chess,
    history: Vec<String>,
    last_move: Option<shakmaty::Move>,
}

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .theme(App::theme)
        .title("P2P Chess")
        .centered()
        .run()
}

impl App {
    fn theme(&self) -> iced::Theme {
        iced::Theme::Oxocarbon
    }

    fn new() -> Self {
        let mut sink_handle =
            rodio::DeviceSinkBuilder::open_default_sink().expect("open default audio stream");
        sink_handle.log_on_drop(false);
        let audio_player = rodio::Player::connect_new(&sink_handle.mixer());

        let svg_pieces = SvgPieces {
            wk: svg::Handle::from_memory(WK),
            wq: svg::Handle::from_memory(WQ),
            wb: svg::Handle::from_memory(WB),
            wn: svg::Handle::from_memory(WN),
            wr: svg::Handle::from_memory(WR),
            wp: svg::Handle::from_memory(WP),
            bk: svg::Handle::from_memory(BK),
            bq: svg::Handle::from_memory(BQ),
            bb: svg::Handle::from_memory(BB),
            bn: svg::Handle::from_memory(BN),
            br: svg::Handle::from_memory(BR),
            bp: svg::Handle::from_memory(BP),
        };

        let board = Board {
            svg_pieces,
            black_orientation: false,
            position: shakmaty::Chess::new(),
            history: vec![],
            last_move: None,
        };

        App {
            screen: Screen::Play(board),
            _sink_handle: sink_handle,
            audio_player,
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::ChessMove([from_square, to_square]) => match &mut self.screen {
                Screen::Play(board) => {
                    let mut sound = MOVE_SOUND;
                    let uci_move = shakmaty::uci::UciMove::Normal {
                        from: from_square,
                        to: to_square,
                        promotion: None,
                    };
                    if let Ok(legal_move) = uci_move.to_move(&board.position) {
                        if legal_move.is_capture() {
                            sound = CAPTURE_SOUND;
                        }
                        if let Ok(source) = Decoder::new(std::io::Cursor::new(sound)) {
                            self.audio_player.stop();
                            self.audio_player.append(source);
                        }
                        if let Ok(new_pos) = board.position.clone().play(legal_move) {
                            board.position = new_pos;
                            board.history.push(uci_move.to_string());
                            // board.last_move_uci = uci_move.to_string();
                            board.last_move = Some(legal_move);
                        }
                    }
                }
            },
        }
    }

    fn view(&self) -> iced::Element<'_, Message> {
        match &self.screen {
            Screen::Play(board) => center(canvas(board).width(640).height(640)).into(),
        }
    }
}

#[derive(Default)]
struct BoardProgramState {
    hovering_piece: Option<shakmaty::Piece>,
    dragging_piece: Option<shakmaty::Piece>,
    drag_top_left: iced::Point,
    drag_from_square: Option<shakmaty::Square>,
    drag_to_square: Option<shakmaty::Square>,
}
impl canvas::Program<Message> for Board {
    type State = BoardProgramState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: iced::Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Option<Action<Message>> {
        match event {
            iced::Event::Mouse(mouse_event) => match mouse_event {
                iced::mouse::Event::CursorMoved { position: _ } => {
                    if let Some(point) = cursor.position_in(bounds) {
                        let x = (point.x / SQUARE_WIDTH).floor();
                        let y = (point.y / SQUARE_WIDTH).floor();
                        state.drag_top_left = iced::Point::new(
                            point.x - (SQUARE_WIDTH / 2.0),
                            point.y - (SQUARE_WIDTH / 2.0),
                        );
                        // if already dragging, request repaint
                        if state.dragging_piece.is_some() {
                            return Some(Action::request_redraw());
                        }
                        let square = self.get_square(x as u32, y as u32);
                        if let Some(piece) = self.position.board().piece_at(square) {
                            state.hovering_piece = Some(piece);
                        } else {
                            state.hovering_piece = None;
                        }
                    }
                }
                iced::mouse::Event::ButtonPressed(button) => match button {
                    iced::mouse::Button::Left => {
                        if let Some(point) = cursor.position_in(bounds) {
                            let x = (point.x / SQUARE_WIDTH).floor();
                            let y = (point.y / SQUARE_WIDTH).floor();
                            state.drag_top_left = iced::Point::new(
                                point.x - (SQUARE_WIDTH / 2.0),
                                point.y - (SQUARE_WIDTH / 2.0),
                            );
                            let square = self.get_square(x as u32, y as u32);
                            state.drag_from_square = Some(square);
                            if let Some(piece) = self.position.board().piece_at(square) {
                                match (piece.color, self.black_orientation) {
                                    (shakmaty::Color::White, false) => {
                                        state.dragging_piece = Some(piece);
                                        state.hovering_piece = None;
                                    }
                                    (shakmaty::Color::Black, true) => {
                                        state.dragging_piece = Some(piece);
                                        state.hovering_piece = None;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {}
                },
                iced::mouse::Event::ButtonReleased(button) => match button {
                    iced::mouse::Button::Left => {
                        if state.dragging_piece.is_some() {
                            state.dragging_piece = None;
                            if let Some(point) = cursor.position_in(bounds) {
                                let x = (point.x / SQUARE_WIDTH).floor();
                                let y = (point.y / SQUARE_WIDTH).floor();
                                state.drag_top_left = iced::Point::new(
                                    point.x - (SQUARE_WIDTH / 2.0),
                                    point.y - (SQUARE_WIDTH / 2.0),
                                );
                                let square = self.get_square(x as u32, y as u32);
                                state.drag_to_square = Some(square);
                                // if let Some(piece) = self.position.board().piece_at(square) {
                                //     state.hovering_piece = Some(piece);
                                // } else {
                                //     state.hovering_piece = None;
                                // }

                                if let Some(from_square) = state.drag_from_square {
                                    if let Some(to_square) = state.drag_to_square {
                                        // send chess move message
                                        return Some(Action::publish(Message::ChessMove([
                                            from_square,
                                            to_square,
                                        ])));
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }

        None
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> iced::mouse::Interaction {
        // change cursor when hovering over piece
        if state.dragging_piece.is_some() {
            return iced::mouse::Interaction::Grabbing;
        }
        if state.hovering_piece.is_some() {
            return iced::mouse::Interaction::Grab;
        }
        iced::mouse::Interaction::default()
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // draw board
        let light_last_move_color = iced::Color::from_rgb8(180, 185, 190);
        let dark_last_move_color = iced::Color::from_rgb8(145, 150, 155);
        let square_size = iced::Size::from([SQUARE_WIDTH, SQUARE_WIDTH]);
        let mut color;
        for file in 0..8 {
            for rank in 0..8 {
                let square = self.get_square(file, rank);
                let light_square = square.is_light();
                if light_square {
                    color = WHITE_SQUARE_COLOR;
                } else {
                    color = BLACK_SQUARE_COLOR;
                }

                let top_left = iced::Point::new(
                    (file * SQUARE_WIDTH as u32) as f32,
                    (rank * SQUARE_WIDTH as u32) as f32,
                );
                let rect = canvas::Path::rectangle(top_left, square_size);

                // last move highlight
                if let Some(lm) = self.last_move {
                    if let Some(from) = lm.from() {
                        if from == square {
                            if light_square {
                                color = light_last_move_color;
                            } else {
                                color = dark_last_move_color;
                            };
                        } else if lm.to() == square {
                            if light_square {
                                color = light_last_move_color;
                            } else {
                                color = dark_last_move_color;
                            };
                        }
                    }
                }

                // prevent drawing duplicate piece while dragging
                // highlight dragging from square while dragging
                if state.dragging_piece.is_some()
                    && let Some(from_square) = state.drag_from_square
                {
                    if from_square == square {
                        if light_square {
                            color = light_last_move_color;
                        } else {
                            color = dark_last_move_color;
                        };

                        frame.fill(&rect, color);
                        continue;
                    }
                }

                frame.fill(&rect, color);
                if let Some(piece) = self.position.board().piece_at(square) {
                    let svg = self.piece_to_svg(piece);
                    frame.draw_svg(iced::Rectangle::new(top_left, square_size), svg);
                }
            }
        }

        // draw dragged piece
        if let Some(piece) = state.dragging_piece {
            let svg = self.piece_to_svg(piece);
            frame.draw_svg(iced::Rectangle::new(state.drag_top_left, square_size), svg);
        }

        vec![frame.into_geometry()]
    }
}

impl Board {
    fn piece_to_svg(&self, piece: shakmaty::Piece) -> &iced::advanced::svg::Handle {
        match (piece.color, piece.role) {
            (shakmaty::Color::White, shakmaty::Role::Pawn) => &self.svg_pieces.wp,
            (shakmaty::Color::White, shakmaty::Role::Knight) => &self.svg_pieces.wn,
            (shakmaty::Color::White, shakmaty::Role::Bishop) => &self.svg_pieces.wb,
            (shakmaty::Color::White, shakmaty::Role::Rook) => &self.svg_pieces.wr,
            (shakmaty::Color::White, shakmaty::Role::Queen) => &self.svg_pieces.wq,
            (shakmaty::Color::White, shakmaty::Role::King) => &self.svg_pieces.wk,
            (shakmaty::Color::Black, shakmaty::Role::Pawn) => &self.svg_pieces.bp,
            (shakmaty::Color::Black, shakmaty::Role::Knight) => &self.svg_pieces.bn,
            (shakmaty::Color::Black, shakmaty::Role::Bishop) => &self.svg_pieces.bb,
            (shakmaty::Color::Black, shakmaty::Role::Rook) => &self.svg_pieces.br,
            (shakmaty::Color::Black, shakmaty::Role::Queen) => &self.svg_pieces.bq,
            (shakmaty::Color::Black, shakmaty::Role::King) => &self.svg_pieces.bk,
        }
    }

    fn get_square(&self, x: u32, y: u32) -> shakmaty::Square {
        if self.black_orientation {
            shakmaty::Square::from_coords(shakmaty::File::new(7 - x), shakmaty::Rank::new(y))
        } else {
            shakmaty::Square::from_coords(shakmaty::File::new(x), shakmaty::Rank::new(7 - y))
        }
    }
}

// fn seconds_to_clock(total_seconds: i64) -> String {
//     let minutes = total_seconds / 60;
//     let seconds = total_seconds % 60;
//     format!("{:02}:{:02}", minutes, seconds)
// }
//
// fn milliseconds_to_clock(millis: i64) -> String {
//     let total_seconds = millis / 1000;
//     let minutes = total_seconds / 60;
//     let seconds = total_seconds % 60;
//     let tenths = (millis % 1000) / 100;
//
//     if minutes > 0 {
//         format!("{:02}:{:02}", minutes, seconds)
//     } else {
//         format!("{:02}.{}", seconds, tenths)
//     }
// }
