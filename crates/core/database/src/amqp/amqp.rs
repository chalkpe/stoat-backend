use std::collections::HashSet;

use crate::events::rabbit::*;
use crate::User;
use amqprs::channel::BasicPublishArguments;
use amqprs::{channel::Channel, connection::Connection, error::Error as AMQPError};
use amqprs::{BasicProperties, FieldTable};
use revolt_models::v0::PushNotification;

use log::{debug, info, warn};
use serde_json::to_string;

/// Filter out users who are currently viewing the channel
async fn filter_viewers(recipients: &[String], channel_id: &str) -> HashSet<String> {
    use redis_kiss::{get_connection, AsyncCommands};

    let mut viewer_ids = HashSet::new();

    // Get Redis connection
    let Ok(mut conn) = get_connection().await else {
        warn!("Failed to get Redis connection for filtering viewers");
        return viewer_ids;
    };

    for user_id in recipients {
        let session_pattern = format!("open_channels:{}:*", user_id);

        // Get all session keys for this user
        let Ok(keys): Result<Vec<String>, _> = conn.keys(&session_pattern).await else {
            debug!("No session keys found for user {}", user_id);
            continue;
        };

        // Check if any session has this channel open
        for key in keys {
            let Ok(members): Result<HashSet<String>, _> = conn.smembers(&key).await else {
                debug!("Failed to get members for key {}", key);
                continue;
            };

            if members.contains(channel_id) {
                debug!(
                    "User {} is currently viewing channel {}",
                    user_id, channel_id
                );
                viewer_ids.insert(user_id.clone());
                break;
            }
        }
    }

    debug!("Filtered viewer IDs: {:?}", viewer_ids);

    viewer_ids
}

#[derive(Clone)]
pub struct AMQP {
    #[allow(unused)]
    connection: Connection,
    channel: Channel,
}

impl AMQP {
    pub fn new(connection: Connection, channel: Channel) -> AMQP {
        AMQP {
            connection,
            channel,
        }
    }

    pub async fn friend_request_accepted(
        &self,
        accepted_request_user: &User,
        sent_request_user: &User,
    ) -> Result<(), AMQPError> {
        let config = revolt_config::config().await;
        let payload = FRAcceptedPayload {
            accepted_user: accepted_request_user.to_owned(),
            user: sent_request_user.id.clone(),
        };
        let payload = to_string(&payload).unwrap();

        debug!(
            "Sending friend request accept payload on channel {}: {}",
            config.pushd.get_fr_accepted_routing_key(),
            payload
        );
        self.channel
            .basic_publish(
                BasicProperties::default()
                    .with_content_type("application/json")
                    .with_persistence(true)
                    .finish(),
                payload.into(),
                BasicPublishArguments::new(
                    &config.pushd.exchange,
                    &config.pushd.get_fr_accepted_routing_key(),
                ),
            )
            .await
    }

    pub async fn friend_request_received(
        &self,
        received_request_user: &User,
        sent_request_user: &User,
    ) -> Result<(), AMQPError> {
        let config = revolt_config::config().await;
        let payload = FRReceivedPayload {
            from_user: sent_request_user.to_owned(),
            user: received_request_user.id.clone(),
        };
        let payload = to_string(&payload).unwrap();

        debug!(
            "Sending friend request received payload on channel {}: {}",
            config.pushd.get_fr_received_routing_key(),
            payload
        );

        self.channel
            .basic_publish(
                BasicProperties::default()
                    .with_content_type("application/json")
                    .with_persistence(true)
                    .finish(),
                payload.into(),
                BasicPublishArguments::new(
                    &config.pushd.exchange,
                    &config.pushd.get_fr_received_routing_key(),
                ),
            )
            .await
    }

    pub async fn generic_message(
        &self,
        user: &User,
        title: String,
        body: String,
        icon: Option<String>,
    ) -> Result<(), AMQPError> {
        let config = revolt_config::config().await;
        let payload = GenericPayload {
            title,
            body,
            icon,
            user: user.to_owned(),
        };
        let payload = to_string(&payload).unwrap();

        debug!(
            "Sending generic payload on channel {}: {}",
            config.pushd.get_generic_routing_key(),
            payload
        );

        self.channel
            .basic_publish(
                BasicProperties::default()
                    .with_content_type("application/json")
                    .with_persistence(true)
                    .finish(),
                payload.into(),
                BasicPublishArguments::new(
                    &config.pushd.exchange,
                    &config.pushd.get_generic_routing_key(),
                ),
            )
            .await
    }

