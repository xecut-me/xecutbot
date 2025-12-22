use anyhow::Result;
use chrono::TimeDelta;
use itertools::Itertools as _;
use teloxide::types::Message;

use crate::{VisitStatus, backend::Backend, time::today};

impl<B: Backend> super::TelegramBot<B> {
    pub(super) async fn get_status(&self) -> Result<String> {
        let today = today();
        let mut visits = self.backend().get_visits(today, today).await?;

        let details = self
            .fetch_persons_details(visits.iter().map(|v| v.person))
            .await?;

        visits.sort_by_key(|v| if details[&v.person].resident { 0 } else { 1 });

        let mut status = String::new();

        let checked_in = visits
            .iter()
            .filter(|v| v.status == VisitStatus::CheckedIn)
            .map(|v| self.format_visit_without_status(v, &details[&v.person]))
            .join("\n");

        let any_resident_inside = visits
            .iter()
            .any(|v| v.status == VisitStatus::CheckedIn && details[&v.person].resident);

        let anybody_inside = visits.iter().any(|v| v.status == VisitStatus::CheckedIn);

        if any_resident_inside {
            status.push_str("🟢 Хакспейс сейчас открыт");
        } else {
            status.push_str("🔒 Хакспейс сейчас закрыт");
            if anybody_inside {
                status.push_str(", но кто-то из гостей внутри???");
            }
            status.push_str(
                "\n\n💡 Если хочешь зайти, можно спросить в чате, возможно кто-то из резидентов может прийти.",
            );
        }

        if !checked_in.is_empty() {
            status.push_str("\n\n👷 Сейчас в хакспейсе:\n");
            status.push_str(&checked_in);
        }

        let planned = visits
            .iter()
            .filter(|v| v.status == VisitStatus::Planned)
            .map(|v| self.format_visit_without_status(v, &details[&v.person]))
            .join("\n");

        if !planned.is_empty() {
            status.push_str("\n\n📅 Планировали зайти:\n");
            status.push_str(&planned);
        }

        let left = visits
            .iter()
            .filter(|v| v.status == VisitStatus::CheckedOut)
            .map(|v| self.format_visit_without_status(v, &details[&v.person]))
            .join("\n");

        if !left.is_empty() {
            status.push_str("\n\n🌆 Уже ушли:\n");
            status.push_str(&left);
        }

        let week_visits = self
            .backend()
            .get_visits(today + TimeDelta::days(1), today + TimeDelta::days(7))
            .await?;

        let details = self
            .fetch_persons_details(week_visits.iter().map(|v| v.person))
            .await?;

        let formatted_week_visits = self.format_visits(week_visits, &details);

        if !formatted_week_visits.is_empty() {
            status.push_str("\n\n🗓️ Планы на неделю:\n\n");
            status.push_str(&formatted_week_visits);
        }

        Ok(status)
    }

    pub(super) async fn handle_status(&self, msg: &Message) -> Result<()> {
        if msg.chat.id == self.config.public_chat_id
            && let Some(msg_id) = self.get_status_message_id()
        {
            self.send_message_reply(
                msg,
                format!(
                    "Посмотри в <a href=\"{}\">закрепе</a>",
                    Message::url_of(self.config.public_chat_id, None, msg_id)
                        .expect("should be able to create url of live status message")
                ),
            )
            .await?;
            return Ok(());
        }

        let status = self.get_status().await?;

        self.send_message_reply(msg, status).await?;

        Ok(())
    }
}
