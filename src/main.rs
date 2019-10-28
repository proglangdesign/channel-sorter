use chrono::{
    offset::{FixedOffset, Utc},
    DateTime, Duration,
};
use serenity::{
    model::{
        channel::Message,
        id::{ChannelId, GuildId},
    },
    prelude::*,
};
use std::{
    convert::TryInto,
    fs::{read, write},
    iter::once,
};

//token in gitignore to prevent leak
const TOKEN: &str = include_str!("bot-token.txt");
const ACTIVE_CATEGORY: ChannelId = ChannelId(530604963911696404);
const INACTIVE_CATEGORY: ChannelId = ChannelId(541808219593506827);
const GUILD: GuildId = GuildId(530598289813536771);

const FILE: &str = "./archived.bincode";

fn main() {
    let mut client = loop {
        match Client::new(
            TOKEN,
            Handler {
                archived_explicitly: read(FILE)
                    .map(|bytes| bincode::deserialize(&bytes).expect("corrupted file"))
                    .unwrap_or(vec![])
                    .into(),
            },
        ) {
            Ok(client) => break client,
            Err(why) => eprintln!("Client creation error: {:?}", why),
        }
    };

    while let Err(why) = client.start() {
        eprintln!("Client error: {:?}", why);
    }
}

struct Handler {
    archived_explicitly: RwLock<Vec<(ChannelId, DateTime<FixedOffset>)>>,
}

impl EventHandler for Handler {
    fn message(&self, ctx: Context, _msg: Message) {
        let mut channels = match GUILD.channels(&ctx) {
            Ok(channels) => channels,
            Err(why) => {
                eprintln!("Couldn't get channels: {:?}", why);
                return;
            }
        };
        let names_and_positions: Vec<_> = channels
            .iter()
            .filter_map(|(_id, guild_channel)| match guild_channel.category_id {
                Some(category) if category == ACTIVE_CATEGORY => {
                    Some((guild_channel.name.clone(), guild_channel.position))
                }
                _ => None,
            })
            .collect();
        let relevant_channels = channels.iter_mut().filter_map(|(_id, guild_channel)| {
            match guild_channel.category_id {
                Some(category) if category == ACTIVE_CATEGORY || category == INACTIVE_CATEGORY => {
                    Some(guild_channel)
                }
                _ => None,
            }
        });
        for channel in relevant_channels {
            //no more than 100 messages allowed
            let messages = match channel.messages(&ctx, |get_messages| get_messages.limit(100)) {
                Ok(messages) => messages,
                Err(_) => continue,
            };
            if messages.is_empty() {
                continue;
            }
            let last_message = messages.iter().find(|message| message.webhook_id == None);
            if let Some(message) = last_message {
                let entry = (
                    channel.id,
                    message.edited_timestamp.unwrap_or(message.timestamp),
                );
                let read_guard = self.archived_explicitly.read();
                if message.content.trim() == "!archive" && !read_guard.contains(&entry) {
                    let index_option = read_guard
                        .iter()
                        .position(|(id, _timestamp)| id == &channel.id);
                    let mut write_guard = self.archived_explicitly.write();
                    if let Some(index) = index_option {
                        write_guard.remove(index);
                    }
                    write_guard.push(entry);
                    self.update_file();
                    let _ = channel.delete_messages(&ctx, once(message));
                }
            }
            let new_category = match &last_message {
                Some(message)
                    if {
                        let timestamp: DateTime<Utc> = message.timestamp.into();
                        Utc::now() - timestamp < Duration::days(30 * 2)
                    } =>
                {
                    if let Some(index) =
                        self.archived_explicitly
                            .read()
                            .iter()
                            .position(|(id, timestamp)| {
                                &channel.id == id && &message.timestamp > timestamp
                            })
                    {
                        self.archived_explicitly.write().remove(index);
                        self.update_file();
                        ACTIVE_CATEGORY
                    } else if self
                        .archived_explicitly
                        .read()
                        .iter()
                        .any(|(id, _timestamp)| id == &channel.id)
                    {
                        INACTIVE_CATEGORY
                    } else {
                        ACTIVE_CATEGORY
                    }
                }
                _ => INACTIVE_CATEGORY,
            };
            if new_category == channel.category_id.unwrap() {
                continue;
            }
            let new_position = names_and_positions
                .iter()
                .max_by(|(cur_name, _cur_pos), (msg_name, _msg_pos)| {
                    if msg_name < &channel.name {
                        msg_name.cmp(cur_name)
                    } else {
                        cur_name.cmp(msg_name)
                    }
                })
                .map(|(name, pos)| if &channel.name < name { *pos } else { pos + 1 })
                .unwrap_or(0)
                .try_into()
                .unwrap();
            let _ = channel.edit(&ctx, |edit_channel| {
                edit_channel.category(new_category).position(new_position)
            });
        }
    }
}

impl Handler {
    fn update_file(&self) {
        let _ = write(
            FILE,
            bincode::serialize(&*self.archived_explicitly.read()).unwrap(),
        );
    }
}
