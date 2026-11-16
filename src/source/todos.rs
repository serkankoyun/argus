//! Open items of `tasks/todo.md` files next to the sessions' working folders,
//! for the Backlog view.
//!
//! The format is plain Markdown and read leniently: headings (`#` to `######`)
//! give each item its section; checkbox list items (`- [ ]`, `* [ ]`, `+ [ ]`,
//! `1. [ ]`, `1) [ ]`) are open items, every other box (`[x]`, `[-]`, `[~]`,
//! ...) is closed and ignored. Lines indented below an open item (plain text or
//! box-less sub points) become its description; an indented open box is an item
//! of its own that remembers its parent. Items under a heading that means
//! "waiting on you" (see [`WAITING_ON_YOU`]) are flagged for the user.
//! Nothing here panics on odd input, and the caps keep huge files cheap.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::model::{Millis, TodoFile, TodoItem};

/// Files read at most (the first ones found).
pub const MAX_TODO_FILES: usize = 20;
/// Bytes read from one file; the rest is left out.
pub const MAX_FILE_BYTES: usize = 1 << 20;
/// Open items kept per file.
pub const MAX_ITEMS_PER_FILE: usize = 200;
/// Open items kept over all files.
pub const MAX_TODO_ITEMS: usize = 500;
/// Files unchanged for longer than this are not read.
pub const TODO_MAX_AGE: Millis = 30 * 24 * 3600 * 1000;

const MAX_TITLE: usize = 300;
const MAX_PARENT: usize = 80;
const MAX_SECTION: usize = 120;
const MAX_DETAIL_LINES: usize = 12;
const MAX_DETAIL: usize = 1200;

/// Heading phrases that mean "waiting on you". Headings are compared after
/// [`fold`] (lowercase, Turkish letters folded, whitespace collapsed) and match
/// when they contain one of these. Bare words such as "decision", "karar" or
/// "onay" are left out on purpose: they name too many ordinary sections.
pub const WAITING_ON_YOU: &[&str] = &[
    "waiting on you",
    "waiting for you",
    "waiting on the user",
    "waiting for the user",
    "waiting on user",
    "waiting for user",
    "waiting on me",
    "needs your",
    "your decision",
    "your approval",
    "decision needed",
    "decisions needed",
    "needs decision",
    "pending decision",
    "awaiting decision",
    "awaiting approval",
    "open questions",
    "questions for you",
    "kullanicidan bekle",
    "senden bekle",
    "sizden bekle",
    "karar bekle",
    "onay bekle",
    "karar gereken",
    "karar verilecek",
    "acik soru",
];

/// Lowercase with Turkish letters folded to ASCII and whitespace collapsed.
pub fn fold(s: &str) -> String {
    let mapped: String = s
        .chars()
        .map(|c| match c {
            'İ' | 'I' | 'ı' => 'i',
            'Ş' | 'ş' => 's',
            'Ğ' | 'ğ' => 'g',
            'Ü' | 'ü' => 'u',
            'Ö' | 'ö' => 'o',
            'Ç' | 'ç' => 'c',
            c => c,
        })
        .collect();
    mapped
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a heading means "waiting on you".
pub fn waits_on_you_heading(heading: &str) -> bool {
    let h = fold(heading);
    WAITING_ON_YOU.iter().any(|k| h.contains(k))
}

/// `tasks/todo.md` files in the given working folders or up to three levels
/// above, each once, at most [`MAX_TODO_FILES`].
pub fn find_todo_files<'a>(cwds: impl Iterator<Item = &'a str>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for cwd in cwds.filter(|c| !c.is_empty()) {
        for dir in Path::new(cwd).ancestors().take(4) {
            let f = dir.join("tasks").join("todo.md");
            if out.len() < MAX_TODO_FILES && !out.contains(&f) && f.is_file() {
                out.push(f);
            }
        }
    }
    out
}

/// Columns of leading whitespace (a tab counts as four) and the rest.
fn indent_of(line: &str) -> (usize, &str) {
    let mut n = 0;
    for (i, c) in line.char_indices() {
        match c {
            ' ' => n += 1,
            '\t' => n += 4,
            _ => return (n, &line[i..]),
        }
    }
    (n, "")
}

