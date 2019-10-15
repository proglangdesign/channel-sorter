use chrono::{offset::Utc, DateTime, Duration};
use serenity::{
    model::{
        channel::Message,
        id::{ChannelId, GuildId},
    },
    prelude::*,
};

//token is in gitignore so that it doesn't get leaked
const TOKEN: &str = include_str!("bot-token.txt");
const ACTIVE_CATEGORY: ChannelId = ChannelId(622797326553186365);
const INACTIVE_CATEGORY: ChannelId = ChannelId(622797326553186367);
const GUILD: GuildId = GuildId(622797326553186364);

fn main() {
    let mut client = Client::new(TOKEN, Handler).expect("Err creating client");

    if let Err(why) = client.start() {
        println!("Client error: {:?}", why);
    }
}

struct Handler;

impl EventHandler for Handler {
    fn message(&self, ctx: Context, _msg: Message) {
        let mut channels = GUILD.channels(&ctx).expect("Err getting channels");
        let relevant_channels = channels.iter_mut().filter_map(|(_id, guild_channel)| {
            match guild_channel.category_id {
                Some(category) if category == ACTIVE_CATEGORY || category == INACTIVE_CATEGORY => {
                    Some(guild_channel)
                }
                _ => None,
            }
        });
        for (cnt, channel) in relevant_channels.enumerate() {
            println!("{}", cnt);
            let new_category = match channel
                .messages(&ctx, |get_messages| get_messages.limit(1))
                .expect("Err getting latest message in channel, even if it didn't exist")
                .get(0)
            {
                Some(message)
                    if {
                        let timestamp_utc: DateTime<Utc> =
                            message.edited_timestamp.unwrap_or(message.timestamp).into();
                        Utc::now() - timestamp_utc < Duration::days(30 * 2)
                    } =>
                {
                    ACTIVE_CATEGORY
                }
                _ => INACTIVE_CATEGORY,
            };
            if new_category == channel.category_id.unwrap() {
                continue;
            }
            let _ = channel.edit(&ctx, |edit_channel| edit_channel.category(new_category));
        }
    }
}
