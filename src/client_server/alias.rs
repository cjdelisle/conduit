use crate::{database::DatabaseGuard, Database, Error, Result, Ruma};
use regex::Regex;
use ruma::{
    api::{
        appservice,
        client::{
            alias::{create_alias, delete_alias, get_alias},
            error::ErrorKind,
        },
        federation,
    },
    RoomAliasId,
};

/// # `PUT /_matrix/client/r0/directory/room/{roomAlias}`
///
/// Creates a new room alias on this server.
pub async fn create_alias_route(
    db: DatabaseGuard,
    body: Ruma<create_alias::v3::Request<'_>>,
) -> Result<create_alias::v3::Response> {
    if body.room_alias.server_name() != db.globals.server_name() {
        return Err(Error::BadRequest(
            ErrorKind::InvalidParam,
            "Alias is from another server.",
        ));
    }

    if db.rooms.id_from_alias(&body.room_alias)?.is_some() {
        return Err(Error::Conflict("Alias already exists."));
    }

    db.rooms
        .set_alias(&body.room_alias, Some(&body.room_id), &db.globals)?;

    db.flush()?;

    Ok(create_alias::v3::Response::new())
}

/// # `DELETE /_matrix/client/r0/directory/room/{roomAlias}`
///
/// Deletes a room alias from this server.
///
/// - TODO: additional access control checks
/// - TODO: Update canonical alias event
pub async fn delete_alias_route(
    db: DatabaseGuard,
    body: Ruma<delete_alias::v3::Request<'_>>,
) -> Result<delete_alias::v3::Response> {
    if body.room_alias.server_name() != db.globals.server_name() {
        return Err(Error::BadRequest(
            ErrorKind::InvalidParam,
            "Alias is from another server.",
        ));
    }

    db.rooms.set_alias(&body.room_alias, None, &db.globals)?;

    // TODO: update alt_aliases?

    db.flush()?;

    Ok(delete_alias::v3::Response::new())
}

/// # `GET /_matrix/client/r0/directory/room/{roomAlias}`
///
/// Resolve an alias locally or over federation.
///
/// - TODO: Suggest more servers to join via
pub async fn get_alias_route(
    db: DatabaseGuard,
    body: Ruma<get_alias::v3::Request<'_>>,
) -> Result<get_alias::v3::Response> {
    get_alias_helper(&db, &body.room_alias).await
}

pub(crate) async fn get_alias_helper(
    db: &Database,
    room_alias: &RoomAliasId,
) -> Result<get_alias::v3::Response> {
    if room_alias.server_name() != db.globals.server_name() {
        let response = db
            .sending
            .send_federation_request(
                &db.globals,
                room_alias.server_name(),
                federation::query::get_room_information::v1::Request { room_alias },
            )
            .await?;

        return Ok(get_alias::v3::Response::new(
            response.room_id,
            response.servers,
        ));
    }

    let mut room_id = None;
    match db.rooms.id_from_alias(room_alias)? {
        Some(r) => room_id = Some(r),
        None => {
            for (_id, registration) in db.appservice.all()? {
                let aliases = registration
                    .get("namespaces")
                    .and_then(|ns| ns.get("aliases"))
                    .and_then(|aliases| aliases.as_sequence())
                    .map_or_else(Vec::new, |aliases| {
                        aliases
                            .iter()
                            .filter_map(|aliases| Regex::new(aliases.get("regex")?.as_str()?).ok())
                            .collect::<Vec<_>>()
                    });

                if aliases
                    .iter()
                    .any(|aliases| aliases.is_match(room_alias.as_str()))
                    && db
                        .sending
                        .send_appservice_request(
                            &db.globals,
                            registration,
                            appservice::query::query_room_alias::v1::Request { room_alias },
                        )
                        .await
                        .is_ok()
                {
                    room_id = Some(db.rooms.id_from_alias(room_alias)?.ok_or_else(|| {
                        Error::bad_config("Appservice lied to us. Room does not exist.")
                    })?);
                    break;
                }
            }
        }
    };

    let room_id = match room_id {
        Some(room_id) => room_id,
        None => {
            return Err(Error::BadRequest(
                ErrorKind::NotFound,
                "Room with alias not found.",
            ))
        }
    };

    Ok(get_alias::v3::Response::new(
        room_id,
        vec![db.globals.server_name().to_owned()],
    ))
}
