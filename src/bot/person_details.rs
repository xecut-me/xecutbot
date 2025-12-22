use std::collections::HashMap;

use anyhow::Result;
use itertools::Itertools as _;
use teloxide::{prelude::Requester as _, types::UserId};

use crate::backend::{Backend, Uid};

pub(super) struct PersonDetails {
    pub resident: bool,
    pub display_name: String,
    pub link: String,
}

impl<B: Backend> super::TelegramBot<B> {
    pub(super) async fn is_resident(&self, id: UserId) -> Result<bool> {
        Ok(self
            .bot
            .get_chat_member(self.config.private_chat_id, id)
            .await?
            .is_present())
    }

    pub(super) async fn fetch_person_details(&self, user: Uid) -> Result<PersonDetails> {
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

    pub(super) async fn fetch_persons_details(
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

    pub(super) fn format_person_link(&self, details: &PersonDetails) -> String {
        format!(
            "<a href=\"{}\">{}</a>{}",
            details.link,
            details.display_name,
            if details.resident { "®️" } else { "" }
        )
    }
}
