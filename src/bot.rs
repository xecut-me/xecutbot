use anyhow::Result;
use chrono::{Locale, NaiveDate, TimeDelta};
use futures::FutureExt;
use itertools::Itertools;
use sqlx::SqlitePool;
use std::{
    collections::HashMap,
    panic::AssertUnwindSafe,
    sync::{Arc, RwLock, Weak},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

use teloxide::{
    payloads::{SendMessage, SendMessageSetters as _},
    prelude::*,
    requests::{HasPayload as _, JsonRequest},
    sugar::request::{RequestLinkPreviewExt as _, RequestReplyExt as _},
    types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode, ReactionType},
    utils::command::BotCommands,
};

use crate::{
    backend::Backend,
    config::TelegramBotConfig,
    date::{ParsedMessage, parse_message_with_date},
    visits::{Visit, VisitStatus, VisitUpdate},
};
use crate::{backend::Uid, utils::today};

#[derive(BotCommands, Clone, Copy)]
#[command(rename_rule = "lowercase")]
enum Command {
    #[command(
        description = "📮 Репостнуть пост в live канал (реплайни на пост, доступно только резидентам)"
    )]
    PostLive,
    #[command(description = "ℹ️ Посмотреть что сейчас происходит в хакспейсе")]
    Status,
    #[command(description = "🗓️ Посмотреть кто собирается в хакспейс в ближайшие дни")]
    GetVisits,
    #[command(
        description = "🗓️ Запланировать зайти в хакспейс (опционально дата в формате YYYY-MM-DD и описание зачем)"
    )]
    PlanVisit,
    #[command(
        description = "🤔 Передумать заходить в хакспейс (опционально дата в формате YYYY-MM-DD)"
    )]
    UnplanVisit,
    #[command(description = "👷 Отметиться как зашедший (опционально описание зачем)")]
    CheckIn,
    #[command(description = "🌆 Отметиться как ушедший")]
    CheckOut,
    #[command(description = "🌒 Закрыть хакспейс")]
    Close,
    #[command(description = "🔃 Сделать закреп с текущей информацией о спейсе")]
    LiveStatus,
    #[command(description = "🧟 Убрать закреп с текущей информацией о спейсе")]
    UnLiveStatus,
}

fn strip_command(text: &str) -> &str {
    if text.starts_with('/') {
        text.split_once(' ').map(|p| p.1).unwrap_or("")
    } else {
        text
    }
}

fn parse_visit_text(author: Uid, msg: &str) -> Result<VisitUpdate> {
    let ParsedMessage { day, purpose } = parse_message_with_date(today(), msg)?;

    Ok(VisitUpdate {
        person: author,
        day: day.unwrap_or_else(today),
        purpose,
        status: VisitStatus::Planned,
    })
}

fn format_close_date(date: NaiveDate) -> Option<&'static str> {
    let today = today();
    match (date - today).num_days() {
        0 => Some("сегодня"),
        1 => Some("завтра"),
        2 => Some("послезавтра"),
        _ => None,
    }
}

fn format_date(date: NaiveDate) -> String {
    let format = if date - today() > TimeDelta::days(60) {
        "%-d %B %Y (%A)"
    } else {
        "%-d %B (%A)"
    };
    let base_date = date
        .format_localized(format, Locale::ru_RU)
        .to_string()
        .to_lowercase();
    if let Some(close_date) = format_close_date(date) {
        return format!("{}, {}", close_date, base_date);
    }
    base_date
}

struct PersonDetails {
    resident: bool,
    display_name: String,
    link: String,
}

const LIVE_UPDATE_INTERVAL: Duration = Duration::from_secs(2);

pub struct TelegramBot<B: Backend> {
    config: TelegramBotConfig,
    bot: Bot,
    status_message_id: RwLock<Option<MessageId>>,
    backend: Weak<B>,
}

impl<B: Backend> TelegramBot<B> {
    pub fn new(config: TelegramBotConfig, backend: Weak<B>) -> Result<Arc<Self>> {
        let bot = Bot::new(config.bot_token.clone());
        Ok(Arc::new(TelegramBot {
            config,
            bot,
            status_message_id: RwLock::new(None),
            backend,
        }))
    }