/// `(level, text)` of a Markdown heading.
fn heading(rest: &str) -> Option<(usize, &str)> {
    let level = rest.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let after = &rest[level..];
    if !after.is_empty() && !after.starts_with([' ', '\t']) {
        return None;
    }
    Some((level, after.trim().trim_end_matches('#').trim()))
}

/// `(open, text)` of a checkbox list item.
fn checkbox(rest: &str) -> Option<(bool, &str)> {
    let after_marker = if let Some(r) = rest.strip_prefix(['-', '*', '+']) {
        r
    } else {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        rest[digits..].strip_prefix(['.', ')'])?
    };
    let r = after_marker.strip_prefix([' ', '\t'])?.trim_start();
    let r = r.strip_prefix('[')?;
    let mut chars = r.chars();
    let mark = chars.next()?;
    let r = chars.as_str().strip_prefix(']')?;
    if !r.is_empty() && !r.starts_with([' ', '\t']) {
        return None;
    }
    Some((mark == ' ', r.trim()))
}

/// Words that start a wait marker, already folded (see [`fold`]).
pub const WAIT_MARKERS: &[&str] = &[
    "waiting on",
    "waiting for",
    "blocked by",
    "bekliyor",
    "beklenen",
    "engel",
];

const MAX_WAITS_ON: usize = 120;

/// One character as [`fold`] sees it.
fn fold_char(c: char) -> char {
    match c {
        'İ' | 'I' | 'ı' => 'i',
        'Ş' | 'ş' => 's',
        'Ğ' | 'ğ' => 'g',
        'Ü' | 'ü' => 'u',
        'Ö' | 'ö' => 'o',
        'Ç' | 'ç' => 'c',
        c => c.to_lowercase().next().unwrap_or(c),
    }
}

/// What a marker segment (`waiting on: X`) names, if it is one.
fn marker_target(segment: &str) -> Option<String> {
    let seg = segment.trim();
    for k in WAIT_MARKERS {
        let n = k.chars().count();
        let head: String = seg.chars().take(n).map(fold_char).collect();
        if head != *k {
            continue;
        }
        let rest: String = seg.chars().skip(n).collect();
        // A word that only starts like the marker ("engelli") is not one.
        if rest.starts_with(|c: char| c.is_alphanumeric()) {
            continue;
        }
        let what = rest.trim_start_matches([':', ' ', '\t']).trim();
        if what.is_empty() {
            continue;
        }
        return Some(cut(what, MAX_WAITS_ON));
    }
    None
}

/// Splits a wait marker off an item text: `(waiting on: X)` anywhere, or
/// `— waiting on: X`, `- blocked by: X`, `; bekliyor: X` at the end.
/// Returns the text without the marker and what it waits for.
pub fn split_wait(text: &str) -> (String, String) {
    let mut from = 0;
    while let Some(open) = text[from..].find('(').map(|i| i + from) {
        let Some(close) = text[open..].find(')').map(|i| i + open) else {
            break;
        };
        if let Some(what) = marker_target(&text[open + 1..close]) {
            let rest = format!(
                "{} {}",
                text[..open].trim_end(),
                text[close + 1..].trim_start()
            );
            return (rest.trim().to_string(), what);
        }
        from = open + 1;
    }
    // The first separator followed by a marker: the target may itself
    // contain separators ("Deploy - waiting on: ops - EU").
    let mut cuts: Vec<(usize, usize)> = [" — ", " - ", ";"]
        .iter()
        .flat_map(|sep| text.match_indices(sep).map(|(i, m)| (i, m.len())))
        .collect();
    cuts.sort_unstable();
    for (i, len) in cuts {
        if let Some(what) = marker_target(&text[i + len..]) {
            return (text[..i].trim().to_string(), what);
        }
    }
    (text.to_string(), String::new())
}

/// Title text without emphasis markers.
fn clean(s: &str) -> String {
    s.replace("**", "").replace("__", "").trim().to_string()
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    } else {
        s.to_string()
    }
}

/// A box on the nesting stack: its indent, title, and item index when open.
struct Open {
    indent: usize,
    title: String,
    item: Option<usize>,
}

/// Open items of a todo file read as bytes (invalid UTF-8 is replaced).
pub fn parse_bytes(bytes: &[u8]) -> (Vec<TodoItem>, u32) {
    parse(&String::from_utf8_lossy(bytes))
}

