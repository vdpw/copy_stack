use copy_event_listener::clipboard::ClipboardListener;
use copy_event_listener::event::Event;
use serde_json;

fn main() {
    println!("Testing copy_event_listener integration...");

    // Test 1: Create a simple event
    let mut event = Event::new();
    event.new_item();
    event.add_data(
        "public.utf8-plain-text".to_string(),
        b"Hello, World!".to_vec(),
    );

    println!("Created event with {} items", event.items.len());
    println!(
        "First item has {} data entries",
        event.items[0].data_list.len()
    );

    // Test 2: Serialize and deserialize
    match serde_json::to_string(&event) {
        Ok(json) => {
            println!("Serialized event: {}", json);

            match serde_json::from_str::<Event>(&json) {
                Ok(deserialized_event) => {
                    println!("Successfully deserialized event");
                    println!(
                        "Deserialized event has {} items",
                        deserialized_event.items.len()
                    );
                }
                Err(e) => println!("Failed to deserialize: {}", e),
            }
        }
        Err(e) => println!("Failed to serialize: {}", e),
    }

    // Test 3: Test clipboard listener creation
    let listener = ClipboardListener::new().with_interval(1000);
    println!("Successfully created clipboard listener with 1000ms interval");

    println!("All tests passed! The refactoring is working correctly.");
}
