use crate::app::App;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use thunderus_core::{MemoryPaths, ViewKind, ViewMaterializer};

impl App {
    /// Handle /memory command
    pub fn handle_memory_command(&mut self) {
        match self.session {
            Some(ref session) => match ViewMaterializer::new(session).materialize(ViewKind::Memory) {
                Ok(content) => self
                    .transcript_mut()
                    .add_system_message(format!("## Project Memory\n\n{}", content)),
                Err(e) => self
                    .transcript_mut()
                    .add_system_message(format!("Failed to materialize memory: {}", e)),
            },
            None => self
                .transcript_mut()
                .add_system_message("No active session to materialize memory from"),
        }
    }

    /// Handle /memory add <fact> command
    pub fn handle_memory_add_command(&mut self, fact: String) {
        if let Some(ref mut session) = self.session {
            let mut hasher = DefaultHasher::new();
            fact.hash(&mut hasher);
            let content_hash = format!("{:x}", hasher.finish());

            match session.append_memory_update("core", "MEMORY.md", "update", &content_hash) {
                Ok(_) => {
                    self.transcript_mut()
                        .add_system_message(format!("Added to memory: {}", fact));
                    self.materialize_views();
                }
                Err(e) => self
                    .transcript_mut()
                    .add_system_message(format!("Failed to add memory: {}", e)),
            }
        } else {
            self.transcript_mut()
                .add_system_message("No active session to add memory to");
        }
    }

    /// Handle /memory search <query> command
    ///
    /// Searches the memory store and displays results in the memory hits panel.
    pub fn handle_memory_search_command(&mut self, query: String) {
        let memory_paths = MemoryPaths::from_thunderus_root(&self.state.config.cwd);
        let db_path = memory_paths.indexes.join("memory.db");

        let store = match tokio::runtime::Handle::try_current() {
            Ok(handle) => match handle.block_on(thunderus_store::MemoryStore::open(&db_path)) {
                Ok(store) => store,
                Err(e) => {
                    return self
                        .transcript_mut()
                        .add_system_message(format!("Failed to open memory store: {}", e));
                }
            },
            Err(_) => {
                return self
                    .transcript_mut()
                    .add_system_message("No tokio runtime available for memory search");
            }
        };

        let hits = match tokio::runtime::Handle::try_current() {
            Ok(handle) => match handle.block_on(store.search(&query, thunderus_store::SearchFilters::default())) {
                Ok(hits) => hits,
                Err(e) => {
                    return self
                        .transcript_mut()
                        .add_system_message(format!("Memory search failed: {}", e));
                }
            },
            Err(_) => {
                return self
                    .transcript_mut()
                    .add_system_message("No tokio runtime available for memory search");
            }
        };

        if hits.is_empty() {
            self.transcript_mut()
                .add_system_message(format!("No memory results found for '{}'", query));
            self.state_mut().memory_hits.clear();
        } else {
            self.transcript_mut()
                .add_system_message(format!("Found {} memory result(s) for '{}'", hits.len(), query));

            let start = std::time::Instant::now();
            let search_time = start.elapsed().as_millis() as u64;

            self.state_mut().memory_hits.set_hits(hits, query, search_time);
        }
    }

    /// Handle /memory pin <id> command
    ///
    /// Pins a memory document to the current context set.
    pub fn handle_memory_pin_command(&mut self, id: String) {
        if self.state().memory_hits.is_pinned(&id) {
            self.state_mut().memory_hits.unpin(&id);
            self.transcript_mut()
                .add_system_message(format!("Unpinned memory: {}", id));
        } else {
            self.state_mut().memory_hits.pin(id.clone());
            self.transcript_mut()
                .add_system_message(format!("Pinned memory: {}", id));
        }

        let pinned_count = self.state().memory_hits.pinned_count();
        if pinned_count > 0 {
            self.transcript_mut()
                .add_system_message(format!("Total pinned: {}", pinned_count));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::create_test_app;
    use crate::transcript;

    #[test]
    fn test_handle_memory_command() {
        let mut app = create_test_app();

        app.handle_memory_command();

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("No active session") || content.contains("Project Memory"));
        } else {
            panic!("Expected SystemMessage");
        }
    }
}