/// Open items of a todo file, capped at [`MAX_ITEMS_PER_FILE`], and how many
/// open items the cap left out.
pub fn parse(text: &str) -> (Vec<TodoItem>, u32) {
    let mut items: Vec<TodoItem> = Vec::new();
    let mut detail_lines: Vec<usize> = Vec::new();
    let mut headings: Vec<(usize, String)> = Vec::new();
    let mut waiting: Option<usize> = None;
    let mut boxes: Vec<Open> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        let (indent, rest) = indent_of(line);
        let rest = rest.trim_end();
        if rest.is_empty() {
            continue;
        }
        if let Some((level, title)) = heading(rest).filter(|_| indent < 4) {
            boxes.clear();
            headings.retain(|(l, _)| *l < level);
            headings.push((level, title.to_string()));
            if waiting.is_some_and(|w| level <= w) {
                waiting = None;
            }
            if waiting.is_none() && waits_on_you_heading(title) {
                waiting = Some(level);
            }
            continue;
        }
        // Everything indented at or left of a box ends that box.
        while boxes.last().is_some_and(|b| b.indent >= indent) {
            boxes.pop();
        }
        if let Some((open, text)) = checkbox(rest) {
            let cleaned = clean(text);
            let (title, waits_on) = split_wait(&cleaned);
            // An item that is nothing but a marker keeps the marker as its title.
            let title = if title.is_empty() { cleaned } else { title };
            let parent = boxes
                .last()
                .map(|b| cut(&b.title, MAX_PARENT))
                .unwrap_or_default();
            let item = (open && !title.is_empty()).then(|| {
                let section = headings
                    .iter()
                    .rev()
                    .take(2)
                    .rev()
                    .map(|(_, h)| h.as_str())
                    .collect::<Vec<_>>()
                    .join(" › ");
                items.push(TodoItem {
                    line: (n + 1).min(u32::MAX as usize) as u32,
                    title: cut(&title, MAX_TITLE),
                    detail: String::new(),
                    section: cut(&section, MAX_SECTION),
                    parent,
                    waits_on_you: waiting.is_some(),
                    waits_on: waits_on.clone(),
                });
                detail_lines.push(0);
                items.len() - 1
            });
            boxes.push(Open {
                indent,
                title,
                item,
            });
            continue;
        }
        // A continuation line of the innermost box above it, when that box is open.
        if let Some(i) = boxes.last().and_then(|b| b.item) {
            let it = &mut items[i];
            if detail_lines[i] < MAX_DETAIL_LINES && it.detail.chars().count() < MAX_DETAIL {
                if !it.detail.is_empty() {
                    it.detail.push('\n');
                }
                it.detail.push_str(rest);
                if it.detail.chars().count() > MAX_DETAIL {
                    it.detail = it.detail.chars().take(MAX_DETAIL).collect();
                }
                detail_lines[i] += 1;
            }
        }
    }
    // A marker may also stand alone on the first continuation line.
    for it in items.iter_mut().filter(|i| i.waits_on.is_empty()) {
        let first = it.detail.lines().next().unwrap_or("");
        let first = first.trim_start_matches(['-', '*', '+', ' ']).trim();
        let inner = first
            .strip_prefix('(')
            .and_then(|f| f.strip_suffix(')'))
            .unwrap_or(first);
        if let Some(what) = marker_target(inner) {
            it.waits_on = what;
        }
    }
    keep_first(items, MAX_ITEMS_PER_FILE)
}

/// At most `max` items: "waiting on you" ones first, then the rest, each in
/// file order; returns the kept items in file order and how many were left out.
fn keep_first(items: Vec<TodoItem>, max: usize) -> (Vec<TodoItem>, u32) {
    if items.len() <= max {
        return (items, 0);
    }
    let omitted = (items.len() - max) as u32;
    let you = items.iter().filter(|i| i.waits_on_you).count().min(max);
    let mut pool_left = max - you;
    let mut you_left = you;
    let kept = items
        .into_iter()
        .filter(|i| {
            let left = if i.waits_on_you {
                &mut you_left
            } else {
                &mut pool_left
            };
            if *left > 0 {
                *left -= 1;
                true
            } else {
                false
            }
        })
        .collect();
    (kept, omitted)
}

