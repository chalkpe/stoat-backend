use authifier::{
    models::{Session, WebPushSubscription},
    Authifier,
};
use revolt_database::Database;
use revolt_result::{create_database_error, Result};
use rocket::{serde::json::Json, State};
use rocket_empty::EmptyResponse;

/// # Push Subscribe
///
/// Create a new Web Push subscription.
///
/// If an existing subscription exists on this session, it will be removed.
/// Also removes subscriptions from other sessions with the same FCM token.
#[openapi(tag = "Web Push")]
#[post("/subscribe", data = "<data>")]
pub async fn subscribe(
    authifier: &State<Authifier>,
    db: &State<Database>,
    mut session: Session,
    data: Json<WebPushSubscription>,
) -> Result<EmptyResponse> {
    let new_subscription = data.into_inner();

    // If this is an FCM subscription, remove the same token from other sessions
    if new_subscription.endpoint == "fcm" {
        if let Err(err) = db
            .remove_duplicate_fcm_subscriptions(&session.user_id, &new_subscription.auth)
            .await
        {
            revolt_config::capture_error(&err);
            // Don't fail, just log the error
        }
    }

    session.subscription = Some(new_subscription);
    session
        .save(authifier)
        .await
        .map(|_| EmptyResponse)
        .map_err(|_| create_database_error!("save", "session"))
}
