use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use url::Url;
use utoipa::ToSchema;
use uuid::Uuid;
use webauthn_rs::Webauthn;
use webauthn_rs::prelude::*;
use webauthn_rs_proto::{AllowCredentials, PublicKeyCredentialRequestOptions};

use crate::AppState;
use crate::entity::{passkeys, users};
use crate::handlers::{json_error, verification_satisfied};
use crate::utils::email::generate_token;
use crate::utils::email_domain_policy::{
    ensure_sign_in_domain_allowed, live_users_by_email, normalize_email,
};
use crate::utils::env::env_flag;
use crate::utils::jwt::create_jwt;
use crate::utils::time::utc_rfc3339;

// In-memory store for registration/auth state
// In production, use Redis or database with expiration
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

/// How long a pending registration/authentication challenge is kept before it expires.
const PASSKEY_STATE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// Ceiling on pending challenges held at once. Sign-in attempts are keyed by a
/// random id, so unlike the per-account keys they would otherwise grow with the
/// request rate; past the ceiling the oldest ceremony is dropped.
const PASSKEY_STATE_MAX_ENTRIES: usize = 10_000;

/// How long the browser keeps the WebAuthn prompt open. Every `Webauthn` is
/// built with it, and a decoy sign-in challenge is issued by the same call as a
/// real one, so the field cannot tell the two apart.
const WEBAUTHN_PROMPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// How many passkeys one account may hold, and so how many credential ids
/// every `login/start` answer lists: an account's own ids are padded with
/// decoys up to this, and an address without passkeys gets this many decoys.
///
/// Ten is well above what one person registers (a phone, a laptop, a password
/// manager and a backup security key or two) and keeps the list cheap for the
/// browser: a security key is handed the allow list in batches it can take,
/// and a U2F-only key is probed once per entry. Accounts that had more before
/// the cap keep every passkey usable, see [`login_options`].
pub const MAX_PASSKEYS_PER_ACCOUNT: usize = 10;

/// Credential-id lengths, in bytes, that widely used platform authenticators,
/// password managers and security keys produce. Decoy ids draw their lengths
/// from these together with the lengths stored on this instance.
const COMMON_CREDENTIAL_ID_LENGTHS: [usize; 4] = [16, 20, 32, 64];

/// The lengths WebAuthn allows a credential id: at least 16 bytes, at most
/// 1023. A stored length outside it is not a credential a browser could
/// report, so decoys never copy it.
const CREDENTIAL_ID_LENGTHS: std::ops::RangeInclusive<usize> = 16..=1023;

/// In-memory map of pending WebAuthn challenges with per-entry expiry. Entries
/// are pruned on insert so abandoned ceremonies can't accumulate unbounded, and
/// expired entries are rejected on lookup.
/// NOTE: process-local - for multi-instance deployments this should move to Redis.
struct ExpiringMap<V> {
    inner: Mutex<HashMap<String, (std::time::Instant, V)>>,
}

impl<V> ExpiringMap<V> {
    fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn insert(&self, key: String, value: V) {
        let mut map = self.inner.lock().unwrap();
        let now = std::time::Instant::now();
        map.retain(|_, (t, _)| now.duration_since(*t) < PASSKEY_STATE_TTL);
        if map.len() >= PASSKEY_STATE_MAX_ENTRIES
            && let Some(oldest) = map
                .iter()
                .min_by_key(|(_, (t, _))| *t)
                .map(|(key, _)| key.clone())
        {
            map.remove(&oldest);
        }
        map.insert(key, (now, value));
    }

    fn remove(&self, key: &str) -> Option<V> {
        let mut map = self.inner.lock().unwrap();
        match map.remove(key) {
            Some((t, v)) if std::time::Instant::now().duration_since(t) < PASSKEY_STATE_TTL => {
                Some(v)
            }
            _ => None,
        }
    }
}

static REG_STATE: Lazy<ExpiringMap<PasskeyRegistration>> = Lazy::new(ExpiringMap::new);

struct PendingPasskeyAuthentication {
    user_id: i32,
    token_version: i32,
    state: PasskeyAuthentication,
}

/// Pending sign-in ceremonies, keyed by the random `login_id` that `login_start`
/// hands to the browser that began them. Keying by the account's email would let
/// anyone who knows the address consume or replace the owner's challenge, and
/// would make two tabs signing in as the same user clobber each other.
static AUTH_STATE: Lazy<ExpiringMap<PendingPasskeyAuthentication>> = Lazy::new(ExpiringMap::new);

/// Resolve the WebAuthn RP ID from an optional `WEBAUTHN_RP_ID` and the frontend origin.
///
/// A non-empty value (after trim) wins; empty or whitespace is treated as unset
/// and the origin host is used. The RP ID must equal the origin host or be a
/// DNS parent of it (`opn.onl` is valid for `https://app.opn.onl`; `other.com`
/// is not). A passkey registered under the wrong RP ID is unusable, so a
/// mismatch is an error rather than a silent substitution.
pub(crate) fn effective_webauthn_rp_id(
    rp_id_env: Option<&str>,
    origin: &Url,
) -> Result<String, String> {
    let origin_host = origin.host_str().unwrap_or("localhost");
    let rp_id = match rp_id_env {
        Some(raw) if !raw.trim().is_empty() => raw.trim(),
        _ => origin_host,
    };
    if !rp_id_is_parent_or_equal(rp_id, origin_host) {
        return Err(format!(
            "WEBAUTHN_RP_ID '{rp_id}' is not the origin host '{origin_host}' or a DNS parent of it. \
             The RP ID must be the frontend host or a parent domain (e.g. opn.onl is valid for \
             https://app.opn.onl; other.com is not). A passkey registered under the wrong RP ID is unusable."
        ));
    }
    Ok(rp_id.to_string())
}

fn rp_id_is_parent_or_equal(rp_id: &str, origin_host: &str) -> bool {
    let rp = rp_id.trim_end_matches('.').to_ascii_lowercase();
    let host = origin_host.trim_end_matches('.').to_ascii_lowercase();
    !rp.is_empty() && (host == rp || host.ends_with(&format!(".{rp}")))
}

