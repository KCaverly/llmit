use super::message::Message;
use anyhow::anyhow;

#[derive(Default)]
pub struct Conversation {
    pub messages: Vec<Message>,
    pub selected_message: Option<usize>,
}

impl Conversation {
    pub fn new(messages: Vec<Message>) -> Self {
        Conversation {
            messages,
            selected_message: None,
        }
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.select_last_message();
    }

    pub fn delete_selected_message(&mut self) {
        if let Some(selected_id) = self.selected_message {
            self.messages.remove(selected_id);
            self.select_prev_message();
        }
    }

    pub fn select_last_message(&mut self) {
        self.selected_message = Some(self.messages.len() - 1);
    }

    pub fn replace_last_message(&mut self, message: Message) {
        self.messages.pop();
        self.messages.push(message);
        self.select_last_message();
    }

    pub fn unfocus(&mut self) {
        // We are no longer changing which note is selected when we focus
    }

    pub fn focus(&mut self) {
        // No longer change which note is selected when we focus
    }

    pub fn get_selected_message(&self) -> anyhow::Result<Message> {
        if let Some(currently_selected) = self.selected_message {
            if let Some(message) = self.messages.get(currently_selected) {
                return anyhow::Ok(message.clone());
            }
        }
        return Err(anyhow!("Could not retrieve message"));
    }

    pub fn select_next_message(&mut self) {
        if let Some(currently_selected) = self.selected_message {
            let next_selected = currently_selected + 1;
            if next_selected < self.messages.len() {
                self.selected_message = Some(next_selected);
            }
        } else {
            self.selected_message = Some(0);
        }
    }

    pub fn select_prev_message(&mut self) {
        if let Some(currently_selected) = self.selected_message {
            if currently_selected > 0 {
                self.selected_message = Some(currently_selected - 1);
            }
        } else {
            self.selected_message = Some(0);
        }
    }
}
