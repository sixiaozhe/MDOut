use std::io::Read;
use std::sync::mpsc::{self, Receiver};

pub enum InputMsg {
    Chunk(String),
    Eof,
    Error(std::io::Error),
}

pub struct Utf8Decoder {
    buf: Vec<u8>,
}

impl Utf8Decoder {
    pub fn new() -> Self {
        Utf8Decoder { buf: Vec::new() }
    }

    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.buf.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.buf) {
                Ok(s) => {
                    out.push_str(s);
                    self.buf.clear();
                    break;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    out.push_str(std::str::from_utf8(&self.buf[..valid]).unwrap());
                    match e.error_len() {
                        Some(len) => {
                            out.push('\u{FFFD}');
                            self.buf.drain(..valid + len);
                        }
                        None => {
                            self.buf.drain(..valid);
                            break;
                        }
                    }
                }
            }
        }
        out
    }

    pub fn finish(mut self) -> String {
        let rest = String::from_utf8_lossy(&self.buf).into_owned();
        self.buf.clear();
        rest
    }
}

impl Default for Utf8Decoder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn spawn_reader<R: Read + Send + 'static>(mut reader: R) -> Receiver<InputMsg> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut decoder = Utf8Decoder::new();
        let mut raw = [0u8; 8192];
        loop {
            match reader.read(&mut raw) {
                Ok(0) => {
                    let tail = decoder.finish();
                    if !tail.is_empty() {
                        let _ = tx.send(InputMsg::Chunk(tail));
                    }
                    let _ = tx.send(InputMsg::Eof);
                    break;
                }
                Ok(n) => {
                    let text = decoder.push(&raw[..n]);
                    if !text.is_empty() && tx.send(InputMsg::Chunk(text)).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    let _ = tx.send(InputMsg::Error(e));
                    break;
                }
            }
        }
    });
    rx
}

pub fn spawn_stdin_reader() -> Receiver<InputMsg> {
    spawn_reader(std::io::stdin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_split_multibyte_char() {
        let mut d = Utf8Decoder::new();
        let bytes = "中".as_bytes();
        assert_eq!(d.push(&bytes[..1]), "");
        assert_eq!(d.push(&bytes[1..2]), "");
        assert_eq!(d.push(&bytes[2..]), "中");
    }

    #[test]
    fn decodes_incremental_ascii_and_cjk() {
        let mut d = Utf8Decoder::new();
        assert_eq!(d.push(b"ab"), "ab");
        assert_eq!(d.push("中".as_bytes()), "中");
        assert_eq!(d.finish(), "");
    }

    #[test]
    fn replaces_invalid_bytes_lossily() {
        let mut d = Utf8Decoder::new();
        assert_eq!(d.push(&[0xff, b'a']), "\u{FFFD}a");
    }

    #[test]
    fn finish_flushes_incomplete_as_replacement() {
        let mut d = Utf8Decoder::new();
        assert_eq!(d.push(&[0xe4, 0xb8]), "");
        assert_eq!(d.finish(), "\u{FFFD}");
    }

    #[test]
    fn reader_thread_forwards_chunks_then_eof() {
        use std::io::Cursor;
        let rx = spawn_reader(Cursor::new(b"hello".to_vec()));
        match rx.recv().unwrap() { InputMsg::Chunk(s) => assert_eq!(s, "hello"), _ => panic!("want chunk") }
        assert!(matches!(rx.recv().unwrap(), InputMsg::Eof));
    }
}
