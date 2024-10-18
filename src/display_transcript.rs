use std::collections::HashMap;
use std::fmt;
use std::io::{stdout, Result};

use crossterm::cursor::MoveTo;
use crossterm::execute;
use crossterm::style::Print;
use crossterm::terminal::{Clear, ClearType};

use crate::handle_events::Event;

enum ConversationItemContentType {
    Text,
    Audio,
    InputText,
    InputAudio,
}

enum ConversationItemRole {
    User,
    Assistant,
    System,
}

impl fmt::Display for ConversationItemRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConversationItemRole::User => write!(f, "User"),
            ConversationItemRole::Assistant => write!(f, "Assistant"),
            ConversationItemRole::System => write!(f, "System"),
        }
    }
}

#[derive(PartialEq)]
enum ConversationItemStatus {
    Completed,
    InProgress,
    Failed,
    InComplete,
}

struct ConversationItemContent {
    content_type: ConversationItemContentType,
    text: Option<String>,       // Only used if type is Text or Input_Text
    audio: Option<String>,      // Only used if type is Audio or InputAudio (base64 encoded)
    transcript: Option<String>, // Only used if type is Audio or InputAudio
}

impl ConversationItemContent {
    fn new(
        content_type: String,
        text: Option<String>,
        audio: Option<String>,
        transcript: Option<String>,
    ) -> Self {
        let content_type = match content_type.as_str() {
            "text" => ConversationItemContentType::Text,
            "audio" => ConversationItemContentType::Audio,
            "input_text" => ConversationItemContentType::InputText,
            "input_audio" => ConversationItemContentType::InputAudio,
            // Error if the content type is not recognized
            _ => panic!("Unrecognized content type: {}", content_type),
        };

        Self {
            content_type,
            text,
            audio,
            transcript,
        }
    }
}

struct ConversationItem {
    item_id: String,
    role: ConversationItemRole,
    status: ConversationItemStatus,
    content: Vec<ConversationItemContent>,
}

impl ConversationItem {
    fn new(
        item_id: String,
        role: String,
        status: String,
        content: Vec<ConversationItemContent>,
    ) -> Self {
        let role = match role.as_str() {
            "user" => ConversationItemRole::User,
            "assistant" => ConversationItemRole::Assistant,
            "system" => ConversationItemRole::System,
            // Error if the role is not recognized
            _ => panic!("Unrecognized role: {}", role),
        };

        let status = match status.as_str() {
            "completed" => ConversationItemStatus::Completed,
            "in_progress" => ConversationItemStatus::InProgress,
            "incomplete" => ConversationItemStatus::InComplete,
            // Error if the status is not recognized
            _ => panic!("Unrecognized status: {}", status),
        };

        Self {
            item_id,
            role,
            status,
            content,
        }
    }

    fn get_content_transcript(&self) -> Option<String> {
        let mut transcript = String::new();
        for content in &self.content {
            if let Some(content_transcript) = &content.transcript {
                transcript.push_str(content_transcript);
            }
        }
        if transcript.is_empty() {
            None
        } else {
            Some(transcript)
        }
    }
}

struct ConversationTracker {
    item_order: Vec<String>,                     // Order of item_ids
    item_map: HashMap<String, ConversationItem>, // Map of item_id to ConversationItem
}

impl ConversationTracker {
    fn new() -> Self {
        Self {
            item_order: Vec::new(),
            item_map: HashMap::new(),
        }
    }

    fn add_item(&mut self, item: ConversationItem) {
        self.item_order.push(item.item_id.clone());
        self.item_map.insert(item.item_id.clone(), item);
    }

    fn get_item(&self, item_id: &str) -> Option<&ConversationItem> {
        self.item_map.get(item_id)
    }

    fn update_item_content_transcript(
        &mut self,
        item_id: &str,
        index: usize,
        transcript_delta: &str,
    ) {
        // Check if the index is within the bounds of the content array
        if let Some(item) = self.item_map.get_mut(item_id) {
            if index < item.content.len() {
                // Update the content at the specified index
                let current_transcript = item.content[index].transcript.clone().unwrap();
                let updated_transcript = format!("{}{}", current_transcript, transcript_delta);

                item.content[index].transcript = Some(updated_transcript);
            } else {
                // Create a new content item with the transcript delta
                item.content.push(ConversationItemContent::new(
                    "text".to_string(),
                    None,
                    None,
                    Some(transcript_delta.to_string()),
                ));
            }
        }
    }