/// Applies [`MAX_TODO_ITEMS`] over all files, "waiting on you" items first.
pub fn cap_total(files: &mut [TodoFile], max: usize) {
    let mut you_left = max;
    let mut pool_left = max.saturating_sub(
        files
            .iter()
            .flat_map(|f| &f.items)
            .filter(|i| i.waits_on_you)
            .count()
            .min(max),
    );
    for f in files.iter_mut() {
        let before = f.items.len();
        f.items.retain(|i| {
            let left = if i.waits_on_you {
                &mut you_left
            } else {
                &mut pool_left
            };
            if *left > 0 {
                *left -= 1;
                true
            } else {
                false
            }
        });
        f.omitted += (before - f.items.len()) as u32;
    }
}

fn millis(t: SystemTime) -> Millis {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as Millis)
        .unwrap_or(0)
}

/// Reads todo files, parsing a file again only when its time or size changed.
#[derive(Default)]
pub struct TodoReader {
    cache: HashMap<PathBuf, (SystemTime, u64, TodoFile)>,
    /// Files parsed so far (for tests).
    pub parses: usize,
}

impl TodoReader {
    /// The files changed within [`TODO_MAX_AGE`], their items capped.
    pub fn read(&mut self, paths: &[PathBuf], now: Millis) -> Vec<TodoFile> {
        self.cache.retain(|p, _| paths.contains(p));
        let mut out = Vec::new();
        for p in paths {
            let Ok(meta) = std::fs::metadata(p) else {
                self.cache.remove(p);
                continue;
            };
            let Ok(mtime) = meta.modified() else { continue };
            if now - millis(mtime) > TODO_MAX_AGE {
                continue;
            }
            let len = meta.len();
            match self.cache.get(p) {
                Some((t, l, f)) if *t == mtime && *l == len => out.push(f.clone()),
                _ => {
                    let Some(f) = read_file(p, millis(mtime)) else {
                        continue;
                    };
                    self.parses += 1;
                    self.cache.insert(p.clone(), (mtime, len, f.clone()));
                    out.push(f);
                }
            }
        }
        cap_total(&mut out, MAX_TODO_ITEMS);
        out
    }
}

