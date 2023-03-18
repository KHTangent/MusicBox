use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::http::Http;
use serenity::model::prelude::*;
use serenity::{async_trait, prelude::*};
use songbird::input::{Input, Restartable};
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler, Songbird, TrackEvent};
use std::sync::Arc;

struct TrackStartNotifier {
	channel_id: ChannelId,
	http: Arc<Http>,
}

#[async_trait]
impl VoiceEventHandler for TrackStartNotifier {
	async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
		if let EventContext::Track(&[(_track_state, track_handle)]) = ctx {
			let title = track_handle
				.metadata()
				.title
				.clone()
				.unwrap_or("Unknown video".to_string());
			self.channel_id
				.say(&self.http, format!("Now playing **{}**", title))
				.await
				.ok();
		}
		None
	}
}

struct TrackEndNotifier {
	channel_id: ChannelId,
	guild_id: GuildId,
	manager: Arc<Songbird>,
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
			let drop_call = match self.manager.get(self.guild_id) {
				Some(call) => {
					let mut handler = call.lock().await;
					if handler.queue().len() == 0 {
						match handler.leave().await {
							Ok(_) => true,
							Err(e) => {
								println!("Error leaving channel: {}", e);
								false
							}
						}
					} else {
						false
					}
				}
				None => false,
			};
			if drop_call {
				self.manager.remove(self.guild_id).await.ok();
			}
		}
		None
	}
}

#[command]
#[only_in(guilds)]
#[description("Play an URL")]
pub async fn play(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
	let query = args.message().to_string();
	let is_url = query.starts_with("https://youtu") || query.starts_with("https://www.youtu");
	let source: Input;
	if is_url {
		source = match Restartable::ytdl(query, true).await {
			Ok(s) => s.into(),
			Err(e) => {
				println!("Error fetching YTDL using URL: {}", e);
				msg.channel_id.say(&ctx.http, "Error fetching URL").await?;
				return Ok(());
			}
		};
	} else {
		source = match Restartable::ytdl_search(query, true).await {
			Ok(s) => s.into(),
			Err(e) => {
				println!("Error searching: {}", e);
				msg.channel_id.say(&ctx.http, "Error getting video").await?;
				return Ok(());
			}
		}
	}
	if let Err(e) = ensure_voice_connected(ctx, msg).await {
		msg.channel_id.say(&ctx.http, e).await?;
		return Ok(());
	}
	let songbird_manager = songbird::get(ctx)
		.await
		.expect("Songbird Voice client is missing")
		.clone();
	let handler_lock = match songbird_manager.get(msg.guild_id.unwrap()) {
		Some(h) => h,
		None => {
			msg.channel_id
				.say(&ctx.http, "Not in a voice channel")
				.await?;
			return Ok(());
		}
	};
	let mut handler = handler_lock.lock().await;
	let title = source
		.metadata
		.title
		.clone()
		.unwrap_or("Unknown video".to_string());
	if handler.queue().len() == 0 {
		// Others will be announced by TrackStartNotifier
		msg.channel_id
			.say(&ctx.http, format!("Playing **{}**", &title))
			.await?;
	} else {
		msg.channel_id
			.say(&ctx.http, format!("Queueing **{}**", &title))
			.await?;
	}
	let stream_handler = handler.enqueue_source(source);

	if let Err(e) = stream_handler.add_event(
		Event::Track(TrackEvent::Play),
		TrackStartNotifier {
			channel_id: msg.channel_id,
			http: Arc::clone(&ctx.http),
		},
	) {
		println!("Error adding event: {}", e);
		msg.channel_id.say(&ctx.http, "Internal error").await?;
		return Ok(());
	}
	if let Err(e) = stream_handler.add_event(
		Event::Track(TrackEvent::End),
		TrackEndNotifier {
			channel_id: msg.channel_id,
			http: Arc::clone(&ctx.http),
			guild_id: msg.guild_id.unwrap(),
			manager: songbird_manager.clone(),
		},
	) {
		println!("Error adding event: {}", e);
		msg.channel_id.say(&ctx.http, "Internal error").await?;
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
			msg.channel_id
				.say(&ctx.http, "Failed to leave channel")
				.await?;
		}
	}
	Ok(())
}

#[command]
#[only_in(guilds)]
#[aliases("np", "now")]
#[description("Display currently playing track")]
pub async fn now_playing(ctx: &Context, msg: &Message) -> CommandResult {
	let guild = msg.guild(&ctx.cache).ok_or("Failed to retrieve guild")?;
	let songbird_manager = songbird::get(ctx)
		.await
		.ok_or("Internal error".to_string())?
		.clone();
	let handler_lock = match songbird_manager.get(guild.id) {
		Some(h) => h,
		None => {
			msg.channel_id
				.say(&ctx.http, "Not in a voice channel")
				.await?;
			return Ok(());
		}
	};
	let handler = handler_lock.lock().await;
	match handler.queue().current() {
		Some(track) => {
			let title = track
				.metadata()
				.title
				.clone()
				.unwrap_or("Unknown video".to_string());
			let duration = track
				.metadata()
				.duration
				.and_then(|d| {
					let secs = d.as_secs();
					let minutes = secs / 60;
					let seconds = secs % 60;
					Some(format!("{}:{:0>2}", minutes, seconds))
				})
				.unwrap_or("??:??".to_string());
			let progress = track
				.get_info()
				.await
				.and_then(|ts| {
					let secs = ts.position.as_secs();
					let minutes = secs / 60;
					let seconds = secs % 60;
					Ok(format!("{}:{:0>2}", minutes, seconds))
				})
				.unwrap_or("??:??".to_string());
			msg.channel_id
				.say(
					&ctx.http,
					format!(
						"Currently playing **{}**\nProgress: {} / {}",
						title, progress, duration
					),
				)
				.await?;
		}
		None => {
			msg.channel_id
				.say(&ctx.http, "Nothing is currently playing")
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
		.expect("Songbird Voice client is missing")
		.clone();
	let _handler = songbird_manager.join(guild.id, channel_id).await;
	Ok(())
}