/// Fail fast at process start if `WEBAUTHN_RP_ID` cannot be used with `FRONTEND_URL`.
pub fn validate_webauthn_rp_id() {
    // Same normalisation as short links: a trailing slash is not part of the origin.
    let rp_origin = crate::utils::env::frontend_base_url();
    let origin_url = match Url::parse(&rp_origin) {
        Ok(url) => url,
        Err(e) => panic!("Invalid FRONTEND_URL for WebAuthn: {e}"),
    };
    let rp_id_env = std::env::var("WEBAUTHN_RP_ID").ok();
    if let Err(e) = effective_webauthn_rp_id(rp_id_env.as_deref(), &origin_url) {
        panic!("{e}");
    }
}

/// The origin and RP ID passkeys are bound to, from `FRONTEND_URL` and
/// `WEBAUTHN_RP_ID`. Shared by the real ceremonies and the decoy challenge so
/// both advertise the same `rpId`.
fn webauthn_origin_and_rp_id() -> (Url, String) {
    let rp_origin = crate::utils::env::frontend_base_url();

    let origin_url = Url::parse(&rp_origin).unwrap_or_else(|e| {
        tracing::error!(
            "Invalid FRONTEND_URL for WebAuthn: {} - using localhost fallback",
            e
        );
        Url::parse("http://localhost:5173").expect("Hardcoded URL should always parse")
    });

    let rp_id_env = std::env::var("WEBAUTHN_RP_ID").ok();
    let effective_rp_id = effective_webauthn_rp_id(rp_id_env.as_deref(), &origin_url)
        .unwrap_or_else(|e| panic!("{e}"));
    (origin_url, effective_rp_id)
}

// Helper to get Webauthn instance
fn get_webauthn() -> Webauthn {
    let (origin_url, effective_rp_id) = webauthn_origin_and_rp_id();

    WebauthnBuilder::new(&effective_rp_id, &origin_url)
        .map_err(|e| {
            tracing::error!("Failed to create WebAuthn builder: {:?}", e);
            e
        })
        .and_then(|builder| {
            builder
                .timeout(WEBAUTHN_PROMPT_TIMEOUT)
                .build()
                .map_err(|e| {
                    tracing::error!("Failed to build WebAuthn: {:?}", e);
                    e
                })
        })
        .unwrap_or_else(|_| {
            // Last resort fallback for development only
            tracing::warn!(
                "WebAuthn falling back to localhost - passkeys may not work in production!"
            );
            let fallback_url = Url::parse("http://localhost:5173").expect("Hardcoded URL");
            WebauthnBuilder::new("localhost", &fallback_url)
                .expect("Localhost WebAuthn builder")
                .timeout(WEBAUTHN_PROMPT_TIMEOUT)
                .build()
                .expect("Localhost WebAuthn build")
        })
}

/// Instance kill-switch for passkeys (default ON, like the other ENABLE_* flags).
/// When `false`, the enroll and login ceremonies are refused; management
/// endpoints (list/delete/rename) keep working so users can still clean up.
fn passkeys_enabled() -> bool {
    env_flag("ENABLE_PASSKEYS", true)
}

#[derive(Deserialize, ToSchema)]
pub struct RegisterStartRequest {
    /// Accepted for wire compatibility but IGNORED server-side: the target
    /// account is taken from the caller's authenticated identity, never from
    /// this field (see `register_start`).
    #[allow(dead_code)]
    pub username: String,
}

#[derive(Serialize)]
pub struct RegisterStartResponse {
    pub options: CreationChallengeResponse,
}

#[derive(Deserialize, ToSchema)]
pub struct RegisterFinishRequest {
    /// Accepted for wire compatibility but IGNORED server-side: the credential
    /// is bound to the caller's authenticated identity (see `register_finish`).
    #[allow(dead_code)]
    pub username: String,
    /// Standard WebAuthn registration credential — not expanded.
    #[schema(value_type = Object)]
    pub credential: RegisterPublicKeyCredential,
}

#[derive(Deserialize, ToSchema)]
pub struct LoginStartRequest {
    pub username: String,
}

#[derive(Serialize, ToSchema)]
pub struct LoginStartResponse {
    /// Standard WebAuthn assertion challenge — not expanded. Its
    /// `allowCredentials` always lists 10 ids, real and decoy alike.
    #[schema(value_type = Object)]
    pub options: RequestChallengeResponse,
    /// Names this sign-in attempt. Send it back unchanged to `login/finish`;
    /// the pending challenge is held under it, usable once, for five minutes.
    pub login_id: String,
}

#[derive(Deserialize, ToSchema)]
pub struct LoginFinishRequest {
    /// The `login_id` that `login/start` returned for this attempt.
    pub login_id: String,
    /// Standard WebAuthn assertion credential — not expanded.
    #[schema(value_type = Object)]
    pub credential: PublicKeyCredential,
}

#[derive(Serialize, ToSchema)]
pub struct PasskeyAuthResponse {
    pub token: String,
    pub email_verified: bool,
    pub is_admin: bool,
}

/// The refusal for one passkey more than [`MAX_PASSKEYS_PER_ACCOUNT`].
fn passkey_limit_reached() -> axum::response::Response {
    json_error(
        StatusCode::FORBIDDEN,
        format!(
            "You have reached the maximum of {MAX_PASSKEYS_PER_ACCOUNT} passkeys for this account. \
             Delete one to add another."
        ),
    )
}

