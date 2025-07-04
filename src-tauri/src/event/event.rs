use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub struct CopyEvent {
    pub id: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub content_type: String, // text, image, file, etc.
}

impl CopyEvent {
    pub fn new(content: String, content_type: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            timestamp: Utc::now(),
            content_type,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Data {
    id: String,
    r#type: String,
    content: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    r#type: String, // push, remove
    data: Data,
}
