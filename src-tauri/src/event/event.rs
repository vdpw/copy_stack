use copy_event_listener::event::{
    Data as ListenerData, Event as ListenerEvent, Item as ListenerItem,
};
use serde::{Deserialize, Serialize};

const EVENT_BLOB_MAGIC: &[u8; 4] = b"CSB1";
const FILTERED_DATA_TYPE_PREFIXES: &[&str] = &["dyn.", "org.chromium.", "com.apple."];

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

pub fn filter_event_for_storage(event: &ListenerEvent) -> Option<ListenerEvent> {
    let items = event
        .items
        .iter()
        .filter_map(|item| {
            let data_list = item
                .data_list
                .iter()
                .filter(|data| should_store_data_type(&data.r#type))
                .cloned()
                .collect::<Vec<_>>();

            if data_list.is_empty() {
                None
            } else {
                Some(ListenerItem { data_list })
            }
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        None
    } else {
        Some(ListenerEvent { items })
    }
}

fn should_store_data_type(data_type: &str) -> bool {
    !FILTERED_DATA_TYPE_PREFIXES
        .iter()
        .any(|prefix| data_type.starts_with(prefix))
}

pub fn encode_event_blob(event: &ListenerEvent) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    output.extend_from_slice(EVENT_BLOB_MAGIC);
    write_u32(&mut output, checked_len(event.items.len(), "items")?);

    for item in &event.items {
        write_u32(
            &mut output,
            checked_len(item.data_list.len(), "item data_list")?,
        );

        for data in &item.data_list {
            let data_type = data.r#type.as_bytes();
            write_u32(&mut output, checked_len(data_type.len(), "data type")?);
            output.extend_from_slice(data_type);
            write_u64(&mut output, checked_len_u64(data.data.len(), "data")?);
            output.extend_from_slice(&data.data);
        }
    }

    Ok(output)
}

pub fn decode_event_blob(blob: &[u8]) -> Result<ListenerEvent, String> {
    let mut reader = BlobReader::new(blob);
    reader.expect_magic(EVENT_BLOB_MAGIC)?;

    let item_count = reader.read_u32()? as usize;
    let mut items = Vec::with_capacity(item_count);

    for _ in 0..item_count {
        let data_count = reader.read_u32()? as usize;
        let mut data_list = Vec::with_capacity(data_count);

        for _ in 0..data_count {
            let data_type_length = reader.read_u32()? as usize;
            let data_type = String::from_utf8(reader.read_bytes(data_type_length)?.to_vec())
                .map_err(|error| format!("invalid data type bytes: {}", error))?;
            let data_length = usize::try_from(reader.read_u64()?)
                .map_err(|_| "data length exceeds usize".to_string())?;
            let data = reader.read_bytes(data_length)?.to_vec();
            data_list.push(ListenerData {
                r#type: data_type,
                data,
            });
        }

        items.push(ListenerItem { data_list });
    }

    reader.expect_finished()?;
    Ok(ListenerEvent { items })
}

pub fn event_from_legacy_json(event_data: &str) -> serde_json::Result<ListenerEvent> {
    serde_json::from_str::<ClipboardEvent>(event_data).map(ListenerEvent::from)
}

fn checked_len(len: usize, label: &str) -> Result<u32, String> {
    u32::try_from(len).map_err(|_| format!("{} length exceeds u32", label))
}

fn checked_len_u64(len: usize, label: &str) -> Result<u64, String> {
    u64::try_from(len).map_err(|_| format!("{} length exceeds u64", label))
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

struct BlobReader<'a> {
    blob: &'a [u8],
    offset: usize,
}

impl<'a> BlobReader<'a> {
    fn new(blob: &'a [u8]) -> Self {
        Self { blob, offset: 0 }
    }

    fn expect_magic(&mut self, expected: &[u8]) -> Result<(), String> {
        let actual = self.read_bytes(expected.len())?;
        if actual == expected {
            Ok(())
        } else {
            Err("invalid event blob header".to_string())
        }
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u64(&mut self) -> Result<u64, String> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_bytes(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| "event blob offset overflow".to_string())?;
        if end > self.blob.len() {
            return Err("event blob ended unexpectedly".to_string());
        }

        let bytes = &self.blob[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn expect_finished(&self) -> Result<(), String> {
        if self.offset == self.blob.len() {
            Ok(())
        } else {
            Err("event blob has trailing bytes".to_string())
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn listener_data(data_type: &str) -> ListenerData {
        ListenerData {
            r#type: data_type.to_string(),
            data: data_type.as_bytes().to_vec(),
        }
    }

    #[test]
    fn filter_event_for_storage_removes_platform_specific_data_types() {
        let event = ListenerEvent {
            items: vec![ListenerItem {
                data_list: vec![
                    listener_data("dyn.agk8"),
                    listener_data("org.chromium.source-url"),
                    listener_data("com.apple.webarchive"),
                    listener_data("public.utf8-plain-text"),
                ],
            }],
        };

        let filtered = filter_event_for_storage(&event).expect("event should keep public data");

        assert_eq!(filtered.items.len(), 1);
        assert_eq!(filtered.items[0].data_list.len(), 1);
        assert_eq!(
            filtered.items[0].data_list[0].r#type,
            "public.utf8-plain-text"
        );
    }

    #[test]
    fn filter_event_for_storage_drops_empty_items_and_events() {
        let event = ListenerEvent {
            items: vec![
                ListenerItem {
                    data_list: vec![listener_data("dyn.agk8")],
                },
                ListenerItem {
                    data_list: vec![listener_data("org.chromium.source-url")],
                },
            ],
        };

        assert!(filter_event_for_storage(&event).is_none());
    }

    #[test]
    fn event_blob_round_trips_listener_events() {
        let event = ListenerEvent {
            items: vec![ListenerItem {
                data_list: vec![
                    ListenerData {
                        r#type: "public.utf8-plain-text".to_string(),
                        data: b"hello".to_vec(),
                    },
                    ListenerData {
                        r#type: "public.png".to_string(),
                        data: vec![0, 1, 2, 255],
                    },
                ],
            }],
        };

        let blob = encode_event_blob(&event).expect("event should encode");
        let decoded = decode_event_blob(&blob).expect("event should decode");

        assert_eq!(decoded.items.len(), 1);
        assert_eq!(decoded.items[0].data_list.len(), 2);
        assert_eq!(
            decoded.items[0].data_list[0].r#type,
            "public.utf8-plain-text"
        );
        assert_eq!(decoded.items[0].data_list[0].data, b"hello");
        assert_eq!(decoded.items[0].data_list[1].r#type, "public.png");
        assert_eq!(decoded.items[0].data_list[1].data, vec![0, 1, 2, 255]);
    }

    #[test]
    fn event_blob_rejects_truncated_data() {
        let event = ListenerEvent {
            items: vec![ListenerItem {
                data_list: vec![listener_data("public.utf8-plain-text")],
            }],
        };
        let mut blob = encode_event_blob(&event).expect("event should encode");
        blob.pop();

        assert!(decode_event_blob(&blob).is_err());
    }

    #[test]
    fn legacy_json_can_be_converted_to_event_blob() {
        let legacy_json =
            r#"{"items":[{"data_list":[{"type":"public.utf8-plain-text","data":[104,105]}]}]}"#;

        let event = event_from_legacy_json(legacy_json).expect("legacy JSON should decode");
        let blob = encode_event_blob(&event).expect("event should encode");
        let decoded = decode_event_blob(&blob).expect("event should decode");

        assert_eq!(decoded.items.len(), 1);
        assert_eq!(
            decoded.items[0].data_list[0].r#type,
            "public.utf8-plain-text"
        );
        assert_eq!(decoded.items[0].data_list[0].data, b"hi");
    }
}