/// Begin passkey enrollment for the authenticated caller. Returns a WebAuthn
/// `CreationChallengeResponse` to pass to the browser's credential API. The
/// response is a standard WebAuthn ceremony object and is not expanded into
/// the schema. An account holds at most 10 passkeys; one more is refused (403).
#[utoipa::path(
    post,
    path = "/auth/passkey/register/start",
    request_body = RegisterStartRequest,
    responses(
        (status = 200, description = "WebAuthn creation challenge"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Passkeys are disabled on this instance, the caller's email is not verified, or the account already holds the maximum of 10 passkeys"),
        (status = 404, description = "User not found"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn register_start(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(_payload): Json<RegisterStartRequest>,
) -> impl IntoResponse {
    if !passkeys_enabled() {
        return json_error(
            StatusCode::FORBIDDEN,
            "Passkeys are disabled on this instance",
        );
    }
    // Passkey enrollment MUST be authenticated: a passkey may only be added to
    // the caller's own account. We derive the target account from the caller's
    // authenticated identity, NOT from the client-supplied `username`. This
    // closes the account-takeover hole where anyone who knew a victim's email
    // could enroll their own authenticator onto the victim's account.
    let auth = match crate::handlers::links::get_jwt_auth_from_header(&state.db, &headers).await {
        Some(auth) => auth,
        None => return json_error(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };

    let user = match users::Entity::find_by_id(auth.user_id)
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .one(&state.db)
        .await
    {
        Ok(Some(u)) => u,
        _ => return json_error(StatusCode::NOT_FOUND, "User not found"),
    };
    if !verification_satisfied(&state, &user) {
        return json_error(
            StatusCode::FORBIDDEN,
            "Verify your email before registering a passkey",
        );
    }
    match passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user.id))
        .count(&state.db)
        .await
    {
        Ok(held) if held >= MAX_PASSKEYS_PER_ACCOUNT as u64 => return passkey_limit_reached(),
        Ok(_) => {}
        Err(_) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to start registration",
            );
        }
    }

    // Deterministic UUID from ID for demo purposes
    let user_unique_id =
        Uuid::from_bytes(user.id.to_le_bytes().repeat(4)[0..16].try_into().unwrap());

    // In a real app, you might want to exclude already registered credentials here
    let exclude_credentials: Option<Vec<CredentialID>> = None;

    let webauthn = get_webauthn();
    let (ccr, reg_state) = match webauthn.start_passkey_registration(
        user_unique_id,
        &user.email,
        &user.email,
        exclude_credentials,
    ) {
        Ok(res) => res,
        Err(_) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to start registration",
            );
        }
    };

    // Key the pending-challenge store by the authenticated user id so the finish
    // step can only complete for the same account that started the ceremony.
    REG_STATE.insert(user.id.to_string(), reg_state);

    (StatusCode::OK, Json(RegisterStartResponse { options: ccr })).into_response()
}

/// Complete passkey enrollment for the authenticated caller. Rejects a
/// credential already registered to any account (409), and an eleventh passkey
/// (403).
#[utoipa::path(
    post,
    path = "/auth/passkey/register/finish",
    request_body = RegisterFinishRequest,
    responses(
        (status = 200, description = "Passkey registered", body = MessageResponse),
        (status = 400, description = "Invalid or expired registration ceremony"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Passkeys are disabled on this instance, the caller's email is not verified, or the account already holds the maximum of 10 passkeys"),
        (status = 404, description = "User not found"),
        (status = 409, description = "This passkey is already registered"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn register_finish(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<RegisterFinishRequest>,
) -> impl IntoResponse {
    if !passkeys_enabled() {
        return json_error(
            StatusCode::FORBIDDEN,
            "Passkeys are disabled on this instance",
        );
    }
    // Same authenticated-identity rule as register_start: the credential is bound
    // to the CALLER's account, never to a client-supplied username. The pending
    // challenge is looked up by the authenticated user id.
    let auth = match crate::handlers::links::get_jwt_auth_from_header(&state.db, &headers).await {
        Some(auth) => auth,
        None => return json_error(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };

    let reg_state = match REG_STATE.remove(&auth.user_id.to_string()) {
        Some(s) => s,
        None => return json_error(StatusCode::BAD_REQUEST, "Registration state not found"),
    };

    let webauthn = get_webauthn();
    let passkey = match webauthn.finish_passkey_registration(&payload.credential, &reg_state) {
        Ok(p) => p,
        Err(_) => {
            return json_error(StatusCode::BAD_REQUEST, "Failed to finish registration");
        }
    };

    let txn = match state.db.begin().await {
        Ok(txn) => txn,
        Err(_) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to register passkey",
            );
        }
    };

    let user = match users::Entity::find_by_id(auth.user_id)
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .lock_exclusive()
        .one(&txn)
        .await
    {
        Ok(Some(user))
            if user.token_version == auth.token_version
                && verification_satisfied(&state, &user) =>
        {
            user
        }
        Ok(Some(_)) => {
            let _ = txn.rollback().await;
            return json_error(
                StatusCode::FORBIDDEN,
                "Verify your email and start registration again",
            );
        }
        _ => {
            let _ = txn.rollback().await;
            return json_error(StatusCode::NOT_FOUND, "User not found");
        }
    };

    // Save passkey to DB - serialize the passkey for storage
    let passkey_json = serde_json::to_string(&passkey).unwrap_or_default();
    let cred_id_str = credential_key(passkey.cred_id());

    // A credential id is globally unique to one authenticator credential.
    // Reject a re-registration of an already-known credential instead of
    // silently creating a duplicate row (the insert result was previously
    // discarded, so a duplicate — or, with the new UNIQUE index, a constraint
    // violation — went unnoticed).
    let already_registered = passkeys::Entity::find()
        .filter(passkeys::Column::CredId.is_in(credential_keys(passkey.cred_id())))
        .one(&txn)
        .await
        .ok()
        .flatten()
        .is_some();
    if already_registered {
        let _ = txn.rollback().await;
        return json_error(StatusCode::CONFLICT, "This passkey is already registered");
    }

    // Checked again under the user row lock: `register_start` refused at the
    // cap, but the account may have reached it since.
    match passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user.id))
        .count(&txn)
        .await
    {
        Ok(held) if held >= MAX_PASSKEYS_PER_ACCOUNT as u64 => {
            let _ = txn.rollback().await;
            return passkey_limit_reached();
        }
        Ok(_) => {}
        Err(_) => {
            let _ = txn.rollback().await;
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to register passkey",
            );
        }
    }

    let passkey_model = passkeys::ActiveModel {
        user_id: Set(user.id),
        cred_id: Set(cred_id_str),
        cred_public_key: Set(passkey_json),
        counter: Set(0),
        ..Default::default()
    };

    if passkeys::Entity::insert(passkey_model)
        .exec(&txn)
        .await
        .is_err()
    {
        let _ = txn.rollback().await;
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to register passkey",
        );
    }
    if txn.commit().await.is_err() {
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to register passkey",
        );
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({ "message": "Passkey registered" })),
    )
        .into_response()
}

