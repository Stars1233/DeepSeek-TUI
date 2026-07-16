//! Byte-sequence builders for keys and paste.
//!
//! These produce the raw bytes a real terminal would deliver to the child's
//! PTY slave. They match crossterm's input-decoding tables (keyboard
//! enhancement off, mouse capture off, bracketed paste on).

/// Plain key press helpers.
pub mod key {
    pub fn ch(c: char) -> Vec<u8> {
        let mut buf = [0u8; 4];
        c.encode_utf8(&mut buf).as_bytes().to_vec()
    }

    pub fn enter() -> Vec<u8> {
        b"\r".to_vec()
    }

    pub fn esc() -> Vec<u8> {
        vec![0x1b]
    }

    pub fn down() -> Vec<u8> {
        b"\x1b[B".to_vec()
    }

    pub fn page_up() -> Vec<u8> {
        b"\x1b[5~".to_vec()
    }

    pub fn text(s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    pub fn alt(c: char) -> Vec<u8> {
        let mut out = vec![0x1b];
        out.extend(ch(c));
        out
    }
}

/// SGR mouse sequences use one-based terminal coordinates.
pub mod mouse {
    pub fn click(row: u16, col: u16) -> Vec<u8> {
        format!(
            "\x1b[<0;{};{}M\x1b[<0;{};{}m",
            col + 1,
            row + 1,
            col + 1,
            row + 1
        )
        .into_bytes()
    }

    pub fn wheel_down(row: u16, col: u16) -> Vec<u8> {
        format!("\x1b[<65;{};{}M", col + 1, row + 1).into_bytes()
    }

    pub fn wheel_up(row: u16, col: u16) -> Vec<u8> {
        format!("\x1b[<64;{};{}M", col + 1, row + 1).into_bytes()
    }
}

/// Bracketed-paste helpers.
///
/// Wraps the payload in `ESC [ 2 0 0 ~` … `ESC [ 2 0 1 ~` so the receiver sees
/// a `crossterm::Event::Paste(text)` rather than a key-by-key stream.
pub mod paste {
    pub fn bracketed(text: &str) -> Vec<u8> {
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(text.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    }

    /// Same as [`bracketed`] but does not wrap — simulates a terminal that
    /// has bracketed paste disabled (e.g. some Windows PowerShell setups).
    /// The child sees the bytes as ordinary keystrokes; an embedded `\n`
    /// becomes an Enter press, which is what reproduces #1073.
    pub fn unbracketed(text: &str) -> Vec<u8> {
        text.replace('\n', "\r").as_bytes().to_vec()
    }
}
