use std::collections::HashMap;

use chrono::{NaiveDate, TimeDelta};
use itertools::Itertools as _;
use teloxide::types::Message;

use anyhow::Result;

use crate::{
    Visit, VisitStatus,
    backend::{Backend, Uid},
    bot::util,
    time::{format_date, today},
    visits::VisitUpdate,
};

use super::person_details::PersonDetails;

fn parse_day_purpose(text: &str) -> (NaiveDate, &str) {
    if let Some(purpose) = text.strip_prefix("завтра") {
        return (today() + TimeDelta::days(1), purpose.trim());
    }
    if let Some(purpose) = text.strip_prefix("послезавтра") {
        return (today() + TimeDelta::days(2), purpose.trim());
    }

    let Ok((date, purpose)) = NaiveDate::parse_and_remainder(text, "%Y-%m-%d") else {
        return (today(), text.trim());
    };

    (date, purpose.trim())
}

pub(super) fn parse_visit_text(author: Uid, msg: &str) -> VisitUpdate {
    let (day, purpose) = parse_day_purpose(msg);
    VisitUpdate {
        person: author,
        day,
        purpose: if purpose.is_empty() {
            None
        } else {
            Some(purpose.to_owned())
        },
        status: VisitStatus::Planned,
    }
}

pub(super) fn parse_visit_message(msg: &Message) -> VisitUpdate {
    parse_visit_text(
        super::util::message_author(msg),
        super::util::message_text(msg),
    )
}

impl<B: Backend> super::TelegramBot<B> {
    pub(super) fn format_visit_without_status(&self, v: &Visit, details: &PersonDetails) -> String {
        format!(
            "{}{}",
            self.format_person_link(details),
            if !v.purpose.is_empty() {
                format!(": \"{}\"", v.purpose)
            } else {
                "".to_owned()
            }
        )
    }

    fn format_visit(&self, v: &Visit, details: &PersonDetails) -> String {
        let status_str = match v.status {
            VisitStatus::Planned => "",
            VisitStatus::CheckedIn => " (сейчас в спейсе 👷)",
            VisitStatus::CheckedOut => " (ушёл 🌆)",
        };
        format!(
            "{}{}",
            self.format_visit_without_status(v, details),
            status_str
        )
    }

    fn format_day<'a>(
        &self,
        vs: impl IntoIterator<Item = &'a Visit>,
        details: &HashMap<Uid, PersonDetails>,
    ) -> String {
        vs.into_iter()
            .sorted_by_key(|v| if details[&v.person].resident { 0 } else { 1 })
            .map(|v| self.format_visit(v, &details[&v.person]))
            .join("\n")
    }

    pub(super) fn format_visits(
        &self,
        mut vs: Vec<Visit>,
        details: &HashMap<Uid, PersonDetails>,
    ) -> String {
        vs.sort_by_key(|v| v.day);

        vs.chunk_by(|v1, v2| v1.day == v2.day)
            .map(|vs| {
                let day = vs[0].day;
                format!("{}:\n{}", format_date(day), self.format_day(vs, details))
            })
            .join("\n\n")
    }

    pub(super) async fn handle_get_visits(&self, msg: &Message) -> Result<()> {
        let visits = self
            .backend()
            .get_visits(today(), today() + TimeDelta::days(185))
            .await?;

        let details = self
            .fetch_persons_details(visits.iter().map(|v| v.person))
            .await?;

        let mut formatted_visits = self.format_visits(visits, &details);

        if !formatted_visits.is_empty() {
            formatted_visits =
                format!("🗓️ Планы посещений на ближайшие полгода:\n\n{formatted_visits}",);
        } else {
            formatted_visits = "😔 Нет никаких планов".to_owned();
        }

        self.send_message_reply(msg, formatted_visits).await?;

        Ok(())
    }

    pub(super) async fn handle_plan_visit(&self, msg: &Message) -> Result<()> {
        let visit_update = parse_visit_message(msg);

        self.backend()
            .plan_visit(visit_update.person, visit_update.day, visit_update.purpose)
            .await?;

        self.acknowledge_message(msg).await?;

        Ok(())
    }

    pub(super) async fn handle_unplan_visit(&self, msg: &Message) -> Result<()> {
        let visit_update = parse_visit_message(msg);

        self.backend()
            .unplan_visit(visit_update.person, visit_update.day)
            .await?;

        self.acknowledge_message(msg).await?;

        Ok(())
    }

    pub(super) async fn handle_check_in(&self, msg: &Message) -> Result<()> {
        let person = util::message_author(msg);
        let purpose_raw = util::message_text(msg);
        let purpose = if purpose_raw.is_empty() {
            None
        } else {
            Some(purpose_raw.to_owned())
        };

        self.backend().check_in(person, purpose).await?;

        self.acknowledge_message(msg).await?;

        Ok(())
    }

    pub(super) async fn handle_check_out(&self, msg: &Message) -> Result<()> {
        let person = util::message_author(msg);

        self.backend().check_out(person).await?;

        self.acknowledge_message(msg).await?;

        Ok(())
    }

    pub(super) async fn handle_close(&self, msg: &Message) -> Result<()> {
        if self.check_is_public_chat_msg(msg).await?.is_none() {
            return Ok(());
        };
        if !self.check_author_is_resident(msg).await? {
            return Ok(());
        }

        self.backend().check_out_everybody().await?;

        self.acknowledge_message(msg).await?;

        Ok(())
    }
}
