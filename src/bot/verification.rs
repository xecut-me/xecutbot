use anyhow::Result;
use std::{sync::Arc, time::Duration};
use teloxide::{
    payloads::SendMessageSetters as _,
    prelude::Requester as _,
    types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, MessageId, User, UserId},
};

use crate::backend::Backend;

const VERIFICATION_TIMEOUT: Duration = Duration::from_secs(5 * 60); // 5 minutes

impl<B: Backend> super::TelegramBot<B> {
    pub(super) async fn initiate_verification(&self, user: &User) -> Result<()> {
        let user_id = user.id;

        // Create unique callback data
        let callback_data = format!("/verify_{}", user_id.0);

        // Send verification message with button
        let message = self
            .send_message_public_chat(format!(
                "👋 {}, если ты не бот, нажми на кнопку в течение 5 минут",
                user.mention().unwrap_or_else(|| user.first_name.clone())
            ))
            .reply_markup(InlineKeyboardMarkup {
                inline_keyboard: vec![vec![InlineKeyboardButton::callback(
                    "✅ Я не бот",
                    callback_data,
                )]],
            })
            .await?;

        let message_id = message.id;

        // Clone necessary data for the timer task
        let bot_clone = self.bot.clone();
        let chat_id = self.config.public_chat_id;

        // Spawn timer task
        let timer_handle = tokio::spawn(async move {
            tokio::time::sleep(VERIFICATION_TIMEOUT).await;

            // Timer expired - ban the user
            if let Err(e) = Self::ban_user(bot_clone, chat_id, user_id, message_id).await {
                log::error!("Failed to ban user {}: {:?}", user_id, e);
            }
        });

        // Store pending verification
        let verification = super::PendingVerification {
            user_id,
            message_id,
            timer_handle,
        };

        self.pending_verifications
            .write()
            .unwrap()
            .insert(user_id, verification);

        log::info!("Initiated verification for user {}", user_id);

        Ok(())
    }

    pub(super) async fn handle_verification_callback(
        &self,
        callback_user_id: UserId,
        target_user_id: UserId,
    ) -> Result<()> {
        // Check if the user clicking is the one who needs to verify
        if callback_user_id != target_user_id {
            log::debug!(
                "User {} tried to verify for user {}",
                callback_user_id,
                target_user_id
            );
            // Wrong user clicked - ignore
            return Ok(());
        }

        // Remove from pending verifications
        let mut verifications = self.pending_verifications.write().unwrap();
        if let Some(verification) = verifications.remove(&target_user_id) {
            // Abort the timer
            verification.timer_handle.abort();

            // Delete the verification message
            let _ = self
                .bot
                .delete_message(self.config.public_chat_id, verification.message_id)
                .await;

            log::info!("User {} verified successfully", target_user_id);
        }

        Ok(())
    }

    async fn ban_user(
        bot: teloxide::Bot,
        chat_id: ChatId,
        user_id: UserId,
        message_id: MessageId,
    ) -> Result<()> {
        // Ban the user
        bot.ban_chat_member(chat_id, user_id).await?;

        // Delete the verification message
        let _ = bot.delete_message(chat_id, message_id).await;

        log::info!("Banned unverified user: {}", user_id);

        Ok(())
    }
}