/// What `passkeys.cred_id` holds for a credential: its id as unpadded base64url,
/// the text a browser reports as `PublicKeyCredential.id`.
fn credential_key(id: &CredentialID) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(id)
}

/// Passkeys saved before `credential_key` existed hold the `Debug` text of the
/// credential id instead, `HumanBinaryData([1, 2, 3])`. Those rows are live, so
/// lookups accept this form too. It is spelled out rather than taken from
/// `{:?}` so that a dependency upgrade that changes `Debug` cannot strand them.
fn legacy_credential_key(id: &CredentialID) -> String {
    let bytes: &[u8] = id.as_ref();
    format!("HumanBinaryData({bytes:?})")
}

/// Both spellings a credential may be stored under.
fn credential_keys(id: &CredentialID) -> [String; 2] {
    [credential_key(id), legacy_credential_key(id)]
}

/// HMAC-SHA256. Keyed with the server secret, it derives the decoy credential
/// ids and the allow-list order of passkey sign-in (see [`login_prf`]).
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let hashed = Sha256::digest(key);
        key_block[..hashed.len()].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer.finalize().into()
}

/// `HMAC(secret, label, username, parts)`, each text field length-prefixed so
/// that no two inputs encode alike. Everything that varies between two sign-in
/// answers apart from the challenge comes from here, so asking twice for the
/// same address gives the same list, as it does for a real account.
fn login_prf(secret: &[u8], label: &str, username: &str, parts: &[u32]) -> [u8; 32] {
    let mut data = Vec::with_capacity(8 + label.len() + username.len() + 4 * parts.len());
    for field in [label.as_bytes(), username.as_bytes()] {
        data.extend((field.len() as u32).to_be_bytes());
        data.extend(field);
    }
    for part in parts {
        data.extend(part.to_be_bytes());
    }
    hmac_sha256(secret, &data)
}

/// The decoy credential id in `slot` of `username`'s allow list.
///
/// Its length is one of `lengths`, picked per (username, slot) by rendezvous
/// hashing: every candidate length gets a keyed score and the highest wins. A
/// length that joins the set later takes over only the slots where it
/// outscores the current winner, so a new kind of authenticator on the
/// instance changes few decoys and leaves the rest as they were.
fn decoy_credential_id(secret: &[u8], username: &str, slot: u32, lengths: &[usize]) -> Vec<u8> {
    let length = lengths
        .iter()
        .copied()
        .max_by_key(|&length| {
            login_prf(
                secret,
                "passkey decoy length",
                username,
                &[slot, length as u32],
            )
        })
        .unwrap_or(COMMON_CREDENTIAL_ID_LENGTHS[2]);
    let mut id = Vec::with_capacity(length + 32);
    let mut block = 0u32;
    while id.len() < length {
        id.extend(login_prf(
            secret,
            "passkey decoy id",
            username,
            &[slot, length as u32, block],
        ));
        block += 1;
    }
    id.truncate(length);
    id
}

/// Fisher-Yates driven by [`login_prf`]: the same permutation for the same
/// username and list size every time, a different one for each username.
fn shuffle_for_username<T>(secret: &[u8], username: &str, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let draw = login_prf(secret, "passkey allow-list order", username, &[i as u32]);
        let draw = u64::from_be_bytes(draw[..8].try_into().expect("eight bytes"));
        items.swap(i, (draw % (i as u64 + 1)) as usize);
    }
}

/// The one builder for the `publicKey` options of every `login/start` answer,
/// real or decoy, so the two share their keys, key order and every value
/// that is not secret.
///
/// `issued` is what webauthn-rs's `start_passkey_authentication` returned:
/// for a real account the challenge stored in the pending state and the
/// account's credential ids, for a decoy a challenge nobody holds and no ids.
/// Only the challenge, timeout, RP ID and user-verification policy are taken
/// from it, and those are the same call's output in both cases. Dropped:
///
/// - `transports` on each credential, which only a real one can carry. The
///   browser then offers every transport.
/// - `extensions`, `hints` and `mediation`, which webauthn-rs 0.5 leaves
///   unset for passkeys anyway. Verification reads none of these: the
///   allowed credentials, policy, challenge and appid all come from the
///   pending `PasskeyAuthentication` kept on the server.
///
/// The ids are padded with decoys from [`decoy_credential_id`] up to
/// [`MAX_PASSKEYS_PER_ACCOUNT`] and put in a per-username order. An account
/// that held more passkeys before the cap was introduced keeps all of them in
/// its list, padded to the next multiple of the cap: dropping some would lock
/// its owner out of those, and the longer list only says the address has more
/// than the cap.
fn login_options(
    issued: RequestChallengeResponse,
    secret: &[u8],
    username: &str,
    decoy_lengths: &[usize],
) -> RequestChallengeResponse {
    let PublicKeyCredentialRequestOptions {
        challenge,
        timeout,
        rp_id,
        allow_credentials,
        user_verification,
        hints: _,
        extensions: _,
    } = issued.public_key;

    let mut ids: Vec<Base64UrlSafeData> = allow_credentials
        .into_iter()
        .map(|credential| credential.id)
        .collect();
    let size = ids.len().div_ceil(MAX_PASSKEYS_PER_ACCOUNT).max(1) * MAX_PASSKEYS_PER_ACCOUNT;
    for slot in ids.len()..size {
        ids.push(decoy_credential_id(secret, username, slot as u32, decoy_lengths).into());
    }
    shuffle_for_username(secret, username, &mut ids);

    RequestChallengeResponse {
        public_key: PublicKeyCredentialRequestOptions {
            challenge,
            timeout,
            rp_id,
            allow_credentials: ids
                .into_iter()
                .map(|id| AllowCredentials {
                    type_: "public-key".to_string(),
                    id,
                    transports: None,
                })
                .collect(),
            user_verification,
            hints: None,
            extensions: None,
        },
        mediation: None,
    }
}

/// Byte length of every stored credential id. `cred_id` holds the id as
/// unpadded base64url, four characters for three bytes, so `chars * 3 / 4`
/// rounded down is the byte count; rows from before that hold the `Debug` text
/// `HumanBinaryData([1, 2, 3])` (see `legacy_credential_key`), whose byte
/// count is its commas plus one.
const STORED_CREDENTIAL_ID_LENGTHS: &str = "SELECT DISTINCT (CASE \
     WHEN cred_id LIKE 'HumanBinaryData(%' \
     THEN length(cred_id) - length(replace(cred_id, ',', '')) + 1 \
     ELSE length(cred_id) * 3 / 4 END)::bigint \
     FROM passkeys";

