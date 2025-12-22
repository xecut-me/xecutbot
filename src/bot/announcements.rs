use crate::{
    backend::{Backend, Uid},
    time::{format_close_date, format_date},
    visits::VisitUpdate,
};
use anyhow::Result;
use chrono::NaiveDate;
use teloxide::{
    payloads::SendMessageSetters as _,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
};

impl<B: Backend> super::TelegramBot<B> {
    pub async fn announce_check_in(&self, visit_update: &VisitUpdate) -> Result<()> {
        self.send_message_public_chat(format!(
            "👷 {} пришёл в хакспейс{}",
            self.format_person_link(&self.fetch_person_details(visit_update.person).await?),
            visit_update
                .purpose
                .as_deref()
                .map(|p| { format!(": \"{p}\"") })
                .unwrap_or_default()
        ))
        .reply_markup(InlineKeyboardMarkup {
            inline_keyboard: vec![vec![
                InlineKeyboardButton::callback("👷 Я тоже в спейсе", "/checkin"),
                InlineKeyboardButton::callback("🌆 А я уже ушёл", "/checkout"),
            ]],
        })
        .await?;
        Ok(())
    }

    pub async fn announce_plan(&self, visit_update: &VisitUpdate) -> Result<()> {
        let day = visit_update.day;
        self.send_message_public_chat(format!(
            "🗓️🚋 {} планирует зайти в хакспейс {}{}",
            self.format_person_link(&self.fetch_person_details(visit_update.person).await?),
            format_date(day),
            visit_update
                .purpose
                .as_deref()
                .map(|p| { format!(": \"{p}\"") })
                .unwrap_or_default()
        ))
        .reply_markup(InlineKeyboardMarkup {
            inline_keyboard: vec![vec![
                InlineKeyboardButton::callback(
                    format!(
                        "🚋 Я тоже зайду {}",
                        format_close_date(day).unwrap_or("в этот день")
                    ),
                    format!("/planvisit {}", day),
                ),
                InlineKeyboardButton::callback("🤔 Или нет", format!("/unplanvisit {}", day)),
            ]],
        })
        .await?;
        Ok(())
    }

    pub async fn announce_unplan(&self, person: Uid, day: NaiveDate) -> Result<()> {
        self.send_message_public_chat(format!(
            "🗓️🤔 {} больше не планирует зайти в хакспейс {}",
            self.format_person_link(&self.fetch_person_details(person).await?),
            format_date(day)
        ))
        .reply_markup(InlineKeyboardMarkup {
            inline_keyboard: vec![vec![
                InlineKeyboardButton::callback(
                    format!(
                        "🚋 А я приду {}",
                        format_close_date(day).unwrap_or("в этот день")
                    ),
                    format!("/planvisit {}", day),
                ),
                InlineKeyboardButton::callback(
                    "🤔 Я тоже не приду",
                    format!("/unplanvisit {}", day),
                ),
            ]],
        })
        .await?;
        Ok(())
    }
}
