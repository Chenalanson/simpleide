//! Open document model and the egui `TextBuffer` adapter.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use eframe::egui::TextBuffer;

use crate::highlight::{self, Highlighter, SharedLayoutCache};
use crate::util::{
    file_name, read_text_file, relative_display, write_text_file, LineEnding, TextEncoding,
};

pub struct EditorBuffer {
    text: String,
    revision: Rc<Cell<u64>>,
}

impl EditorBuffer {
    pub fn new(text: String) -> Self {
        Self {
            text,
            revision: Rc::new(Cell::new(0)),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision.get()
    }

    pub fn revision_handle(&self) -> Rc<Cell<u64>> {
        Rc::clone(&self.revision)
    }

    pub fn as_string(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: String) {
        if self.text != text {
            self.text = text;
            self.bump();
        }
    }

    pub fn mark_dirty(&mut self) {
        self.bump();
    }

    fn bump(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
    }

    fn byte_index(&self, char_index: usize) -> usize {
        if char_index == 0 {
            return 0;
        }
        self.text
            .char_indices()
            .nth(char_index)
            .map(|(index, _)| index)
            .unwrap_or(self.text.len())
    }
}

impl TextBuffer for EditorBuffer {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        &self.text
    }

    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        if text.is_empty() {
            return 0;
        }
        let byte_index = self.byte_index(char_index);
        self.text.insert_str(byte_index, text);
        self.bump();
        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: std::ops::Range<usize>) {
        if char_range.start >= char_range.end {
            return;
        }
        let start = self.byte_index(char_range.start);
        let end = self.byte_index(char_range.end);
        if start < end && end <= self.text.len() {
            self.text.replace_range(start..end, "");
            self.bump();
        }
    }

    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }
}

pub struct Document {
    pub path: PathBuf,
    pub name: String,
    pub rel_path: String,
    pub buffer: EditorBuffer,
    pub saved_revision: u64,
    pub line_ending: LineEnding,
    pub bom: bool,
    pub encoding: TextEncoding,
    pub syntax_name: Option<String>,
    pub language: String,
    pub cursor_index: usize,
    pub cursor_line: usize,
    pub cursor_column: usize,
    pub read_only: bool,
    pub bookmarks: BTreeSet<usize>,
    pub layout_cache: SharedLayoutCache,
    pub minimap_cache: Option<crate::minimap::Cache>,
    pub macro_issues: Vec<crate::macro_check::MacroIssue>,
    pub macro_issue_revision: u64,
}

impl Document {
    pub fn open(path: &Path, root: &Path, highlighter: &Highlighter) -> Result<Self, String> {
        let loaded = read_text_file(path)?;
        let (syntax_name, language) = highlighter.detect(path);
        let buffer = EditorBuffer::new(loaded.text);
        Ok(Self {
            path: path.to_path_buf(),
            name: file_name(path),
            rel_path: relative_display(root, path),
            buffer,
            saved_revision: 0,
            line_ending: loaded.line_ending,
            bom: loaded.bom,
            encoding: loaded.encoding,
            syntax_name,
            language,
            cursor_index: 0,
            cursor_line: 1,
            cursor_column: 1,
            read_only: false,
            bookmarks: BTreeSet::new(),
            layout_cache: highlight::new_cache(),
            minimap_cache: None,
            macro_issues: Vec::new(),
            macro_issue_revision: u64::MAX,
        })
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer.revision() != self.saved_revision
    }

    pub fn save(&mut self) -> Result<(), String> {
        let revision_before_save = self.buffer.revision();
        write_text_file(
            &self.path,
            self.buffer.as_string(),
            self.line_ending,
            self.bom,
            self.encoding,
        )?;
        self.saved_revision = revision_before_save;
        Ok(())
    }

    pub fn update_cursor(&mut self, char_index: usize) {
        self.cursor_index = char_index;
        let (line, column) = crate::util::line_column_at_char(self.buffer.as_string(), char_index);
        self.cursor_line = line;
        self.cursor_column = column;
    }

    pub fn newline_label(&self) -> &'static str {
        self.line_ending.label()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer() -> EditorBuffer {
        EditorBuffer::new("abc\ndef".to_owned())
    }

    #[test]
    fn buffer_inserts_at_character_indices() {
        let mut buffer = buffer();
        buffer.insert_text("中", 3);
        assert_eq!(buffer.as_str(), "abc中\ndef");
        assert_eq!(buffer.revision(), 1);
    }

    #[test]
    fn buffer_deletes_character_ranges() {
        let mut buffer = buffer();
        buffer.delete_char_range(1..4);
        assert_eq!(buffer.as_str(), "adef");
        assert_eq!(buffer.revision(), 1);
    }
}
