use chrono::{offset::Utc, DateTime, Duration};
use serenity::{
    model::{
        channel::Message,
        id::{ChannelId, GuildId},
    },
    prelude::*,
};

struct Handler;

impl EventHandler for Handler {
    fn message(&self, ctx: Context, _msg: Message) {
        const ACTIVE_CATEGORY_ID: u64 = 622797326553186365;
        const INACTIVE_CATEGORY_ID: u64 = 622797326553186367;
        let guild: GuildId = GuildId(622797326553186364);
        let mut channels = guild.channels(&ctx).expect("Err getting channels");
        let relevant_channels = channels.iter_mut().filter_map(|(_id, guild_channel)| {
            match guild_channel.category_id {
                Some(id) if id == ACTIVE_CATEGORY_ID || id == INACTIVE_CATEGORY_ID => {
                    Some(guild_channel)
                }
                _ => None,
            }
        });
        for (cnt, channel) in relevant_channels.enumerate() {
            println!("{}", cnt);
            let new_category_id = match channel
                .messages(&ctx, |get_messages| get_messages.limit(1))
                .expect("Err getting latest message in channel, even if it didn't exist")
                .get(0)
            {
                Some(message)
                    if {
                        let timestamp_utc: DateTime<Utc> =
                            message.edited_timestamp.unwrap_or(message.timestamp).into();
                        Utc::now() - timestamp_utc < Duration::seconds(10) //days(30 * 2)
                    } =>
                {
                    ACTIVE_CATEGORY_ID
                }
                _ => INACTIVE_CATEGORY_ID,
            };
            if new_category_id == channel.category_id.unwrap_or(channel.id).0 {
                continue;
            }
            let _ = channel.edit(&ctx, |edit_channel| {
                edit_channel.category(ChannelId(new_category_id))
            });
        }
    }
}

fn main() {
    //token is in gitignore so that it doesn't get leaked
    const TOKEN: &str = include_str!("bot-token.txt");

    let mut client = Client::new(TOKEN, Handler).expect("Err creating client");

    if let Err(why) = client.start() {
        println!("Client error: {:?}", why);
    }
}
