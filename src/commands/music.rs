use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::http::Http;
use serenity::model::prelude::*;
use serenity::{async_trait, prelude::*};
use songbird::input::{Input, Restartable};
use songbird::{Call, Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent};
use std::sync::Arc;

struct TrackEndNotifier {
	channel_id: ChannelId,
	http: Arc<Http>,
}

#[async_trait]
impl VoiceEventHandler for TrackEndNotifier {
	async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
		if let EventContext::Track(&[(_track_state, track_handle)]) = ctx {
			let title = track_handle
				.metadata()
				.title
				.clone()
				.unwrap_or("Unknown video".to_string());
			self.channel_id
				.say(&self.http, format!("Finished playing track **{}**", title))
				.await
				.ok();
		}
		None
	}
}

#[command]
#[only_in(guilds)]
#[description("Play an URL")]
pub async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	let url = match args.single_quoted::<String>() {
		Ok(u) => u,
		Err(_) => {
			msg.channel_id
				.say(&ctx.http, "Please provide an URL")
				.await?;
			return Ok(());
		}
	};
	if !url.starts_with("https://youtu") && !url.starts_with("https://www.youtu") {
		msg.channel_id.say(&ctx.http, "Invalid YouTube URL").await?;
		return Ok(());
	}
	if let Err(e) = ensure_voice_connected(ctx, msg).await {
		msg.channel_id.say(&ctx.http, e).await?;
		return Ok(());
	}
	let handler_lock = match get_voice_handler(ctx, msg).await {
		Ok(hl) => hl,
		Err(e) => {
			msg.channel_id.say(&ctx.http, e).await?;
			return Ok(());
		}
	};
	let mut handler = handler_lock.lock().await;

	let source: Input = match Restartable::ytdl(url, true).await {
		Ok(s) => s.into(),
		Err(e) => {
			println!("Error fetching YTDL: {}", e);
			msg.channel_id.say(&ctx.http, "Error fetching URL").await?;
			return Ok(());
		}
	};
	let title = source
		.metadata
		.title
		.clone()
		.unwrap_or("Unknown video".to_string());
	let stream_handler = handler.play_only_source(source);
	if let Err(e) = stream_handler.add_event(
		Event::Track(TrackEvent::End),
		TrackEndNotifier {
			channel_id: msg.channel_id,
			http: Arc::clone(&ctx.http),
		},
	) {
		println!("Error adding event: {}", e);
		msg.channel_id.say(&ctx.http, "Internal error").await?;
		return Ok(());
	}
	msg.channel_id
		.say(&ctx.http, format!("Playing **{}**", &title))
		.await?;
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
			msg.channel_id
				.say(&ctx.http, "Failed to leave channel")
				.await?;
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
