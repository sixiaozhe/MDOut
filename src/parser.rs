use pulldown_cmark::Options;

pub fn options() -> Options {
    let mut o = Options::empty();
    o.insert(Options::ENABLE_TABLES);
    o.insert(Options::ENABLE_TASKLISTS);
    o.insert(Options::ENABLE_STRIKETHROUGH);
    o
}