    // Can not panic during execution of run(), because Backend is already constructed and not destructed yet
    fn backend(&self) -> Arc<B> {
        self.backend.upgrade().expect("Backend to be available")
    }

    async fn send_alert(&self) -> Result<()> {
        self.bot
            .send_message(self.config.alert_chat_id, "💥 Что-то пошло не так")
            .await?;
        Ok(())
    }

    pub async fn run(
        self: Arc<Self>,
        shutdown_signal: impl Future<Output = ()> + Send + 'static,
    ) -> Result<()> {
        log::info!("Starting Telegram bot");

        self.bot.set_my_commands(Command::bot_commands()).await?;

        *self.status_message_id.write().unwrap() =
            Self::load_status_message_id(self.backend().pool()).await?;

        let self_clone_outer1 = self.clone();

        let handle_message = move |msg: Message, cmd: Command| {
            let self_clone = self_clone_outer1.clone();
            async move {
                let res = AssertUnwindSafe(self_clone.clone().handle_message(&msg, cmd))
                    .catch_unwind()
                    .await;
                if matches!(res, Err(_) | Ok(Err(_))) {
                    self_clone.send_alert().await?;
                    self_clone
                        .send_message_reply(
                            &msg,
                            "😬 Что-то пошло не так, но админ уже об этом знает",
                        )
                        .await?;
                    if let Ok(e) = res {
                        return e;
                    }
                }
                Ok(())
            }
        };

        let self_clone_outer2 = self.clone();

        let handle_callback = move |q: CallbackQuery| {
            let self_clone = self_clone_outer2.clone();
            async move {
                let res = AssertUnwindSafe(self_clone.clone().handle_callback(&q))
                    .catch_unwind()
                    .await;
                self_clone.bot.answer_callback_query(q.id.clone()).await?;
                if matches!(res, Err(_) | Ok(Err(_))) {
                    self_clone.send_alert().await?;
                    self_clone
                        .send_message_public_chat(
                            "😬 Что-то пошло не так, но админ уже об этом знает",
                        )
                        .await?;
                    if let Ok(e) = res {
                        return e;
                    }
                }
                Ok(())
            }
        };

        let handler = dptree::entry()
            .branch(
                Update::filter_message()
                    .filter_command::<Command>()
                    .endpoint(handle_message),
            )
            .branch(Update::filter_callback_query().endpoint(handle_callback));

        let live_update_ct = self.clone().spawn_update_live_task().await;

        let mut dispatcher = Dispatcher::builder(self.bot.clone(), handler).build();

        let token = dispatcher.shutdown_token();

        tokio::spawn(async move {
            dispatcher.dispatch().await;
        });

        shutdown_signal.await;

        match token.shutdown() {
            Ok(f) => {
                f.await;
            }
            Err(_) => {
                log::info!(
                    "Shutdown signal received, the dispatcher isn't running, ignoring the signal"
                )
            }
        }

        live_update_ct.cancel();

        Ok(())
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

    async fn spawn_update_live_task(self: &Arc<Self>) -> CancellationToken {
        let cancellation_token = CancellationToken::new();
        let result = cancellation_token.clone();
        let self_clone = self.clone();

        tokio::task::spawn(async move {
            let mut interval = tokio::time::interval(LIVE_UPDATE_INTERVAL);

            let mut last_live_status = None;

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = cancellation_token.cancelled() => { break }
                };
                log::trace!("Updating status message");
                let new_live_status = match self_clone.get_status().await {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("Error getting live status: {:?}", e);
                        continue;
                    }
                };
                if last_live_status.is_none_or(|ref v| v != &new_live_status)
                    && let Err(e) = self_clone
                        .update_live_status_message(&new_live_status)
                        .await
                {
                    log::error!("Error updating status message: {:?}", e);
                }
                last_live_status = Some(new_live_status);
            }
        });

        result
    }

    async fn handle_message(self: Arc<Self>, msg: &Message, cmd: Command) -> Result<()> {
        match cmd {
            Command::PostLive => self.handle_post_live(msg).await,
            Command::Status => self.handle_status(msg).await,
            Command::GetVisits => self.handle_get_visits(msg).await,
            Command::PlanVisit => self.handle_plan_visit(msg).await,
            Command::UnplanVisit => self.handle_unplan_visit(msg).await,
            Command::CheckIn => self.handle_check_in(msg).await,
            Command::CheckOut => self.handle_check_out(msg).await,
            Command::Close => self.handle_close(msg).await,
            Command::LiveStatus => self.handle_live_status(msg).await,
            Command::UnLiveStatus => self.handle_unlive_status(msg).await,
        }
    }

    async fn is_resident(&self, id: UserId) -> Result<bool> {
        Ok(self
            .bot
            .get_chat_member(self.config.private_chat_id, id)
            .await?
            .is_present())
    }

    async fn fetch_person_details(&self, user: Uid) -> Result<PersonDetails> {
        let user_id = user.0;
        let chat_member = self
            .bot
            .get_chat_member(self.config.public_chat_id, user_id)
            .await?;
        let resident = self.is_resident(user_id).await?;
        let display_name = if let Some(ref username) = chat_member.user.username {
            username.clone()
        } else {
            chat_member.user.full_name()
        };
        let link = chat_member.user.preferably_tme_url().to_string();
        Ok(PersonDetails {
            resident,
            display_name,
            link,
        })
    }

    async fn fetch_persons_details(
        &self,
        persons: impl IntoIterator<Item = Uid>,
    ) -> Result<HashMap<Uid, PersonDetails>> {
        Ok(futures::future::try_join_all(
            persons.into_iter().unique().map(async |user| -> Result<_> {
                Ok((user, self.fetch_person_details(user).await?))
            }),
        )
        .await?
        .into_iter()
        .collect::<HashMap<_, _>>())
    }

    async fn check_is_public_chat_msg(&self, msg: &Message) -> Result<Option<ChatId>> {
        let chat_id = msg.chat.id;
        if chat_id != self.config.public_chat_id {
            log::debug!("check_is_public_chat_msg failed: {:?}", msg);
            self.send_message_reply(msg, "❌ Нужно написать в публичный чат спейса")
                .await?;
            return Ok(None);
        }
        Ok(Some(chat_id))
    }

    async fn check_author_is_resident(&self, msg: &Message) -> Result<bool> {
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

    async fn handle_post_live(&self, msg: &Message) -> Result<()> {
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

    fn format_person_link(&self, details: &PersonDetails) -> String {
        format!(
            "<a href=\"{}\">{}</a>{}",
            details.link,
            details.display_name,
            if details.resident { "®️" } else { "" }
        )
    }

    fn format_visit_without_status(&self, v: &Visit, details: &PersonDetails) -> String {
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

    async fn get_status(&self) -> Result<String> {
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

    async fn handle_status(&self, msg: &Message) -> Result<()> {
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

    fn format_visits(&self, mut vs: Vec<Visit>, details: &HashMap<Uid, PersonDetails>) -> String {
        vs.sort_by_key(|v| v.day);

        vs.chunk_by(|v1, v2| v1.day == v2.day)
            .map(|vs| {
                let day = vs[0].day;
                format!("{}:\n{}", format_date(day), self.format_day(vs, details))
            })
            .join("\n\n")
    }

    async fn handle_get_visits(&self, msg: &Message) -> Result<()> {
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

    async fn load_status_message_id(pool: &SqlitePool) -> Result<Option<MessageId>> {
        let message_id = sqlx::query!("SELECT message_id FROM status_messages")
            .map(|r| r.message_id)
            .fetch_optional(pool)
            .await?
            .map(|id| MessageId(id as i32));
        Ok(message_id)
    }

    async fn save_status_message_id(pool: &SqlitePool, id: Option<MessageId>) -> Result<()> {
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

    async fn set_status_message_id(&self, id: Option<MessageId>) -> Result<()> {
        Self::save_status_message_id(self.backend().pool(), id).await?;
        *self.status_message_id.write().unwrap() = id;
        Ok(())
    }

    fn get_status_message_id(&self) -> Option<MessageId> {
        *self.status_message_id.read().unwrap()
    }

    async fn handle_live_status(&self, msg: &Message) -> Result<()> {
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

    async fn handle_unlive_status(&self, msg: &Message) -> Result<()> {
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
            + &crate::utils::now()
                .format_localized("%c %Z", Locale::ru_RU)
                .to_string()
    }

    async fn update_live_status_message(&self, live_status: &str) -> Result<()> {
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

    fn message_text(msg: &Message) -> &str {
        strip_command(msg.text().expect("message to have text"))
    }

    fn message_author(msg: &Message) -> Uid {
        Uid(msg.from.as_ref().expect("message to have author").id)
    }

    fn common_modifiers(send_message: JsonRequest<SendMessage>) -> JsonRequest<SendMessage> {
        send_message
            .disable_notification(true)
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
    }

    fn send_message_public_chat(&self, text: impl Into<String>) -> JsonRequest<SendMessage> {
        Self::common_modifiers(self.bot.send_message(self.config.public_chat_id, text))
    }

    fn send_message_reply(
        &self,
        msg: &Message,
        text: impl Into<String>,
    ) -> JsonRequest<SendMessage> {
        Self::common_modifiers(
            self.bot
                .send_message(msg.chat.id, text)
                .with_payload_mut(|p| p.message_thread_id = msg.thread_id)
                .reply_to(msg.id),
        )
    }

    async fn acknowledge_message(&self, msg: &Message) -> Result<()> {
        self.bot
            .set_message_reaction(msg.chat.id, msg.id)
            .reaction(vec![ReactionType::Emoji {
                emoji: "✍".to_owned(),
            }])
            .await?;
        Ok(())
    }

    async fn handle_plan_visit(&self, msg: &Message) -> Result<()> {
        let Ok(visit_update) = parse_visit_text(Self::message_author(msg), Self::message_text(msg))
        else {
            self.send_message_reply(msg, "Несуществующая дата").await?;
            return Ok(());
        };

        self.backend()
            .plan_visit(visit_update.person, visit_update.day, visit_update.purpose)
            .await?;

        self.acknowledge_message(msg).await?;

        Ok(())
    }

    async fn handle_unplan_visit(&self, msg: &Message) -> Result<()> {
        let Ok(visit_update) = parse_visit_text(Self::message_author(msg), Self::message_text(msg))
        else {
            self.send_message_reply(msg, "Несуществующая дата").await?;
            return Ok(());
        };

        self.backend()
            .unplan_visit(visit_update.person, visit_update.day)
            .await?;

        self.acknowledge_message(msg).await?;

        Ok(())
    }

    async fn handle_check_in(&self, msg: &Message) -> Result<()> {
        let person = Self::message_author(msg);
        let purpose_raw = Self::message_text(msg);
        let purpose = if purpose_raw.is_empty() {
            None
        } else {
            Some(purpose_raw.to_owned())
        };

        self.backend().check_in(person, purpose).await?;

        self.acknowledge_message(msg).await?;

        Ok(())
    }

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

    async fn handle_check_out(&self, msg: &Message) -> Result<()> {
        let person = Self::message_author(msg);

        self.backend().check_out(person).await?;

        self.acknowledge_message(msg).await?;

        Ok(())
    }

    async fn handle_close(&self, msg: &Message) -> Result<()> {
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

    async fn handle_callback(&self, q: &CallbackQuery) -> Result<()> {
        let msg = q
            .regular_message()
            .ok_or_else(|| anyhow::anyhow!("message too old"))?;

        let Some(data) = q.data.as_deref() else {
            return Ok(());
        };

        let author = Uid(q.from.id);

        if data.starts_with("/planvisit") {
            let Ok(visit_update) = parse_visit_text(author, strip_command(data)) else {
                self.send_message_reply(msg, "Неправильная дата").await?;
                return Ok(());
            };

            self.backend()
                .plan_visit(visit_update.person, visit_update.day, visit_update.purpose)
                .await?;
        } else if data.starts_with("/unplanvisit") {
            let Ok(visit_update) = parse_visit_text(author, strip_command(data)) else {
                self.send_message_reply(msg, "Неправильная дата").await?;
                return Ok(());
            };

            self.backend()
                .unplan_visit(visit_update.person, visit_update.day)
                .await?;
        } else if data == "/checkin" {
            self.backend().check_in(author, None).await?;
        } else if data == "/checkout" {
            self.backend().check_out(author).await?;
        } else {
            anyhow::bail!("unhandled callback query: {:?}", q);
        }

        Ok(())
    }
}
