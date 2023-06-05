use zellij_tile::prelude::{
    Key,
    hide_self,
    post_message_to,
    open_file,
    open_file_floating,
    open_file_with_line,
    open_file_with_line_floating,
    open_terminal,
    open_terminal_floating,
};
use std::path::PathBuf;
use crate::search::{SearchType, MessageToSearch, ResultsOfSearch};
use crate::search_results::SearchResult;

pub const CURRENT_SEARCH_TERM: &str = "/data/current_search_term";

#[derive(Default)]
pub struct SearchState {
    pub search_term: String,
    pub file_name_search_results: Vec<SearchResult>,
    pub file_contents_search_results: Vec<SearchResult>,
    pub loading: bool,
    pub loading_animation_offset: u8,
    pub selected_search_result: usize,
    pub should_open_floating: bool,
    pub search_filter: SearchType,
    pub display_rows: usize,
    pub display_columns: usize,
    pub displayed_search_results: (usize, Vec<SearchResult>), // usize is selected index
}

impl SearchState {
    pub fn handle_key(&mut self, key: Key) {
        match key {
            Key::Down  => self.move_search_selection_down(),
            Key::Up  => self.move_search_selection_up(),
            Key::Char('\n')  => self.open_search_result_in_editor(),
            Key::BackTab  => self.open_search_result_in_terminal(),
            Key::Ctrl('f')  => {
                self.should_open_floating = !self.should_open_floating;
            },
            Key::Ctrl('r')  => self.toggle_search_filter(),
            Key::Esc  => {
                hide_self();
                self.clear_state();
            }
            _  => self.append_to_search_term(key),
        }
    }
    pub fn update_file_name_search_results(&mut self, mut results_of_search: ResultsOfSearch) {
        if self.search_term == results_of_search.search_term {
            self.file_name_search_results = results_of_search.search_results.drain(..).collect();
            self.update_displayed_search_results();
        }
    }
    pub fn update_file_contents_search_results(&mut self, mut results_of_search: ResultsOfSearch) {
        if self.search_term == results_of_search.search_term {
            self.file_contents_search_results = results_of_search.search_results.drain(..).collect();
            self.update_displayed_search_results();
        }
    }
    pub fn change_size(&mut self, rows: usize, cols: usize) {
        self.display_rows = rows;
        self.display_columns = cols;
    }
    pub fn progress_animation(&mut self) {
        if self.loading_animation_offset == u8::MAX {
            self.loading_animation_offset = 0;
        } else {
            self.loading_animation_offset =
                self.loading_animation_offset.saturating_add(1);
        }
    }
    fn move_search_selection_down(&mut self) {
        if self.displayed_search_results.0 < self.max_search_selection_index() {
            self.displayed_search_results.0 += 1;
        }
    }
    fn move_search_selection_up(&mut self) {
        self.displayed_search_results.0 = self.displayed_search_results.0.saturating_sub(1);
    }
    fn open_search_result_in_editor(&mut self) {
        match self.selected_search_result_entry() {
            Some(SearchResult::File { path, .. }) => {
                if self.should_open_floating {
                    open_file_floating(&PathBuf::from(path))
                } else {
                    open_file(&PathBuf::from(path));
                }
            }
            Some(SearchResult::LineInFile { path, line_number, .. }) => {
                if self.should_open_floating {
                    open_file_with_line_floating(&PathBuf::from(path), line_number);
                } else {
                    open_file_with_line(&PathBuf::from(path), line_number);
                }
            },
            None => eprintln!("Search results not found"),
        }
    }
    fn open_search_result_in_terminal(&mut self) {
        let dir_path_of_result = |path: &str| -> PathBuf {
            let file_path = PathBuf::from(path);
            let mut dir_path = file_path.components();
            drop(dir_path.next_back()); // remove file name to stay with just the folder
            dir_path.as_path().into()
        };
        let selected_search_result_entry = self.selected_search_result_entry();
        if let Some(SearchResult::File { path, .. }) | Some(SearchResult::LineInFile { path, .. }) = selected_search_result_entry {
            let dir_path = dir_path_of_result(&path);
            if self.should_open_floating {
                open_terminal_floating(&dir_path);
            } else {
                open_terminal(&dir_path);
            }
        }
    }
    fn toggle_search_filter(&mut self) {
        self.search_filter.progress()
    }
    fn clear_state(&mut self) {
        self.file_name_search_results.clear();
        self.file_contents_search_results.clear();
        self.search_term.clear();
    }
    fn append_to_search_term(&mut self, key: Key) {
        match key {
            Key::Char(character) => {
                self.search_term.push(character);
            },
            Key::Backspace => {
                self.search_term.pop();
                if self.search_term.len() == 0 {
                    self.clear_state();
                }
            },
            _ => {},
        }
        match std::fs::write(CURRENT_SEARCH_TERM, &self.search_term) {
            Ok(_) => if !self.search_term.is_empty() {
                post_message_to("file_name_search", serde_json::to_string(&MessageToSearch::Search).unwrap(), "".to_owned());
                post_message_to("file_contents_search", serde_json::to_string(&MessageToSearch::Search).unwrap(), "".to_owned());
            },
            Err(e) => eprintln!("Failed to write search term to HD, aborting search: {}", e)
        }
    }
    fn max_search_selection_index(&self) -> usize {
        self.displayed_search_results.1.len().saturating_sub(1)
    }
    fn update_displayed_search_results(&mut self) {
        let mut search_results_of_interest = match self.search_filter {
            SearchType::NamesAndContents => {
                let mut all_search_results = self.file_name_search_results.clone();
                all_search_results.append(&mut self.file_contents_search_results.clone());
                all_search_results.sort_by(|a, b| {
                    b.score().cmp(&a.score())
                });
                all_search_results
            }
            SearchType::Names => {
                self.file_name_search_results.clone()
            }
            SearchType::Contents => {
                self.file_contents_search_results.clone()
            }
        };
        let mut height_taken_up_by_results = 0;
        let mut displayed_search_results = vec![];
        while let Some(search_result) = search_results_of_interest.pop() {
            if height_taken_up_by_results + search_result.rendered_height() > self.rows_for_results() {
                break;
            }
            height_taken_up_by_results += search_result.rendered_height();
            displayed_search_results.push(search_result);
        }
        let new_index = self.selected_search_result_entry().and_then(|currently_selected_search_result| {
            displayed_search_results.iter().position(|r| r.is_same_entry(&currently_selected_search_result))
        }).unwrap_or(0);
        self.displayed_search_results = (new_index, displayed_search_results);
    }
    fn selected_search_result_entry(&self) -> Option<SearchResult> {
        self.displayed_search_results.1.get(self.displayed_search_results.0).cloned()
    }
    fn rows_for_results(&self) -> usize {
        self.display_rows.saturating_sub(3) // search line and 2 controls lines
    }
}