/// The lengths decoy credential ids are drawn from: the common ones plus every
/// length a passkey stored on this instance has, so that a real id of an
/// unusual length is never the only id of that length in the lists the
/// instance hands out. Read on every `login/start`, real or decoy alike.
async fn decoy_id_lengths(db: &DatabaseConnection) -> Vec<usize> {
    let mut lengths = COMMON_CREDENTIAL_ID_LENGTHS.to_vec();
    match db
        .query_all(Statement::from_string(
            DbBackend::Postgres,
            STORED_CREDENTIAL_ID_LENGTHS,
        ))
        .await
    {
        Ok(rows) => lengths.extend(
            rows.iter()
                .filter_map(|row| row.try_get_by_index::<i64>(0).ok())
                .filter_map(|length| usize::try_from(length).ok())
                .filter(|length| CREDENTIAL_ID_LENGTHS.contains(length)),
        ),
        Err(e) => tracing::warn!("Could not read stored passkey id lengths: {e}"),
    }
    lengths.sort_unstable();
    lengths.dedup();
    lengths
}

/// The 200 every `login/start` that gets this far answers with.
async fn login_start_answer(
    db: &DatabaseConnection,
    username: &str,
    issued: RequestChallengeResponse,
    login_id: String,
) -> axum::response::Response {
    let secret = std::env::var("JWT_SECRET").unwrap_or_default();
    let lengths = decoy_id_lengths(db).await;
    let options = login_options(issued, secret.as_bytes(), username, &lengths);
    (
        StatusCode::OK,
        Json(LoginStartResponse { options, login_id }),
    )
        .into_response()
}

/// A challenge nobody will be able to answer, under a `login_id` that names no
/// pending state. webauthn-rs issues it exactly as it issues a real one, for an
/// empty credential list, and the state that could verify it is dropped here;
/// `login_finish` treats the id like any other unknown attempt.
async fn decoy_login_start(db: &DatabaseConnection, username: &str) -> axum::response::Response {
    match get_webauthn().start_passkey_authentication(&[]) {
        Ok((issued, _unanswerable)) => {
            login_start_answer(db, username, issued, generate_token()).await
        }
        Err(_) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to start authentication",
        ),
    }
}

/// Begin passkey login. Returns a WebAuthn `RequestChallengeResponse` and a
/// `login_id` to send back to `login/finish`.
///
/// Every answer has one shape, whatever the address: `allowCredentials` lists
/// exactly 10 entries of `type` and `id` (no `transports`), in an order fixed
/// per address. An account's own passkeys are mixed in among decoy ids that
/// stay the same from one request to the next; an unknown address, a disabled
/// account and an account without passkeys get 10 decoys. Browsers skip ids
/// their authenticators do not hold. Pending state is stored only when the
/// account exists and has parseable passkeys.
#[utoipa::path(
    post,
    path = "/auth/passkey/login/start",
    request_body = LoginStartRequest,
    responses(
        (status = 200, description = "WebAuthn assertion challenge and the login_id that names this attempt. allowCredentials always lists 10 credential ids, real and decoy alike", body = LoginStartResponse),
        (status = 403, description = "Passkeys are disabled on this instance"),
    ),
    tag = "Authentication"
)]
pub async fn login_start(
    State(state): State<AppState>,
    Json(payload): Json<LoginStartRequest>,
) -> impl IntoResponse {
    if !passkeys_enabled() {
        return json_error(
            StatusCode::FORBIDDEN,
            "Passkeys are disabled on this instance",
        );
    }
    let username = normalize_email(&payload.username);
    if ensure_sign_in_domain_allowed(&state.db, &username)
        .await
        .is_err()
    {
        return decoy_login_start(&state.db, &username).await;
    }

    let user = live_users_by_email(&payload.username)
        .filter(users::Column::DisabledAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    let Some(user) = user else {
        return decoy_login_start(&state.db, &username).await;
    };

    // In row order, so the account's ids sit in the same slots every time.
    let db_passkeys = passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user.id))
        .order_by_asc(passkeys::Column::Id)
        .all(&state.db)
        .await
        .unwrap_or(vec![]);

    let allow_credentials: Vec<Passkey> = db_passkeys
        .iter()
        .filter_map(|pk| serde_json::from_str(&pk.cred_public_key).ok())
        .collect();

    if allow_credentials.is_empty() {
        return decoy_login_start(&state.db, &username).await;
    }

    let webauthn = get_webauthn();
    let (issued, auth_state) = match webauthn.start_passkey_authentication(&allow_credentials) {
        Ok(res) => res,
        Err(_) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to start authentication",
            );
        }
    };

    let login_id = generate_token();
    AUTH_STATE.insert(
        login_id.clone(),
        PendingPasskeyAuthentication {
            user_id: user.id,
            token_version: user.token_version,
            state: auth_state,
        },
    );

    login_start_answer(&state.db, &username, issued, login_id).await
}

