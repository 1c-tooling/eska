//! Standalone T63 research harness; compile in a private playground crate, not the eska binary.

use quick_xml::{NsReader, events::Event, name::ResolveResult};
use std::{
    borrow::Cow,
    error::Error,
    fs,
    io::{BufRead, BufReader},
    path::Path,
    time::Instant,
};

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_NODES: usize = 1_000_000;
const MAX_DEPTH: usize = 256;
type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Default, Eq, PartialEq)]
struct Stats {
    elements: usize,
    metadata_elements: usize,
    attributes: usize,
    text_bytes: usize,
    normalized_text_copy_bytes: usize,
    max_depth: usize,
    first_metadata_offset: Option<usize>,
    event_buffer_capacity: usize,
}

/// Parse a bounded file with the existing tree library and inspect decoded values/positions.
fn tree(path: &Path) -> Result<Stats> {
    let input = fs::read_to_string(path)?;
    let document = roxmltree::Document::parse_with_options(
        &input,
        roxmltree::ParsingOptions {
            nodes_limit: MAX_NODES as u32,
            ..Default::default()
        },
    )?;
    let mut stats = Stats::default();
    for node in document.descendants() {
        if node.is_element() {
            let depth = node.ancestors().filter(roxmltree::Node::is_element).count();
            if depth > MAX_DEPTH {
                return Err("depth limit".into());
            }
            stats.max_depth = stats.max_depth.max(depth);
            stats.elements += 1;
            stats.attributes += node.attributes().len();
            if node.tag_name().namespace() == Some(MD) {
                stats.metadata_elements += 1;
                stats
                    .first_metadata_offset
                    .get_or_insert(node.range().start);
            }
        } else if node.is_text()
            && let Some(text) = node.text_storage()
        {
            stats.text_bytes += text.as_str().len();
            if matches!(text, roxmltree::StringStorage::Owned(_)) {
                stats.normalized_text_copy_bytes += text.as_str().len();
            }
        }
    }
    Ok(stats)
}

/// Account for normalized text copies without pretending to measure all parser allocations.
fn text(stats: &mut Stats, value: Cow<'_, str>) {
    stats.text_bytes += value.len();
    if matches!(value, Cow::Owned(_)) {
        stats.normalized_text_copy_bytes += value.len();
    }
}

/// Inspect namespace-aware events with explicit document, DTD, depth and event limits.
fn stream_input(mut input: impl BufRead) -> Result<Stats> {
    // quick-xml 0.42 removes the UTF-8 BOM without adding it to buffer_position.
    let bom_bytes = usize::from(input.fill_buf()?.starts_with(&[0xef, 0xbb, 0xbf])) * 3;
    let mut reader = NsReader::from_reader(input);
    let mut buffer = Vec::new();
    let mut stats = Stats::default();
    let mut depth = 0;
    let mut roots = 0;
    let mut events = 0;
    let mut nodes = 1;
    let mut previous_text = false;
    loop {
        let (namespace, event) = reader.read_resolved_event_into(&mut buffer)?;
        if matches!(namespace, ResolveResult::Unknown(_)) {
            return Err("unknown namespace prefix".into());
        }
        let metadata =
            matches!(namespace, ResolveResult::Bound(namespace) if namespace.as_ref() == MD);
        let end = reader.buffer_position() as usize;
        let empty = matches!(event, Event::Empty(_));
        let current_text = depth > 0
            && matches!(
                event,
                Event::Text(_) | Event::CData(_) | Event::GeneralRef(_)
            );
        if matches!(
            event,
            Event::Start(_) | Event::Empty(_) | Event::Comment(_) | Event::PI(_)
        ) || (current_text && !previous_text)
        {
            nodes += 1;
        }
        if nodes > MAX_NODES {
            return Err("node limit".into());
        }
        previous_text = current_text;
        match event {
            Event::Start(element) | Event::Empty(element) => {
                if depth == 0 {
                    roots += 1;
                }
                stats.elements += 1;
                stats.max_depth = stats.max_depth.max(depth + 1);
                if stats.max_depth > MAX_DEPTH {
                    return Err("depth limit".into());
                }
                for attribute in element.attributes() {
                    let attribute = attribute?;
                    if attribute.key.as_ref() != "xmlns"
                        && !attribute.key.as_ref().starts_with("xmlns:")
                    {
                        stats.attributes += 1;
                        let _ = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)?;
                    }
                }
                if metadata {
                    stats.metadata_elements += 1;
                    let start = bom_bytes + end - element.len() - if empty { 3 } else { 2 };
                    stats.first_metadata_offset.get_or_insert(start);
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or("unbalanced XML")?;
            }
            Event::Text(value) => {
                let value = value.xml10_content();
                if depth == 0 {
                    if !value.trim().is_empty() {
                        return Err("text outside root".into());
                    }
                } else {
                    text(&mut stats, value);
                }
            }
            Event::CData(value) => {
                if depth == 0 {
                    return Err("CDATA outside root".into());
                }
                text(&mut stats, value.xml10_content());
            }
            Event::GeneralRef(value) => {
                if depth == 0 {
                    return Err("reference outside root".into());
                }
                if let Some(character) = value.resolve_char_ref()? {
                    stats.text_bytes += character.len_utf8();
                } else {
                    let resolved = match &*value {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "apos" => "'",
                        "quot" => "\"",
                        _ => return Err("unknown entity".into()),
                    };
                    stats.text_bytes += resolved.len();
                }
            }
            Event::DocType(_) => return Err("DTD disabled".into()),
            Event::Eof => break,
            _ => {}
        }
        events += 1;
        if events > MAX_NODES * 2 {
            return Err("event limit".into());
        }
        stats.event_buffer_capacity = stats.event_buffer_capacity.max(buffer.capacity());
        buffer.clear();
    }
    if depth != 0 || roots != 1 {
        return Err("missing or multiple roots".into());
    }
    Ok(stats)
}

/// Stream from a bounded file with one reusable event buffer.
fn stream(path: &Path) -> Result<Stats> {
    stream_input(BufReader::new(fs::File::open(path)?))
}

/// Run repeated parse-and-inspect operations; OS cache state and peak RSS are measured externally.
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: xml-parser-bench tree|stream FILE REPEATS".into());
    }
    let path = Path::new(&args[2]);
    let bytes = fs::metadata(path)?.len();
    if bytes > MAX_BYTES {
        return Err("byte limit".into());
    }
    let repeats: usize = args[3].parse()?;
    if repeats == 0 {
        return Err("positive repeats required".into());
    }
    let parser: fn(&Path) -> Result<Stats> = match args[1].as_str() {
        "tree" => tree,
        "stream" => stream,
        _ => return Err("unknown parser".into()),
    };
    let mut timings = Vec::with_capacity(repeats);
    let mut result = Stats::default();
    for _ in 0..repeats {
        let started = Instant::now();
        result = std::hint::black_box(parser(path)?);
        timings.push(started.elapsed().as_nanos());
    }
    timings.sort_unstable();
    println!(
        "mode={} bytes={bytes} repeats={repeats} median_ns={} p95_ns={} stats={result:?}",
        args[1],
        timings[repeats / 2],
        timings[((repeats * 95).div_ceil(100) - 1).min(repeats - 1)]
    );
    Ok(())
}
