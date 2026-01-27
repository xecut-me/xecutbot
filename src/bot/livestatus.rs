use std::{sync::Arc, time::Duration};

use anyhow::Result;
use chrono::Locale;
use sqlx::SqlitePool;
use teloxide::{
    payloads::{
        EditMessageTextSetters as _, PinChatMessageSetters as _, SendMessageSetters as _,
        UnpinChatMessageSetters as _,
    },
    prelude::Requester as _,
    sugar::request::RequestLinkPreviewExt as _,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Message, MessageId, ParseMode},
};
use tokio_util::sync::CancellationToken;

use crate::backend::Backend;

const LIVE_UPDATE_INTERVAL: Duration = Duration::from_secs(10);

pub(super) async fn load_status_message_id(pool: &SqlitePool) -> Result<Option<MessageId>> {
    let message_id = sqlx::query!("SELECT message_id FROM status_messages")
        .map(|r| r.message_id)
        .fetch_optional(pool)
        .await?
        .map(|id| MessageId(id as i32));
    Ok(message_id)
}

pub(super) async fn save_status_message_id(pool: &SqlitePool, id: Option<MessageId>) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query!("DELETE FROM status_messages")
        .execute(&mut *tx)
        .await?;
    if let Some(id) = id {
        sqlx::query!("INSERT INTO status_messages (message_id) VALUES (?1)", id.0)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

impl<B: Backend> super::TelegramBot<B> {
    async fn set_status_message_id(&self, id: Option<MessageId>) -> Result<()> {
        save_status_message_id(self.backend().pool(), id).await?;
        *self.status_message_id.write().unwrap() = id;
        Ok(())
    }

    pub(super) fn get_status_message_id(&self) -> Option<MessageId> {
        *self.status_message_id.read().unwrap()
    }

    fn live_status_markup() -> InlineKeyboardMarkup {
        InlineKeyboardMarkup {
            inline_keyboard: vec![
                vec![
                    InlineKeyboardButton::callback("👷 Я зашёл", "/checkin"),
                    InlineKeyboardButton::callback("🌆 Я ушёл", "/checkout"),
                ],
                vec![
                    InlineKeyboardButton::callback("🚋 Зайду сегодня", "/planvisit"),
                    InlineKeyboardButton::callback("🤔 Передумал", "/unplanvisit"),
                ],
            ],
        }
    }

    pub(super) async fn handle_live_status(&self, msg: &Message) -> Result<()> {
        let Some(chat_id) = self.check_is_public_chat_msg(msg).await? else {
            return Ok(());
        };
        if !self.check_author_is_resident(msg).await? {
            return Ok(());
        }

        if let Some(msg_id) = self.get_status_message_id() {
            self.bot
                .unpin_chat_message(chat_id)
                .message_id(msg_id)
                .await?;
            self.set_status_message_id(None).await?;
        }

        let msg_id = self
            .send_message_public_chat(Self::get_full_live_status(&self.get_status().await?))
            .reply_markup(Self::live_status_markup())
            .await?
            .id;
        self.set_status_message_id(Some(msg_id)).await?;

        self.bot
            .pin_chat_message(chat_id, msg_id)
            .disable_notification(true)
            .await?;

        Ok(())
    }

    pub(super) async fn handle_unlive_status(&self, msg: &Message) -> Result<()> {
        let Some(chat_id) = self.check_is_public_chat_msg(msg).await? else {
            return Ok(());
        };
        if !self.check_author_is_resident(msg).await? {
            return Ok(());
        }

        if let Some(msg_id) = self.get_status_message_id() {
            self.bot
                .unpin_chat_message(chat_id)
                .message_id(msg_id)
                .await?;
            self.set_status_message_id(None).await?;
        }
        Ok(())
    }

    fn get_full_live_status(live_status: &str) -> String {
        live_status.to_owned()
            + "\n\nОбновлено: "
            + &crate::datetime::now()
                .format_localized("%c %Z", Locale::ru_RU)
                .to_string()
    }

    pub(super) async fn update_live_status_message(&self, live_status: &str) -> Result<()> {
        let Some(msg_id) = self.get_status_message_id() else {
            return Ok(());
        };

        self.bot
            .edit_message_text(
                self.config.public_chat_id,
                msg_id,
                Self::get_full_live_status(live_status),
            )
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
            .reply_markup(Self::live_status_markup())
            .await?;

        Ok(())
    }

    pub(super) async fn update_live_task(self: Arc<Self>, ct: CancellationToken) {
        log::info!("Started live update task");

        let mut interval = tokio::time::interval(LIVE_UPDATE_INTERVAL);

        let mut last_live_status = None;

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = ct.cancelled() => { break }
            };
            log::trace!("Updating status message");
            let new_live_status = match self.get_status().await {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Error getting live status: {:?}", e);
                    continue;
                }
            };
            if last_live_status.is_none_or(|ref v| v != &new_live_status)
                && let Err(e) = self.update_live_status_message(&new_live_status).await
            {
                log::error!("Error updating status message: {:?}", e);
            }
            last_live_status = Some(new_live_status);
        }

        log::info!("Stopped live update task");
    }

    pub(super) async fn handle_post_live(&self, msg: &Message) -> Result<()> {
        let Some(chat_id) = self.check_is_public_chat_msg(msg).await? else {
            return Ok(());
        };
        if !self.check_author_is_resident(msg).await? {
            return Ok(());
        }

        let Some(original_message) = msg.reply_to_message() else {
            log::debug!("message is not a reply: {:?}", msg);
            self.send_message_reply(msg, "❌ Нужно ответить на сообщение")
                .await?;
            return Ok(());
        };

        self.bot
            .send_message(
                self.config.public_channel_id,
                original_message
                    .url()
                    .expect("original message to have URL"),
            )
            .disable_link_preview(true)
            .await?;

        log::debug!("message posted");

        let forwarded_message_url = self
            .bot
            .forward_message(self.config.public_channel_id, chat_id, original_message.id)
            .await?
            .url()
            .expect("forwarded message to have URL");

        log::debug!("original message forwarded");

        let channel_name = self
            .bot
            .get_chat(self.config.public_channel_id)
            .await?
            .title()
            .unwrap_or("канал")
            .to_owned();

        self.send_message_reply(
            msg,
            format!("✔️ Запостил в <a href=\"{forwarded_message_url}\">{channel_name}</a>"),
        )
        .await?;

        Ok(())
    }
}
