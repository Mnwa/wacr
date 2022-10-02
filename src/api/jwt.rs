use actix_web::dev::RequestHead;
use actix_web::guard::Guard;
use actix_web::{post, web, HttpResponse, Responder};
use chrono::{DateTime, NaiveDateTime, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use log::error;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::str::FromStr;

const BEARER: &str = "Bearer ";

pub fn jwt_token_guard(service_token: String) -> impl Guard {
    actix_web::guard::fn_guard(move |ctx| {
        extract_access_token(ctx.head())
            .and_then(|token| {
                decode::<Claims>(
                    &token,
                    &DecodingKey::from_secret(service_token.as_bytes()),
                    &Validation::default(),
                )
                .ok()
            })
            .filter(|v| {
                let dt =
                    DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(v.claims.exp, 0), Utc);

                dt >= Utc::now()
            })
            .map(|v| {
                ctx.req_data_mut()
                    .insert(UserId(OwnerId::from_str(v.claims.id.as_str()).unwrap()));
            })
            .is_some()
    })
}

#[post("/token/generate")]
pub async fn generate_vk_jwt_method(
    secure: web::Data<JwtConfig>,
    payload: web::Json<GetVkJwtTokenRequest>,
) -> impl Responder {
    let (user_id, timestamp) =
        match extract_user_id_from_vk_query(payload.query.as_str(), secure.service_key.as_str()) {
            Ok(r) => r,
            Err(e) => {
                return HttpResponse::BadRequest().json(JwtTokenBadResponse {
                    error: e.to_string(),
                })
            }
        };

    let expiration = timestamp + secure.expiration;

    let claims = Claims {
        id: user_id.0.to_string(),
        exp: expiration,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secure.service_key.as_bytes()),
    );
    match token {
        Ok(token) => HttpResponse::Ok().json(JwtTokenResponse { token, expiration }),
        Err(e) => HttpResponse::BadRequest().json(JwtTokenBadResponse {
            error: e.to_string(),
        }),
    }
}

fn extract_access_token(head: &RequestHead) -> Option<String> {
    extract_access_token_from_header(head).or_else(|| extract_access_token_from_query(head))
}

fn extract_access_token_from_query(head: &RequestHead) -> Option<String> {
    extract_any_data_from_query::<AccessTokenQuery>(head).map(|a| a.access_token)
}

fn extract_any_data_from_query<T: DeserializeOwned>(head: &RequestHead) -> Option<T> {
    head.uri.query().and_then(|v| {
        serde_urlencoded::from_str(v)
            .map_err(|e| {
                error!("extracting sequence error: {}", e);
                e
            })
            .ok()
    })
}

fn extract_access_token_from_header(head: &RequestHead) -> Option<String> {
    head.headers.get("Authorization").and_then(|head| {
        let token = head
            .to_str()
            .ok()
            .filter(|t| t.starts_with(BEARER))?
            .trim_start_matches(BEARER);
        Some(token.to_string())
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccessTokenQuery {
    pub access_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    id: String,
    exp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtTokenResponse {
    token: String,
    expiration: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtTokenBadResponse {
    error: String,
}

#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub service_key: String,
    pub expiration: i64,
}

pub type OwnerId = i64;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct UserId(pub OwnerId);

#[derive(Debug, Serialize, Deserialize)]
pub struct GetVkJwtTokenRequest {
    query: String,
}

fn extract_user_id_from_vk_query(
    query: &str,
    service_token: &str,
) -> Result<(UserId, i64), actix_web::error::Error> {
    let parsed_params: BTreeMap<&str, &str> = serde_urlencoded::from_str(query)?;

    let sign = parsed_params
        .get("sign")
        .ok_or_else(|| actix_web::error::ErrorForbidden("empty sign"))?;

    let cleared_params = parsed_params
        .iter()
        .filter(|(k, _)| k.starts_with("vk_"))
        .collect::<BTreeMap<_, _>>();

    let cleared_query = serde_urlencoded::to_string(cleared_params)?;

    let mut mac: Hmac<Sha256> = Hmac::new_from_slice(service_token.as_bytes())
        .map_err(actix_web::error::ErrorInternalServerError)?;
    mac.update(cleared_query.as_bytes());

    let result = mac.finalize();

    let generated_sign = base64::encode(result.into_bytes())
        .replace('+', "-")
        .replace('/', "_");

    let generated_sign = generated_sign.trim_end_matches('=');

    if &generated_sign != sign {
        return Err(actix_web::error::ErrorForbidden("invalid sign"));
    }

    let user_id = parsed_params
        .get("vk_user_id")
        .ok_or_else(|| actix_web::error::ErrorForbidden("empty vk id"))
        .and_then(|id| {
            id.parse::<OwnerId>()
                .map_err(actix_web::error::ErrorForbidden)
        })
        .map(UserId)?;

    let timestamp = parsed_params
        .get("vk_ts")
        .ok_or_else(|| actix_web::error::ErrorForbidden("empty timestamp"))
        .and_then(|id| id.parse::<i64>().map_err(actix_web::error::ErrorForbidden))?;

    Ok((user_id, timestamp))
}

#[cfg(test)]
mod tests {
    use crate::api::jwt::{extract_user_id_from_vk_query, UserId};

    #[test]
    fn extract_vk_uid() {
        let (uid, timestamp) = extract_user_id_from_vk_query(
            "vk_access_token_settings=&vk_app_id=8040721&vk_are_notifications_enabled=0&vk_is_app_user=1&vk_is_favorite=0&vk_language=ru&vk_platform=desktop_web&vk_ref=other&vk_ts=1641048381&vk_user_id=277790772&sign=r-I95gw8ot4RK0NkhEiORPDhFI3p0NylEbk2CPr2ZS8", 
            "MatyOmcbNc78YsfEzOdB"
        ).unwrap();

        assert_eq!(uid, UserId(277790772));
        assert_eq!(timestamp, 1641048381);
    }
}