/// Complete passkey login and issue a JWT on success.
///
/// A `login_id` that is unknown, expired, already used or minted for a decoy
/// challenge, and an assertion that does not verify, all get the same `401`.
/// The pending challenge is consumed by the first attempt, right or wrong.
#[utoipa::path(
    post,
    path = "/auth/passkey/login/finish",
    request_body = LoginFinishRequest,
    responses(
        (status = 200, description = "Authenticated; JWT issued", body = PasskeyAuthResponse),
        (status = 401, description = "Authentication failed: no pending attempt under this login_id, or the assertion did not verify"),
        (status = 403, description = "Passkeys are disabled on this instance"),
    ),
    tag = "Authentication"
)]
pub async fn login_finish(
    State(state): State<AppState>,
    Json(payload): Json<LoginFinishRequest>,
) -> impl IntoResponse {
    if !passkeys_enabled() {
        return json_error(
            StatusCode::FORBIDDEN,
            "Passkeys are disabled on this instance",
        );
    }
    // One answer for "no such attempt" (unknown, expired, replayed, or a decoy's
    // id) and "the assertion did not verify", so the pair says nothing about
    // whether the address has a passkey.
    let authentication_failed = || json_error(StatusCode::UNAUTHORIZED, "Authentication failed");
    let Some(pending) = AUTH_STATE.remove(&payload.login_id) else {
        return authentication_failed();
    };

    let webauthn = get_webauthn();
    let auth_result =
        match webauthn.finish_passkey_authentication(&payload.credential, &pending.state) {
            Ok(res) => res,
            Err(_) => return authentication_failed(),
        };

    let cred_keys = credential_keys(auth_result.cred_id());

    // Serialize login completion against factor revocation and every other
    // token-version transition. Both paths lock the user row first, then the
    // passkey row. If revocation wins, the version/factor check below fails. If
    // login wins, the subsequent revoke bumps the version and invalidates this
    // newly-issued token.
    let txn = match state.db.begin().await {
        Ok(txn) => txn,
        Err(_) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to authenticate");
        }
    };

    let user = match users::Entity::find_by_id(pending.user_id)
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .lock_exclusive()
        .one(&txn)
        .await
    {
        Ok(Some(user)) if user.token_version == pending.token_version => user,
        _ => {
            let _ = txn.rollback().await;
            return json_error(StatusCode::UNAUTHORIZED, "Authentication state was revoked");
        }
    };

    let passkey_db = match passkeys::Entity::find()
        .filter(passkeys::Column::CredId.is_in(cred_keys))
        .filter(passkeys::Column::UserId.eq(pending.user_id))
        .lock_exclusive()
        .one(&txn)
        .await
    {
        Ok(Some(passkey)) => passkey,
        _ => {
            let _ = txn.rollback().await;
            return json_error(
                StatusCode::UNAUTHORIZED,
                "Authentication factor was revoked",
            );
        }
    };

    // Persist the updated signature counter in the credential blob read by the
    // verifier, not only in the convenience counter column.
    let mut stored_passkey = match serde_json::from_str::<Passkey>(&passkey_db.cred_public_key) {
        Ok(passkey) => passkey,
        Err(_) => {
            let _ = txn.rollback().await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to authenticate");
        }
    };
    stored_passkey.update_credential(&auth_result);
    let updated_blob = match serde_json::to_string(&stored_passkey) {
        Ok(blob) => blob,
        Err(_) => {
            let _ = txn.rollback().await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to authenticate");
        }
    };

    let mut active_pk: passkeys::ActiveModel = passkey_db.into();
    active_pk.cred_public_key = Set(updated_blob);
    active_pk.counter = Set(auth_result.counter() as i32);
    active_pk.last_used = Set(Some(Utc::now().naive_utc()));
    if active_pk.update(&txn).await.is_err() {
        let _ = txn.rollback().await;
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to authenticate");
    };

    let token = match create_jwt(user.id, &user.email, user.token_version) {
        Ok(t) => t,
        Err(_) => {
            let _ = txn.rollback().await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create token");
        }
    };

    if txn.commit().await.is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to authenticate");
    }

    (
        StatusCode::OK,
        Json(PasskeyAuthResponse {
            token,
            email_verified: user.email_verified,
            is_admin: user.is_admin,
        }),
    )
        .into_response()
}

#[derive(Serialize, ToSchema)]
pub struct PasskeyInfo {
    pub id: i32,
    pub name: String,
    pub created_at: String,
    pub last_used: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct PasskeyListResponse {
    pub passkeys: Vec<PasskeyInfo>,
}

/// List user's passkeys
#[utoipa::path(
    get,
    path = "/auth/passkeys",
    responses(
        (status = 200, description = "The caller's registered passkeys", body = PasskeyListResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn list_passkeys(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let user_id = match crate::handlers::links::get_jwt_auth_from_header(&state.db, &headers).await
    {
        Some(auth) => auth.user_id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    let user_passkeys = passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user_id))
        .all(&state.db)
        .await
        .unwrap_or_default();

    let passkey_list: Vec<PasskeyInfo> = user_passkeys
        .into_iter()
        .map(|pk| PasskeyInfo {
            id: pk.id,
            name: pk.name.unwrap_or_else(|| format!("Passkey {}", pk.id)),
            created_at: utc_rfc3339(pk.created_at),
            last_used: pk.last_used.map(utc_rfc3339),
        })
        .collect();

    (
        StatusCode::OK,
        Json(PasskeyListResponse {
            passkeys: passkey_list,
        }),
    )
        .into_response()
}

#[derive(Deserialize, ToSchema)]
pub struct DeletePasskeyRequest {
    pub passkey_id: i32,
}

#[derive(Serialize, ToSchema)]
pub struct DeletePasskeyResponse {
    pub message: String,
    /// A fresh session token. Deleting a passkey signs out every session
    /// issued before it, the caller's included; this one replaces theirs.
    pub token: String,
}

/// Delete a passkey
#[utoipa::path(
    post,
    path = "/auth/passkey/delete",
    request_body = DeletePasskeyRequest,
    responses(
        (status = 200, description = "Passkey deleted; the response carries a fresh token for the caller's session", body = DeletePasskeyResponse),
        (status = 400, description = "Cannot delete the account's only login method"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Passkey not found"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn delete_passkey(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<DeletePasskeyRequest>,
) -> impl IntoResponse {
    let auth = match crate::handlers::links::get_jwt_auth_from_header(&state.db, &headers).await {
        Some(auth) => auth,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    let txn = match state.db.begin().await {
        Ok(txn) => txn,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to delete passkey"})),
            )
                .into_response();
        }
    };

    // Lock order matches login_finish: user first, factor second.
    let user = match users::Entity::find_by_id(auth.user_id)
        .filter(users::Column::DeletedAt.is_null())
        .lock_exclusive()
        .one(&txn)
        .await
    {
        Ok(Some(user)) if user.token_version == auth.token_version => user,
        _ => {
            let _ = txn.rollback().await;
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    let passkey = match passkeys::Entity::find_by_id(payload.passkey_id)
        .filter(passkeys::Column::UserId.eq(auth.user_id))
        .lock_exclusive()
        .one(&txn)
        .await
    {
        Ok(Some(passkey)) => passkey,
        _ => {
            let _ = txn.rollback().await;
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Passkey not found"})),
            )
                .into_response();
        }
    };

    if user.password_hash.is_empty() {
        let passkey_count = passkeys::Entity::find()
            .filter(passkeys::Column::UserId.eq(auth.user_id))
            .count(&txn)
            .await
            .unwrap_or(0);
        if passkey_count <= 1 {
            let _ = txn.rollback().await;
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Cannot delete the last passkey when no password is set"
                })),
            )
                .into_response();
        }
    }

    let Some(next_token_version) = auth.token_version.checked_add(1) else {
        let _ = txn.rollback().await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to delete passkey"})),
        )
            .into_response();
    };
    // The bump below signs out every session issued before it, this caller's
    // included, so hand back a token for the new version. Minted before the
    // commit: if it cannot be, nothing is deleted and the session survives.
    let token = match create_jwt(user.id, &user.email, next_token_version) {
        Ok(token) => token,
        Err(_) => {
            let _ = txn.rollback().await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to delete passkey"})),
            )
                .into_response();
        }
    };
    let mut active_user: users::ActiveModel = user.into();
    active_user.token_version = Set(next_token_version);
    let result = async {
        passkey.delete(&txn).await?;
        active_user.update(&txn).await?;
        Ok::<(), DbErr>(())
    }
    .await;

    if result.is_err() {
        let _ = txn.rollback().await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to delete passkey"})),
        )
            .into_response();
    }
    if txn.commit().await.is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to delete passkey"})),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(DeletePasskeyResponse {
            message: "Passkey deleted successfully".to_string(),
            token,
        }),
    )
        .into_response()
}

