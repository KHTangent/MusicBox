use serenity::framework::standard::macros::command;
use serenity::framework::standard::CommandResult;
use serenity::model::prelude::*;
use serenity::prelude::*;

#[command]
pub async fn hello(ctx: &Context, msg: &Message) -> CommandResult {
	msg.channel_id
		.say(&ctx.http, format!("Hallo {}", msg.author.name))
		.await?;
	Ok(())
}
