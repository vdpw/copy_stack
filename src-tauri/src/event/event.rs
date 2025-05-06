#[derive(Clone, serde::Serialize)]
pub struct Data {
    id: String,
    r#type: String,
    content: String,
}

#[derive(Clone, serde::Serialize)]
pub struct Event {
    r#type: String, // push, remove
    data: Data,
}
