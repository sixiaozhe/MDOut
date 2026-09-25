pub enum InputMsg { Chunk(String), Eof, Error(std::io::Error) }
