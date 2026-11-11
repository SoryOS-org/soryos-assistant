use std::borrow::Cow;
use std::collections::HashMap;

use serde_json::{json, Value};
use tracing::debug;
use uuid::Uuid;

pub(crate) const INTERNAL_CHUNK_SERVER_ID: &str = "sprite_internal";
pub(crate) const INTERNAL_CHUNK_FETCH_TOOL: &str = "fetch_chunk";
pub(crate) const INTERNAL_TEXT_FIND_TOOL: &str = "find_text";
const CHUNK_THRESHOLD_BYTES: usize = 120 * 1024;
const CHUNK_PAGE_BYTES: usize = 60 * 1024;
const CHUNK_PREVIEW_BYTES: usize = 512;

#[derive(Default)]
pub(crate) struct ChunkStore {
    entries: HashMap<String, ChunkEntry>,
}

struct ChunkEntry {
    data: String,
    page_offsets: Vec<usize>,
    field_path: String,
}

impl ChunkStore {
    pub(crate) fn chunk_large_fields(
        &mut self,
        session_token: &str,
        step_index: u32,
        output: &Value,
    ) -> Option<Value> {
        self.chunk_nested_fields(session_token, step_index, "", output)
    }

    pub(crate) fn fetch_chunk(&self, arguments: Value) -> Result<Value, String> {
        let chunk_id = arguments
            .get("chunk_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "chunk_id is required".to_string())?;
        let page = arguments
            .get("page")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u32;

        let entry = match self.entries.get(chunk_id) {
            Some(value) => value,
            None => {
                debug!(chunk_id = %chunk_id, "Chunk lookup missed");
                return Err("chunk not found".to_string());
            }
        };

        let total_pages = entry.page_offsets.len().saturating_sub(1);
        if page as usize >= total_pages {
            return Err("page out of range".to_string());
        }
        let start = entry.page_offsets[page as usize];
        let end = entry.page_offsets[page as usize + 1];
        let slice = entry.data[start..end].to_string();

        debug!(
            chunk_id = %chunk_id,
            page = page,
            total_pages = total_pages,
            "Served chunk page"
        );

        Ok(json!({
            "chunk_id": chunk_id,
            "field_path": entry.field_path,
            "page": page,
            "content": slice,
            "has_more": (page as usize + 1) < total_pages,
            "page_size": CHUNK_PAGE_BYTES,
            "total_pages": total_pages,
        }))
    }

    pub(crate) fn find_text(&self, arguments: Value) -> Result<Value, String> {
        let chunk_id = arguments
            .get("chunk_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "chunk_id is required".to_string())?;
        let query = arguments
            .get("query")
            .and_then(|value| value.as_str())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "query must be a non-empty string".to_string())?;
        let case_sensitive = arguments
            .get("case_sensitive")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let max_matches = arguments
            .get("max_matches")
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .unwrap_or(5)
            .clamp(1, 25);

        let entry = self
            .entries
            .get(chunk_id)
            .ok_or_else(|| "chunk not found".to_string())?;

        let (matches, total_matches) =
            locate_chunk_matches(&entry.data, query, case_sensitive, max_matches);

        Ok(json!({
            "chunk_id": chunk_id,
            "field_path": entry.field_path,
            "query": query,
            "case_sensitive": case_sensitive,
            "matches": matches,
            "total_matches": total_matches,
        }))
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    fn chunk_value_at(
        &mut self,
        session_token: &str,
        step_index: u32,
        field_path: &str,
        value: &Value,
    ) -> Option<Value> {
        let effective_path = if field_path.is_empty() {
            "value"
        } else {
            field_path
        };
        let serialized = serde_json::to_string(value).ok()?;
        if serialized.len() <= CHUNK_THRESHOLD_BYTES {
            return None;
        }

        let total_bytes = serialized.len();
        let preview = truncate_preview(&serialized);
        let (chunk_id, total_pages) =
            self.store_chunk(session_token, step_index, effective_path, serialized);

        debug!(
            chunk_id = %chunk_id,
            field_path = %effective_path,
            total_pages = total_pages,
            total_bytes = total_bytes,
            "Chunked planner field"
        );

        Some(json!({
            "chunked": true,
            "chunk_id": chunk_id,
            "field_path": effective_path,
            "preview": preview,
            "total_bytes": total_bytes,
            "page_size": CHUNK_PAGE_BYTES,
            "total_pages": total_pages,
            "next_page": 0,
            "hint": format!("Use {INTERNAL_CHUNK_SERVER_ID}.{INTERNAL_CHUNK_FETCH_TOOL} to fetch additional pages for {effective_path}."),
        }))
    }

    fn chunk_nested_fields(
        &mut self,
        session_token: &str,
        step_index: u32,
        field_path: &str,
        value: &Value,
    ) -> Option<Value> {
        match value {
            Value::Object(map) => {
                let mut mutated = false;
                let mut next_map = serde_json::Map::with_capacity(map.len());
                for (key, field_value) in map {
                    let child_path = join_field_path(field_path, key);
                    if let Some(replacement) = self.chunk_nested_fields(
                        session_token,
                        step_index,
                        &child_path,
                        field_value,
                    ) {
                        next_map.insert(key.clone(), replacement);
                        mutated = true;
                    } else {
                        next_map.insert(key.clone(), field_value.clone());
                    }
                }
                if mutated {
                    Some(Value::Object(next_map))
                } else {
                    None
                }
            }
            Value::Array(items) => {
                let mut mutated = false;
                let mut next_items = Vec::with_capacity(items.len());
                for (idx, item) in items.iter().enumerate() {
                    let segment = format!("[{idx}]");
                    let child_path = join_field_path(field_path, &segment);
                    if let Some(replacement) =
                        self.chunk_nested_fields(session_token, step_index, &child_path, item)
                    {
                        next_items.push(replacement);
                        mutated = true;
                    } else {
                        next_items.push(item.clone());
                    }
                }
                if mutated {
                    Some(Value::Array(next_items))
                } else {
                    None
                }
            }
            _ => self.chunk_value_at(session_token, step_index, field_path, value),
        }
    }

    fn store_chunk(
        &mut self,
        session_token: &str,
        step_index: u32,
        field_path: &str,
        serialized: String,
    ) -> (String, u32) {
        let chunk_id = format!("chunk-{}-{}-{}", session_token, step_index, Uuid::new_v4());
        let page_offsets = compute_page_offsets(&serialized);
        let total_pages = page_offsets.len().saturating_sub(1).max(1) as u32;
        let entry = ChunkEntry {
            data: serialized,
            page_offsets,
            field_path: field_path.to_string(),
        };
        self.entries.insert(chunk_id.clone(), entry);
        debug!(
            chunk_id = %chunk_id,
            field_path = %field_path,
            total_pages = total_pages,
            "Stored chunk in cache"
        );
        (chunk_id, total_pages)
    }
}

pub(crate) fn truncate_mcp_output(value: &mut Value) {
    if let Some(output) = value.get_mut("output") {
        truncate_strings(output);
    }
}

fn truncate_strings(node: &mut Value) {
    const LIMIT: usize = 400;
    match node {
        Value::String(s) => {
            if s.len() > LIMIT {
                let mut end = LIMIT;
                while end > 0 && !s.is_char_boundary(end) {
                    end -= 1;
                }
                let mut truncated = s[..end].to_string();
                truncated.push('.');
                *s = truncated;
            }
        }
        Value::Array(items) => {
            for item in items {
                truncate_strings(item);
            }
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                truncate_strings(value);
            }
        }
        _ => {}
    }
}

fn join_field_path(current: &str, segment: &str) -> String {
    if current.is_empty() {
        segment.to_string()
    } else if segment.starts_with('[') {
        format!("{current}{segment}")
    } else {
        format!("{current}.{segment}")
    }
}

fn locate_chunk_matches(
    haystack: &str,
    needle: &str,
    case_sensitive: bool,
    max_matches: usize,
) -> (Vec<Value>, usize) {
    if needle.is_empty() {
        return (Vec::new(), 0);
    }

    let hay_bytes: Cow<[u8]> = if case_sensitive {
        Cow::Borrowed(haystack.as_bytes())
    } else {
        Cow::Owned(ascii_lowercase_bytes(haystack))
    };
    let needle_bytes: Cow<[u8]> = if case_sensitive {
        Cow::Borrowed(needle.as_bytes())
    } else {
        Cow::Owned(ascii_lowercase_bytes(needle))
    };

    let hay = hay_bytes.as_ref();
    let ned = needle_bytes.as_ref();
    let mut matches: Vec<Value> = Vec::new();
    let mut total_matches = 0usize;
    let mut index = 0usize;

    while index + ned.len() <= hay.len() {
        if &hay[index..index + ned.len()] == ned {
            total_matches += 1;
            if matches.len() < max_matches {
                let preview = build_match_preview(haystack, index, needle.len());
                matches.push(json!({
                    "offset": index,
                    "preview": preview,
                }));
            }
            index += 1;
        } else {
            index += 1;
        }
    }

    (matches, total_matches)
}

fn ascii_lowercase_bytes(text: &str) -> Vec<u8> {
    text.bytes().map(|byte| byte.to_ascii_lowercase()).collect()
}

fn build_match_preview(text: &str, start: usize, match_len: usize) -> String {
    const CONTEXT_BYTES: usize = 120;
    let snippet_start = clamp_boundary_backward(text, start.saturating_sub(CONTEXT_BYTES));
    let snippet_end =
        clamp_boundary_forward(text, (start + match_len + CONTEXT_BYTES).min(text.len()));

    let mut snippet = text[snippet_start..snippet_end].to_string();
    if snippet_start > 0 {
        snippet = format!(".{snippet}");
    }
    if snippet_end < text.len() {
        snippet.push('.');
    }
    snippet
}

fn clamp_boundary_backward(text: &str, mut index: usize) -> usize {
    if index > text.len() {
        index = text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn clamp_boundary_forward(text: &str, mut index: usize) -> usize {
    if index > text.len() {
        index = text.len();
    }
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn truncate_preview(full: &str) -> String {
    if full.len() <= CHUNK_PREVIEW_BYTES {
        full.to_string()
    } else {
        let mut end = CHUNK_PREVIEW_BYTES.min(full.len());
        while end < full.len() && !full.is_char_boundary(end) {
            end += 1;
        }
        format!("{}.", &full[..end])
    }
}

fn compute_page_offsets(data: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    let mut last = 0;
    for (idx, _) in data.char_indices() {
        if idx == 0 {
            continue;
        }
        if idx - last >= CHUNK_PAGE_BYTES {
            offsets.push(idx);
            last = idx;
        }
    }
    if *offsets.last().unwrap_or(&0) != data.len() {
        offsets.push(data.len());
    }
    offsets
}
