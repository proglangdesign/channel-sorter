use {
    chrono::{
        offset::{FixedOffset, Utc},
        DateTime, Duration,
    },
    serenity::{
        model::{
            channel::Message,
            id::{ChannelId, GuildId},
            permissions::Permissions,
        },
        prelude::*,
    },
    std::{
        convert::TryInto,
        fs::{read, write},
        iter::once,
        mem::drop,
    },
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
                archived: read(FILE)
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
    archived: Mutex<Vec<(ChannelId, DateTime<FixedOffset>)>>,
}

impl EventHandler for Handler {
    fn message(&self, ctx: Context, _msg: Message) {
        let mut archived_lock = self.archived.lock();
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
                if message.content.trim() == "!archive"
                    && match channel.permissions_for_user(&ctx, &message.author) {
                        Ok(permissions) => permissions,
                        Err(_) => continue,
                    }
                    .contains(Permissions::MANAGE_MESSAGES)
                    && !archived_lock.contains(&entry)
                {
                    if let Some(index) = archived_lock
                        .iter()
                        .position(|(id, _timestamp)| id == &channel.id)
                    {
                        archived_lock.remove(index);
                    }
                    archived_lock.push(entry);
                    update_file(&archived_lock);
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
                    if let Some(index) = archived_lock.iter().position(|(id, timestamp)| {
                        &channel.id == id && &message.timestamp > timestamp
                    }) {
                        archived_lock.remove(index);
                        update_file(&archived_lock);
                        ACTIVE_CATEGORY
                    } else if archived_lock
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
        //makes sure the message functions always run in sequence
        drop(archived_lock);
    }
}

fn update_file(archived: &[(ChannelId, DateTime<FixedOffset>)]) {
    let _ = write(FILE, bincode::serialize(&archived).unwrap());
}
