use futures::StreamExt;
use std::fmt;
use std::str::from_utf8;
use std::time::Instant;
use textwrap::core::Word;
use textwrap::wrap_algorithms::{wrap_optimal_fit, Penalties};
use textwrap::WordSeparator;

use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*};
use replicate_rs::predictions::PredictionStatus;

use super::Component;
use crate::agent::completion::stream_completion;
use crate::agent::conversation::Conversation;
use crate::agent::message::{Message, Role};
use crate::mode::Mode;
use crate::styles::{
    ACTIVE_COLOR, ASSISTANT_COLOR, FOCUSED_COLOR, SYSTEM_COLOR, UNFOCUSED_COLOR, USER_COLOR,
};
use crate::{action::Action, tui::Frame};
use async_channel::Sender;

use crate::config::{Config, KeyBindings};

#[derive(Default)]
enum ViewerState {
    Active,
    Focused,
    #[default]
    Unfocused,
}

#[derive(Default)]
pub struct Viewer {
    command_tx: Option<Sender<Action>>,
    config: Config,
    conversation: Conversation,
    state: ViewerState,
}

impl Viewer {
    pub fn new(focused: bool) -> Self {
        let state = if focused {
            ViewerState::Focused
        } else {
            ViewerState::Unfocused
        };

        Self {
            state,
            ..Default::default()
        }
    }
}

impl Component for Viewer {
    fn register_action_handler(&mut self, tx: Sender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::ReceiveMessage(message) => {
                self.conversation.add_message(message);
            }
            Action::StreamMessage(message) => {
                // Simply replace the last message
                self.conversation.replace_last_message(message);
            }

            Action::SwitchMode(mode) => match mode {
                Mode::Viewer => {
                    self.state = ViewerState::Focused;
                    self.conversation.unfocus();
                }
                Mode::ActiveViewer => {
                    self.state = ViewerState::Active;
                    self.conversation.focus();
                }
                Mode::ModelSelector => {
                    self.state = ViewerState::Unfocused;
                    self.conversation.unfocus();
                }
                Mode::Input => {
                    self.state = ViewerState::Unfocused;
                }
                Mode::ActiveInput => {
                    self.state = ViewerState::Unfocused;
                }
            },
            Action::SelectNextMessage => {
                self.conversation.select_next_message();
            }
            Action::SelectPreviousMessage => {
                self.conversation.select_prev_message();
            }
            Action::DeleteSelectedMessage => {
                self.conversation.delete_selected_message();
            }
            Action::SendMessage(message) => {
                // Lets clean this up at some point
                // I don't think this cloning is ideal
                let model = message.model.clone();
                let action_tx = self.command_tx.clone().unwrap();
                let mut messages = self.conversation.messages.clone();
                tokio::spawn(async move {
                    action_tx
                        .send(Action::ReceiveMessage(message.clone()))
                        .await
                        .ok();

                    if let Some(model) = model {
                        let mut content = String::new();

                        action_tx
                            .send(Action::ReceiveMessage(Message {
                                role: Role::Assistant,
                                content: content.clone(),
                                status: Some(PredictionStatus::Starting),
                                model: Some(model.clone()),
                            }))
                            .await
                            .ok();
                        messages.push(message);

                        let stream = stream_completion(&model, messages).await;
                        match stream {
                            Ok((status, mut stream)) => {
                                while let Some(event) = stream.next().await {
                                    match event {
                                        Ok(event) => {
                                            if event.event == "done" {
                                                break;
                                            }
                                            content.push_str(event.data.as_str());
                                            action_tx
                                                .send(Action::StreamMessage(Message {
                                                    role: Role::Assistant,
                                                    content: content.clone(),
                                                    status: None,
                                                    model: Some(model.clone()),
                                                }))
                                                .await
                                                .ok();
                                        }
                                        Err(err) => {
                                            panic!("{:?}", err);
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                panic!("{err}");
                            }
                        }
                    }
                });
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, rect: Rect) -> Result<()> {
        // Render Messages
        let mut message_items = Vec::new();
        let mut line_count: usize = 0;
        for message in &self.conversation.messages {
            let mut message_lines = Vec::new();

            match message.role {
                Role::System => message_lines.push(Line::from(vec![Span::styled(
                    "System",
                    Style::default().fg(SYSTEM_COLOR).bold(),
                )])),
                Role::User => message_lines.push(Line::from(vec![Span::styled(
                    "User",
                    Style::default().fg(USER_COLOR).bold(),
                )])),
                Role::Assistant => {
                    let mut spans = Vec::new();
                    spans.push(Span::styled(
                        "Assistant",
                        Style::default().fg(ASSISTANT_COLOR).bold(),
                    ));

                    if let Some(model) = &message.model {
                        let (owner, model_name) = model.get_model_details();
                        spans.push(Span::styled(
                            format!(": ({owner}/{model_name})"),
                            Style::default().fg(ASSISTANT_COLOR),
                        ));
                    }

                    message_lines.push(Line::from(spans));
                }
            }

            for line in message.content.split("\n") {
                let words = WordSeparator::AsciiSpace
                    .find_words(line)
                    .collect::<Vec<_>>();
                let subs = lines_to_strings(
                    wrap_optimal_fit(&words, &[rect.width as f64 - 2.0], &Penalties::new())
                        .unwrap(),
                );

                for sub in subs {
                    message_lines.push(Line::from(vec![Span::styled(
                        sub,
                        Style::default().fg(Color::White),
                    )]));
                }
            }

            let mut break_line = String::new();
            for _ in 0..(rect.width - 2) {
                break_line.push('-');
            }
            message_lines.push(Line::from(vec![Span::styled(break_line, Style::default())]));

            line_count = message_lines.len();

            // Add seperator to the bottom of the message
            message_items.push(ListItem::new(Text::from(message_lines)));
        }

        let vertical_scroll = 0;
        let list = List::new(message_items.clone())
            .block(
                Block::default()
                    .title(" Conversation ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Thick)
                    .style(Style::default().fg(match self.state {
                        ViewerState::Active => ACTIVE_COLOR,
                        ViewerState::Unfocused => UNFOCUSED_COLOR,
                        ViewerState::Focused => FOCUSED_COLOR,
                    }))
                    .bg(Color::Black),
            )
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::ITALIC)
                    .bg(Color::DarkGray),
            )
            .highlight_symbol("");

        let mut list_state = ListState::default().with_selected(self.conversation.selected_message);

        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut scrollbar_state = ScrollbarState::new(line_count).position(vertical_scroll);

        f.render_stateful_widget(list, rect, &mut list_state);
        f.render_stateful_widget(
            scrollbar,
            rect.inner(&Margin {
                vertical: 1,
                horizontal: 0,
            }), // using a inner vertical margin of 1 unit makes the scrollbar inside the block
            &mut scrollbar_state,
        );
        Ok(())
    }
}
//
// Helper to convert wrapped lines to a Vec<String>.
fn lines_to_strings(lines: Vec<&[Word<'_>]>) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|word| &**word)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
}
