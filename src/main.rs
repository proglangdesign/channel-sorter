use chrono::{offset::Utc, DateTime, Duration};
use serenity::{
    model::{
        channel::Message,
        id::{ChannelId, GuildId},
    },
    prelude::*,
};
use std::convert::TryInto;

//token is in gitignore so that it doesn't get leaked
const TOKEN: &str = include_str!("bot-token.txt");
const ACTIVE_CATEGORY: ChannelId = ChannelId(530604963911696404);
const INACTIVE_CATEGORY: ChannelId = ChannelId(541808219593506827);
const GUILD: GuildId = GuildId(530598289813536771);

fn main() {
    let mut client = loop {
        match Client::new(TOKEN, Handler) {
            Ok(client) => break client,
            Err(why) => eprintln!("Client creation error: {:?}", why),
        }
    };

    while let Err(why) = client.start() {
        eprintln!("Client error: {:?}", why);
    }
}

struct Handler;

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
        'channel_loop: for channel in relevant_channels {
            //no more than 100 messages is allowed
            let messages = match channel.messages(&ctx, |get_messages| get_messages.limit(100)) {
                Ok(messages) => messages,
                Err(_) => {
                    //we just skip this channel if we can't access it
                    continue 'channel_loop;
                }
            };
            let last_message = messages.iter().find(|message| message.webhook_id == None);
            let new_category = match &last_message {
                Some(message)
                    if {
                        let timestamp: DateTime<Utc> =
                            message.edited_timestamp.unwrap_or(message.timestamp).into();
                        Utc::now() - timestamp < Duration::days(30 * 2)
                    } =>
                {
                    ACTIVE_CATEGORY
                }
                //empty channels get ignored
                None if messages.is_empty() => continue 'channel_loop,
                _ => INACTIVE_CATEGORY,
            };
            if new_category == channel.category_id.unwrap() {
                continue;
            }
            let new_position = names_and_positions
                .iter()
                .fold(&(String::from(""), 0), |cur, msg| {
                    if msg.0 >= channel.name && msg.0 < cur.0 {
                        msg
                    } else {
                        cur
                    }
                })
                .1
                .try_into()
                .unwrap();
            let _ = channel.edit(&ctx, |edit_channel| {
                edit_channel.category(new_category).position(new_position)
            });
        }
    }
}
