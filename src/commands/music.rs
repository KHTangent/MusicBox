use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;

#[command]
#[only_in(guilds)]
#[description("Play an URL")]
pub async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	let url = match args.single_quoted::<String>() {
		Ok(u) => u,
		Err(_) => {
			msg.reply(&ctx.http, "Please provide an URL").await?;
			return Ok(());
		}
	};
	msg.reply(&ctx.http, format!("Playing \"{}\"...", &url))
		.await?;
	if let Err(e) = ensure_voice_connected(ctx, msg).await {
		msg.reply(&ctx.http, e).await?;
		return Ok(());
	}
	Ok(())
}

#[command]
#[only_in(guilds)]
#[description("Stop current playback")]
pub async fn stop(ctx: &Context, msg: &Message) -> CommandResult {
	let guild = msg.guild(&ctx.cache).ok_or("Failed to retrieve guild")?;
	let songbird_manager = songbird::get(ctx)
		.await
		.ok_or("Internal error".to_string())?
		.clone();
	if songbird_manager.get(guild.id).is_some() {
		if let Err(_) = songbird_manager.remove(guild.id).await {
			msg.reply(&ctx.http, "Failed to leave channel").await?;
		}
	}
	Ok(())
}

async fn ensure_voice_connected(ctx: &Context, msg: &Message) -> Result<(), String> {
	let guild = msg.guild(&ctx.cache).ok_or("Failed to retrieve guild")?;
	let channel_id = guild
		.voice_states
		.get(&msg.author.id)
		.and_then(|e| e.channel_id)
		.ok_or("You must be in a VC to use this command".to_string())?;
	let songbird_manager = songbird::get(ctx)
		.await
		.ok_or("Internal error".to_string())?
		.clone();
	let _handler = songbird_manager.join(guild.id, channel_id).await;
	Ok(())
}

/*
async fn get_voice_handler(ctx: &Context, msg: &Message) -> Result<Arc<Mutex<Call>>, String> {
	let guild = msg.guild(&ctx.cache).ok_or("Failed to retrieve guild")?;
	let songbird_manager = songbird::get(ctx)
		.await
		.ok_or("Internal error".to_string())?
		.clone();
	let handler_lock = songbird_manager
		.get(guild.id)
		.ok_or("Not in a voice channel")?;
	Ok(handler_lock)
}
 */
