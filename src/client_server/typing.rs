use crate::{database::DatabaseGuard, utils, Result, Ruma};
use ruma::api::client::typing::create_typing_event;

/// # `PUT /_matrix/client/r0/rooms/{roomId}/typing/{userId}`
///
/// Sets the typing state of the sender user.
pub async fn create_typing_event_route(
    db: DatabaseGuard,
    body: Ruma<create_typing_event::v3::Request<'_>>,
) -> Result<create_typing_event::v3::Response> {
    use create_typing_event::v3::Typing;

    let sender_user = body.sender_user.as_ref().expect("user is authenticated");

    if let Typing::Yes(duration) = body.state {
        db.rooms.edus.typing_add(
            sender_user,
            &body.room_id,
            duration.as_millis() as u64 + utils::millis_since_unix_epoch(),
            &db.globals,
        )?;
    } else {
        db.rooms
            .edus
            .typing_remove(sender_user, &body.room_id, &db.globals)?;
    }

    Ok(create_typing_event::v3::Response {})
}