fn read_file(p: &Path, modified: Millis) -> Option<TodoFile> {
    let mut bytes = Vec::new();
    std::fs::File::open(p)
        .ok()?
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    let truncated = bytes.len() > MAX_FILE_BYTES;
    bytes.truncate(MAX_FILE_BYTES);
    let (items, omitted) = parse_bytes(&bytes);
    // `<root>/tasks/todo.md`
    let root = p.parent().and_then(Path::parent).unwrap_or(Path::new(""));
    Some(TodoFile {
        path: p.display().to_string(),
        root: root.display().to_string(),
        project: root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        host: String::new(),
        modified,
        items,
        omitted: omitted + u32::from(truncated),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(items: &[TodoItem]) -> Vec<&str> {
        items.iter().map(|i| i.title.as_str()).collect()
    }

    #[test]
    fn open_boxes_and_checked_are_skipped() {
        let text = "# Plan\n- [ ] alpha\n* [ ] beta\n+ [ ] gamma\n1. [ ] delta\n2) [ ] epsilon\n\
                    - [x] done one\n- [X] done two\n- [-] dropped\n- [~] partial\n- [/] half\n\
                    - [>] moved\n- [] not a box\n-[ ] no space\n- [ ]glued\n- plain item\n";
        let (items, omitted) = parse(text);
        assert_eq!(
            titles(&items),
            ["alpha", "beta", "gamma", "delta", "epsilon"]
        );
        assert_eq!(omitted, 0);
        assert_eq!(items[0].line, 2);
        assert_eq!(items[0].section, "Plan");
    }

    #[test]
    fn continuation_lines_become_detail() {
        let text = "## Work\n- [ ] **Write** the importer\n  reads the export file\n\n  - keeps unknown columns\n    and logs them\n- [ ] next one\n  first line\nback at the margin\n  not attached\n";
        let (items, _) = parse(text);
        assert_eq!(items[0].title, "Write the importer");
        assert_eq!(
            items[0].detail,
            "reads the export file\n- keeps unknown columns\nand logs them"
        );
        assert_eq!(items[1].detail, "first line");
        // Caps: 12 lines, 1200 characters.
        let mut long = String::from("- [ ] long\n");
        for k in 0..30 {
            long.push_str(&format!("  line {k}\n"));
        }
        let (items, _) = parse(&long);
        assert_eq!(items[0].detail.lines().count(), 12);
        let wide = format!(
            "- [ ] wide\n  {}\n  {}\n",
            "a".repeat(1000),
            "b".repeat(1000)
        );
        let (items, _) = parse(&wide);
        assert_eq!(items[0].detail.chars().count(), 1200);
    }

    #[test]
    fn nested_boxes_keep_parent() {
        let text = "- [ ] Parent task\n  - [ ] Child one\n    child note\n  - [x] Child done\n    ignored note\n  parent note\n- [x] Closed parent\n  - [ ] Orphan child\n";
        let (items, _) = parse(text);
        assert_eq!(titles(&items), ["Parent task", "Child one", "Orphan child"]);
        assert_eq!(items[0].parent, "");
        assert_eq!(items[1].parent, "Parent task");
        assert_eq!(items[1].detail, "child note");
        assert_eq!(items[0].detail, "parent note");
        assert_eq!(items[2].parent, "Closed parent");
    }

    #[test]
    fn waiting_headings_en_tr() {
        let text = "\
## Waiting on you
- [ ] pick a name
### Details
- [ ] inherits
## Backlog
- [ ] not waiting
## Kullanıcıdan beklenenler
- [ ] tr one
## Next
### Karar bekleyenler
- [ ] tr two
### Other
- [ ] after sibling
## Kararlar
- [ ] plain decisions
## Kullanıcı arayüzü
- [ ] ui work
# DECISION   NEEDED
- [ ] spaced
";
        let (items, _) = parse(text);
        let w: Vec<(&str, bool)> = items
            .iter()
            .map(|i| (i.title.as_str(), i.waits_on_you))
            .collect();
        assert_eq!(
            w,
            [
                ("pick a name", true),
                ("inherits", true),
                ("not waiting", false),
                ("tr one", true),
                ("tr two", true),
                ("after sibling", false),
                ("plain decisions", false),
                ("ui work", false),
                ("spaced", true),
            ]
        );
        assert_eq!(items[1].section, "Waiting on you › Details");
        assert!(waits_on_you_heading("SENDEN BEKLENENLER"));
        assert!(waits_on_you_heading("Açık sorular"));
        assert!(!waits_on_you_heading("Onay"));
        assert_eq!(fold("  İŞLEM   Çağrı "), "islem cagri");
    }

    #[test]
    fn wait_markers_en_tr() {
        let text = "\
- [ ] Ship the export (waiting on: legal review)
- [ ] Rotate keys — blocked by: #12
- [ ] Pick a font - Waiting For: you
- [ ] Yeni ekran; BEKLİYOR: tasarım onayı
- [ ] Rapor (beklenen: kullanıcı kararı) sonra
- [ ] Erişim (engel: VPN)
- [ ] Engelli erişimi ekranı
- [ ] Plain - item
- [ ] Note below
  waiting on: vendor reply
- [ ] Paren below
  (bekliyor: sen)
";
        let (items, _) = parse(text);
        let got: Vec<(&str, &str)> = items
            .iter()
            .map(|i| (i.title.as_str(), i.waits_on.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("Ship the export", "legal review"),
                ("Rotate keys", "#12"),
                ("Pick a font", "you"),
                ("Yeni ekran", "tasarım onayı"),
                ("Rapor sonra", "kullanıcı kararı"),
                ("Erişim", "VPN"),
                ("Engelli erişimi ekranı", ""),
                ("Plain - item", ""),
                ("Note below", "vendor reply"),
                ("Paren below", "sen"),
            ]
        );
        let (_, long) = split_wait(&format!("x (waiting on: {})", "y".repeat(400)));
        assert_eq!(long.chars().count(), 120);
    }

    #[test]
    fn first_trailing_marker_wins_and_marker_only_items_stay() {
        assert_eq!(
            split_wait("Deploy - waiting on: ops - EU"),
            ("Deploy".to_string(), "ops - EU".to_string())
        );
        assert_eq!(
            split_wait("A - B; blocked by: x; y"),
            ("A - B".to_string(), "x; y".to_string())
        );
        let (items, _) = parse("- [ ] (blocked by: #12)\n- [ ] **(waiting on: legal)**\n");
        let got: Vec<(&str, &str)> = items
            .iter()
            .map(|i| (i.title.as_str(), i.waits_on.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("(blocked by: #12)", "#12"),
                ("(waiting on: legal)", "legal")
            ]
        );
    }

    #[test]
    fn malformed_input_never_panics() {
        for text in [
            "",
            "# a\n## b\n###### c\n",
            "- [ ]\n- [ ]   \n",
            "####### not a heading\n- [ ] after\n",
            "- [ ] crlf\r\n  more\r\n",
            "\t- [ ] tab\n\t\tdetail\n",
            "- [",
            "1",
            "12345678901. [ ] long number",
            "#\n#\n- [ ] x",
        ] {
            let _ = parse(text);
        }
        let (items, _) = parse_bytes(b"- [ ] caf\xff\xfe ok\n  \xc3\n");
        assert_eq!(items.len(), 1);
        assert!(items[0].title.starts_with("caf"));
        let (items, _) = parse("- [ ] crlf\r\n  more\r\n");
        assert_eq!(items[0].detail, "more");
        let (items, _) = parse("####### seven\n- [ ] after\n");
        assert_eq!(items[0].section, "");
        let (items, _) = parse("\t- [ ] tab\n\t\tdetail\n");
        assert_eq!(items[0].detail, "detail");
        let (items, _) = parse(&format!("- [ ] {}", "x".repeat(5000)));
        assert_eq!(items[0].title.chars().count(), 300);
    }

    #[test]
    fn caps_hold_on_huge_files() {
        let mut text = String::from("## Pool\n");
        for k in 0..50_000 {
            text.push_str(&format!("- [ ] item {k}\n"));
        }
        text.push_str("## Waiting on you\n- [ ] the late one\n");
        let (items, omitted) = parse(&text);
        assert_eq!(items.len(), MAX_ITEMS_PER_FILE);
        assert_eq!(omitted, 50_001 - MAX_ITEMS_PER_FILE as u32);
        assert!(items.iter().any(|i| i.waits_on_you));
        let mut files: Vec<TodoFile> = (0..4)
            .map(|_| TodoFile {
                items: items.clone(),
                ..TodoFile::default()
            })
            .collect();
        cap_total(&mut files, MAX_TODO_ITEMS);
        let total: usize = files.iter().map(|f| f.items.len()).sum();
        assert_eq!(total, MAX_TODO_ITEMS);
        assert!(files.iter().all(|f| f.items.iter().any(|i| i.waits_on_you)));
        assert_eq!(files[3].omitted, 200 - files[3].items.len() as u32);
    }

    #[test]
    fn discovery_and_cache() {
        let base = std::env::temp_dir().join(format!("argus-todos-test-{}", std::process::id()));
        let proj = base.join("proj");
        let deep = proj.join("sub").join("deep");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::create_dir_all(proj.join("tasks")).unwrap();
        let none = base.join("empty");
        std::fs::create_dir_all(&none).unwrap();
        assert!(find_todo_files([none.to_str().unwrap()].into_iter()).is_empty());
        let todo = proj.join("tasks").join("todo.md");
        std::fs::write(&todo, "- [ ] one\n").unwrap();
        let found =
            find_todo_files([deep.to_str().unwrap(), proj.to_str().unwrap(), ""].into_iter());
        assert_eq!(found, vec![todo.clone()]);

        let now = crate::model::now_millis();
        let mut r = TodoReader::default();
        let files = r.read(&found, now);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].project, "proj");
        assert_eq!(files[0].root, proj.display().to_string());
        assert_eq!(titles(&files[0].items), ["one"]);
        r.read(&found, now);
        assert_eq!(r.parses, 1, "unchanged file is not parsed again");
        std::fs::write(&todo, "- [ ] one\n- [ ] two\n").unwrap();
        let files = r.read(&found, now);
        assert_eq!(r.parses, 2);
        assert_eq!(files[0].items.len(), 2);
        // Files older than 30 days are skipped.
        assert!(r.read(&found, now + TODO_MAX_AGE + 60_000).is_empty());
        // A big file is read up to the byte cap.
        let mut big = String::from("- [ ] first\n");
        while big.len() < 2 * MAX_FILE_BYTES {
            big.push_str("filler line that is not a box at all\n");
        }
        big.push_str("- [ ] beyond the cap\n");
        std::fs::write(&todo, &big).unwrap();
        let files = r.read(&found, now);
        assert_eq!(titles(&files[0].items), ["first"]);
        assert_eq!(files[0].omitted, 1);
        let _ = std::fs::remove_dir_all(&base);
    }
}
