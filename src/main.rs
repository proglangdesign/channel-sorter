use chrono::{
    naive::NaiveDateTime,
    offset::{FixedOffset, Utc},
    DateTime, Duration,
};
use serenity::{
    model::{
        channel::{GuildChannel, Message},
        id::{ChannelId, GuildId, RoleId},
    },
    prelude::*,
};
use std::{collections::HashMap, convert::TryInto, fs, sync::Arc};

//token in gitignore to prevent leak
const TOKEN: &str = include_str!("bot-token.txt");
const ACTIVE_A_THRU_M: ChannelId = ChannelId(530604963911696404);
const ACTIVE_N_THRU_Z: ChannelId = ChannelId(745791509290418187);
const INACTIVE_CATEGORY: ChannelId = ChannelId(541808219593506827);
const STICKY_CHANNEL: ChannelId = ChannelId(688618253563592718);
const PERMISSION_ROLE: RoleId = RoleId(530618624122028042);
const GUILD: GuildId = GuildId(530598289813536771);

const FILE: &str = "./archived.bincode";

fn main() {
    let mut client = loop {
        match Client::new(
            TOKEN,
            Handler {
                archived: decode_file(),
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

impl Handler {
    fn archive_channels(&self, ctx: &Context, channels: &mut HashMap<ChannelId, GuildChannel>) {
        let relevant_channels = channels.iter_mut().filter_map(|(&id, guild_channel)| {
            if id == STICKY_CHANNEL {
                return None;
            }
            match guild_channel.category_id {
                Some(category)
                    if [ACTIVE_A_THRU_M, ACTIVE_N_THRU_Z, INACTIVE_CATEGORY]
                        .contains(&category) =>
                {
                    Some(guild_channel)
                }
                _ => None,
            }
        });

        let mut archived_lock = self.archived.lock();
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
                    && message
                        .author
                        .has_role(ctx, GUILD, PERMISSION_ROLE)
                        .unwrap_or(false)
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
                }
            }

            let new_category = match &last_message {
                Some(&ref message)
                    if {
                        let timestamp: DateTime<Utc> = message.timestamp.into();
                        Utc::now() - timestamp < Duration::days(30 * 2)
                    } =>
                {
                    let active_category = {
                        if &*channel.name < "n" {
                            ACTIVE_A_THRU_M
                        } else {
                            ACTIVE_N_THRU_Z
                        }
                    };
                    if let Some(index) = archived_lock.iter().position(|(id, timestamp)| {
                        &channel.id == id && &message.timestamp > timestamp
                    }) {
                        archived_lock.remove(index);
                        update_file(&archived_lock);
                        active_category
                    } else if archived_lock
                        .iter()
                        .any(|(id, _timestamp)| id == &channel.id)
                    {
                        INACTIVE_CATEGORY
                    } else {
                        active_category
                    }
                }
                _ => INACTIVE_CATEGORY,
            };
            if new_category == channel.category_id.unwrap() {
                continue;
            }

            if let Err(why) = channel.edit(ctx, |edit_channel| edit_channel.category(new_category))
            {
                eprintln!("Couldn't edit channel: {:?}", why);
                return;
            }
        }
        drop(archived_lock);
    }

    fn sort_channels(ctx: &Context, channels: &mut HashMap<ChannelId, GuildChannel>) {
        for category in &[ACTIVE_A_THRU_M, ACTIVE_N_THRU_Z, INACTIVE_CATEGORY] {
            let mut channels = channels
                .iter_mut()
                .filter_map(|(_id, guild_channel)| match guild_channel.category_id {
                    Some(cat) if &cat == category => Some(guild_channel),
                    _ => None,
                })
                .collect::<Vec<_>>();
            channels.sort_by_key(|channel| channel.position);
            let old_positions = channels
                .iter()
                .enumerate()
                .map(|(idx, channel)| (channel.id, idx))
                .collect::<HashMap<_, _>>();
            channels.sort_by_key(|channel| channel.name.clone());
            if let Some(pos) = channels
                .iter()
                .position(|channel| channel.id == STICKY_CHANNEL)
            {
                channels.swap(pos, 0);
            }
            if let Err(why) = channels
                .into_iter()
                .enumerate()
                .filter(|(idx, channel)| {
                    let old_position = *old_positions.get(&channel.id).unwrap();
                    let new_position = *idx;
                    old_position != new_position
                })
                .try_for_each(|(new_position, channel)| -> serenity::Result<()> {
                    channel.edit(ctx, |edit_channel| {
                        edit_channel.position(new_position as u64)
                    })?;
                    Ok(())
                })
            {
                eprintln!("Couldn't edit channel: {:?}", why);
                return;
            }
        }
    }

    fn tick(&self, ctx: &Context) {
        let mut channels = match GUILD.channels(&ctx) {
            Ok(channels) => channels,
            Err(why) => {
                eprintln!("Couldn't get channels: {:?}", why);
                return;
            }
        };
        self.archive_channels(ctx, &mut channels);
        Self::sort_channels(ctx, &mut channels);
    }
}

impl EventHandler for Handler {
    fn channel_create(&self, ctx: Context, _channel: Arc<RwLock<GuildChannel>>) {
        self.tick(&ctx);
    }

    fn channel_delete(&self, ctx: Context, _channel: Arc<RwLock<GuildChannel>>) {
        self.tick(&ctx);
    }

    fn message(&self, ctx: Context, _msg: Message) {
        self.tick(&ctx);
    }
}

fn update_file(archived: &[(ChannelId, DateTime<FixedOffset>)]) {
    let mut out = Vec::with_capacity(20 * archived.len());
    for (id, timestamp) in archived {
        out.extend_from_slice(&id.0.to_le_bytes());
        out.extend_from_slice(&timestamp.timestamp().to_le_bytes());
        out.extend_from_slice(&timestamp.offset().local_minus_utc().to_le_bytes());
    }
    if let Err(why) = fs::write(FILE, out) {
        eprintln!("Couldn't update archived channel file: {:?}", why);
        return;
    };
}

fn decode_file() -> Mutex<Vec<(ChannelId, DateTime<FixedOffset>)>> {
    fs::read(FILE)
        .map(|bytes| {
            let mut result = Vec::with_capacity(bytes.len() / 20);
            let mut slice = &bytes[..];
            while !slice.is_empty() {
                result.push((
                    ChannelId(u64::from_le_bytes((&slice[..8]).try_into().unwrap())),
                    DateTime::<FixedOffset>::from_utc(
                        NaiveDateTime::from_timestamp(
                            i64::from_le_bytes((&slice[8..16]).try_into().unwrap()),
                            0, //nanoseconds to make Unix timestamp more precise, not needed
                        ),
                        FixedOffset::east(i32::from_le_bytes((&slice[16..20]).try_into().unwrap())),
                    ),
                ));
                slice = &slice[20..];
            }
            result
        })
        .unwrap_or_default()
        .into()
}
