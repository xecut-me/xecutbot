use teloxide::{
    payloads::{SendMessage, SendMessageSetters as _, SetMessageReactionSetters as _},
    prelude::Requester as _,
    requests::{HasPayload as _, JsonRequest},
    sugar::request::{RequestLinkPreviewExt as _, RequestReplyExt as _},
    types::{ChatId, Message, ParseMode, ReactionType},
};

use anyhow::Result;

use crate::backend::{Backend, Uid};

pub(super) fn strip_command(text: &str) -> &str {
    if text.starts_with('/') {
        text.split_once(' ').map(|p| p.1).unwrap_or("")
    } else {
        text
    }
}

pub(super) fn message_text(msg: &Message) -> &str {
    strip_command(msg.text().expect("message to have text"))
}

pub(super) fn message_author(msg: &Message) -> Uid {
    Uid(msg.from.as_ref().expect("message to have author").id)
}

pub(super) fn common_modifiers(send_message: JsonRequest<SendMessage>) -> JsonRequest<SendMessage> {
    send_message
        .disable_notification(true)
        .parse_mode(ParseMode::Html)
        .disable_link_preview(true)
}

impl<B: Backend> super::TelegramBot<B> {
    pub(super) async fn check_is_public_chat_msg(&self, msg: &Message) -> Result<Option<ChatId>> {
        let chat_id = msg.chat.id;
        if chat_id != self.config.public_chat_id {
            log::debug!("check_is_public_chat_msg failed: {:?}", msg);
            self.send_message_reply(msg, "❌ Нужно написать в публичный чат спейса")
                .await?;
            return Ok(None);
        }
        Ok(Some(chat_id))
    }

    pub(super) async fn check_author_is_resident(&self, msg: &Message) -> Result<bool> {
        if !self
            .is_resident(msg.from.as_ref().expect("message to have author").id)
            .await?
        {
            log::debug!("check_author_is_resident failed: {:?}", msg);
            self.send_message_reply(msg, "❌ Нужно быть резидентом")
                .await?;
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn send_message_public_chat(
        &self,
        text: impl Into<String>,
    ) -> JsonRequest<SendMessage> {
        common_modifiers(self.bot.send_message(self.config.public_chat_id, text))
    }

    pub(super) fn send_message_reply(
        &self,
        msg: &Message,
        text: impl Into<String>,
    ) -> JsonRequest<SendMessage> {
        common_modifiers(
            self.bot
                .send_message(msg.chat.id, text)
                .with_payload_mut(|p| p.message_thread_id = msg.thread_id)
                .reply_to(msg.id),
        )
    }

    pub(super) async fn acknowledge_message(&self, msg: &Message) -> Result<()> {
        self.bot
            .set_message_reaction(msg.chat.id, msg.id)
            .reaction(vec![ReactionType::Emoji {
                emoji: "✍".to_owned(),
            }])
            .await?;
        Ok(())
    }
}
