use revolt_database::{
    util::{permissions::DatabasePermissionQuery, reference::Reference},
    Database, MessageFilter, MessageQuery, MessageTimePeriod, User,
};
use revolt_models::v0::{self, BulkAttachmentsResponse, MessageSort};
use revolt_permissions::{calculate_channel_permissions, ChannelPermission};
use revolt_result::{create_error, Result};
use rocket::{serde::json::Json, State};
use validator::Validate;

/// # Fetch Attachments
///
/// Fetch attachments uploaded to a channel.
#[openapi(tag = "Messaging")]
#[get("/<target>/attachments?<options..>")]
pub async fn query(
    db: &State<Database>,
    user: User,
    target: Reference<'_>,
    options: v0::OptionsQueryAttachments,
) -> Result<Json<BulkAttachmentsResponse>> {
    options.validate().map_err(|error| {
        create_error!(FailedValidation {
            error: error.to_string()
        })
    })?;

    let channel = target.as_channel(db).await?;

    let mut query = DatabasePermissionQuery::new(db, &user).channel(&channel);
    calculate_channel_permissions(&mut query)
        .await
        .throw_if_lacking_channel_permission(ChannelPermission::ReadMessageHistory)?;

    let v0::OptionsQueryAttachments {
        limit,
        before,
        after,
    } = options;

    // Fetch messages with attachments, paginated by message ID
    let messages = db
        .fetch_messages(MessageQuery {
            filter: MessageFilter {
                channel: Some(channel.id().to_string()),
                has_attachments: Some(true),
                ..Default::default()
            },
            time_period: MessageTimePeriod::Absolute {
                before,
                after,
                sort: Some(MessageSort::Latest),
            },
            limit,
        })
        .await?;

    // Flatten attachments from messages, setting message_id on each
    let attachments = messages
        .into_iter()
        .flat_map(|msg| {
            let message_id = msg.id.clone();
            msg.attachments
                .unwrap_or_default()
                .into_iter()
                .map(move |mut file| {
                    file.message_id = Some(message_id.clone());
                    file.into()
                })
        })
        .collect();

    Ok(Json(BulkAttachmentsResponse { attachments }))
}
