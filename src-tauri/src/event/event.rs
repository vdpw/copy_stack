use copy_event_listener::event::{
    Data as ListenerData, Event as ListenerEvent, Item as ListenerItem,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClipboardEvent {
    pub items: Vec<ClipboardItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub data_list: Vec<ClipboardData>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClipboardData {
    pub r#type: String,
    pub data: Vec<u8>,
}

impl From<&ListenerEvent> for ClipboardEvent {
    fn from(event: &ListenerEvent) -> Self {
        Self {
            items: event
                .items
                .iter()
                .map(|item| ClipboardItem {
                    data_list: item
                        .data_list
                        .iter()
                        .map(|data| ClipboardData {
                            r#type: data.r#type.clone(),
                            data: data.data.clone(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

impl From<ClipboardEvent> for ListenerEvent {
    fn from(event: ClipboardEvent) -> Self {
        Self {
            items: event
                .items
                .into_iter()
                .map(|item| ListenerItem {
                    data_list: item
                        .data_list
                        .into_iter()
                        .map(|data| ListenerData {
                            r#type: data.r#type,
                            data: data.data,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

pub fn serialize_event(event: &ListenerEvent) -> serde_json::Result<String> {
    serde_json::to_string(&ClipboardEvent::from(event))
}

pub fn deserialize_event(event_data: &str) -> serde_json::Result<ListenerEvent> {
    serde_json::from_str::<ClipboardEvent>(event_data).map(ListenerEvent::from)
}

pub fn deserialize_clipboard_event(event_data: &str) -> serde_json::Result<ClipboardEvent> {
    serde_json::from_str(event_data)
}