#[derive(Deserialize, ToSchema)]
pub struct RenamePasskeyRequest {
    pub passkey_id: i32,
    pub name: String,
}

/// Rename a passkey
#[utoipa::path(
    post,
    path = "/auth/passkey/rename",
    request_body = RenamePasskeyRequest,
    responses(
        (status = 200, description = "Passkey renamed"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Passkey not found"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn rename_passkey(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<RenamePasskeyRequest>,
) -> impl IntoResponse {
    let user_id = match crate::handlers::links::get_jwt_auth_from_header(&state.db, &headers).await
    {
        Some(auth) => auth.user_id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    // Verify the passkey belongs to the user
    let passkey = passkeys::Entity::find_by_id(payload.passkey_id)
        .filter(passkeys::Column::UserId.eq(user_id))
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(pk) = passkey {
        let mut active_pk: passkeys::ActiveModel = pk.into();
        active_pk.name = Set(Some(payload.name.clone()));

        match active_pk.update(&state.db).await {
            Ok(_) => (
                StatusCode::OK,
                Json(serde_json::json!({"message": "Passkey renamed successfully"})),
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to rename passkey"})),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Passkey not found"})),
        )
            .into_response()
    }
}

#[cfg(test)]
mod webauthn_rp_id_tests {
    use super::effective_webauthn_rp_id;
    use url::Url;

    fn origin(s: &str) -> Url {
        Url::parse(s).expect("test origin must parse")
    }

    #[test]
    fn non_empty_webauthn_rp_id_wins_when_it_is_a_parent_of_the_origin() {
        let got = effective_webauthn_rp_id(Some("opn.onl"), &origin("https://app.opn.onl"))
            .expect("parent RP ID must be accepted");
        assert_eq!(got, "opn.onl");
    }

    #[test]
    fn empty_or_whitespace_webauthn_rp_id_falls_back_to_origin_host() {
        let origin = origin("https://app.opn.onl");
        assert_eq!(
            effective_webauthn_rp_id(None, &origin).unwrap(),
            "app.opn.onl"
        );
        assert_eq!(
            effective_webauthn_rp_id(Some(""), &origin).unwrap(),
            "app.opn.onl"
        );
        assert_eq!(
            effective_webauthn_rp_id(Some("   \t"), &origin).unwrap(),
            "app.opn.onl"
        );
    }

    #[test]
    fn webauthn_rp_id_equal_to_origin_host_is_accepted() {
        assert_eq!(
            effective_webauthn_rp_id(Some("app.opn.onl"), &origin("https://app.opn.onl")).unwrap(),
            "app.opn.onl"
        );
    }

    #[test]
    fn webauthn_rp_id_that_is_not_a_parent_of_the_origin_is_rejected() {
        let err = effective_webauthn_rp_id(Some("other.com"), &origin("https://app.opn.onl"))
            .expect_err("unrelated RP ID must be rejected");
        assert!(err.contains("other.com"), "{err}");
        assert!(err.contains("app.opn.onl"), "{err}");
    }
}

#[cfg(test)]
mod pending_state_tests {
    use super::*;

    #[test]
    fn a_pending_entry_is_handed_out_once() {
        let map = ExpiringMap::new();
        map.insert("attempt".to_string(), 7);
        assert_eq!(map.remove("other"), None);
        assert_eq!(map.remove("attempt"), Some(7));
        assert_eq!(map.remove("attempt"), None);
    }

    #[test]
    fn the_map_stays_under_its_ceiling_and_drops_the_oldest() {
        let map = ExpiringMap::new();
        for n in 0..PASSKEY_STATE_MAX_ENTRIES + 25 {
            map.insert(n.to_string(), n);
        }
        assert_eq!(map.inner.lock().unwrap().len(), PASSKEY_STATE_MAX_ENTRIES);
        assert_eq!(map.remove("0"), None, "the oldest ceremony made room");
        let newest = PASSKEY_STATE_MAX_ENTRIES + 24;
        assert_eq!(map.remove(&newest.to_string()), Some(newest));
    }

    #[test]
    fn credential_ids_are_stored_as_base64url() {
        let id = CredentialID::from(vec![0xfb, 0xff, 0xbe, 0x01]);
        assert_eq!(credential_key(&id), "-_--AQ");
    }

    /// Rows saved by earlier versions hold this text; changing it would strand
    /// those passkeys.
    #[test]
    fn the_old_credential_key_format_is_pinned() {
        let id = CredentialID::from(vec![1, 2, 3]);
        assert_eq!(legacy_credential_key(&id), "HumanBinaryData([1, 2, 3])");
    }
}