    fn item_content_transcript_done(&mut self, item_id: &str, index: usize, transcript: &str) {
        // Check if the index is within the bounds of the content array
        if let Some(item) = self.item_map.get_mut(item_id) {
            if index < item.content.len() {
                // Update the content at the specified index
                item.content[index].transcript = Some(transcript.to_string());
            }
        }
    }

    fn update_item_status(&mut self, item_id: &str, status: ConversationItemStatus) {
        if let Some(item) = self.item_map.get_mut(item_id) {
            item.status = status;
        }
    }
}

// Define the transcript display mode as a closure with state
pub fn create_transcript_display() -> impl FnMut(&Event) -> Result<()> {
    let mut conversation_tracker = ConversationTracker::new();

    move |event: &Event| -> Result<()> {
        let event_type = &event.event_type;

        match event_type.as_str() {
            "conversation.item.created" => {
                // Add the new item to the conversation tracker
                let item_data = event.data.get("item").unwrap();
                // Get the content of the item, will be first value of the content array
                let content_data = item_data["content"].as_array().unwrap();

                let content = content_data
                    .iter()
                    .map(|content_item| {
                        let content_type = content_item["type"].as_str().unwrap().to_string();
                        let text = content_item["text"].as_str().map(|s| s.to_string());
                        let audio = content_item["audio"].as_str().map(|s| s.to_string());
                        let transcript = content_item["transcript"].as_str().map(|s| s.to_string());

                        ConversationItemContent::new(content_type, text, audio, transcript)
                    })
                    .collect();

                let item = ConversationItem::new(
                    item_data["id"].as_str().unwrap().to_string(),
                    item_data["role"].as_str().unwrap().to_string(),
                    item_data["status"].as_str().unwrap().to_string(),
                    content,
                );
                conversation_tracker.add_item(item);
            }
            "response.audio_transcript.delta" => {
                // Update the item in the conversation tracker
                let item_id = event.data["item_id"].as_str().unwrap();
                let transcript_delta = event.data["delta"].as_str().unwrap();
                let index = event.data["content_index"].as_u64().unwrap() as usize;

                conversation_tracker.update_item_content_transcript(
                    item_id,
                    index,
                    transcript_delta,
                );
            }
            "response.audio_transcript.done" => {
                // Update the item in the conversation tracker
                let item_id = event.data["item_id"].as_str().unwrap();
                let index = event.data["content_index"].as_u64().unwrap() as usize;

                conversation_tracker.update_item_status(item_id, ConversationItemStatus::Completed);
                conversation_tracker.item_content_transcript_done(
                    item_id,
                    index,
                    event.data["transcript"].as_str().unwrap(),
                );
            }
            "conversation.item.input_audio_transcription.completed" => {
                // Update the item in the conversation tracker
                let item_id = event.data["item_id"].as_str().unwrap();
                let index = event.data["content_index"].as_u64().unwrap() as usize;

                conversation_tracker.update_item_status(item_id, ConversationItemStatus::Completed);
                conversation_tracker.item_content_transcript_done(
                    item_id,
                    index,
                    event.data["transcript"].as_str().unwrap(),
                );
            }
            _ => return Ok(()),
        }

        // Display the conversation
        let mut stdout = stdout();

        // Iterate over the items in the conversation tracker, with enumeration
        for (index, item_id) in conversation_tracker.item_order.iter().enumerate() {
            let current_line = index;

            // Set the cursor position to the current line
            execute!(stdout, MoveTo(0, current_line as u16))?;
            // Clear the current line
            execute!(stdout, Clear(ClearType::CurrentLine))?;

            // Get the item from the conversation tracker
            if let Some(item) = conversation_tracker.get_item(item_id) {
                // Get the status_code of the item
                let status_code = match item.status {
                    ConversationItemStatus::Completed => "",
                    ConversationItemStatus::InProgress => "…",
                    ConversationItemStatus::Failed => "<failed>",
                    ConversationItemStatus::InComplete => "<incomplete>",
                };
                // Display the item
                let text = item
                    .get_content_transcript()
                    .unwrap_or_else(|| "".to_string());

                execute!(
                    stdout,
                    Print(format!("{} {}: {}{}", index, item.role, text, status_code))
                )?;
                // Print newline after each
                execute!(stdout, Print("\n"))?;
            }
        }

        Ok(())
    }
}
