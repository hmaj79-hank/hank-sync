//! Protocol messages

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Put {
        path: String,
        size: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        hash: Option<String>,
    },
    List {
        path: String,
        #[serde(default)]
        recursive: bool,
        #[serde(default)]
        long: bool,
    },
    Get {
        path: String,
    },
    Status,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Done {
        written: u64,
    },
    List {
        entries: Vec<FileEntry>,
    },
    File {
        size: u64,
    },
    Status {
        root: String,
        total_size: u64,
        file_count: u64,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
}