    pub async fn message_sent(
        &self,
        recipients: Vec<String>,
        mut payload: PushNotification,
    ) -> Result<(), AMQPError> {
        if recipients.is_empty() {
            return Ok(());
        }

        let config = revolt_config::config().await;
        let channel_id = payload.channel.id().to_string();

        // Spoiler handling
        if (payload.body.contains("[[") || payload.body.contains("\\[\\["))
            && (payload.body.contains("]]") || payload.body.contains("\\]\\]"))
        {
            payload.body = "(스포일러)".to_string();
        }
        if let Some(ref content) = payload.message.content {
            if (content.contains("[[") || content.contains("\\[\\["))
                && (content.contains("]]") || content.contains("\\]\\]"))
            {
                payload.message.content = Some("(스포일러)".to_string());
            }
        }

        let payload = MessageSentPayload {
            notification: payload,
            users: recipients.clone(),
        };
        let payload = to_string(&payload).unwrap();

        // Filter out users who are currently viewing the channel
        let viewer_ids = filter_viewers(&recipients, &channel_id).await;
        let recipients = (&recipients.into_iter().collect::<HashSet<String>>() - &viewer_ids)
            .into_iter()
            .collect::<Vec<String>>();

        // If all recipients are viewing the channel, don't send notifications
        if recipients.is_empty() {
            debug!(
                "Everyone is viewing channel {}, not sending notification: {}",
                config.pushd.get_message_routing_key(),
                payload
            );
            return Ok(());
        }

        debug!(
            "Sending message payload on channel {}: {}",
            config.pushd.get_message_routing_key(),
            payload
        );

        self.channel
            .basic_publish(
                BasicProperties::default()
                    .with_content_type("application/json")
                    .with_persistence(true)
                    .finish(),
                payload.into(),
                BasicPublishArguments::new(
                    &config.pushd.exchange,
                    &config.pushd.get_message_routing_key(),
                ),
            )
            .await
    }

    pub async fn mass_mention_message_sent(
        &self,
        server_id: String,
        payload: Vec<PushNotification>,
    ) -> Result<(), AMQPError> {
        let config = revolt_config::config().await;

        let payload = MassMessageSentPayload {
            notifications: payload,
            server_id,
        };
        let payload = to_string(&payload).unwrap();

        let routing_key = config.pushd.get_mass_mention_routing_key();

        debug!(
            "Sending mass mention payload on channel {}: {}",
            routing_key, payload
        );

        self.channel
            .basic_publish(
                BasicProperties::default()
                    .with_content_type("application/json")
                    .with_persistence(true)
                    .finish(),
                payload.into(),
                BasicPublishArguments::new(&config.pushd.exchange, routing_key.as_str()),
            )
            .await
    }

    pub async fn ack_message(
        &self,
        user_id: String,
        channel_id: String,
        message_id: String,
    ) -> Result<(), AMQPError> {
        let config = revolt_config::config().await;

        let payload = AckPayload {
            user_id: user_id.clone(),
            channel_id: channel_id.clone(),
            message_id,
        };
        let payload = to_string(&payload).unwrap();

        info!(
            "Sending ack payload on channel {}: {}",
            config.pushd.ack_queue, payload
        );

        let mut headers = FieldTable::new();
        headers.insert(
            "x-deduplication-header".try_into().unwrap(),
            format!("{}-{}", &user_id, &channel_id).into(),
        );

        self.channel
            .basic_publish(
                BasicProperties::default()
                    .with_content_type("application/json")
                    .with_persistence(true)
                    //.with_headers(headers)
                    .finish(),
                payload.into(),
                BasicPublishArguments::new(&config.pushd.exchange, &config.pushd.ack_queue),
            )
            .await
    }
}
