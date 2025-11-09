use authifier::models::Session;
use revolt_database::{
    util::{permissions::DatabasePermissionQuery, reference::Reference},
    Database, User,
};
use revolt_permissions::{calculate_channel_permissions, ChannelPermission};
use revolt_result::{create_error, Result};
use revolt_rocket_okapi::{openapi, revolt_okapi::schemars::JsonSchema};
use rocket::{serde::json::Json, State};
use rocket_empty::EmptyResponse;
use serde::{Deserialize, Serialize};

/// Request body for channel activity
#[derive(Deserialize, JsonSchema)]
pub struct ChannelActivityRequest {
    /// Type of activity: 'open' to mark channel as open, 'close' to mark as closed
    #[serde(rename = "type")]
    pub activity_type: ChannelActivityType,
}

/// Type of channel activity
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChannelActivityType {
    Open,
    Close,
}

/// # Update Channel Activity
///
/// Mark a channel as opened or closed by the user.
#[openapi(tag = "Channel Information")]
#[put("/<target>", data = "<data>")]
pub async fn update_activity(
    db: &State<Database>,
    user: User,
    session: Session,
    target: Reference<'_>,
    data: Json<ChannelActivityRequest>,
) -> Result<EmptyResponse> {
    if user.bot.is_some() {
        return Err(create_error!(IsBot));
    }

    let channel = target.as_channel(db).await?;
    let mut query = DatabasePermissionQuery::new(db, &user).channel(&channel);
    calculate_channel_permissions(&mut query)
        .await
        .throw_if_lacking_channel_permission(ChannelPermission::ViewChannel)?;

    // Update channel activity in Redis
    update_channel_activity_in_redis(&user.id, &session.id, channel.id(), &data.activity_type)
        .await?;

    Ok(EmptyResponse)
}

/// Update channel activity status in Redis
async fn update_channel_activity_in_redis(
    user_id: &str,
    session_id: &str,
    channel_id: &str,
    activity_type: &ChannelActivityType,
) -> Result<()> {
    use redis_kiss::{get_connection, AsyncCommands};

    let mut conn = get_connection()
        .await
        .map_err(|_| create_error!(InternalError))?;

    let session_key = format!("open_channels:{}:{}", user_id, session_id);

    match activity_type {
        ChannelActivityType::Open => {
            // Add channel to open channels set
            let _: () = conn
                .sadd(&session_key, channel_id)
                .await
                .map_err(|_| create_error!(InternalError))?;

            // Set TTL for the session key (5 minutes)
            let _: () = conn
                .expire(&session_key, 300)
                .await
                .map_err(|_| create_error!(InternalError))?;
        }
        ChannelActivityType::Close => {
            // Remove channel from the set
            let _: () = conn
                .srem(&session_key, channel_id)
                .await
                .map_err(|_| create_error!(InternalError))?;
        }
    }

    Ok(())
}
