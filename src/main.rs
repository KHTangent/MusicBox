mod commands;

use dotenv;
use serenity::client::bridge::gateway::ShardManager;
use serenity::framework::standard::{
	help_commands, Args, CommandGroup, CommandResult, HelpOptions,
};
use serenity::model::prelude::{Message, UserId};
use std::collections::HashSet;
use std::env;
use std::sync::Arc;

use serenity::async_trait;
use serenity::framework::standard::macros::{group, help};
use serenity::framework::StandardFramework;
use serenity::model::gateway::Ready;
use serenity::prelude::*;

use crate::commands::hello::*;

pub struct ShardManagerContainer;
impl TypeMapKey for ShardManagerContainer {
	type Value = Arc<Mutex<ShardManager>>;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
	async fn ready(&self, _: Context, ready: Ready) {
		println!("Signed in as {}", ready.user.name);
	}
}

#[group]
#[commands(hello)]
struct General;

#[help]
async fn help_command(
	context: &Context,
	msg: &Message,
	args: Args,
	help_options: &'static HelpOptions,
	groups: &[&'static CommandGroup],
	owners: HashSet<UserId>,
) -> CommandResult {
	let _ = help_commands::with_embeds(context, msg, args, help_options, groups, owners).await;
	Ok(())
}

#[tokio::main]
async fn main() {
	dotenv::dotenv().ok();
	let token = env::var("TOKEN").expect("Set a token in the TOKEN env variable");
	let owner = u64::from_str_radix(
		&env::var("OWNER").expect("Set an owner with the OWNER env variable"),
		10,
	)
	.expect("OWNER must be a valid integer");
	let framework = StandardFramework::new()
		.configure(|c| {
			c.owners(vec![UserId(owner)].into_iter().collect())
				.prefix(".")
		})
		.group(&GENERAL_GROUP)
		.help(&HELP_COMMAND);

	let intents = GatewayIntents::GUILD_MESSAGES
		| GatewayIntents::DIRECT_MESSAGES
		| GatewayIntents::MESSAGE_CONTENT;
	let mut client = Client::builder(&token, intents)
		.framework(framework)
		.event_handler(Handler)
		.await
		.expect("Failed to create client");

	{
		let mut data = client.data.write().await;
		data.insert::<ShardManagerContainer>(client.shard_manager.clone());
	}
	let shard_manager = client.shard_manager.clone();

	tokio::spawn(async move {
		tokio::signal::ctrl_c()
			.await
			.expect("Failed to register Ctrl+C handler");
		shard_manager.lock().await.shutdown_all().await;
	});

	if let Err(err) = client.start().await {
		println!("Failed to start bot: {}", err);
	}
}