#[cfg(test)]
mod login_options_tests {
    use super::*;
    use webauthn_rs_proto::{
        AuthenticatorTransport, Mediation, PublicKeyCredentialHints,
        RequestAuthenticationExtensions, UserVerificationPolicy,
    };

    const SECRET: &[u8] = b"login-options-test-secret";
    const LENGTHS: &[usize] = &[16, 20, 32, 64];

    /// What webauthn-rs might issue for `ids`, with every optional field a
    /// real answer could carry filled in.
    fn issued(ids: &[Vec<u8>]) -> RequestChallengeResponse {
        RequestChallengeResponse {
            public_key: PublicKeyCredentialRequestOptions {
                challenge: vec![7u8; 32].into(),
                timeout: Some(300_000),
                rp_id: "test".to_string(),
                allow_credentials: ids
                    .iter()
                    .map(|id| AllowCredentials {
                        type_: "public-key".to_string(),
                        id: id.clone().into(),
                        transports: Some(vec![AuthenticatorTransport::Internal]),
                    })
                    .collect(),
                user_verification: UserVerificationPolicy::Required,
                hints: Some(vec![PublicKeyCredentialHints::ClientDevice]),
                extensions: Some(RequestAuthenticationExtensions {
                    appid: None,
                    uvm: Some(true),
                    hmac_get_secret: None,
                }),
            },
            mediation: Some(Mediation::Conditional),
        }
    }

    fn ids(options: &RequestChallengeResponse) -> Vec<Vec<u8>> {
        options
            .public_key
            .allow_credentials
            .iter()
            .map(|credential| credential.id.to_vec())
            .collect()
    }

    fn real_ids(count: usize) -> Vec<Vec<u8>> {
        (0..count).map(|n| vec![n as u8 + 1; 20]).collect()
    }

    /// The fields the builder drops must be ones webauthn-rs leaves unset for
    /// passkeys. An upgrade that starts sending one fails here, so that someone
    /// checks whether verification now depends on it.
    #[test]
    fn webauthn_rs_issues_no_field_the_builder_drops() {
        let (issued, _) = get_webauthn()
            .start_passkey_authentication(&[])
            .expect("challenge");
        assert!(issued.public_key.extensions.is_none());
        assert!(issued.public_key.hints.is_none());
        assert!(issued.mediation.is_none());
    }

    #[test]
    fn every_list_holds_the_cap_and_keeps_the_real_ids() {
        for count in 0..=MAX_PASSKEYS_PER_ACCOUNT {
            let real = real_ids(count);
            let options = login_options(issued(&real), SECRET, "a@example.com", LENGTHS);
            let listed = ids(&options);
            assert_eq!(listed.len(), MAX_PASSKEYS_PER_ACCOUNT, "{count} real ids");
            for id in &real {
                assert!(listed.contains(id), "{count} real ids: one is missing");
            }
            for id in &listed {
                assert!(
                    real.contains(id) || LENGTHS.contains(&id.len()),
                    "a decoy of length {}",
                    id.len()
                );
            }
        }
    }

    #[test]
    fn only_the_challenge_and_shared_settings_survive() {
        let options = login_options(issued(&real_ids(2)), SECRET, "a@example.com", LENGTHS);
        assert!(options.mediation.is_none());
        assert!(options.public_key.hints.is_none());
        assert!(options.public_key.extensions.is_none());
        assert_eq!(options.public_key.challenge.to_vec(), vec![7u8; 32]);
        for credential in &options.public_key.allow_credentials {
            assert_eq!(credential.type_, "public-key");
            assert!(credential.transports.is_none());
        }
    }

    #[test]
    fn an_account_over_the_cap_lists_every_passkey() {
        let real = real_ids(MAX_PASSKEYS_PER_ACCOUNT + 1);
        let options = login_options(issued(&real), SECRET, "a@example.com", LENGTHS);
        let listed = ids(&options);
        assert_eq!(listed.len(), 2 * MAX_PASSKEYS_PER_ACCOUNT);
        assert!(real.iter().all(|id| listed.contains(id)));
    }

    #[test]
    fn the_list_is_fixed_per_address_and_differs_between_addresses() {
        let first = ids(&login_options(
            issued(&[]),
            SECRET,
            "a@example.com",
            LENGTHS,
        ));
        let again = ids(&login_options(
            issued(&[]),
            SECRET,
            "a@example.com",
            LENGTHS,
        ));
        let other = ids(&login_options(
            issued(&[]),
            SECRET,
            "b@example.com",
            LENGTHS,
        ));
        assert_eq!(first, again);
        assert!(first.iter().all(|id| !other.contains(id)));
    }

    #[test]
    fn real_ids_land_in_different_slots_for_different_addresses() {
        let real = real_ids(1);
        let slots: std::collections::BTreeSet<usize> = (0..50)
            .map(|n| {
                let username = format!("user{n}@example.com");
                let listed = ids(&login_options(issued(&real), SECRET, &username, LENGTHS));
                listed.iter().position(|id| *id == real[0]).expect("listed")
            })
            .collect();
        assert!(slots.len() > 5, "real id slots: {slots:?}");
    }

    /// A length that joins the set takes over some slots and leaves every
    /// other decoy exactly as it was.
    #[test]
    fn a_new_length_moves_few_decoys() {
        let mut kept = 0;
        let mut moved = 0;
        for n in 0..100 {
            let username = format!("user{n}@example.com");
            for slot in 0..MAX_PASSKEYS_PER_ACCOUNT as u32 {
                let before = decoy_credential_id(SECRET, &username, slot, LENGTHS);
                let after = decoy_credential_id(SECRET, &username, slot, &[16, 20, 32, 48, 64]);
                if after.len() == 48 {
                    moved += 1;
                } else {
                    assert_eq!(before, after, "an unmoved decoy changed");
                    kept += 1;
                }
            }
        }
        assert!(moved > 0 && moved < kept, "moved {moved}, kept {kept}");
    }

    #[test]
    fn decoy_ids_reach_any_length() {
        let long = decoy_credential_id(SECRET, "a@example.com", 0, &[100]);
        assert_eq!(long.len(), 100);
        assert_ne!(long[..32], long[32..64]);
    }
}
